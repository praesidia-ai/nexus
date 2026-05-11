//! Agent TV — multiplexed SSE stream that translates internal
//! [`LiveBuildEvent`] traffic into the [`AgentTvEvent`] contract consumed by
//! the `/[projectId]/agent-tv` frontend.
//!
//! The handler subscribes to the per-project broadcast channel in
//! `AppState::build_event_bus`, emits an initial [`AgentTvEvent::RosterUpdate`]
//! with all ten ZeroClaw agents in the `idle` state, then translates each
//! inbound `LiveBuildEvent` into one or more `AgentTvEvent`s while tracking
//! per-agent cumulative cost. A `Heartbeat` is emitted every 5 seconds so the
//! frontend can detect a stale connection.
//!
//! See [`docs/AGENT_TV_CONTRACT.md`](../../../../docs/AGENT_TV_CONTRACT.md).

use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::sync::Arc;

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::broadcast;
use tokio::time::{interval, Duration, Instant, MissedTickBehavior};

use crate::{
    error::{ApiError, ApiResult},
    handlers::live_build_handler::LiveBuildEvent,
    security::{auth::Scope, project_access::ProjectAccess},
    state::AppState,
};

// ---------------------------------------------------------------------------
// Roster — the ten ZeroClaw agents
// ---------------------------------------------------------------------------

/// Static table of ZeroClaw agents. Ordering matches the frontend 2x5 grid:
/// `nova atlas kai luna orion` / `sage ivy rex leo mia`.
pub const ROSTER: &[(&str, &str, &str, &str)] = &[
    ("nova", "Nova", "\u{1F680}", "Software engineering"),
    ("atlas", "Atlas", "\u{2601}\u{FE0F}", "Cloud & infra"),
    ("kai", "Kai", "\u{1F50D}", "Research"),
    ("luna", "Luna", "\u{270D}\u{FE0F}", "Writing"),
    ("orion", "Orion", "\u{1F6E1}\u{FE0F}", "Security"),
    ("sage", "Sage", "\u{1F4CA}", "Data"),
    ("ivy", "Ivy", "\u{1F4E3}", "Marketing"),
    ("rex", "Rex", "\u{2699}\u{FE0F}", "DevOps"),
    ("leo", "Leo", "\u{1F3AF}", "Product"),
    ("mia", "Mia", "\u{1F4AC}", "Support"),
];

fn roster_cards() -> Vec<AgentCard> {
    ROSTER
        .iter()
        .map(|(id, name, emoji, role)| AgentCard {
            id: (*id).to_string(),
            name: (*name).to_string(),
            emoji: (*emoji).to_string(),
            role: (*role).to_string(),
            status: "idle".to_string(),
        })
        .collect()
}

#[cfg(test)]
fn is_known_agent(id: &str) -> bool {
    ROSTER.iter().any(|(slug, _, _, _)| *slug == id)
}

// ---------------------------------------------------------------------------
// Contract types (must mirror docs/AGENT_TV_CONTRACT.md exactly)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    pub id: String,
    pub name: String,
    pub emoji: String,
    pub role: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentTvEvent {
    RosterUpdate {
        agents: Vec<AgentCard>,
    },
    AgentStatus {
        agent_id: String,
        status: String,
        current_file: Option<String>,
        cost_usd: f64,
        tokens_in: u64,
        tokens_out: u64,
    },
    AgentThought {
        agent_id: String,
        content: String,
        timestamp_ms: u64,
    },
    AgentFileWrite {
        agent_id: String,
        path: String,
        lines: usize,
        kind: String,
    },
    AgentToolCall {
        agent_id: String,
        tool: String,
        args_preview: String,
    },
    Heartbeat {
        elapsed_ms: u64,
    },
    Complete {
        total_cost_usd: f64,
        total_files: usize,
        duration_ms: u64,
        agents_used: Vec<String>,
    },
    Error {
        message: String,
    },
}

impl AgentTvEvent {
    fn event_name(&self) -> &'static str {
        match self {
            AgentTvEvent::RosterUpdate { .. } => "roster_update",
            AgentTvEvent::AgentStatus { .. } => "agent_status",
            AgentTvEvent::AgentThought { .. } => "agent_thought",
            AgentTvEvent::AgentFileWrite { .. } => "agent_file_write",
            AgentTvEvent::AgentToolCall { .. } => "agent_tool_call",
            AgentTvEvent::Heartbeat { .. } => "heartbeat",
            AgentTvEvent::Complete { .. } => "complete",
            AgentTvEvent::Error { .. } => "error",
        }
    }

    fn into_sse(self) -> Event {
        let name = self.event_name();
        let data = serde_json::to_string(&self).unwrap_or_else(|_| "{}".to_string());
        Event::default().event(name).data(data)
    }
}

// ---------------------------------------------------------------------------
// Translation helpers
// ---------------------------------------------------------------------------

/// Map an arbitrary agent label (e.g. `"Nova"`, `"nova (coder)"`,
/// `"ZeroClaw: Atlas"`) to the stable roster slug used by the frontend.
///
/// Falls back to `"nova"` for unknown labels so the UI can still show
/// activity even when the emitter uses an unmapped name.
pub fn agent_id_from_label(label: &str) -> String {
    let lower = label.to_lowercase();
    for (slug, name, _, _) in ROSTER {
        let name_lower = name.to_lowercase();
        // Fast path — exact match on slug or display name.
        if lower == *slug || lower == name_lower {
            return (*slug).to_string();
        }
        // Word-boundary match — protects against "nova-1" and "Nova (coder)".
        let bounded = lower.split(|c: char| !c.is_alphanumeric());
        for token in bounded {
            if token == *slug || token == name_lower {
                return (*slug).to_string();
            }
        }
    }
    "nova".to_string()
}

/// Map a free-form action verb into the canonical status pill values.
///
/// Contract values: `idle | thinking | writing | reviewing | done | error`.
pub fn action_to_status(action: &str) -> String {
    let a = action.to_lowercase();
    if a.contains("error") || a.contains("fail") {
        "error".to_string()
    } else if a.contains("review") || a.contains("audit") || a.contains("inspect") {
        "reviewing".to_string()
    } else if a.contains("writ")
        || a.contains("generat")
        || a.contains("creat")
        || a.contains("edit")
        || a.contains("updat")
        || a.contains("implement")
        || a.contains("coding")
    {
        "writing".to_string()
    } else if a.contains("complete") || a.contains("done") || a.contains("finish") {
        "done".to_string()
    } else if a.contains("idle") || a.contains("wait") {
        "idle".to_string()
    } else {
        "thinking".to_string()
    }
}

// ---------------------------------------------------------------------------
// Per-run state machine — maintained inside the SSE task.
// ---------------------------------------------------------------------------

#[derive(Default)]
struct TvState {
    last_active_agent: Option<String>,
    engaged: HashSet<String>,
    /// Real, ground-truth per-agent cost in USD. Populated by
    /// [`LiveBuildEvent::AgentTokens`] deltas. Falls back to an equal-share
    /// split of the project running total until the first real sample
    /// arrives (best-effort — better than showing $0 on every card).
    cost_by_agent: HashMap<String, f64>,
    /// Cumulative per-agent token counters, driven by `AgentTokens` deltas.
    tokens_in_by_agent: HashMap<String, u64>,
    tokens_out_by_agent: HashMap<String, u64>,
    /// True once at least one real `AgentTokens` sample has been observed.
    /// Until then, `update_costs_equal_share` runs so cards still move.
    have_real_tokens: bool,
    files_written: usize,
    last_total_cost: f64,
}

impl TvState {
    /// Mark an agent as engaged. Returns true iff this is a new engagement
    /// (i.e. the set grew), so the caller can trigger a cost redistribution.
    fn mark_engaged(&mut self, agent_id: &str) -> bool {
        let inserted = self.engaged.insert(agent_id.to_string());
        self.last_active_agent = Some(agent_id.to_string());
        inserted
    }

    /// Fallback cost attribution — evenly split `total_project_cost` across
    /// engaged agents. Only used before any real [`AgentTokens`] sample has
    /// been received; afterwards the ground-truth accumulator wins.
    fn update_costs_equal_share(&mut self, total_project_cost: f64) {
        if self.have_real_tokens {
            return;
        }
        let total_changed = (total_project_cost - self.last_total_cost).abs() >= f64::EPSILON;
        let expected_entries = self.engaged.len();
        let tracked_entries = self
            .cost_by_agent
            .keys()
            .filter(|k| self.engaged.contains(*k))
            .count();
        let membership_changed = tracked_entries != expected_entries;
        if !total_changed && !membership_changed {
            return;
        }
        self.last_total_cost = total_project_cost;
        if self.engaged.is_empty() {
            return;
        }
        let share = total_project_cost / self.engaged.len() as f64;
        for id in &self.engaged {
            self.cost_by_agent.insert(id.clone(), share);
        }
    }

    /// Record a real LLM-call delta for `agent_id`. Accumulates both tokens
    /// and cost. Flips `have_real_tokens` so the equal-share fallback stops.
    fn record_tokens(&mut self, agent_id: &str, tokens_in: u64, tokens_out: u64, cost_usd: f64) {
        self.have_real_tokens = true;
        *self.tokens_in_by_agent.entry(agent_id.to_string()).or_insert(0) += tokens_in;
        *self
            .tokens_out_by_agent
            .entry(agent_id.to_string())
            .or_insert(0) += tokens_out;
        *self.cost_by_agent.entry(agent_id.to_string()).or_insert(0.0) += cost_usd;
    }

    fn cost_for(&self, agent_id: &str) -> f64 {
        self.cost_by_agent.get(agent_id).copied().unwrap_or(0.0)
    }

    fn tokens_in_for(&self, agent_id: &str) -> u64 {
        self.tokens_in_by_agent.get(agent_id).copied().unwrap_or(0)
    }

    fn tokens_out_for(&self, agent_id: &str) -> u64 {
        self.tokens_out_by_agent
            .get(agent_id)
            .copied()
            .unwrap_or(0)
    }
}

/// Canonicalise a free-form "agent" label for any of the three ingest paths:
/// ZeroClaw roster slug (`nova`), coding-agent role (`coder`, `architect`…),
/// or display name (`Atlas`). Returns a stable roster slug.
pub fn normalise_agent(label: &str) -> String {
    // First try the roster fast-path.
    let rl = label.to_lowercase();
    for (slug, name, _, _) in ROSTER {
        if rl == *slug || rl == name.to_lowercase() {
            return (*slug).to_string();
        }
    }
    // Map coding-agent role names onto the most thematically-appropriate
    // roster member. Keeps Agent TV lively when oneshot/coding routes fire
    // events attributed to role (not roster) names.
    match rl.as_str() {
        "architect" => "kai".into(),
        "coder" | "debugger" | "refactor" => "nova".into(),
        "reviewer" => "orion".into(),
        "tester" => "sage".into(),
        "performance" => "rex".into(),
        "ux" => "luna".into(),
        "devops" => "rex".into(),
        "product" => "leo".into(),
        _ => agent_id_from_label(label),
    }
}

/// Translate one [`LiveBuildEvent`] into zero-or-more [`AgentTvEvent`]s,
/// mutating [`TvState`] as a side effect. Pure apart from the state update —
/// tested directly in the unit tests.
fn translate(
    state: &mut TvState,
    event: LiveBuildEvent,
    run_start: Instant,
    total_project_cost: f64,
) -> Vec<AgentTvEvent> {
    state.update_costs_equal_share(total_project_cost);

    match event {
        LiveBuildEvent::AgentAction { agent, action, target } => {
            let agent_id = normalise_agent(&agent);
            state.mark_engaged(&agent_id);
            state.update_costs_equal_share(total_project_cost);
            vec![AgentTvEvent::AgentStatus {
                agent_id: agent_id.clone(),
                status: action_to_status(&action),
                current_file: target,
                cost_usd: state.cost_for(&agent_id),
                tokens_in: state.tokens_in_for(&agent_id),
                tokens_out: state.tokens_out_for(&agent_id),
            }]
        }
        LiveBuildEvent::FileCreated { path, lines, .. } => {
            let agent_id = state
                .last_active_agent
                .clone()
                .unwrap_or_else(|| "nova".to_string());
            state.mark_engaged(&agent_id);
            state.files_written += 1;
            vec![AgentTvEvent::AgentFileWrite {
                agent_id,
                path,
                lines,
                kind: "created".to_string(),
            }]
        }
        LiveBuildEvent::FileUpdated { path, .. } => {
            let agent_id = state
                .last_active_agent
                .clone()
                .unwrap_or_else(|| "nova".to_string());
            state.mark_engaged(&agent_id);
            state.files_written += 1;
            vec![AgentTvEvent::AgentFileWrite {
                agent_id,
                path,
                // change_summary isn't a line count — surface 0 so the UI
                // falls back to the "updated" pill without faking a number.
                lines: 0,
                kind: "updated".to_string(),
            }]
        }
        LiveBuildEvent::ComponentReady { path, component, .. } => {
            let agent_id = state
                .last_active_agent
                .clone()
                .unwrap_or_else(|| "nova".to_string());
            state.mark_engaged(&agent_id);
            vec![AgentTvEvent::AgentStatus {
                agent_id: agent_id.clone(),
                status: "done".to_string(),
                current_file: Some(if path.is_empty() { component } else { path }),
                cost_usd: state.cost_for(&agent_id),
                tokens_in: state.tokens_in_for(&agent_id),
                tokens_out: state.tokens_out_for(&agent_id),
            }]
        }
        LiveBuildEvent::BuildStep { step, status, .. } => {
            // Build pipeline steps belong to the DevOps agent.
            let agent_id = "rex".to_string();
            state.mark_engaged(&agent_id);
            let status_pill = match status.as_str() {
                "failed" | "error" => "error".to_string(),
                "success" | "ok" | "completed" => "done".to_string(),
                "running" | "in_progress" | "started" => "writing".to_string(),
                _ => "thinking".to_string(),
            };
            vec![AgentTvEvent::AgentStatus {
                agent_id: agent_id.clone(),
                status: status_pill,
                current_file: Some(step),
                cost_usd: state.cost_for(&agent_id),
                tokens_in: state.tokens_in_for(&agent_id),
                tokens_out: state.tokens_out_for(&agent_id),
            }]
        }
        LiveBuildEvent::Progress { .. } => {
            // UI has its own progress bar — intentionally drop.
            Vec::new()
        }
        LiveBuildEvent::TasteScore { overall, message } => {
            // Review quality belongs to the security/reviewer agent.
            let agent_id = "orion".to_string();
            state.mark_engaged(&agent_id);
            let current = if message.is_empty() {
                format!("taste: {overall}")
            } else {
                format!("taste {overall} — {message}")
            };
            vec![AgentTvEvent::AgentStatus {
                agent_id: agent_id.clone(),
                status: "reviewing".to_string(),
                current_file: Some(current),
                cost_usd: state.cost_for(&agent_id),
                tokens_in: state.tokens_in_for(&agent_id),
                tokens_out: state.tokens_out_for(&agent_id),
            }]
        }
        LiveBuildEvent::AgentThought { agent, content } => {
            let agent_id = normalise_agent(&agent);
            state.mark_engaged(&agent_id);
            let timestamp_ms = run_start.elapsed().as_millis() as u64;
            vec![AgentTvEvent::AgentThought {
                agent_id,
                content,
                timestamp_ms,
            }]
        }
        LiveBuildEvent::AgentToolCall { agent, tool, args_preview } => {
            let agent_id = normalise_agent(&agent);
            state.mark_engaged(&agent_id);
            vec![AgentTvEvent::AgentToolCall {
                agent_id,
                tool,
                args_preview,
            }]
        }
        LiveBuildEvent::AgentTokens {
            agent,
            tokens_in,
            tokens_out,
            cost_usd,
        } => {
            let agent_id = normalise_agent(&agent);
            state.mark_engaged(&agent_id);
            state.record_tokens(&agent_id, tokens_in, tokens_out, cost_usd);
            // Emit a status refresh so the cost/token ticker moves without
            // waiting for the next AgentAction event. Preserve the
            // card's current status by reporting `thinking` — benign when
            // the agent is genuinely working, and we avoid faking `done`.
            vec![AgentTvEvent::AgentStatus {
                agent_id: agent_id.clone(),
                status: "thinking".to_string(),
                current_file: None,
                cost_usd: state.cost_for(&agent_id),
                tokens_in: state.tokens_in_for(&agent_id),
                tokens_out: state.tokens_out_for(&agent_id),
            }]
        }
        LiveBuildEvent::Complete {
            files_created,
            duration_ms,
            ..
        } => {
            let total_files = state.files_written.max(files_created);
            let agents_used: Vec<String> = state.engaged.iter().cloned().collect();
            // When we've accumulated ground-truth per-agent cost, prefer that
            // sum; otherwise fall back to the project daily total.
            let total_cost_usd = if state.have_real_tokens {
                state.cost_by_agent.values().sum::<f64>()
            } else {
                total_project_cost
            };
            vec![AgentTvEvent::Complete {
                total_cost_usd,
                total_files,
                duration_ms: if duration_ms > 0 {
                    duration_ms
                } else {
                    run_start.elapsed().as_millis() as u64
                },
                agents_used,
            }]
        }
        LiveBuildEvent::Error { message, .. } => {
            vec![AgentTvEvent::Error { message }]
        }
    }
}

// ---------------------------------------------------------------------------
// Cost accessor
// ---------------------------------------------------------------------------

/// Look up today's spend for `project_id`. Returns 0 if nothing billed yet.
async fn project_cost_today(app: &Arc<AppState>, project_id: &str) -> f64 {
    let summary = app.cost_tracker.daily_summary().await;
    summary
        .by_project
        .get(project_id)
        .copied()
        .unwrap_or(0.0)
}

// ---------------------------------------------------------------------------
// SSE handler
// ---------------------------------------------------------------------------

/// `GET /projects/:id/agent-tv/stream`
///
/// Multiplexes `LiveBuildEvent`s from `build_event_bus` into `AgentTvEvent`s
/// per the contract in `docs/AGENT_TV_CONTRACT.md`. Emits a roster on
/// connect, a heartbeat every 5 seconds, and forwards translated events.
#[tracing::instrument(skip(app), fields(project_id = %access.project_id))]
pub async fn agent_tv_stream(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    access
        .require_scope(&Scope::ProjectRead)
        .map_err(|_| ApiError::Forbidden("Missing required scope: project:read".to_string()))?;

    let project_id = access.project_id.clone();

    // Subscribe (or create) the per-project broadcast channel.
    let rx = {
        let mut bus = app.build_event_bus.write().await;
        let tx = bus
            .entry(project_id.clone())
            .or_insert_with(|| broadcast::channel(4096).0);
        tx.subscribe()
    };

    tracing::info!("Agent TV stream connected");

    let stream = build_stream(app.clone(), project_id, rx);
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

fn build_stream(
    app: Arc<AppState>,
    project_id: String,
    mut rx: broadcast::Receiver<LiveBuildEvent>,
) -> impl Stream<Item = Result<Event, Infallible>> {
    async_stream::stream! {
        let run_start = Instant::now();
        let mut state = TvState::default();

        // 1. Initial roster.
        let roster = AgentTvEvent::RosterUpdate { agents: roster_cards() };
        yield Ok::<Event, Infallible>(roster.into_sse());

        // 2. Heartbeat ticker — one every 5 seconds.
        let mut heartbeat = interval(Duration::from_secs(5));
        heartbeat.set_missed_tick_behavior(MissedTickBehavior::Skip);
        // Burn the immediate first tick so we don't emit at t=0.
        heartbeat.tick().await;

        loop {
            tokio::select! {
                tick = heartbeat.tick() => {
                    let _ = tick;
                    let elapsed_ms = run_start.elapsed().as_millis() as u64;
                    let ev = AgentTvEvent::Heartbeat { elapsed_ms };
                    yield Ok(ev.into_sse());
                }
                recv = rx.recv() => {
                    match recv {
                        Ok(event) => {
                            let total_cost = project_cost_today(&app, &project_id).await;
                            for translated in translate(&mut state, event, run_start, total_cost) {
                                yield Ok(translated.into_sse());
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            let ev = Event::default()
                                .event("lag")
                                .data(json!({
                                    "message": "stream lagged",
                                    "skipped": skipped,
                                }).to_string());
                            yield Ok(ev);
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            // Emit a terminal event so frontends don't hang
                            // waiting for more data on a dead stream.
                            let ev = Event::default()
                                .event("complete")
                                .data(json!({
                                    "reason": "stream_closed",
                                    "elapsed_ms": run_start.elapsed().as_millis() as u64,
                                }).to_string());
                            yield Ok(ev);
                            break;
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_id_from_label_maps_roster() {
        for (slug, name, _, _) in ROSTER {
            assert_eq!(agent_id_from_label(slug), *slug);
            assert_eq!(agent_id_from_label(name), *slug);
            assert_eq!(
                agent_id_from_label(&name.to_uppercase()),
                *slug,
                "uppercase {name}"
            );
        }
        // Bracketed / annotated labels still resolve.
        assert_eq!(agent_id_from_label("Nova (coder)"), "nova");
        assert_eq!(agent_id_from_label("ZeroClaw: Atlas"), "atlas");
        assert_eq!(agent_id_from_label("rex-1"), "rex");
        // Unknown labels fall back to nova so the UI always has a home.
        assert_eq!(agent_id_from_label("some-random-thing"), "nova");
    }

    #[test]
    fn action_to_status_covers_common_verbs() {
        assert_eq!(action_to_status("writing code"), "writing");
        assert_eq!(action_to_status("Generating UI"), "writing");
        assert_eq!(action_to_status("creating file"), "writing");
        assert_eq!(action_to_status("editing index.ts"), "writing");
        assert_eq!(action_to_status("implementing handler"), "writing");
        assert_eq!(action_to_status("reviewing PR"), "reviewing");
        assert_eq!(action_to_status("auditing code"), "reviewing");
        assert_eq!(action_to_status("thinking about design"), "thinking");
        assert_eq!(action_to_status("planning architecture"), "thinking");
        assert_eq!(action_to_status("completed build"), "done");
        assert_eq!(action_to_status("done"), "done");
        assert_eq!(action_to_status("error: oom"), "error");
        assert_eq!(action_to_status("failed to compile"), "error");
        assert_eq!(action_to_status("idle"), "idle");
    }

    #[test]
    fn roster_contains_all_ten_agents() {
        assert_eq!(ROSTER.len(), 10);
        let cards = roster_cards();
        assert_eq!(cards.len(), 10);
        assert!(cards.iter().all(|c| c.status == "idle"));
        let ids: Vec<_> = cards.iter().map(|c| c.id.as_str()).collect();
        for expected in [
            "nova", "atlas", "kai", "luna", "orion", "sage", "ivy", "rex", "leo", "mia",
        ] {
            assert!(ids.contains(&expected), "missing {expected}");
        }
    }

    #[test]
    fn translate_agent_action_emits_status_and_tracks_engagement() {
        let mut state = TvState::default();
        let run_start = Instant::now();
        let out = translate(
            &mut state,
            LiveBuildEvent::AgentAction {
                agent: "Nova".to_string(),
                action: "writing handler".to_string(),
                target: Some("src/lib.rs".to_string()),
            },
            run_start,
            2.0,
        );
        assert_eq!(out.len(), 1);
        match &out[0] {
            AgentTvEvent::AgentStatus {
                agent_id,
                status,
                current_file,
                cost_usd,
                ..
            } => {
                assert_eq!(agent_id, "nova");
                assert_eq!(status, "writing");
                assert_eq!(current_file.as_deref(), Some("src/lib.rs"));
                // Only one engaged agent, so it gets the whole pot.
                assert!((*cost_usd - 2.0).abs() < 1e-9);
            }
            other => panic!("unexpected event: {other:?}"),
        }
        assert_eq!(state.last_active_agent.as_deref(), Some("nova"));
        assert!(state.engaged.contains("nova"));
    }

    #[test]
    fn translate_file_events_attribute_to_last_active_agent() {
        let mut state = TvState::default();
        let run_start = Instant::now();
        // Start with an explicit action from atlas.
        let _ = translate(
            &mut state,
            LiveBuildEvent::AgentAction {
                agent: "atlas".to_string(),
                action: "provisioning".to_string(),
                target: None,
            },
            run_start,
            0.0,
        );
        let out = translate(
            &mut state,
            LiveBuildEvent::FileCreated {
                path: "terraform/main.tf".to_string(),
                language: "hcl".to_string(),
                lines: 42,
                component_type: None,
            },
            run_start,
            0.0,
        );
        assert_eq!(out.len(), 1);
        match &out[0] {
            AgentTvEvent::AgentFileWrite {
                agent_id,
                path,
                lines,
                kind,
            } => {
                assert_eq!(agent_id, "atlas");
                assert_eq!(path, "terraform/main.tf");
                assert_eq!(*lines, 42);
                assert_eq!(kind, "created");
            }
            other => panic!("unexpected event: {other:?}"),
        }
        assert_eq!(state.files_written, 1);

        let updated = translate(
            &mut state,
            LiveBuildEvent::FileUpdated {
                path: "terraform/main.tf".to_string(),
                change_summary: "tweak".to_string(),
            },
            run_start,
            0.0,
        );
        match &updated[0] {
            AgentTvEvent::AgentFileWrite { kind, .. } => assert_eq!(kind, "updated"),
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn translate_build_step_routed_to_rex() {
        let mut state = TvState::default();
        let out = translate(
            &mut state,
            LiveBuildEvent::BuildStep {
                step: "npm install".to_string(),
                status: "running".to_string(),
                duration_ms: None,
            },
            Instant::now(),
            0.0,
        );
        match &out[0] {
            AgentTvEvent::AgentStatus { agent_id, status, .. } => {
                assert_eq!(agent_id, "rex");
                assert_eq!(status, "writing");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn translate_taste_score_routed_to_orion_reviewing() {
        let mut state = TvState::default();
        let out = translate(
            &mut state,
            LiveBuildEvent::TasteScore {
                overall: 87,
                message: "solid".to_string(),
            },
            Instant::now(),
            0.0,
        );
        match &out[0] {
            AgentTvEvent::AgentStatus { agent_id, status, current_file, .. } => {
                assert_eq!(agent_id, "orion");
                assert_eq!(status, "reviewing");
                assert!(current_file.as_deref().unwrap().contains("87"));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn translate_progress_is_dropped() {
        let mut state = TvState::default();
        let out = translate(
            &mut state,
            LiveBuildEvent::Progress {
                percent: 50,
                message: "halfway".to_string(),
                files_created: 3,
                files_total: Some(6),
            },
            Instant::now(),
            0.0,
        );
        assert!(out.is_empty());
    }

    #[test]
    fn translate_complete_includes_engaged_agents_and_total_cost() {
        let mut state = TvState::default();
        let run_start = Instant::now();
        let _ = translate(
            &mut state,
            LiveBuildEvent::AgentAction {
                agent: "nova".to_string(),
                action: "writing".to_string(),
                target: None,
            },
            run_start,
            1.0,
        );
        let _ = translate(
            &mut state,
            LiveBuildEvent::AgentAction {
                agent: "atlas".to_string(),
                action: "provisioning".to_string(),
                target: None,
            },
            run_start,
            1.0,
        );
        let out = translate(
            &mut state,
            LiveBuildEvent::Complete {
                files_created: 5,
                duration_ms: 1234,
                taste_score: Some(90),
            },
            run_start,
            3.50,
        );
        match &out[0] {
            AgentTvEvent::Complete {
                total_cost_usd,
                total_files,
                duration_ms,
                agents_used,
            } => {
                assert!((*total_cost_usd - 3.50).abs() < 1e-9);
                assert_eq!(*total_files, 5);
                assert_eq!(*duration_ms, 1234);
                assert!(agents_used.contains(&"nova".to_string()));
                assert!(agents_used.contains(&"atlas".to_string()));
                assert_eq!(agents_used.len(), 2);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn translate_error_forwards_message() {
        let mut state = TvState::default();
        let out = translate(
            &mut state,
            LiveBuildEvent::Error {
                message: "boom".to_string(),
                recoverable: false,
            },
            Instant::now(),
            0.0,
        );
        match &out[0] {
            AgentTvEvent::Error { message } => assert_eq!(message, "boom"),
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn cost_is_split_equally_across_engaged_agents_until_real_tokens_arrive() {
        let mut state = TvState::default();
        state.mark_engaged("nova");
        state.mark_engaged("atlas");
        state.mark_engaged("rex");
        state.update_costs_equal_share(3.0);
        assert!((state.cost_for("nova") - 1.0).abs() < 1e-9);
        assert!((state.cost_for("atlas") - 1.0).abs() < 1e-9);
        assert!((state.cost_for("rex") - 1.0).abs() < 1e-9);
        assert_eq!(state.cost_for("mia"), 0.0);
    }

    #[test]
    fn agent_tokens_event_populates_real_per_agent_cost_and_tokens() {
        let mut state = TvState::default();
        let out = translate(
            &mut state,
            LiveBuildEvent::AgentTokens {
                agent: "coder".into(),
                tokens_in: 1_000,
                tokens_out: 500,
                cost_usd: 0.012_5,
            },
            Instant::now(),
            99.0, // project total — should now be ignored for the slug
        );
        match &out[0] {
            AgentTvEvent::AgentStatus {
                agent_id,
                cost_usd,
                tokens_in,
                tokens_out,
                status,
                ..
            } => {
                assert_eq!(agent_id, "nova"); // coder role → nova roster slot
                assert!((*cost_usd - 0.012_5).abs() < 1e-9);
                assert_eq!(*tokens_in, 1_000);
                assert_eq!(*tokens_out, 500);
                assert_eq!(status, "thinking");
            }
            other => panic!("unexpected event: {other:?}"),
        }
        assert!(state.have_real_tokens);

        // A second delta accumulates, doesn't replace.
        let _ = translate(
            &mut state,
            LiveBuildEvent::AgentTokens {
                agent: "nova".into(),
                tokens_in: 200,
                tokens_out: 100,
                cost_usd: 0.002_5,
            },
            Instant::now(),
            99.0,
        );
        assert!((state.cost_for("nova") - 0.015).abs() < 1e-9);
        assert_eq!(state.tokens_in_for("nova"), 1_200);
        assert_eq!(state.tokens_out_for("nova"), 600);
    }

    #[test]
    fn agent_thought_event_emits_thought_and_engages_agent() {
        let mut state = TvState::default();
        let out = translate(
            &mut state,
            LiveBuildEvent::AgentThought {
                agent: "Atlas".into(),
                content: "Provisioning...".into(),
            },
            Instant::now(),
            0.0,
        );
        assert_eq!(out.len(), 1);
        match &out[0] {
            AgentTvEvent::AgentThought { agent_id, content, .. } => {
                assert_eq!(agent_id, "atlas");
                assert_eq!(content, "Provisioning...");
            }
            other => panic!("unexpected event: {other:?}"),
        }
        assert!(state.engaged.contains("atlas"));
    }

    #[test]
    fn agent_tool_call_event_emits_tool_call() {
        let mut state = TvState::default();
        let out = translate(
            &mut state,
            LiveBuildEvent::AgentToolCall {
                agent: "reviewer".into(),
                tool: "file_read".into(),
                args_preview: "src/lib.rs".into(),
            },
            Instant::now(),
            0.0,
        );
        match &out[0] {
            AgentTvEvent::AgentToolCall {
                agent_id,
                tool,
                args_preview,
            } => {
                assert_eq!(agent_id, "orion"); // reviewer → orion
                assert_eq!(tool, "file_read");
                assert_eq!(args_preview, "src/lib.rs");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn complete_uses_accumulated_per_agent_sum_when_real_tokens_available() {
        let mut state = TvState::default();
        let _ = translate(
            &mut state,
            LiveBuildEvent::AgentTokens {
                agent: "nova".into(),
                tokens_in: 1_000,
                tokens_out: 500,
                cost_usd: 1.25,
            },
            Instant::now(),
            99.0,
        );
        let _ = translate(
            &mut state,
            LiveBuildEvent::AgentTokens {
                agent: "atlas".into(),
                tokens_in: 500,
                tokens_out: 250,
                cost_usd: 0.75,
            },
            Instant::now(),
            99.0,
        );
        let out = translate(
            &mut state,
            LiveBuildEvent::Complete {
                files_created: 0,
                duration_ms: 100,
                taste_score: None,
            },
            Instant::now(),
            99.0,
        );
        match &out[0] {
            AgentTvEvent::Complete { total_cost_usd, .. } => {
                // Real sum, not the project fallback.
                assert!((*total_cost_usd - 2.0).abs() < 1e-9);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn known_agent_lookup() {
        assert!(is_known_agent("nova"));
        assert!(is_known_agent("rex"));
        assert!(!is_known_agent("whoever"));
    }

    #[test]
    fn agent_tv_event_serializes_with_snake_case_type_tag() {
        let ev = AgentTvEvent::Heartbeat { elapsed_ms: 42 };
        let json = serde_json::to_value(&ev).unwrap();
        assert_eq!(json.get("type").and_then(|v| v.as_str()), Some("heartbeat"));
        assert_eq!(json.get("elapsed_ms").and_then(|v| v.as_u64()), Some(42));
    }
}

// ---------------------------------------------------------------------------
// Persistent replay — see docs/NEXUS_MASTER_PLAN.md §4 + migration
// 004_agent_tv_replay.sql. The live stream above is ephemeral; these
// endpoints serve the same events out of SQLite so a run URL becomes a
// permanent artifact that can be embedded, shared, and scrubbed.
// ---------------------------------------------------------------------------

/// Summary row for the replay index + gallery. `merkle_root` /
/// `signature` come straight off the persisted `agent_tv_runs` row
/// (stamped by `agent_tv_sink::complete_run` when the Trust
/// Certificate is auto-issued) — surfaced inline so the gallery + the
/// run detail view can render a "✓ signed" badge without a second
/// round-trip to `/trust/cert/:runId`.
#[derive(Debug, Clone, Serialize)]
pub struct AgentTvRunSummary {
    pub id: String,
    pub project_id: String,
    pub run_kind: String,
    pub prompt: Option<String>,
    pub status: String,
    pub total_events: i64,
    pub total_tokens: i64,
    pub total_cost_usd: f64,
    pub duration_ms: Option<i64>,
    pub visibility: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merkle_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// One row of `agent_tv_events` as returned to the client. `payload`
/// stays as raw JSON — the frontend scrubber is the sole consumer.
#[derive(Debug, Clone, Serialize)]
pub struct AgentTvReplayEvent {
    pub seq: i64,
    pub event_type: String,
    pub conductor: Option<String>,
    pub mini_kind: Option<String>,
    pub payload: serde_json::Value,
    pub ts: String,
}

/// `GET /tv/:runId` — full replay of a single run: summary + events.
///
/// Returns 404 if the run does not exist or if the caller lacks access
/// (the access check is delegated to `validate_project_access` once we
/// resolve the run's project).
#[tracing::instrument(skip(app))]
pub async fn replay_run(
    State(app): State<Arc<AppState>>,
    axum::extract::Path(run_id): axum::extract::Path<String>,
) -> ApiResult<axum::Json<serde_json::Value>> {
    let db = app.db.lock().await;

    let summary: AgentTvRunSummary = db
        .query_row(
            "SELECT id, project_id, run_kind, prompt, status, total_events,
                    total_tokens, total_cost_usd, duration_ms, visibility,
                    started_at, completed_at, merkle_root, signature
             FROM agent_tv_runs WHERE id = ?1",
            [&run_id],
            |row| {
                Ok(AgentTvRunSummary {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    run_kind: row.get(2)?,
                    prompt: row.get(3)?,
                    status: row.get(4)?,
                    total_events: row.get(5)?,
                    total_tokens: row.get(6)?,
                    total_cost_usd: row.get(7)?,
                    duration_ms: row.get(8)?,
                    visibility: row.get(9)?,
                    started_at: row.get(10)?,
                    completed_at: row.get(11)?,
                    merkle_root: row.get(12)?,
                    signature: row.get(13)?,
                })
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                ApiError::NotFound(format!("run {run_id} not found"))
            }
            other => ApiError::Internal(format!("load run: {other}")),
        })?;

    // Private runs require a real access check. Public + unlisted runs
    // are readable by anyone who has the id (unlisted) or browses the
    // gallery (public). We treat "no auth context" as public-only here.
    if summary.visibility == "private" {
        return Err(ApiError::NotFound(format!("run {run_id} not found")));
    }

    let mut stmt = db
        .prepare(
            "SELECT seq, event_type, conductor, mini_kind, payload, ts
             FROM agent_tv_events
             WHERE run_id = ?1
             ORDER BY seq ASC",
        )
        .map_err(|e| ApiError::Internal(format!("prepare events: {e}")))?;

    let rows = stmt
        .query_map([&run_id], |row| {
            let payload_str: String = row.get(4)?;
            let payload = serde_json::from_str(&payload_str)
                .unwrap_or(serde_json::Value::Null);
            Ok(AgentTvReplayEvent {
                seq: row.get(0)?,
                event_type: row.get(1)?,
                conductor: row.get(2)?,
                mini_kind: row.get(3)?,
                payload,
                ts: row.get(5)?,
            })
        })
        .map_err(|e| ApiError::Internal(format!("query events: {e}")))?;

    let mut events = Vec::with_capacity(summary.total_events.max(0) as usize);
    for r in rows {
        match r {
            Ok(e) => events.push(e),
            Err(err) => {
                tracing::warn!(error = %err, "skipping malformed event row");
            }
        }
    }

    Ok(axum::Json(json!({
        "run": summary,
        "events": events,
    })))
}

/// `GET /tv/:runId/embed` — oEmbed discovery endpoint (JSON) and
/// `GET /tv/:runId/embed.html` — thin iframe page — together let a
/// Nexus run be dropped into a tweet, blog, or PR description and
/// auto-expanded into a live Agent TV panel (NEXUS_MASTER_PLAN §4).
///
/// oEmbed format 1.0, type=rich, per <https://oembed.com>.
#[tracing::instrument(skip(app))]
pub async fn embed_oembed(
    State(app): State<Arc<AppState>>,
    axum::extract::Path(run_id): axum::extract::Path<String>,
) -> ApiResult<axum::Json<serde_json::Value>> {
    // Confirm the run exists + is shareable. Private → 404.
    let visibility: String = {
        let db = app.db.lock().await;
        db.query_row(
            "SELECT visibility FROM agent_tv_runs WHERE id = ?1",
            [&run_id],
            |row| row.get(0),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                ApiError::NotFound(format!("run {run_id} not found"))
            }
            other => ApiError::Internal(format!("load run: {other}")),
        })?
    };
    if visibility == "private" {
        return Err(ApiError::NotFound(format!("run {run_id} not found")));
    }

    // The embed_html and gallery URLs stay relative — whatever host
    // the client is talking to is the host the iframe will hit.
    let html = format!(
        "<iframe src=\"/tv/{run_id}/embed.html\" width=\"640\" height=\"400\" \
         frameborder=\"0\" allow=\"fullscreen\" style=\"border-radius:12px\"></iframe>"
    );

    Ok(axum::Json(json!({
        "version": "1.0",
        "type": "rich",
        "provider_name": "Nexus",
        "provider_url": "https://nexus.praesidia.ai",
        "title": format!("Nexus run {run_id}"),
        "html": html,
        "width": 640,
        "height": 400,
    })))
}

/// `GET /tv/:runId/embed.html` — minimal standalone iframe page that
/// renders the live Agent TV dashboard for a single run. Meant for
/// oEmbed consumption, not direct human traffic.
#[tracing::instrument(skip(app))]
pub async fn embed_html(
    State(app): State<Arc<AppState>>,
    axum::extract::Path(run_id): axum::extract::Path<String>,
) -> axum::response::Response {
    use axum::response::IntoResponse;

    let visibility: Option<String> = {
        let db = app.db.lock().await;
        db.query_row(
            "SELECT visibility FROM agent_tv_runs WHERE id = ?1",
            [&run_id],
            |row| row.get(0),
        )
        .ok()
    };
    let ok = matches!(visibility.as_deref(), Some("public") | Some("unlisted"));

    if !ok {
        return (
            axum::http::StatusCode::NOT_FOUND,
            [(axum::http::header::CONTENT_TYPE, "text/html")],
            r#"<!doctype html><meta charset="utf-8"><title>Not found</title><p>This Nexus run is private or does not exist.</p>"#,
        )
            .into_response();
    }

    // Self-contained HTML page with a placeholder that the frontend
    // SPA (served at `/`) will upgrade when iframed — for now we just
    // render a card with the run id and a link through to the full
    // replay view. A future revision pipes the live SSE stream here.
    let body = format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Nexus — run {run_id}</title>
<style>
  body {{ margin: 0; background: #0b0d11; color: #e4e7ee; font: 14px/1.5 system-ui, sans-serif; }}
  .card {{ padding: 20px; height: 100vh; box-sizing: border-box; display: flex; flex-direction: column; justify-content: center; }}
  .title {{ font-size: 18px; font-weight: 600; margin-bottom: 6px; }}
  .sub {{ color: #9099ab; margin-bottom: 16px; }}
  a {{ color: #7cd1ff; text-decoration: none; }}
</style>
</head>
<body>
<div class="card">
  <div class="title">Nexus · run {run_id}</div>
  <div class="sub">Live 10-conductor / mini-agent swarm replay.</div>
  <a href="/tv/{run_id}" target="_parent">Open full Agent TV ↗</a>
</div>
</body>
</html>"#
    );
    (
        axum::http::StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (
                axum::http::header::CACHE_CONTROL,
                "public, max-age=60",
            ),
            // Allow framing — this endpoint exists to be iframed.
            (
                axum::http::header::HeaderName::from_static("x-frame-options"),
                "ALLOWALL",
            ),
        ],
        body,
    )
        .into_response()
}

/// `GET /tv` — public gallery of opt-in runs, most recent first.
#[tracing::instrument(skip(app))]
pub async fn list_public_runs(
    State(app): State<Arc<AppState>>,
) -> ApiResult<axum::Json<serde_json::Value>> {
    let db = app.db.lock().await;
    let mut stmt = db
        .prepare(
            "SELECT id, project_id, run_kind, prompt, status, total_events,
                    total_tokens, total_cost_usd, duration_ms, visibility,
                    started_at, completed_at, merkle_root, signature
             FROM agent_tv_runs
             WHERE visibility = 'public'
             ORDER BY started_at DESC
             LIMIT 100",
        )
        .map_err(|e| ApiError::Internal(format!("prepare gallery: {e}")))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(AgentTvRunSummary {
                id: row.get(0)?,
                project_id: row.get(1)?,
                run_kind: row.get(2)?,
                prompt: row.get(3)?,
                status: row.get(4)?,
                total_events: row.get(5)?,
                total_tokens: row.get(6)?,
                total_cost_usd: row.get(7)?,
                duration_ms: row.get(8)?,
                visibility: row.get(9)?,
                started_at: row.get(10)?,
                completed_at: row.get(11)?,
                merkle_root: row.get(12)?,
                signature: row.get(13)?,
            })
        })
        .map_err(|e| ApiError::Internal(format!("query gallery: {e}")))?;

    let runs: Vec<AgentTvRunSummary> = rows.filter_map(|r| r.ok()).collect();
    Ok(axum::Json(json!({ "runs": runs })))
}
