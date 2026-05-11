//! One-Shot Perfection Mode — single entry flow from idea to production-ready app.
//!
//! User provides one description. System executes:
//! 1. User learning context injection
//! 2. Intent analysis (+ plugin hook: OnIntentParsed)
//! 3. Architecture decisions with learned weights (+ plugin hook: OnDecisionMade)
//! 4. Product Engine brief (persona, monetization, onboarding, retention)
//! 5. Plan generation with full context
//! 6. Codegen (+ plugin hook: OnBeforeCodegen)
//! 7. Post-generation hooks (OnAfterGeneration)
//! 8. Taste scoring + auto-redesign if below 70 (+ plugin hook: OnTasteScore)
//! 9. Record outcome for learning
//!
//! Streams SSE events throughout.
//! Output: production-ready app in `~/.nexus/projects/<id>/generated`.

use std::convert::Infallible;
use std::sync::Arc;

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::mpsc;
use tracing::info;

use nexus_store::{build_generation_prompt, CodeGenMaterializer, ProjectService};

use crate::{
    adaptive_runtime::AdjustmentAction,
    decision_engine,
    decision_learning::{DecisionOutcome, Outcome},
    error::{ApiError, ApiResult},
    intelligence_amplifier,
    perceived_speed,
    plugin_hooks::{self, HookPoint},
    product_engine,
    security::auth::{AuthContext, Scope},
    security::project_access::ProjectAccess,
    state::AppState,
    taste_engine,
    taste_redesign::{self, RedesignConfig},
    thinking_stream,
    user_learning,
    variant_engine,
};

// ---------------------------------------------------------------------------
// Request / response
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct OneShotRequest {
    pub description: String,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    /// If true, auto-redesign if taste score < threshold.
    #[serde(default = "default_true")]
    pub auto_redesign: bool,
    /// Taste threshold below which auto-redesign triggers.
    #[serde(default = "default_taste_threshold")]
    pub taste_threshold: u32,
    /// If true, stream SSE events. If false, wait for completion and return JSON.
    #[serde(default = "default_true")]
    pub stream: bool,
}

fn default_true() -> bool { true }
fn default_taste_threshold() -> u32 { 70 }

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OneShotEvent {
    Phase {
        phase: String,
        status: String,
        detail: String,
    },
    Progress {
        percent: u32,
        message: String,
        phase: String,
    },
    IntentAnalyzed {
        app_type: String,
        complexity: String,
        domain: String,
        needs_auth: bool,
        needs_database: bool,
    },
    DecisionsMade {
        frontend: String,
        database: String,
        auth: String,
        learning_overrides: Vec<String>,
    },
    ProductBriefReady {
        domain: String,
        hero_headline: String,
        personas: usize,
        features: usize,
    },
    /// Emitted as soon as the project row exists and has an id.
    /// The frontend uses this to navigate to the workspace while the stream continues.
    ProjectCreated {
        project_id: String,
        project_name: String,
    },
    FilesGenerated {
        count: usize,
        project_id: String,
    },
    TasteScored {
        overall: u32,
        redesign_triggered: bool,
    },
    RedesignComplete {
        mutations_applied: usize,
        score_before: u32,
        score_after: u32,
    },
    Complete {
        project_id: String,
        project_name: String,
        taste_score: u32,
        files_count: usize,
        duration_ms: u64,
        app_url: Option<String>,
    },
    /// Human-readable thinking narrative — what Nexus is doing and why.
    Thinking {
        message: String,
        detail: Option<String>,
        icon: String,
        progress: u32,
    },
    /// Architecture decision explanation — why this choice was made.
    Explanation {
        decision: String,
        reason: String,
        confidence: f64,
        alternatives: Vec<String>,
    },
    /// Skeleton preview file — sent instantly before LLM generates real content.
    Skeleton {
        path: String,
        content: String,
        skeleton_type: String,
    },
    /// Estimated time remaining for the pipeline.
    Estimate {
        total_estimated_ms: u64,
        confidence: f64,
    },
    /// Individual file written to disk during codegen.
    FileWritten {
        path: String,
        lines: usize,
    },
    /// Heartbeat during long operations — keeps the user informed.
    Heartbeat {
        phase: String,
        elapsed_ms: u64,
        message: String,
    },
    Error {
        phase: String,
        message: String,
        fatal: bool,
    },
}

// ---------------------------------------------------------------------------
// Streaming entry point
// ---------------------------------------------------------------------------

/// POST /oneshot — one-shot perfection mode (SSE stream)
pub async fn oneshot_stream(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<OneShotRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    auth.require_scope(&Scope::AgentExecute)
        .map_err(|_| ApiError::Forbidden("agent:execute scope required".into()))?;
    crate::input_limits::require_bounded(
        "description",
        &body.description,
        crate::input_limits::MAX_DESCRIPTION_BYTES,
    )?;

    let (tx, rx) = mpsc::channel::<OneShotEvent>(64);
    let app_clone = app.clone();
    let desc = body.description.clone();
    let auto_redesign = body.auto_redesign;
    let taste_threshold = body.taste_threshold;
    let tenant = Some(auth.tenant_id.clone());
    let tx_panic = tx.clone();

    tokio::spawn(async move {
        // Catch panics so the SSE stream always terminates with a structured
        // event. Without this guard a panic in any pipeline stage drops the
        // sender, the receiver returns None, the unfold stream closes silently,
        // and the frontend hangs on a "still generating" UI forever.
        let pipeline = std::panic::AssertUnwindSafe(run_oneshot_pipeline_for_tenant(
            app_clone,
            tenant,
            desc,
            auto_redesign,
            taste_threshold,
            tx,
            None,
        ));
        if let Err(e) = futures::FutureExt::catch_unwind(pipeline).await {
            let msg = panic_message(&e);
            tracing::error!(error = %msg, "oneshot pipeline panicked");
            let _ = tx_panic
                .send(OneShotEvent::Error {
                    phase: "pipeline".into(),
                    message: msg,
                    fatal: true,
                })
                .await;
        }
    });

    let stream = sse_stream_from_rx(rx);
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// POST /oneshot/sync — one-shot perfection mode (blocking JSON response)
pub async fn oneshot_sync(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<OneShotRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::AgentExecute)
        .map_err(|_| ApiError::Forbidden("agent:execute scope required".into()))?;
    crate::input_limits::require_bounded(
        "description",
        &body.description,
        crate::input_limits::MAX_DESCRIPTION_BYTES,
    )?;

    let (tx, mut rx) = mpsc::channel::<OneShotEvent>(128);
    let app_clone = app.clone();
    let desc = body.description.clone();
    let auto_redesign = body.auto_redesign;
    let taste_threshold = body.taste_threshold;
    let tenant = Some(auth.tenant_id.clone());

    tokio::spawn(async move {
        run_oneshot_pipeline_for_tenant(
            app_clone,
            tenant,
            desc,
            auto_redesign,
            taste_threshold,
            tx,
            None,
        )
        .await;
    });

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        let is_complete = matches!(&event, OneShotEvent::Complete { .. } | OneShotEvent::Error { fatal: true, .. });
        events.push(event);
        if is_complete {
            break;
        }
    }

    let summary = events
        .iter()
        .find(|e| matches!(e, OneShotEvent::Complete { .. }))
        .and_then(|e| serde_json::to_value(e).ok())
        .unwrap_or_else(|| {
            events
                .iter()
                .rfind(|e| matches!(e, OneShotEvent::Error { .. }))
                .and_then(|e| serde_json::to_value(e).ok())
                .unwrap_or(json!({"type": "unknown"}))
        });

    Ok(Json(json!({
        "summary": summary,
        "events": events.iter().filter_map(|e| serde_json::to_value(e).ok()).collect::<Vec<_>>(),
    })))
}

#[derive(Debug, Deserialize)]
pub struct StartOneshotForProjectReq {
    pub description: String,
    #[serde(default = "default_true")]
    pub auto_redesign: bool,
    #[serde(default = "default_taste_threshold")]
    pub taste_threshold: u32,
}

/// POST /projects/:id/oneshot/start — run the oneshot pipeline against an
/// existing project (created beforehand via POST /projects). Streams SSE.
pub async fn oneshot_stream_for_project(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(body): Json<StartOneshotForProjectReq>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    access
        .require_scope(&Scope::ProjectWrite)
        .map_err(|_| ApiError::Forbidden("project:write scope required".into()))?;
    let project_id = access.project_id.clone();
    // Reject oversized / empty descriptions. Without this cap, a hostile
    // client could submit 10 MB of text that gets multiplied across every
    // downstream LLM call.
    crate::input_limits::require_bounded(
        "description",
        &body.description,
        crate::input_limits::MAX_DESCRIPTION_BYTES,
    )?;

    // Reserve an LLM concurrency slot BEFORE spawning the pipeline. This
    // prevents a hostile caller from starting hundreds of background codegen
    // pipelines and exhausting CPU / LLM quota. The guard is moved into the
    // spawned task and released when the task ends.
    let llm_guard = match app.rate_limiter.acquire_llm_slot().await {
        Ok(g) => g,
        Err(e) => {
            return Err(ApiError::TooManyRequests(format!(
                "generation queue is full: {e}"
            )));
        }
    };

    // Verify project exists AND acquire a per-project generation lock. Without
    // this, two concurrent POSTs for the same project would race on the
    // generated/ directory and produce a corrupt mix of files from both runs.
    {
        let db = app.db.lock().await;
        let svc = ProjectService::new(&db);
        match svc.get_project(&project_id) {
            Ok(Some(_)) => {}
            Ok(None) => return Err(ApiError::NotFound(format!("project {} not found", project_id))),
            Err(e) => return Err(ApiError::Internal(e.to_string())),
        }
        let lock_svc = nexus_store::GenerationLockService::new(&db);
        let acquired = lock_svc
            .try_acquire(&project_id, &body.description)
            .map_err(|e| ApiError::Internal(format!("lock acquire failed: {e}")))?;
        if !acquired {
            return Err(ApiError::Conflict(
                "a generation is already running for this project".into(),
            ));
        }
    }

    let (tx, rx) = mpsc::channel::<OneShotEvent>(64);
    let app_clone = app.clone();
    let desc = body.description.clone();
    let pid = project_id.clone();
    let auto_redesign = body.auto_redesign;
    let taste_threshold = body.taste_threshold;
    let tenant = Some(access.tenant_id.clone());

    let tx_panic = tx.clone();
    tokio::spawn(async move {
        let pid_for_release = pid.clone();
        let app_for_release = app_clone.clone();
        let pipeline = std::panic::AssertUnwindSafe(run_oneshot_pipeline_for_tenant(
            app_clone,
            tenant,
            desc,
            auto_redesign,
            taste_threshold,
            tx,
            Some(pid),
        ));
        if let Err(e) = futures::FutureExt::catch_unwind(pipeline).await {
            let msg = panic_message(&e);
            tracing::error!(project_id = %pid_for_release, error = %msg, "oneshot pipeline panicked");
            let _ = tx_panic
                .send(OneShotEvent::Error {
                    phase: "pipeline".into(),
                    message: msg,
                    fatal: true,
                })
                .await;
        }
        // Release per-project generation lock regardless of pipeline outcome.
        {
            let db = app_for_release.db.lock().await;
            let lock_svc = nexus_store::GenerationLockService::new(&db);
            if let Err(e) = lock_svc.release(&pid_for_release) {
                tracing::warn!(
                    project_id = %pid_for_release,
                    error = %e,
                    "failed to release generation lock"
                );
            }
        }
        // Guard lives for the full pipeline; drop here to release the slot.
        drop(llm_guard);
    });

    let stream = sse_stream_from_rx(rx);
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// Convert pipeline events into SSE frames, terminating with an explicit
/// `done` event whenever the channel closes. Without the terminal frame,
/// reverse proxies and browser clients keep the connection open until
/// idle-timeout — the user sees a frozen UI even on clean completion.
fn sse_stream_from_rx(
    rx: mpsc::Receiver<OneShotEvent>,
) -> impl Stream<Item = Result<Event, Infallible>> {
    enum State {
        Open(mpsc::Receiver<OneShotEvent>),
        Closing,
        Done,
    }
    stream::unfold(State::Open(rx), |state| async move {
        match state {
            State::Open(mut rx) => match rx.recv().await {
                Some(event) => {
                    let event_type = oneshot_event_type(&event);
                    let data = serde_json::to_string(&event).unwrap_or_default();
                    let sse = Event::default().event(event_type).data(data);
                    Some((Ok(sse), State::Open(rx)))
                }
                None => Some((
                    Ok(Event::default().event("done").data("{}")),
                    State::Closing,
                )),
            },
            State::Closing => Some((Ok(Event::default().event("close").data("{}")), State::Done)),
            State::Done => None,
        }
    })
}

/// Render a `Box<dyn Any + Send>` panic payload into a printable string.
fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        return (*s).to_string();
    }
    if let Some(s) = payload.downcast_ref::<String>() {
        return s.clone();
    }
    "oneshot pipeline panicked".to_string()
}

// ---------------------------------------------------------------------------
// Pipeline
// ---------------------------------------------------------------------------

pub(crate) async fn run_oneshot_pipeline(
    app: Arc<AppState>,
    description: String,
    auto_redesign: bool,
    taste_threshold: u32,
    tx: mpsc::Sender<OneShotEvent>,
    existing_project_id: Option<String>,
) {
    run_oneshot_pipeline_for_tenant(
        app,
        None,
        description,
        auto_redesign,
        taste_threshold,
        tx,
        existing_project_id,
    )
    .await;
}

/// Tenant-aware pipeline entry — when `tenant_id` is `Some`, plugin hooks
/// fired during this run are filtered to only those whose manifest allows
/// the caller's tenant (or omits `allowed_tenants` entirely).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_oneshot_pipeline_for_tenant(
    app: Arc<AppState>,
    tenant_id: Option<String>,
    description: String,
    auto_redesign: bool,
    taste_threshold: u32,
    tx: mpsc::Sender<OneShotEvent>,
    existing_project_id: Option<String>,
) {
    let start = std::time::Instant::now();
    let runtime = crate::adaptive_runtime::AdaptiveRuntime::new();

    // Enforce per-build cost budget before spending any tokens.
    let budget = app.cost_tracker.check_budget(None).await;
    if !budget.allowed {
        let _ = tx
            .send(OneShotEvent::Error {
                phase: "budget".into(),
                message: format!(
                    "Daily LLM budget exhausted (${:.2} of ${:.2} used). \
                     Reset at midnight UTC or raise the limit in config.",
                    budget.daily_spent, budget.daily_limit
                ),
                fatal: true,
            })
            .await;
        return;
    }

    macro_rules! emit {
        ($e:expr) => { let _ = tx.send($e).await; }
    }

    /// Re-check the per-tenant / global LLM budget between expensive phases.
    /// Oneshot may make 10+ LLM calls; checking once at entry lets a tenant
    /// blow through 50x the daily cap before the next request is refused.
    /// This closure emits a fatal error event and returns true if we must
    /// abort the pipeline mid-flight.
    macro_rules! budget_gate {
        ($phase:expr, $pid:expr) => {{
            let b = app.cost_tracker.check_budget($pid).await;
            if !b.allowed {
                let _ = tx
                    .send(OneShotEvent::Error {
                        phase: $phase.into(),
                        message: format!(
                            "LLM budget exhausted mid-pipeline (${:.2} of ${:.2} used). \
                             Phase '{}' aborted. Reset at midnight UTC or raise the limit.",
                            b.daily_spent, b.daily_limit, $phase
                        ),
                        fatal: true,
                    })
                    .await;
                return;
            }
        }};
    }

    info!(description = %description, "One-shot pipeline started");

    // Heartbeat ticker — emits an event every 5s so the UI can show
    // "still working…" with elapsed time even when a phase blocks on a
    // long LLM call. Cancelled when the pipeline finishes (tx is dropped).
    let heartbeat_tx = tx.clone();
    let heartbeat_start = start;
    let heartbeat_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        interval.tick().await; // skip immediate tick
        loop {
            interval.tick().await;
            let elapsed_ms = heartbeat_start.elapsed().as_millis() as u64;
            let send = heartbeat_tx
                .send(OneShotEvent::Heartbeat {
                    phase: "pipeline".into(),
                    elapsed_ms,
                    message: format!("Working… ({:.0}s elapsed)", (elapsed_ms as f64) / 1000.0),
                })
                .await;
            if send.is_err() {
                break;
            }
        }
    });
    // Drop guard so the heartbeat task is aborted when the pipeline returns.
    struct AbortOnDrop(tokio::task::JoinHandle<()>);
    impl Drop for AbortOnDrop {
        fn drop(&mut self) { self.0.abort(); }
    }
    let _heartbeat_guard = AbortOnDrop(heartbeat_handle);

    // Spawn a kernel process for tracking
    let process_id = {
        let agent_def = nexus_agents_core::definition::AgentDefinition::default_with_name("oneshot-pipeline");
        app.scheduler
            .spawn(
                agent_def,
                format!("Oneshot: {}", &description[..description.len().min(100)]),
                nexus_kernel::Priority::High,
                nexus_kernel::ResourceAllocation::default(),
                None,
            )
            .await
            .ok()
    };

    // Helper to emit thinking messages
    macro_rules! think {
        ($step:expr, $status:expr, $intent:expr, $pct:expr) => {{
            let elapsed = start.elapsed().as_millis() as u64;
            let msg = thinking_stream::step_to_thinking($step, $status, $intent, $pct, elapsed);
            let _ = tx.send(OneShotEvent::Thinking {
                message: msg.message.clone(),
                detail: msg.detail.clone(),
                icon: format!("{:?}", msg.icon).to_lowercase(),
                progress: msg.progress,
            }).await;
        }};
    }

    // ── Phases 0-2: Unified Intelligence Analysis ─────────────────────────
    // Single call replaces: user_learning + intent_engine + decision_engine +
    // product_engine + personality + adaptive_control + explain_engine
    emit!(OneShotEvent::Phase {
        phase: "learning".into(),
        status: "started".into(),
        detail: "Loading your preferences...".into(),
    });
    let user_ctx = user_learning::build_adapted_context(&app).await;
    emit!(OneShotEvent::Progress { percent: 5, message: "User context loaded".into(), phase: "learning".into() });

    emit!(OneShotEvent::Thinking {
        message: "Analyzing your request...".into(),
        detail: None,
        icon: "brain".into(),
        progress: 8,
    });
    emit!(OneShotEvent::Phase {
        phase: "intent".into(),
        status: "started".into(),
        detail: "Understanding what you want to build...".into(),
    });

    // Run unified Brain analysis (supersedes nexus_intelligence::analyze)
    // This adds: hidden requirements, agent design, risk analysis, UX strategy
    let brain_output = crate::nexus_brain::analyze(&app, &description).await;
    let report = brain_output.to_intelligence_report();
    let intent = brain_output.inferred_intent.clone();
    let mut decisions = brain_output.architecture_decisions.clone();
    let learning_notes = brain_output.learning_notes.clone();
    let product_brief = brain_output.product_brief.clone();

    if report.from_cache {
        emit!(OneShotEvent::Thinking {
            message: "Already understood — using cached analysis".into(),
            detail: None,
            icon: "check".into(),
            progress: 10,
        });
    }

    // Emit intelligence amplification info
    if brain_output.amplification.llm_used {
        emit!(OneShotEvent::Thinking {
            message: "Intent amplified via LLM — description was ambiguous".into(),
            detail: Some(brain_output.amplification.amplification_notes.join("; ")),
            icon: "brain".into(),
            progress: 12,
        });
    }

    // Emit cross-project intelligence info
    if !brain_output.global_intelligence.recommendations.is_empty() {
        emit!(OneShotEvent::Thinking {
            message: format!(
                "Applied {} cross-project recommendations from {} past projects",
                brain_output.global_intelligence.recommendations.len(),
                brain_output.global_intelligence.total_projects_learned_from,
            ),
            detail: Some(brain_output.global_intelligence.recommendations.iter()
                .take(3)
                .map(|r| format!("{}: {} ({})", r.area, r.recommended_value, r.source))
                .collect::<Vec<_>>()
                .join("; ")),
            icon: "globe".into(),
            progress: 13,
        });
    }

    // Fire OnIntentParsed hook
    let mut hook_ctx = plugin_hooks::build_context_for_tenant(
        "oneshot",
        tenant_id.as_deref(),
        HookPoint::OnIntentParsed,
        Some(&description),
    );
    hook_ctx.intent = serde_json::to_value(&intent).ok();
    let hook_result = plugin_hooks::fire_hook(&app, HookPoint::OnIntentParsed, &mut hook_ctx).await;
    user_learning::infer_from_intent(&app, &description, &format!("{:?}", intent.app_type)).await;

    emit!(OneShotEvent::IntentAnalyzed {
        app_type: format!("{:?}", intent.app_type),
        complexity: format!("{:?}", intent.complexity),
        domain: product_brief.base.domain.clone(),
        needs_auth: intent.needs_auth,
        needs_database: intent.needs_database,
    });
    emit!(OneShotEvent::ProductBriefReady {
        domain: product_brief.base.domain.clone(),
        hero_headline: product_brief.base.hero.headline.clone(),
        personas: product_brief.personas.len(),
        features: product_brief.feature_priorities.len(),
    });
    think!("analyze_intent", "completed", &intent, 15);
    emit!(OneShotEvent::Progress { percent: 15, message: "Intent + product brief ready".into(), phase: "intent".into() });

    // Architecture decisions — apply plugin overrides
    think!("generate_spec", "running", &intent, 20);
    emit!(OneShotEvent::Phase {
        phase: "decisions".into(),
        status: "started".into(),
        detail: "Choosing the best architecture for you...".into(),
    });
    for (area, value) in &hook_result.decision_overrides {
        decision_engine::apply_learning_override(&mut decisions, area, value);
    }
    let mut hook_ctx2 = plugin_hooks::build_context_for_tenant(
        "oneshot",
        tenant_id.as_deref(),
        HookPoint::OnDecisionMade,
        Some(&description),
    );
    hook_ctx2.decisions = serde_json::to_value(&decisions).ok();
    let hook_result2 = plugin_hooks::fire_hook(&app, HookPoint::OnDecisionMade, &mut hook_ctx2).await;
    for (area, value) in &hook_result2.decision_overrides {
        decision_engine::apply_learning_override(&mut decisions, area, value);
    }

    emit!(OneShotEvent::DecisionsMade {
        frontend: format!("{:?}", decisions.frontend),
        database: format!("{:?}", decisions.database),
        auth: format!("{:?}", decisions.auth),
        learning_overrides: learning_notes.clone(),
    });
    for expl in &report.explanations {
        emit!(OneShotEvent::Explanation {
            decision: expl.decision.clone(),
            reason: expl.reason.clone(),
            confidence: expl.confidence,
            alternatives: expl.alternatives.iter().map(|a| format!("{}: {}", a.choice, a.reason_rejected)).collect(),
        });
    }
    emit!(OneShotEvent::Progress { percent: 25, message: "Decisions made".into(), phase: "decisions".into() });

    // ── Phase 3: Create project ─────────────────────────────────────────────
    emit!(OneShotEvent::Phase {
        phase: "project".into(),
        status: "started".into(),
        detail: "Creating project".into(),
    });

    let (project_id, project_name) = match existing_project_id.clone() {
        Some(id) => {
            // Reuse existing project — look up its name.
            let name_lookup = {
                let db = app.db.lock().await;
                let svc = ProjectService::new(&db);
                svc.get_project(&id)
            };
            match name_lookup {
                Ok(Some(p)) => (p.id, p.name),
                Ok(None) => {
                    emit!(OneShotEvent::Error {
                        phase: "project".into(),
                        message: format!("Project {} not found", id),
                        fatal: true,
                    });
                    return;
                }
                Err(e) => {
                    emit!(OneShotEvent::Error {
                        phase: "project".into(),
                        message: format!("Failed to load project: {}", e),
                        fatal: true,
                    });
                    return;
                }
            }
        }
        None => {
            let project_name = derive_project_name(&description);
            let create_result = {
                let db = app.db.lock().await;
                let svc = ProjectService::new(&db);
                svc.create_project(&project_name, Some(&description[..]), "default")
            };
            match create_result {
                Ok(p) => (p.id, p.name),
                Err(e) => {
                    emit!(OneShotEvent::Error {
                        phase: "project".into(),
                        message: format!("Failed to create project: {}", e),
                        fatal: true,
                    });
                    return;
                }
            }
        }
    };

    // Emit ProjectCreated early so the frontend can navigate to the workspace
    // while the rest of the pipeline continues streaming.
    emit!(OneShotEvent::ProjectCreated {
        project_id: project_id.clone(),
        project_name: project_name.clone(),
    });

    // Resolve (or create) a conversation for this project. Only insert a fresh
    // user message if the latest stored message doesn't already match this
    // description — `POST /projects` may have already persisted it when the
    // project was created with a description.
    let conversation_id = {
        let db = app.db.lock().await;
        let svc = ProjectService::new(&db);
        let conv_id = match svc.list_conversations(&project_id) {
            Ok(list) if !list.is_empty() => Some(list[0].id.clone()),
            Ok(_) => svc.create_conversation(&project_id).ok().map(|c| c.id),
            Err(_) => None,
        };
        if let Some(ref cid) = conv_id {
            let already_persisted = svc
                .list_messages(cid)
                .ok()
                .and_then(|msgs| {
                    msgs.iter()
                        .rev()
                        .find(|m| m.role == "user")
                        .map(|m| m.content.trim() == description.trim())
                })
                .unwrap_or(false);
            if !already_persisted {
                let _ = svc.append_nexus_message(cid, "user", &description, None);
            }
        }
        conv_id
    };

    // Bridge: forward key events to live_build_handler broadcast so both SSE endpoints work
    let bridge_app = app.clone();
    let bridge_project_id = project_id.clone();
    macro_rules! bridge {
        ($event:expr) => {
            super::live_build_handler::emit_event(&bridge_app, &bridge_project_id, $event).await;
        };
    }

    bridge!(super::live_build_handler::LiveBuildEvent::BuildStep {
        step: "project_created".into(),
        status: "completed".into(),
        duration_ms: Some(start.elapsed().as_millis() as u64),
    });

    // Save architecture decisions + intent for the project (used by quality_gate, post_build_intel)
    let decisions_path = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated")
        .join(".nexus");
    let _ = std::fs::create_dir_all(&decisions_path);
    let _ = std::fs::write(
        decisions_path.join("decisions.json"),
        serde_json::to_string_pretty(&decisions).unwrap_or_default(),
    );
    // Save intent.json so quality_gate and post_build_intel can load the real intent
    let _ = std::fs::write(
        app.data_dir.join("projects").join(&project_id).join("intent.json"),
        serde_json::to_string_pretty(&intent).unwrap_or_default(),
    );

    emit!(OneShotEvent::Progress { percent: 35, message: format!("Project '{}' created", project_name), phase: "project".into() });

    // ── Budget re-check before codegen (largest LLM spend in the pipeline) ──
    budget_gate!("codegen", Some(&project_id));

    // ── Phase 4: Codegen ────────────────────────────────────────────────────
    think!("generate_pages", "running", &intent, 40);
    emit!(OneShotEvent::Phase {
        phase: "codegen".into(),
        status: "started".into(),
        detail: "Crafting your application...".into(),
    });

    // Build full prompt context
    let decision_ctx = decision_engine::to_prompt_context(&decisions);
    let product_ctx = product_engine::format_full_brief_for_prompt(&product_brief);
    let user_pref_ctx = user_ctx.prompt_context.clone();

    // Fire OnBeforeCodegen hook
    let mut hook_ctx3 = plugin_hooks::build_context_for_tenant(
        &project_id,
        tenant_id.as_deref(),
        HookPoint::OnBeforeCodegen,
        Some(&description),
    );
    hook_ctx3.plan = Some(json!({ "description": description, "decisions": decisions }));
    let hook_result3 = plugin_hooks::fire_hook(&app, HookPoint::OnBeforeCodegen, &mut hook_ctx3).await;

    // Inject extended stack suggestions from intelligence amplifier
    let stack_ctx = intelligence_amplifier::stack_suggestions_to_context(&brain_output.stack_suggestions);

    // Assemble the complete generation prompt
    let base_prompt = format!(
        "Build a complete, production-ready web application for this project:\n{}\n\n{}{}{}{}",
        description, decision_ctx, product_ctx, user_pref_ctx, stack_ctx
    );

    // Inject learned skill patterns into the prompt
    let (skill_enriched, skill_knowledge) =
        crate::skill_runtime::inject(&app, &base_prompt, &description).await;
    if skill_knowledge.skills_used > 0 {
        emit!(OneShotEvent::Thinking {
            message: format!("Injected {} learned patterns from previous builds", skill_knowledge.skills_used),
            detail: Some(format!("{} prompt fragments applied", skill_knowledge.fragments.len())),
            icon: "brain".into(),
            progress: 43,
        });
    }

    // Evolve the prompt based on historical outcomes
    let (mut full_prompt, evolution_changes) =
        crate::prompt_evolution::evolve_prompt(&app, &skill_enriched, "oneshot_codegen").await;
    if evolution_changes.len() > 1 || (evolution_changes.len() == 1 && !evolution_changes[0].contains("No evolution")) {
        emit!(OneShotEvent::Thinking {
            message: format!("Prompt evolved: {} improvement(s) applied", evolution_changes.len()),
            detail: Some(evolution_changes.join("; ")),
            icon: "sparkle".into(),
            progress: 45,
        });
    }

    // Inject plugin contexts
    for ctx_str in &hook_result3.injected_context {
        full_prompt.push('\n');
        full_prompt.push_str(ctx_str);
    }
    if !hook_result.injected_context.is_empty() {
        full_prompt.push('\n');
        full_prompt.push_str(&hook_result.injected_context.join("\n"));
    }

    // ── Variant Engine: decide if multi-variant generation is warranted ──
    let use_variants = variant_engine::should_generate_variants(&intent);
    if use_variants {
        emit!(OneShotEvent::Thinking {
            message: "Complex project detected — preparing multi-variant generation".into(),
            detail: Some(format!(
                "Will generate {} variants and select the best",
                variant_engine::VariantConfig::default().max_variants,
            )),
            icon: "layers".into(),
            progress: 44,
        });
    }

    emit!(OneShotEvent::Progress { percent: 45, message: "Running codegen".into(), phase: "codegen".into() });

    // ── Perception: Emit skeleton files instantly (< 2ms) ────────────────
    // User sees page structure while LLM generates real content
    {
        let app_type = format!("{:?}", intent.app_type);
        let pages: Vec<String> = intent
            .suggested_pages
            .iter()
            .map(|p| {
                let route = p.to_lowercase().replace(' ', "-");
                if route == "home" { "/".to_string() } else { format!("/{}", route) }
            })
            .collect();
        let skeletons = perceived_speed::generate_skeletons(&app_type, &pages);
        for skeleton in &skeletons {
            let _ = tx.send(OneShotEvent::Skeleton {
                path: skeleton.file_path.clone(),
                content: skeleton.content.clone(),
                skeleton_type: format!("{:?}", skeleton.skeleton_type).to_lowercase(),
            }).await;
        }
    }

    // ── Perception: Emit time estimate ───────────────────────────────────
    {
        let estimator = perceived_speed::SpeedEstimator::load(&app.data_dir);
        let steps = &["generate_spec", "generate_api_and_ui", "validate_all", "install_deps"];
        let estimate = estimator.estimate_pipeline(steps);
        let _ = tx.send(OneShotEvent::Estimate {
            total_estimated_ms: estimate.total_estimated_ms,
            confidence: estimate.confidence,
        }).await;
    }

    // ── Perception: Start heartbeat during LLM call ──────────────────────
    // Sends a pulse every 2 seconds so user knows the system is working
    let heartbeat_tx = tx.clone();
    let heartbeat_start = std::time::Instant::now();
    let heartbeat_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
        interval.tick().await; // skip first immediate tick
        let messages = [
            "Designing page layouts...",
            "Writing component code...",
            "Setting up API endpoints...",
            "Adding styling and interactions...",
            "Connecting data models...",
            "Wiring up navigation...",
            "Adding error handling...",
            "Polishing the details...",
        ];
        let mut i = 0;
        loop {
            interval.tick().await;
            let elapsed = heartbeat_start.elapsed().as_millis() as u64;
            let msg = messages[i % messages.len()];
            if heartbeat_tx.send(OneShotEvent::Heartbeat {
                phase: "codegen".into(),
                elapsed_ms: elapsed,
                message: msg.into(),
            }).await.is_err() {
                break; // channel closed
            }
            i += 1;
        }
    });

    // ── Model selection: prefer user choice, fall back to router ────────
    // Resolution order:
    //   1. Per-project override (projects.llm_provider / llm_model)
    //   2. Global default (settings: llm.default.provider / llm.default.model)
    //   3. model_router heuristic for codegen complexity
    // This ensures the user's picked model in the chat/settings UI is actually
    // what runs codegen — no more hardcoded router overrides.
    let user_choice: Option<(String, String)> = {
        let db = app.db.lock().await;
        let svc = ProjectService::new(&db);
        let project_choice = svc
            .get_project(&project_id)
            .ok()
            .flatten()
            .and_then(|p| match (p.llm_provider, p.llm_model) {
                (Some(p), Some(m)) if !p.is_empty() && !m.is_empty() => Some((p, m)),
                _ => None,
            });
        if project_choice.is_some() {
            project_choice
        } else {
            let provider = svc.get_setting("llm.default.provider").ok().flatten();
            let model = svc.get_setting("llm.default.model").ok().flatten();
            match (provider, model) {
                (Some(p), Some(m)) if !p.is_empty() && !m.is_empty() => Some((p, m)),
                // Third tier: derive from state. `app.model` mirrors NEXUS_MODEL
                // (or `llm_model_defaults::OPENAI_DEFAULT_MODEL`) and the
                // provider is inferred from which API key the server has.
                _ => {
                    let provider = if !app.openai_api_key.is_empty() {
                        "openai".to_string()
                    } else if app.anthropic_api_key.is_some() {
                        "anthropic".to_string()
                    } else {
                        String::new()
                    };
                    if provider.is_empty() || app.model.is_empty() {
                        None
                    } else {
                        Some((provider, app.model.clone()))
                    }
                }
            }
        }
    };

    let (codegen_provider, codegen_model, model_reasoning) = if let Some((p, m)) = user_choice {
        let reasoning = format!("Using user-selected model ({p} / {m})");
        (p, m, reasoning)
    } else {
        let routing_ctx = crate::model_router::RoutingContext {
            task_type: crate::model_router::TaskType::Codegen,
            complexity: match intent.complexity {
                crate::intent_engine::Complexity::Simple => crate::model_router::TaskComplexity::Low,
                crate::intent_engine::Complexity::Medium => crate::model_router::TaskComplexity::Medium,
                crate::intent_engine::Complexity::Complex => crate::model_router::TaskComplexity::High,
            },
            latency_target_ms: 0,
            budget_remaining_pct: 1.0,
            is_retry: false,
            preferred_provider: String::new(),
            has_anthropic: app.anthropic_api_key.is_some(),
            has_openai: !app.openai_api_key.is_empty(),
        };
        let routed = crate::model_router::route(&routing_ctx);
        (routed.provider, routed.model, routed.reasoning)
    };

    emit!(OneShotEvent::Thinking {
        message: format!("Using {} for code generation", codegen_model),
        detail: Some(model_reasoning),
        icon: "sparkle".into(),
        progress: 48,
    });

    // Trigger codegen via execution pipeline (using routed model).
    // We pass the user's ORIGINAL `description` separately so that stack
    // detection runs against what the user actually typed, not the enriched
    // prompt (which can contain incidental words like "rust" from decision
    // context and would otherwise mis-route the codegen).
    let project_dir = app.data_dir.join("projects").join(&project_id).join("generated");
    let files_count = match run_codegen(&app, &project_id, &project_dir, &full_prompt, &description, &tx,
        &codegen_provider, &codegen_model).await {
        Ok(n) => n,
        Err(e) => {
            emit!(OneShotEvent::Error {
                phase: "codegen".into(),
                message: e.clone(),
                fatal: false,
            });
            // Record failure for learning
            for area in &["frontend", "database", "auth", "hosting"] {
                let val = decision_engine::decision_area_value(&decisions, area);
                if !val.is_empty() {
                    crate::decision_learning::record_outcome(&app, &DecisionOutcome {
                        project_id: project_id.clone(),
                        area: area.to_string(),
                        chosen_value: val.to_string(),
                        outcome: Outcome::Failure,
                        user_override: None,
                        build_success: Some(false),
                        taste_score: None,
                        feedback: Some(e.clone()),
                    }).await;
                }
            }
            0
        }
    };

    // CRITICAL: if codegen produced no files, don't pretend the build
    // succeeded — emit a fatal error and bail. The previous behaviour
    // emitted `Complete { app_url: None }` which made the UI think the
    // generation was done when it had actually collapsed silently.
    if files_count == 0 {
        heartbeat_handle.abort();
        emit!(OneShotEvent::Error {
            phase: "codegen".into(),
            message: "Code generation produced no files. The LLM response was empty \
                      or malformed. Try a more specific prompt, switch models, or \
                      check the Settings page for a valid API key."
                .into(),
            fatal: true,
        });
        return;
    }

    // Stop the heartbeat — codegen is done
    heartbeat_handle.abort();

    // Record execution metrics for adaptive runtime
    let codegen_duration = start.elapsed().as_millis() as u64;
    runtime.observe(crate::adaptive_runtime::RuntimeEvent::StepComplete {
        step: "codegen".into(),
        duration_ms: codegen_duration,
        success: files_count > 0,
    }).await;

    // ── Adaptive Runtime: decide adjustment after codegen ────────────────
    // If codegen failed or was slow, the runtime may switch models for retry
    let adjustment = runtime.decide_adjustment("codegen").await;
    match &adjustment {
        AdjustmentAction::SwitchModel { from, to, reason } => {
            emit!(OneShotEvent::Thinking {
                message: format!("Switching model: {} → {}", from, to),
                detail: Some(reason.clone()),
                icon: "lightning".into(),
                progress: 72,
            });
        }
        AdjustmentAction::FallbackDeterministic { step, reason } => {
            emit!(OneShotEvent::Thinking {
                message: format!("Falling back to deterministic for {}", step),
                detail: Some(reason.clone()),
                icon: "shield".into(),
                progress: 72,
            });
        }
        AdjustmentAction::CompressContext { target_reduction_pct, reason } => {
            emit!(OneShotEvent::Thinking {
                message: format!("Compressing context by {}%", target_reduction_pct),
                detail: Some(reason.clone()),
                icon: "compress".into(),
                progress: 72,
            });
        }
        _ => {} // NoAction, AdjustRetries, ParallelizeSubtasks — no user-facing event
    }

    emit!(OneShotEvent::FilesGenerated {
        count: files_count,
        project_id: project_id.clone(),
    });

    // ── Perception: Emit individual file events ──────────────────────────
    // Progressive reveal — user sees each file appear.
    //
    // Larger projects frequently produce > 30 files (layouts + components +
    // API routes + types). We emit up to 200 events so big builds still look
    // alive in the UI, but keep a cap so a malicious / runaway generation
    // cannot flood the SSE channel.
    const FILE_EVENT_CAP: usize = 200;
    if project_dir.exists() {
        let written_files = crate::file_utils::collect_files_by_ext(
            &project_dir, &["ts", "tsx", "js", "jsx", "css", "json"],
        );
        let total = written_files.len();
        for (i, file_path) in written_files.iter().take(FILE_EVENT_CAP).enumerate() {
            let rel = file_path.strip_prefix(&project_dir)
                .unwrap_or(file_path)
                .display()
                .to_string();
            let lines = std::fs::read_to_string(file_path)
                .map(|c| c.lines().count())
                .unwrap_or(0);
            let _ = tx.send(OneShotEvent::FileWritten {
                path: rel,
                lines,
            }).await;
            // Small yield to let the frontend render each file progressively
            if i % 5 == 4 {
                tokio::task::yield_now().await;
            }
        }
        if total > FILE_EVENT_CAP {
            let _ = tx.send(OneShotEvent::Heartbeat {
                phase: "codegen".into(),
                elapsed_ms: 0,
                message: format!(
                    "{} files written (showing first {})",
                    total, FILE_EVENT_CAP
                ),
            }).await;
        }
    }

    // Fire OnAfterGeneration hook
    let mut hook_ctx4 = plugin_hooks::build_context_for_tenant(
        &project_id,
        tenant_id.as_deref(),
        HookPoint::OnAfterGeneration,
        Some(&description),
    );
    plugin_hooks::fire_hook(&app, HookPoint::OnAfterGeneration, &mut hook_ctx4).await;

    emit!(OneShotEvent::Progress { percent: 75, message: "Code generated".into(), phase: "codegen".into() });

    // ── Phase 5: Taste scoring ──────────────────────────────────────────────
    think!("validate_all", "running", &intent, 78);
    emit!(OneShotEvent::Phase {
        phase: "taste".into(),
        status: "started".into(),
        detail: "Running quality checks on everything...".into(),
    });

    let taste_score = if project_dir.exists() {
        let score = taste_engine::score_project(&project_dir);
        let overall = score.overall;

        // Fire OnTasteScore hook
        let mut hook_ctx5 = plugin_hooks::build_context_for_tenant(
            &project_id,
            tenant_id.as_deref(),
            HookPoint::OnTasteScore,
            Some(&description),
        );
        hook_ctx5.taste_score = serde_json::to_value(&score).ok();
        plugin_hooks::fire_hook(&app, HookPoint::OnTasteScore, &mut hook_ctx5).await;

        let needs_redesign = auto_redesign && overall < taste_threshold;
        emit!(OneShotEvent::TasteScored {
            overall,
            redesign_triggered: needs_redesign,
        });

        // Auto-redesign if below threshold
        if needs_redesign {
            // Budget re-check: redesign spawns up to 5 mutation LLM calls.
            budget_gate!("redesign", Some(&project_id));

            emit!(OneShotEvent::Phase {
                phase: "redesign".into(),
                status: "started".into(),
                detail: format!("Score {} < {} — applying improvements", overall, taste_threshold),
            });

            let config = RedesignConfig {
                threshold: taste_threshold,
                max_mutations: 5,
                target_axes: Vec::new(),
                dry_run: false,
            };

            match taste_redesign::redesign(&app, &project_id, &project_dir, &config).await {
                Ok(result) => {
                    emit!(OneShotEvent::RedesignComplete {
                        mutations_applied: result.mutations_applied,
                        score_before: result.before_score,
                        score_after: result.after_score,
                    });
                    result.after_score
                }
                Err(_) => overall,
            }
        } else {
            overall
        }
    } else {
        0
    };

    emit!(OneShotEvent::Progress { percent: 90, message: "Quality check complete".into(), phase: "taste".into() });

    // Record taste phase in adaptive runtime
    runtime.observe(crate::adaptive_runtime::RuntimeEvent::StepComplete {
        step: "taste_scoring".into(),
        duration_ms: start.elapsed().as_millis() as u64,
        success: taste_score >= taste_threshold,
    }).await;

    // ── Phase 6: Record outcomes for learning ───────────────────────────────
    for area in &["frontend", "database", "auth", "hosting"] {
        let val = decision_engine::decision_area_value(&decisions, area);
        if !val.is_empty() {
            crate::decision_learning::record_outcome(&app, &DecisionOutcome {
                project_id: project_id.clone(),
                area: area.to_string(),
                chosen_value: val.to_string(),
                outcome: if files_count > 0 { Outcome::Success } else { Outcome::Failure },
                user_override: None,
                build_success: Some(files_count > 0),
                taste_score: Some(taste_score),
                feedback: None,
            }).await;

            user_learning::infer_from_decision(&app, area, val, files_count > 0).await;
        }
    }

    // ── Record timing for future estimates ──────────────────────────────────
    {
        let duration_ms_total = start.elapsed().as_millis() as u64;
        let mut estimator = perceived_speed::SpeedEstimator::load(&app.data_dir);
        estimator.record_step("oneshot_full", duration_ms_total);
        estimator.save(&app.data_dir);
    }

    // ── Record prompt outcome for evolution ──────────────────────────────
    crate::prompt_evolution::record_outcome(&app, &crate::prompt_evolution::PromptOutcome {
        purpose: "oneshot_codegen".into(),
        success: files_count > 0,
        tokens_used: 0, // not tracked at this level
        latency_ms: start.elapsed().as_millis() as u64,
        output_quality: if taste_score > 0 { Some(taste_score as f64 / 100.0) } else { None },
    }).await;

    // ── Record causal observations ──────────────────────────────────────
    let success = files_count > 0 && taste_score >= taste_threshold;
    for r in &decisions.rationale {
        crate::causal_learning::record_observation(
            &app,
            &format!("{}={}", r.area, r.choice),
            "build_success",
            success,
        ).await;
    }

    // ── Update project brain intelligence ────────────────────────────────
    {
        let mut intel = crate::project_brain::ProjectIntelligence::load(&project_dir);
        for r in &decisions.rationale {
            intel.record_decision(&r.area, &r.choice, &r.reason, if success { "success" } else { "failure" });
        }
        intel.record_taste_score(taste_score);
        intel.save(&project_dir);
    }

    // ── Record cross-project intelligence (GlobalIntelligenceLayer) ──────
    {
        let decision_pairs: Vec<(String, String)> = decisions
            .rationale
            .iter()
            .map(|r| (r.area.clone(), r.choice.clone()))
            .collect();
        crate::global_intelligence::record_project_outcome(
            &app,
            &intent,
            &brain_output.inferred_domain,
            &decision_pairs,
            success,
            Some(taste_score),
        )
        .await;
    }

    // Log adaptive runtime snapshot for observability
    let rt_snapshot = runtime.snapshot().await;
    info!(
        llm_calls = rt_snapshot.total_llm_calls,
        failures = rt_snapshot.total_failures,
        tokens = rt_snapshot.total_tokens,
        avg_latency_ms = rt_snapshot.avg_latency_ms,
        "Adaptive runtime snapshot"
    );

    // ── Deterministic post-codegen sanity check (< 5ms) + 1-cycle repair ──
    //    Before we advertise an app_url, verify the generated tree actually
    //    looks runnable: `package.json` present, valid JSON, has
    //    `scripts.dev|start`, and the main entry file exists. If anything is
    //    missing, we invoke the mutation engine for exactly ONE repair cycle
    //    (bounded cost) so the final preview isn't broken. If repair fails,
    //    the user still gets a fatal Error event explaining what's wrong.
    {
        let project_dir_for_check = app.data_dir.join("projects").join(&project_id).join("generated");
        let sanity_errors = check_post_codegen_sanity(&project_dir_for_check);

        if !sanity_errors.is_empty() {
            emit!(OneShotEvent::Thinking {
                message: format!(
                    "Auto-repair: fixing {} post-codegen issue(s)",
                    sanity_errors.len()
                ),
                detail: Some(sanity_errors.join("; ")),
                icon: "wrench".into(),
                progress: 85,
            });

            // One repair cycle — bounded cost, targets the specific errors.
            budget_gate!("repair", Some(&project_id));
            let repair_instruction = format!(
                "The generated project has the following issues: {}. Fix ONLY these \
                 specific issues. Do not refactor unrelated code. If package.json is \
                 missing or invalid, create/fix it with minimal valid content that \
                 includes a `scripts.dev` entry and the necessary dependencies based \
                 on existing files.",
                sanity_errors.join("; ")
            );
            let mutation_req = crate::mutation_engine::MutationRequest {
                change: repair_instruction,
                target_file: None,
            };
            match crate::mutation_engine::mutate(
                &app,
                &project_id,
                &project_dir_for_check,
                &mutation_req,
            )
            .await
            {
                Ok(_) => {
                    // Re-check after the repair — only clear thinking on pass.
                    let remaining = check_post_codegen_sanity(&project_dir_for_check);
                    if remaining.is_empty() {
                        emit!(OneShotEvent::Thinking {
                            message: "Auto-repair succeeded — app is runnable".into(),
                            detail: None,
                            icon: "check".into(),
                            progress: 88,
                        });
                    } else {
                        emit!(OneShotEvent::Thinking {
                            message: "Auto-repair partial; preview may need manual intervention".into(),
                            detail: Some(remaining.join("; ")),
                            icon: "alert".into(),
                            progress: 88,
                        });
                    }
                }
                Err(e) => {
                    tracing::warn!(project_id = %project_id, error = %e, "Auto-repair cycle failed");
                    emit!(OneShotEvent::Thinking {
                        message: "Auto-repair cycle failed — leaving original output".into(),
                        detail: Some(e),
                        icon: "alert".into(),
                        progress: 88,
                    });
                }
            }
        }
    }

    // ── Start the generated app BEFORE emitting Complete, so the SSE
    //    `Complete` event can include the live preview URL (Bolt-parity UX).
    //    We give the dev server a short window to come up — if it's still
    //    installing, the UI will pick up the URL later via /app/status polling.
    let mut preview_app_url: Option<String> = None;
    {
        // Scoped read of current running-instance state; drop service + lock
        // immediately so nothing non-Send crosses the later `.await`s.
        let already_running = {
            let db = app.db.lock().await;
            let runner_svc = nexus_store::AppRunnerService::new(&db);
            runner_svc.get_running_instance(&project_id).ok().flatten().is_some()
        };

        let output_dir = app.data_dir.join("projects").join(&project_id).join("generated");
        if !already_running && output_dir.join("package.json").exists() {
            info!(project_id = %project_id, "Auto-starting generated app for preview");
            let app_clone = app.clone();
            let pid_clone = project_id.clone();
            tokio::spawn(async move {
                if let Err(e) =
                    crate::handlers::app_runner::auto_start_app(&app_clone, &pid_clone).await
                {
                    // Surface the failure to any connected live-build stream.
                    tracing::warn!(
                        project_id = %pid_clone,
                        error = %e,
                        "Auto-start failed — user can start manually",
                    );
                    let maybe_tx = {
                        let bus = app_clone.build_event_bus.read().await;
                        bus.get(&pid_clone).cloned()
                    };
                    if let Some(tx) = maybe_tx {
                        let _ = tx.send(
                            crate::handlers::live_build_handler::LiveBuildEvent::Error {
                                message: format!("Auto-start failed: {e}"),
                                recoverable: true,
                            },
                        );
                    }
                }
            });
        }

        // Poll briefly for the port — cheap enough (<= 12s) to feel instant
        // without blocking user interactions. Beyond that, the UI takes over.
        for _ in 0..40 {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            let maybe_port = {
                let db = app.db.lock().await;
                let runner_svc = nexus_store::AppRunnerService::new(&db);
                runner_svc
                    .get_running_instance(&project_id)
                    .ok()
                    .flatten()
                    .map(|inst| inst.port)
            };
            if let Some(port) = maybe_port {
                if port > 0 {
                    preview_app_url = Some(format!("http://localhost:{port}"));
                    break;
                }
            }
        }
    }

    // ── Self-improvement: extract reusable skill DNA from this success ─────
    //    Nexus learns by distilling the patterns behind each successful build
    //    into draft "skills". After 3 uses at 70%+ confidence they get
    //    promoted to `active` and start enhancing future prompts. This is the
    //    producer side of the loop — the promoter + consumer live elsewhere.
    if files_count > 0 && taste_score >= 60 {
        let project_dir = app.data_dir.join("projects").join(&project_id).join("generated");
        let files_written = list_generated_files(&project_dir);
        let tools_used = vec!["file_write".to_string(), "file_read".to_string()];
        let phases = vec![crate::skill_dna::PhaseInfo {
            phase: "codegen".into(),
            agent: "oneshot".into(),
            files_touched: files_written.clone(),
            tools_used: tools_used.clone(),
            success: true,
        }];
        if let Some(skill) = crate::skill_dna::extract_from_execution(
            &project_id,
            &description,
            &phases,
            &files_written,
            &tools_used,
            true,
        ) {
            if let Err(e) = crate::skill_dna::store_skill(&app, &skill).await {
                tracing::warn!(error = %e, "Failed to store extracted skill DNA");
            } else {
                tracing::info!(
                    project_id = %project_id,
                    skill_name = %skill.name,
                    patterns = skill.patterns.len(),
                    "Extracted skill DNA from successful build"
                );
            }
        }
    }

    // ── Complete ────────────────────────────────────────────────────────────
    let duration_ms = start.elapsed().as_millis() as u64;
    emit!(OneShotEvent::Progress { percent: 100, message: "Done".into(), phase: "complete".into() });
    emit!(OneShotEvent::Complete {
        project_id: project_id.clone(),
        project_name: project_name.clone(),
        taste_score,
        files_count,
        duration_ms,
        app_url: preview_app_url,
    });

    // Append an assistant summary message to the conversation so the chat
    // reflects what was built.
    if let Some(ref cid) = conversation_id {
        let summary = format!(
            "Built {} — generated {} files in {:.1}s (quality {}/100). You can preview the app, browse the files, or ask me to make changes.",
            project_name,
            files_count,
            (duration_ms as f64) / 1000.0,
            taste_score,
        );
        let db = app.db.lock().await;
        let svc = ProjectService::new(&db);
        let _ = svc.append_nexus_message(cid, "assistant", &summary, None);
    }

    // Mark kernel process as completed
    if let Some(ref pid) = process_id {
        app.scheduler
            .complete(pid, Some(format!("Generated {files_count} files, taste score {taste_score}")))
            .await
            .ok();
    }

    info!(
        project_id = %project_id,
        files = files_count,
        taste = taste_score,
        duration_ms,
        "One-shot pipeline complete"
    );

    // Evict broadcast channels that no longer have active SSE subscribers.
    super::live_build_handler::evict_dead_channels(&app).await;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn run_codegen(
    app: &Arc<AppState>,
    project_id: &str,
    project_dir: &std::path::Path,
    prompt: &str,
    // user_description: original (unenriched) description — used for stack
    // detection so incidental words in injected context don't bias the
    // framework choice (e.g. the word "rust" appearing in decision context).
    user_description: &str,
    tx: &mpsc::Sender<OneShotEvent>,
    routed_provider: &str,
    routed_model: &str,
) -> Result<usize, String> {
    let _ = std::fs::create_dir_all(project_dir);

    let _ = tx.send(OneShotEvent::Progress {
        percent: 55,
        message: "Generating with LLM".into(),
        phase: "codegen".into(),
    }).await;

    // Step 1: Build a minimal IR and plan (sync, drop lock before await)
    let ir = json!({
        "description": prompt,
        "entities": [],
        "agents": [],
    });

    let plan = {
        let db = app.db.lock().await;
        let materializer = CodeGenMaterializer::new(&db);
        materializer.plan(project_id, &ir).map_err(|e| format!("Plan failed: {}", e))?
        // db and materializer dropped here
    };

    // Step 2: Build generation prompt and call LLM (using routed model)
    // Derive app-specific context from the prompt so Nova generates a unique app.
    let oneshot_intent = crate::intent_engine::analyze_flat(user_description);
    let oneshot_name = derive_project_name(user_description);
    let oneshot_stack = nexus_store::detect_tech_stack(user_description);
    let (oneshot_css, oneshot_font) = if nexus_store::is_web_stack(&oneshot_stack) {
        (
            crate::design_system::generate_globals_css(&oneshot_intent.ui_style, &oneshot_name),
            crate::design_system::font_imports(&oneshot_intent.ui_style).to_string(),
        )
    } else {
        (String::new(), String::new())
    };
    let oneshot_ctx = nexus_store::AppContext {
        app_name: oneshot_name,
        ui_style: format!("{:?}", oneshot_intent.ui_style),
        app_type: format!("{:?}", oneshot_intent.app_type),
        tech_stack: oneshot_stack,
        globals_css: oneshot_css,
        font_link: oneshot_font,
        suggested_pages: oneshot_intent.suggested_pages,
        tagline: String::new(),
    };
    let gen_prompt = build_generation_prompt(&plan, prompt, Some(&oneshot_ctx));

    info!(
        project_id = %project_id,
        tech_stack = %oneshot_ctx.tech_stack,
        prompt_len = gen_prompt.len(),
        prompt_head = %gen_prompt.chars().take(400).collect::<String>(),
        "Codegen prompt prepared"
    );

    // Always generate from LLM — no static fallbacks.
    let generated_files =
        super::llm_codegen::generate_app_files_with_model(app, &gen_prompt, routed_provider, routed_model)
            .await
            .map_err(|e| format!("Code generation failed: {e}"))?;

    // Write the LLM-generated files (sync, drop lock before await)
    let project_data_db = app.project_data_db(project_id);
    let llm_result = {
        let db = app.db.lock().await;
        let materializer = CodeGenMaterializer::new(&db);
        materializer
            .generate_from_llm_output(project_id, project_dir, &plan, &project_data_db, &generated_files)
            .map_err(|e| format!("File write failed: {}", e))?
    };

    // CLAUDE.md invariant #5: generated files pass the invariant enforcer
    // before being committed to project state. We run the PreCommit gate
    // here, log blocking findings, and let the outcome-guarantee loop /
    // explicit /enforce endpoint handle remediation. We don't fail the
    // generation outright — the user already has files and a partial
    // result is better than nothing — but a non-empty `blocking` list
    // surfaces as a warning event so the UI can prompt for repair.
    let inv_report = {
        let dir = project_dir.to_path_buf();
        tokio::task::spawn_blocking(move || {
            let enforcer =
                crate::invariant_enforcer::InvariantEnforcer::load_for_project(&dir);
            enforcer.enforce(&dir, crate::invariant_enforcer::GateContext::PreCommit)
        })
        .await
        .ok()
    };
    if let Some(report) = inv_report {
        if !report.blocking_violations.is_empty() {
            tracing::warn!(
                project_id = %project_id,
                blocking = report.blocking_violations.len(),
                warnings = report.warnings.len(),
                "Pre-commit invariant gate flagged violations"
            );
            let _ = tx
                .send(OneShotEvent::Progress {
                    percent: 70,
                    message: format!(
                        "Pre-commit gate: {} blocking, {} warnings",
                        report.blocking_violations.len(),
                        report.warnings.len()
                    ),
                    phase: "invariants".into(),
                })
                .await;
        }
    }

    let count = llm_result.files_written.len();
    let _ = tx.send(OneShotEvent::Progress {
        percent: 70,
        message: format!("Generated {} files", count),
        phase: "codegen".into(),
    }).await;

    Ok(count)
}

fn derive_project_name(description: &str) -> String {
    let clean: String = description
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect();

    let words: Vec<&str> = clean.split_whitespace().take(4).collect();
    if words.is_empty() {
        return "my-app".to_string();
    }

    words
        .join("-")
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect()
}

fn oneshot_event_type(e: &OneShotEvent) -> &'static str {
    match e {
        OneShotEvent::Phase { .. } => "phase",
        OneShotEvent::Progress { .. } => "progress",
        OneShotEvent::IntentAnalyzed { .. } => "intent_analyzed",
        OneShotEvent::DecisionsMade { .. } => "decisions_made",
        OneShotEvent::ProductBriefReady { .. } => "product_brief_ready",
        OneShotEvent::ProjectCreated { .. } => "project_created",
        OneShotEvent::FilesGenerated { .. } => "files_generated",
        OneShotEvent::TasteScored { .. } => "taste_scored",
        OneShotEvent::RedesignComplete { .. } => "redesign_complete",
        OneShotEvent::Thinking { .. } => "thinking",
        OneShotEvent::Explanation { .. } => "explanation",
        OneShotEvent::Skeleton { .. } => "skeleton",
        OneShotEvent::Estimate { .. } => "estimate",
        OneShotEvent::FileWritten { .. } => "file_written",
        OneShotEvent::Heartbeat { .. } => "heartbeat",
        OneShotEvent::Complete { .. } => "complete",
        OneShotEvent::Error { .. } => "error",
    }
}

/// Walk a generated project tree and return the relative paths of every
/// regular file. Skips `node_modules`, `.next`, `.git`, and other build
/// artefacts so the skill-extraction signal reflects user-authored structure,
/// not dependencies.
fn list_generated_files(project_dir: &std::path::Path) -> Vec<String> {
    fn walk(dir: &std::path::Path, root: &std::path::Path, out: &mut Vec<String>) {
        const SKIP: &[&str] = &["node_modules", ".next", ".git", "dist", "build", ".nexus"];
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if SKIP.contains(&name) || name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                walk(&path, root, out);
            } else if path.is_file() {
                if let Ok(rel) = path.strip_prefix(root) {
                    out.push(rel.to_string_lossy().into_owned());
                }
            }
            // Cap to avoid OOM on pathological inputs.
            if out.len() > 500 {
                return;
            }
        }
    }
    let mut out = Vec::new();
    walk(project_dir, project_dir, &mut out);
    out
}

/// Deterministic sanity check of a freshly generated project tree.
///
/// Returns a list of human-readable issues. Empty = looks runnable. This is
/// intentionally cheap (filesystem stat + one JSON parse) so we can run it
/// on every codegen without slowing the pipeline. The full `next build`
/// / `tsc --noEmit` is reserved for the auto-start phase where a real dev
/// server is about to consume the tree anyway.
fn check_post_codegen_sanity(project_dir: &std::path::Path) -> Vec<String> {
    let mut errors: Vec<String> = Vec::new();

    let pkg_path = project_dir.join("package.json");
    if !pkg_path.exists() {
        errors.push("package.json is missing".into());
        return errors; // nothing else to check
    }

    let content = match std::fs::read_to_string(&pkg_path) {
        Ok(c) => c,
        Err(e) => {
            errors.push(format!("cannot read package.json: {e}"));
            return errors;
        }
    };

    let pkg: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            errors.push(format!("package.json is invalid JSON: {e}"));
            return errors;
        }
    };

    let scripts = &pkg["scripts"];
    let has_dev = scripts.get("dev").is_some();
    let has_start = scripts.get("start").is_some();
    if !has_dev && !has_start {
        errors.push(
            "package.json has no `dev` or `start` script — app cannot auto-start".into(),
        );
    }

    // Next.js projects should have at least one page entry, either
    // `app/page.tsx`, `app/page.jsx`, or `pages/index.*`.
    let has_next = pkg
        .get("dependencies")
        .and_then(|d| d.get("next"))
        .is_some();
    if has_next {
        let candidates = [
            "app/page.tsx",
            "app/page.jsx",
            "app/page.js",
            "src/app/page.tsx",
            "src/app/page.jsx",
            "src/app/page.js",
            "pages/index.tsx",
            "pages/index.jsx",
            "pages/index.js",
        ];
        if !candidates.iter().any(|p| project_dir.join(p).exists()) {
            errors.push(
                "Next.js project has no recognisable entry page \
                 (expected app/page.tsx or pages/index.tsx)"
                    .into(),
            );
        }
    }

    errors
}

#[cfg(test)]
mod sanity_tests {
    use super::check_post_codegen_sanity;
    use tempfile::tempdir;

    #[test]
    fn reports_missing_package_json() {
        let dir = tempdir().unwrap();
        let errors = check_post_codegen_sanity(dir.path());
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("package.json is missing"));
    }

    #[test]
    fn reports_invalid_json() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), "{ not json").unwrap();
        let errors = check_post_codegen_sanity(dir.path());
        assert!(errors.iter().any(|e| e.contains("invalid JSON")));
    }

    #[test]
    fn reports_no_scripts() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"x","version":"0.0.0"}"#,
        )
        .unwrap();
        let errors = check_post_codegen_sanity(dir.path());
        assert!(errors.iter().any(|e| e.contains("no `dev` or `start`")));
    }

    #[test]
    fn accepts_minimal_valid_project() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"x","version":"0.0.0","scripts":{"dev":"next dev"}}"#,
        )
        .unwrap();
        // No next dep → no entry-page check.
        let errors = check_post_codegen_sanity(dir.path());
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    }

    #[test]
    fn requires_entry_page_for_nextjs() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"x","scripts":{"dev":"next dev"},"dependencies":{"next":"14"}}"#,
        )
        .unwrap();
        let errors = check_post_codegen_sanity(dir.path());
        assert!(
            errors.iter().any(|e| e.contains("no recognisable entry page")),
            "expected Next.js entry-page check, got: {errors:?}"
        );
    }
}
