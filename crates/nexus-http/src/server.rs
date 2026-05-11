//! Axum router — all routes wired here.

use std::sync::Arc;

use axum::http::{header::HeaderName, HeaderValue};
use axum::{
    extract::DefaultBodyLimit,
    routing::{delete, get, post, put},
    Router,
};
use tower_http::{
    catch_panic::CatchPanicLayer,
    cors::{Any, CorsLayer},
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};

/// Maximum request body size — 10 MiB. Covers every JSON payload we accept
/// including large file-write POSTs. Protects against memory-amplification
/// DoS from a hostile client.
const BODY_LIMIT_BYTES: usize = 10 * 1024 * 1024;
use tracing::warn;

use crate::{
    handlers::{
        a2a_handler, agent_run, agent_skills, agent_tv, agents, app_agent_handler, app_runner,
        audit_trail_handler, background_agents, borrowed_agents as borrowed_agents_handler,
        brain_handler, business_handler, chat, code_graph, codegen, coding, coding_agents_handler,
        collaboration, cost_handler, credits_handler, decision_handler, decision_learning_handler,
        enforcement, eval_handler, evolution_handler, evolution_phase_handler, execution, explain,
        forge, global_memory, governance_handler, guarantee_handler, hitl_handler,
        integrations_handler, intelligence_handler, intelligence_layer_handler, intent_handler,
        intent_to_app, invariants_handler, kernel_handler, knowledge, learning_handler,
        live_build_handler, live_update_handler, living_system_handler, marketplace, mcp_handler,
        memory_handler, metrics, multimodal_handler, mutation_handler, observability, oneshot,
        orchestrator_handler, planner, plugin_handler, plugins, portal, preferences_handler,
        product_engine_handler, production_sim_handler, projects, sandbox_handler,
        security_handler, self_improvement_handler, settings, smoke_test_handler, speed_handler,
        super_agents_handler, swarm, tables, taste_handler, teams_handler, templates_handler,
        trust_cert, trust_handler, user_learning_handler, user_sim_handler, vault,
        wave_pipeline_handler, webhook_handler, workflow_designer_handler, workflows,
    },
    state::AppState,
};

pub fn create_router(state: Arc<AppState>) -> Router {
    let cors = build_cors_layer();

    Router::new()
        // ── Projects ──────────────────────────────────────────────────────
        .route("/projects", get(projects::list_projects).post(projects::create_project))
        .route("/projects/:id", get(projects::get_project).delete(projects::delete_project))
        .route("/projects/:id/fork", post(projects::fork_project))
        .route("/projects/:id/phase", post(projects::update_phase))
        .route("/projects/:id/model", get(projects::get_project_model).post(projects::set_project_model))

        // ── Conversations & messages ───────────────────────────────────────
        .route(
            "/projects/:id/conversations",
            get(projects::list_conversations).post(projects::create_conversation),
        )
        .route(
            "/projects/:id/conversations/:conv_id/messages",
            get(projects::list_messages),
        )

        // ── Chat (SSE) ─────────────────────────────────────────────────────
        .route("/projects/:id/chat", post(chat::chat_stream))

        // ── Knowledge ─────────────────────────────────────────────────────
        .route(
            "/projects/:id/knowledge",
            get(knowledge::list_knowledge).post(knowledge::add_knowledge),
        )
        .route("/projects/:id/knowledge/:item_id", delete(knowledge::delete_knowledge))

        // ── Tables & records ───────────────────────────────────────────────
        .route("/projects/:id/tables", get(tables::list_tables))
        .route(
            "/projects/:id/tables/:table_name/records",
            get(tables::list_records).post(tables::insert_record),
        )
        .route(
            "/projects/:id/tables/:table_name/records/:rowid",
            delete(tables::delete_record),
        )

        // ── Agents ────────────────────────────────────────────────────────
        .route(
            "/projects/:id/agents",
            get(agents::list_agents).post(workflow_designer_handler::create_agent),
        )
        .route("/projects/:id/agents/design", post(workflow_designer_handler::design_workflow))
        .route("/projects/:id/agents/design/materialize", post(workflow_designer_handler::materialize_workflow))
        .route("/projects/:id/agents/:agent_id", get(agents::get_agent).delete(agents::delete_agent))
        .route("/projects/:id/agents/:agent_id/run", post(agents::run_agent))
        .route("/projects/:id/agents/:agent_id/stop", post(agents::stop_agent))
        .route("/projects/:id/agents/:agent_id/deploy", post(agents::deploy_agent))
        .route("/projects/:id/agents/:agent_id/model", put(agents::update_agent_model))

        // ── Agent Skills ────────────────────────────────────────────────
        .route("/skills/builtins", get(agent_skills::list_builtin_skills))
        .route("/projects/:id/skills", get(agent_skills::list_skills).post(agent_skills::create_skill))
        .route("/projects/:id/skills/:skill_id", put(agent_skills::update_skill).delete(agent_skills::delete_skill))
        .route("/projects/:id/agents/:agent_id/skills", get(agent_skills::list_agent_skills).post(agent_skills::assign_skill))
        .route("/projects/:id/agents/:agent_id/skills/:skill_id", delete(agent_skills::unassign_skill))
        .route("/projects/:id/agents/:agent_id/prompt", get(agent_skills::preview_agent_prompt))

        // ── Workflows ─────────────────────────────────────────────────────
        .route(
            "/projects/:id/workflows",
            get(workflows::list_runs).post(workflows::run_workflow),
        )
        .route("/projects/:id/workflows/:run_id", get(workflows::get_run))
        .route("/projects/:id/workflows/:run_id/steps", get(workflows::list_run_steps))
        .route("/projects/:id/workflows/:run_id/cancel", post(workflows::cancel_run))

        // ── Portal ────────────────────────────────────────────────────────
        .route("/projects/:id/portal", get(portal::get_portal))
        .route("/projects/:id/portal/publish", post(portal::publish_portal))
        .route("/portal/:slug", get(portal::serve_portal_index))
        .route("/portal/:slug/:file", get(portal::serve_portal_asset))

        // ── Vault ─────────────────────────────────────────────────────────
        .route(
            "/projects/:id/vault",
            get(vault::list_vault).post(vault::set_vault),
        )
        .route(
            "/projects/:id/vault/:key",
            get(vault::get_vault).post(vault::set_vault_by_key).delete(vault::delete_vault),
        )

        // ── Metrics & feedback ───────────────────────────────────────────
        .route("/projects/:id/metrics", get(metrics::list_run_metrics))
        .route(
            "/projects/:id/feedback",
            get(metrics::list_feedback).post(metrics::add_feedback),
        )
        .route("/projects/:id/signals", get(metrics::detect_signals))
        .route("/runs/:run_id/metrics", get(metrics::list_step_metrics))

        // ── Planner (plan-before-code) ─────────────────────────────────
        .route("/projects/:id/plan/generate", post(planner::generate_plan))
        .route("/projects/:id/plan/approve", post(planner::approve_plan))
        .route("/projects/:id/plan", get(planner::get_plan))

        // ── Coding agent (parallel tasks) ────────────────────────────────
        .route("/projects/:id/coding/tasks", get(coding::list_tasks).post(coding::create_task))
        .route("/projects/:id/coding/tasks/batch", post(coding::create_batch))
        .route("/projects/:id/coding/tasks/:task_id", get(coding::get_task).delete(coding::delete_task))

        // ── Code generation ─────────────────────────────────────────────
        .route("/projects/:id/codegen/plan", post(codegen::create_plan))
        .route("/projects/:id/codegen/generate", post(codegen::generate))

        // ── App runner ──────────────────────────────────────────────────
        .route("/projects/:id/app/start", post(app_runner::start_app))
        .route("/projects/:id/app/stop", post(app_runner::stop_app))
        .route("/projects/:id/app/restart", post(app_runner::restart_app))
        .route("/projects/:id/app/status", get(app_runner::app_status))
        .route("/projects/:id/app/instances", get(app_runner::list_instances))
        .route("/projects/:id/app/logs", get(app_runner::app_logs))
        .route("/projects/:id/app/logs/stream", get(app_runner::stream_logs))
        .route("/projects/:id/app/build-steps", get(app_runner::get_build_steps))
        .route("/projects/:id/app/auto-restart", post(app_runner::toggle_auto_restart))
        .route("/projects/:id/app/env", get(app_runner::list_env_vars).post(app_runner::set_env_var))
        .route("/projects/:id/app/env/:key", delete(app_runner::delete_env_var))
        .route("/projects/:id/app/files", get(app_runner::list_files))
        .route("/projects/:id/app/files/*path", get(app_runner::read_file).put(app_runner::write_file).delete(app_runner::delete_file))
        .route("/projects/:id/app/download", get(app_runner::download_zip))
        .route("/projects/:id/app/github", post(app_runner::push_to_github))
        .route("/projects/:id/app/deploy-server", post(app_runner::deploy_to_server))
        .route("/projects/:id/deploy", post(app_runner::deploy_project))

        // ── Agent Loop (Claude Code-style) ─────────────────────────────
        .route("/projects/:id/agent/run", post(agent_run::run_agent))
        .route("/projects/:id/agent/brain", get(agent_run::get_brain))
        .route("/projects/:id/agent/brain/rescan", post(agent_run::rescan_brain))
        .route("/agent/models", get(agent_run::list_agent_models))

        // ── Super Coding Agent System ──────────────────────────────────
        .route("/projects/:id/coding-agent/run", post(coding_agents_handler::run_coding_agent))
        .route("/projects/:id/coding-agent/status", get(coding_agents_handler::get_coding_status))
        .route("/projects/:id/coding-agent/evolve", post(coding_agents_handler::trigger_evolution))
        .route("/projects/:id/coding-agent/history", get(coding_agents_handler::list_executions))

        // ── Wave Pipeline (50+ parallel mini-agents) ──────────────────
        .route("/projects/:id/wave-pipeline/run", post(wave_pipeline_handler::run_wave_pipeline))
        .route("/projects/:id/wave-pipeline/agents", get(wave_pipeline_handler::list_wave_agents))

        // ── Settings ────────────────────────────────────────────────────
        .route("/settings/providers", get(settings::list_providers))
        .route("/settings/api-keys", get(settings::list_api_keys).post(settings::set_api_key))
        .route("/settings/api-keys/:provider", delete(settings::delete_api_key))
        .route("/settings/default-model", get(settings::get_default_model).post(settings::set_default_model))
        .route("/settings/sandbox", get(settings::get_sandbox_settings).post(settings::set_sandbox_settings))

        // ── Explain My App ─────────────────────────────────────────────
        .route("/projects/:id/explain", get(explain::explain_app))

        // ── Intelligence Layer ───────────────────────────────────────────
        .route("/intelligence/predict", post(intelligence_handler::predict))
        .route("/intelligence/explain", post(intelligence_handler::explain_decisions))
        .route("/intelligence/personality/:profile", get(intelligence_handler::get_personality))
        .route("/intelligence/mode", post(intelligence_handler::compute_mode))
        .route("/projects/:id/post-build", get(intelligence_handler::post_build_analysis))
        .route("/projects/:id/improve", post(intelligence_handler::run_improvement))

        // ── Evolution System ─────────────────────────────────────────────
        .route("/evolution/agents/:role/versions", get(evolution_handler::list_agent_versions))
        .route("/evolution/agents/:role/champion", get(evolution_handler::get_champion))
        .route("/evolution/agents/:role/mutate", post(evolution_handler::mutate_agent))
        .route("/evolution/agents/:role/promote", post(evolution_handler::check_promotions))
        .route("/evolution/skills", get(evolution_handler::list_skills))
        .route("/evolution/skills/promote", post(evolution_handler::promote_skills))
        .route("/evolution/scheduler", get(evolution_handler::list_scheduled_tasks))
        .route("/projects/:id/quality-gate", get(evolution_handler::get_quality_gate))
        .route("/projects/:id/quality-gate/run", post(evolution_handler::run_quality_gate))

        // ── Observability ──────────────────────────────────────────────
        .route("/projects/:id/traces", get(observability::list_project_traces))
        .route("/projects/:id/traces/:trace_id/logs", get(observability::get_trace_logs))
        .route("/projects/:id/audit", get(observability::list_audit))
        .route("/projects/:id/evolution/:agent", get(observability::get_evolution))
        .route("/projects/:id/healing", get(observability::list_healing))

        // ── Code Graph ────────────────────────────────────────────────────
        .route("/projects/:id/code-graph", get(code_graph::get_code_graph))
        .route("/projects/:id/code-graph/rebuild", post(code_graph::rebuild_code_graph))
        .route("/projects/:id/code-graph/impact", get(code_graph::impact_analysis))
        .route("/projects/:id/code-graph/symbol", get(code_graph::find_symbol))

        // ── Global Memory ────────────────────────────────────────────────
        .route("/memory", get(global_memory::list_memories).post(global_memory::remember))
        .route("/memory/:key", delete(global_memory::forget))

        // ── Persistent Memory (Episodic + Semantic + Vector) ────────────
        .route("/memory/episodes", post(memory_handler::record_episode))
        .route("/memory/recall", post(memory_handler::recall_episodes))
        .route("/memory/episodes/project/:id", get(memory_handler::project_episode_history))
        .route(
            "/memory/knowledge",
            get(memory_handler::list_knowledge).post(memory_handler::learn_knowledge),
        )
        .route("/memory/knowledge/query", post(memory_handler::query_knowledge))
        .route("/memory/consolidate", post(memory_handler::consolidate))
        .route("/memory/health", get(memory_handler::memory_health))

        // ── Background Agents ────────────────────────────────────────────
        .route("/projects/:id/background-agents", get(background_agents::list_bg_agents).post(background_agents::create_bg_agent))
        .route("/projects/:id/background-agents/:agent_id", post(background_agents::toggle_bg_agent).delete(background_agents::delete_bg_agent))

        // ── Marketplace ──────────────────────────────────────────────────
        .route("/marketplace", get(marketplace::list_marketplace))
        .route("/marketplace/:item_id/install", post(marketplace::install_item))

        // ── Legacy generation endpoints (DEPRECATED — use /oneshot) ────
        // Both routes are kept for backwards compatibility only.
        // They will be removed in a future release.
        // Migrate to POST /oneshot which receives all active development.
        .route("/intent-to-app", post(oneshot::oneshot_stream))
        .route("/pipeline/run", post(intent_to_app::run_pipeline))

        // ── Intent Engine (hybrid deterministic + semantic) ────────────
        // Single canonical handler; intent_to_app::analyze_intent_endpoint removed (duplicate).
        .route("/intent/analyze", post(intent_handler::analyze))
        .route("/intent/clarify", post(intent_handler::clarify))

        // ── Collaboration / Multiplayer ────────────────────────────────
        .route("/projects/:id/collab", post(collaboration::create_session))
        .route("/projects/:id/collab/:session_id/join", post(collaboration::join_session))
        .route("/projects/:id/collab/:session_id/participants", get(collaboration::list_participants))
        .route("/projects/:id/collab/:session_id/activity", get(collaboration::recent_activity).post(collaboration::post_activity))
        .route("/projects/:id/collab/:session_id/presence", get(collaboration::presence_stream))

        // ── Execution Core ───────────────────────────────────────────────
        .route("/projects/:id/execute/validate", post(execution::validate_proposal))
        .route("/projects/:id/execute/apply", post(execution::apply_proposal))

        // ── Invariants ───────────────────────────────────────────────────
        .route("/projects/:id/invariants", get(invariants_handler::check))
        .route("/projects/:id/invariants/fix", post(invariants_handler::auto_fix_endpoint))

        // ── Incremental Mutation ────────────────────────────────────────
        .route("/projects/:id/mutate", post(mutation_handler::mutate))

        // ── Outcome Guarantee (smart, cost-aware, explainable) ───────────
        .route("/projects/:id/guarantee", post(guarantee_handler::run_guarantee))
        .route("/projects/:id/guarantee/certificate", get(guarantee_handler::get_certificate))
        .route("/projects/:id/guarantee/cost-estimate", get(guarantee_handler::cost_estimate))

        // ── Invariant Enforcement ───────────────────────────────────────
        .route("/projects/:id/enforce", get(enforcement::enforce_gate))
        .route("/projects/:id/enforce/rules", get(enforcement::list_enforcement_rules))
        .route("/projects/:id/enforce/config", put(enforcement::update_enforcement_config))

        // ── Smoke Test ───────────────────────────────────────────────────
        .route("/projects/:id/smoke-test", post(smoke_test_handler::run_smoke_test))

        // ── Production Simulation ───────────────────────────────────────
        .route("/projects/:id/simulate", post(production_sim_handler::simulate))

        // ── Live Incremental Updates ────────────────────────────────────
        .route("/projects/:id/live-update", post(live_update_handler::apply_update))
        .route("/projects/:id/live-update/history", get(live_update_handler::list_updates))
        .route("/projects/:id/live-update/:update_id/rollback", post(live_update_handler::rollback_update))

        // ── User Simulation Testing ─────────────────────────────────────
        .route("/projects/:id/sim-test/journeys", get(user_sim_handler::list_journeys))
        .route("/projects/:id/sim-test/run", post(user_sim_handler::run_tests))

        // ── Agent Orchestrator ──────────────────────────────────────────
        .route("/projects/:id/orchestrate", post(orchestrator_handler::orchestrate))
        .route("/projects/:id/orchestrate/decompose", post(orchestrator_handler::decompose_task))

        // ── Cost Intelligence ───────────────────────────────────────────
        .route("/costs/summary", get(cost_handler::daily_summary))
        .route("/costs/budget", get(cost_handler::budget_status).post(cost_handler::set_budget))
        .route("/costs/recommendations", get(cost_handler::recommendations))
        .route("/costs/model-route", get(cost_handler::model_route))

        // ── Human-in-the-Loop ───────────────────────────────────────────
        .route("/projects/:id/gates", get(hitl_handler::list_gates))
        .route("/projects/:id/gates/pending", get(hitl_handler::pending_gates))
        .route("/projects/:id/gates/resolve", post(hitl_handler::resolve_gate))

        // ── Guarantee Certificates ────────────────────────────────────────
        .route("/projects/:id/certificates", get(guarantee_handler::list_certificates))

        // ── Perceived Speed ─────────────────────────────────────────────
        .route("/projects/:id/speed/estimate", get(speed_handler::estimate_pipeline))
        .route("/projects/:id/speed/skeletons", post(speed_handler::generate_skeletons))

        // ── Super Agents (Performance Optimization) ──────────────────────
        .route("/super-agents/status", get(super_agents_handler::get_status))
        .route("/super-agents/history", get(super_agents_handler::get_history))
        .route("/super-agents/metrics", get(super_agents_handler::get_metrics))
        .route("/super-agents/agents", get(super_agents_handler::list_agents))
        .route("/super-agents/mode", post(super_agents_handler::set_mode))
        .route("/super-agents/pause", post(super_agents_handler::set_pause))
        .route("/super-agents/trigger", post(super_agents_handler::trigger_agent))

        // Plugin routes live under admin_routes() to require SystemAdmin scope.

        // ── Taste Engine (pipeline-based scoring + simple scoring) ────────
        .route("/projects/:id/taste/score", post(taste_handler::score_taste_full))
        .route("/projects/:id/taste/profile", post(taste_handler::set_taste_profile))
        .route("/projects/:id/taste/report", get(taste_handler::get_taste_report))
        .route("/projects/:id/taste", get(taste_handler::score_taste))
        .route("/projects/:id/taste/redesign", post(taste_handler::redesign_taste))
        .route("/projects/:id/taste/rewrite-copy", post(taste_handler::rewrite_copy))
        .route("/projects/:id/taste/history", get(taste_handler::taste_history))
        .route("/projects/:id/decisions", get(taste_handler::get_decisions))

        // ── Intent Analysis (with decisions + plugins) ───────────────────
        .route("/intent/analyze-full", post(taste_handler::analyze_with_decisions))

        // ── Decision Learning ────────────────────────────────────────────
        .route("/decisions/feedback", post(decision_learning_handler::record_feedback))
        .route("/decisions/feedback/batch", post(decision_learning_handler::record_feedback_batch))
        .route("/decisions/weights", get(decision_learning_handler::get_weights))
        .route("/decisions/learning", get(decision_learning_handler::get_learning_summary))
        .route("/decisions/recommend", get(decision_learning_handler::recommend_for_area))

        // ── Decision Engine (explainable + coherence) ────────────────────
        .route("/projects/:id/decisions/explain", get(decision_handler::explain_decisions))
        .route("/projects/:id/decisions/override", post(decision_handler::override_decision))

        // ── Product Engine ────────────────────────────────────────────────
        .route("/product/brief", post(product_engine_handler::generate_brief))
        .route("/product/brief/prompt", post(product_engine_handler::generate_brief_prompt))
        .route("/product/personas", post(product_engine_handler::generate_personas))
        .route("/product/monetization", post(product_engine_handler::generate_monetization))
        .route("/product/onboarding", post(product_engine_handler::generate_onboarding))
        .route("/product/retention", post(product_engine_handler::generate_retention))
        .route("/product/features", post(product_engine_handler::prioritize_features))
        .route("/product/landing-copy", post(product_engine_handler::generate_landing_copy))

        // ── Live Build Mode ───────────────────────────────────────────────
        .route("/projects/:id/live-build/stream", get(live_build_handler::stream_build_events))
        .route("/projects/:id/agent-tv/stream", get(agent_tv::agent_tv_stream))
        .route("/projects/:id/live-build/status", get(live_build_handler::get_build_status))

        // ── Agent TV persistent replay + public gallery (NEXUS_MASTER_PLAN §4) ──
        .route("/tv", get(agent_tv::list_public_runs))
        .route("/tv/:runId", get(agent_tv::replay_run))
        .route("/tv/:runId/embed", get(agent_tv::embed_oembed))
        .route("/tv/:runId/embed.html", get(agent_tv::embed_html))

        // ── Swarm runtime (NEXUS_MASTER_PLAN §2) ─────────────────────────
        .route("/projects/:id/swarm/run", post(swarm::run_swarm))

        // ── Nexus Forge — free community marketplace (§9) ────────────────
        .route("/projects/:id/forge/:listingId/install", post(forge::install))
        .route("/projects/:id/forge/:listingId/review", post(forge::review))
        .route("/forge/trending", get(forge::trending))
        .route("/forge/top-rated", get(forge::top_rated))
        .route("/forge/newest", get(forge::newest))

        // ── Trust Certificate v1 (NEXUS_MASTER_PLAN §7) ──────────────────
        .route("/.well-known/nexus-trust.json", get(trust_cert::well_known_trust))
        .route("/trust/cert/:runId", get(trust_cert::get_cert))
        .route("/trust/verify", post(trust_cert::verify))

        // ── Borrowed Agents federation (NEXUS_MASTER_PLAN §10) ───────────
        .route(
            "/federation/borrowable-agents",
            get(borrowed_agents_handler::list_borrowable_agents),
        )
        .route("/federation/borrow", post(borrowed_agents_handler::borrow))
        .route(
            "/federation/verify-receipt",
            post(borrowed_agents_handler::verify),
        )

        // ── User Learning System ──────────────────────────────────────────
        .route("/learning/profile", get(user_learning_handler::get_profile))
        .route("/learning/context", get(user_learning_handler::get_context))
        .route("/learning/preferences", post(user_learning_handler::set_preference))
        .route("/learning/observe", post(user_learning_handler::observe_preference))
        .route("/learning/decay", post(user_learning_handler::trigger_decay))
        .route("/learning/preferences/:category/:key", delete(user_learning_handler::delete_preference))

        // ── Unified Learning Memory ────────────────────────────────────────
        .route("/learning/signal", post(learning_handler::record_signal))
        .route("/learning/insights", get(learning_handler::get_insights))
        .route("/learning/best-choice", get(learning_handler::get_best_choice))
        .route("/learning/style", get(learning_handler::get_style))
        .route("/learning/quality-baseline", get(learning_handler::get_quality_baseline))

        // ── One-Shot Perfection Mode ──────────────────────────────────────
        .route("/oneshot", post(oneshot::oneshot_stream))
        .route("/oneshot/sync", post(oneshot::oneshot_sync))
        .route("/projects/:id/oneshot/start", post(oneshot::oneshot_stream_for_project))

        // ── Nexus Brain (unified intelligence) ──────────────────────────
        .route("/brain/analyze", post(brain_handler::analyze))
        .route("/brain/agents", post(brain_handler::design_agents))
        .route("/brain/hidden-requirements", post(brain_handler::hidden_requirements))
        .route("/projects/:id/taste-gate", post(brain_handler::run_taste_gate))

        // ── Living System (adaptive runtime, predictions, autonomous) ────
        .route("/living/predict", post(living_system_handler::predict_next))
        .route("/living/route-model", post(living_system_handler::route_model))
        .route("/living/runtime", get(living_system_handler::runtime_snapshot))
        .route("/projects/:id/autonomous", post(living_system_handler::run_autonomous))
        .route("/projects/:id/intelligence", get(living_system_handler::get_project_intelligence))

        // ── Control Plane (safety + trust) ──────────────────────────────
        .route("/control/mode", post(observability::set_control_mode))
        .route("/control/status", get(observability::control_status))
        .route("/control/breaker/trip", post(observability::trip_breaker))
        .route("/control/breaker/reset", post(observability::reset_breaker))

        // ── Feedback Engine ──────────────────────────────────────────────
        .route("/feedback", post(trust_handler::submit_feedback))

        // ── Runtime Testing ──────────────────────────────────────────────
        .route("/projects/:id/test", post(trust_handler::run_runtime_tests))

        // ── Unified Memory ──────────────────────────────────────────────
        .route("/memory/unified", get(trust_handler::get_unified_memory))
        .route("/memory/decay", post(trust_handler::decay_memory))
        .route("/projects/:id/memory", get(trust_handler::get_project_memory))

        // ── Plugin Installation (real) ──────────────────────────────────
        .route("/projects/:id/plugins/install", post(trust_handler::install_plugin))
        .route("/projects/:id/plugins/uninstall/:plugin_id", post(trust_handler::uninstall_plugin))

        // ── Collective Intelligence (Agent Council) ──────────────────────
        .route("/council/deliberate", post(evolution_phase_handler::council_deliberate))

        // ── Anticipation Engine ──────────────────────────────────────────
        .route("/anticipate", post(evolution_phase_handler::anticipate))

        // ── Prompt Evolution ─────────────────────────────────────────────
        .route("/prompts/evolve", post(evolution_phase_handler::evolve_prompt))
        .route("/prompts/performance", get(evolution_phase_handler::prompt_performance))

        // ── Causal Learning ──────────────────────────────────────────────
        .route("/causal/graph", get(intelligence_layer_handler::get_causal_graph))
        .route("/causal/overrides", get(intelligence_layer_handler::get_causal_overrides))

        // ── Strategy Engine ──────────────────────────────────────────────
        .route("/projects/:id/strategy", post(intelligence_layer_handler::get_strategy))
        .route("/projects/:id/strategy/next", post(intelligence_layer_handler::next_best_feature))

        // ── Runtime Observer ─────────────────────────────────────────────
        .route("/projects/:id/observe", post(intelligence_layer_handler::record_runtime_event))
        .route("/projects/:id/health", get(intelligence_layer_handler::runtime_health))

        // ── Community Marketplace (contributions) ──────────────────────────
        .route("/marketplace/community/search", get(marketplace::search_contributions))
        .route("/marketplace/community/featured", get(marketplace::featured))
        .route("/marketplace/community/contributions/:id", get(marketplace::get_contribution).delete(marketplace::uninstall_contribution))
        .route("/marketplace/community/contributions/:id/install", post(marketplace::install_contribution))
        .route("/marketplace/community/validate", post(marketplace::validate_contribution))
        // Pkg-registry marketplace — read-only routes are tenant-safe.
        // INSTALL/UNINSTALL move to admin_routes() because they fetch and
        // execute arbitrary code from the configured registry; allowing every
        // authed user to trigger a tarball install is a cross-tenant code
        // injection vector (next tenant to use that package name runs the
        // attacker's code).
        .route("/marketplace/search", get(marketplace::pkg_search))
        .route("/marketplace/installed", get(marketplace::pkg_list_installed))
        .route("/marketplace/categories", get(marketplace::pkg_categories))
        .route("/marketplace/packages/:name", get(marketplace::pkg_get))
        .route("/marketplace/packages/:name/reviews", post(marketplace::pkg_submit_review))

        // ── Security (API keys + Audit log) ──────────────────────────────
        .route("/auth/keys", post(security_handler::create_key).get(security_handler::list_keys))
        .route("/auth/keys/:id", delete(security_handler::revoke_key))
        .route("/audit/log", get(security_handler::get_audit_log))

        // ── Observability (traces, costs, metrics, health) ────────────────
        .route("/observability/traces", get(observability::list_tenant_traces))
        .route("/observability/traces/:trace_id", get(observability::get_trace))
        .route("/observability/costs", get(observability::cost_dashboard))
        .route("/observability/costs/budget", get(observability::budget_check))
        .route("/metrics", get(observability::prometheus_metrics))
        .route("/observability/health", get(observability::health_detailed))

        // ── Teams (Multi-Agent Collaboration) ─────────────────────────────
        .route("/teams", get(teams_handler::list_teams).post(teams_handler::create_team))
        .route("/teams/templates", get(teams_handler::list_templates))
        .route("/teams/:id", get(teams_handler::get_team).delete(teams_handler::delete_team))
        .route("/teams/:id/run", post(teams_handler::run_team))
        .route(
            "/teams/:id/runs",
            get(teams_handler::list_runs).post(teams_handler::start_run),
        )
        .route("/teams/runs/:run_id", get(teams_handler::get_run))
        .route("/teams/runs/:run_id/result", get(teams_handler::get_run_result))
        .route("/teams/runs/:run_id/stream", get(teams_handler::stream_run))
        .route("/teams/:id/runs/:run_id/inject", post(teams_handler::inject_message))
        .route("/teams/runs/:run_id/pause", post(teams_handler::pause_run))
        .route("/teams/runs/:run_id/resume", post(teams_handler::resume_run))

        // ── MCP, Sandbox, Webhooks are in the admin-gated router below ──

        // ── Integrations (GitHub, DB, Notifications) ──────────────────────
        .route("/integrations/health", get(integrations_handler::integrations_health))
        .route("/integrations/github/repos", get(integrations_handler::github_list_repos))
        .route("/integrations/notifications/send", post(integrations_handler::send_notification))

        // ── Evaluation & Benchmarking ──────────────────────────────────────
        .route("/eval/run", post(eval_handler::run_eval))
        .route("/eval/suites", get(eval_handler::list_suites))
        .route("/eval/results", get(eval_handler::list_results))

        // ── Webhooks are in the admin-gated router below ────────────────

        // ── Self-Improvement ────────────────────────────────────────────
        .route("/self-improvement/learn", post(self_improvement_handler::learn))
        .route("/self-improvement/report", get(self_improvement_handler::report))
        .route("/self-improvement/skills", get(self_improvement_handler::list_skills))
        .route(
            "/self-improvement/suggested-skills",
            get(self_improvement_handler::list_suggested_skills),
        )
        .route("/self-improvement/extract", post(self_improvement_handler::extract_patterns))
        .route("/self-improvement/promote", post(self_improvement_handler::promote_skills))
        .route("/self-improvement/skills/usage", post(self_improvement_handler::record_skill_usage))
        .route("/self-improvement/compose-prompt", post(self_improvement_handler::compose_prompt))
        // Multi-modal agent support (vision, audio, documents)
        .route("/multimodal/analyze", post(multimodal_handler::analyze))
        .route("/multimodal/describe-image", post(multimodal_handler::describe_image))
        .route("/multimodal/transcribe", post(multimodal_handler::transcribe))
        .route("/multimodal/tts", post(multimodal_handler::text_to_speech))
        .route("/multimodal/voice-chat", post(multimodal_handler::voice_chat))
        // App-as-Agent platform
        .route("/app-agents", get(app_agent_handler::list_bindings))
        .route("/projects/:id/app-agents", get(app_agent_handler::list_project_bindings).post(app_agent_handler::attach_agent))
        .route("/projects/:project_id/app-agents/:binding_id", delete(app_agent_handler::detach_agent))
        .route("/projects/:project_id/app-agents/:binding_id/tools", get(app_agent_handler::list_tools))
        .route("/projects/:project_id/app-agents/:binding_id/invoke", post(app_agent_handler::invoke_tool))
        // Edge deployment profile
        .route("/edge/profile", get(|| async {
            axum::Json(serde_json::to_value(crate::edge::current_profile()).unwrap_or_default())
        }))

        // ── Workflow Engine (persistent, resumable, branching) ─────────
        .route("/workflows/start", post(workflows::start_persistent_workflow))
        .route("/workflows", get(workflows::list_persistent_workflows))
        .route("/workflows/:id", get(workflows::get_persistent_workflow))
        .route("/workflows/:id/resume", post(workflows::resume_persistent_workflow))
        .route("/workflows/:id/approve/:step", post(workflows::approve_persistent_step))
        .route("/workflows/:id/cancel", post(workflows::cancel_persistent_workflow))

        // ── Kernel: Process Management ─────────────────────────────────
        .route("/kernel/processes", get(kernel_handler::list_processes).post(kernel_handler::spawn_process))
        .route("/kernel/processes/:pid", get(kernel_handler::get_process))
        .route("/kernel/processes/:pid/suspend", post(kernel_handler::suspend_process))
        .route("/kernel/processes/:pid/resume", post(kernel_handler::resume_process))
        .route("/kernel/processes/:pid/kill", post(kernel_handler::kill_process))

        // ── Kernel: Reactive Agents ──────────────────────────────────────
        .route("/kernel/reactive", get(kernel_handler::list_reactive).post(kernel_handler::register_reactive))
        .route("/kernel/reactive/:name", delete(kernel_handler::unregister_reactive))

        // ── Kernel: Collaboration Sessions (Layer 8) ─────────────────────
        .route("/sessions", post(kernel_handler::create_session))
        .route("/sessions/:id", get(kernel_handler::get_session))
        .route("/sessions/:id/join", post(kernel_handler::join_session))
        .route("/sessions/:id/leave", post(kernel_handler::leave_session))
        .route("/sessions/:id/message", post(kernel_handler::send_message))

        // ── Kernel: Federation (Layer 9) ─────────────────────────────────
        .route("/kernel/federation/peers", get(kernel_handler::list_peers))
        .route("/kernel/federation/connect", post(kernel_handler::connect_peer))
        .route("/kernel/federation/:peer_id", delete(kernel_handler::disconnect_peer))

        // ── Business / CEO Dashboard ────────────────────────────────────
        .route("/projects/:id/business", get(business_handler::get_business_overview))
        .route("/projects/:id/business/teams", get(business_handler::list_business_teams).post(business_handler::create_business_team))
        .route("/projects/:id/business/events", get(business_handler::list_business_events))

        // ── Credits ──────────────────────────────────────────────────────
        .route("/credits", get(credits_handler::get_credits))
        .route("/credits/usage", get(credits_handler::get_credit_usage))
        .route("/credits/spend", post(credits_handler::spend_credits))
        .route("/credits/check", post(credits_handler::check_credits))

        // ── Preferences ─────────────────────────────────────────────────
        .route("/preferences", get(preferences_handler::get_preferences).put(preferences_handler::update_preferences))
        .route("/preferences/onboarding", post(preferences_handler::complete_onboarding_step))
        .route("/preferences/onboarding/complete", post(preferences_handler::complete_onboarding))

        // ── Templates ───────────────────────────────────────────────────
        .route("/templates", get(templates_handler::list_templates))
        .route("/templates/:id", get(templates_handler::get_template))
        .route("/templates/:id/use", post(templates_handler::use_template))

        // ── Governance — read + utility endpoints (auth required) ────────
        // Mutating governance endpoints (POST policies, DELETE policies,
        // POST kill-switch) are gated behind SystemAdmin in admin_routes().
        // Without that, any tenant could halt every agent platform-wide.
        .route("/governance/policies", get(governance_handler::list_policies))
        .route("/governance/evaluate", post(governance_handler::evaluate_action))
        .route("/governance/compliance", post(governance_handler::check_compliance))
        .route("/governance/pii/redact", post(governance_handler::redact_pii_endpoint))

        // ── Audit Trail ───────────────────────────────────────────────────
        // SECURITY: GET endpoints are read-only and authenticated via the
        // global auth_middleware. The POST (manual append) endpoint moved to
        // admin_routes() — without that gate any authenticated tenant could
        // forge a `system / kill_switch_activated` entry.
        .route("/audit/chain", get(audit_trail_handler::list_audit_entries))
        .route("/audit/chain/verify", get(audit_trail_handler::verify_audit_chain))
        .route("/audit/pubkey", get(audit_trail_handler::get_pubkey))

        // ── A2A Protocol ─────────────────────────────────────────────────
        .route("/.well-known/agent.json", get(a2a_handler::agent_card))
        .route("/a2a", post(a2a_handler::dispatch))
        .route("/a2a/peers", get(a2a_handler::list_peers).post(a2a_handler::discover_peer))
        .route("/a2a/peers/:url", delete(a2a_handler::remove_peer))
        // Federation — cross-instance Nexus delegation
        .route("/a2a/federation/peers", get(a2a_handler::federation_list_peers).post(a2a_handler::federation_register_peer))
        .route("/a2a/federation/peers/:id", delete(a2a_handler::federation_remove_peer))
        .route("/a2a/federation/delegate", post(a2a_handler::federation_delegate))
        .route("/a2a/federation/health-check", post(a2a_handler::federation_health_check))

        // ── Privileged routes (require SystemAdmin scope) ────────────────
        .merge(admin_routes())

        // ── Health ────────────────────────────────────────────────────────
        .route("/health", get(health))
        .route("/health/ready", get(crate::health_checks::readiness))
        .route("/health/live", get(crate::health_checks::liveness))
        .route("/health/detailed", get(crate::health_checks::detailed_health))

        // Auth middleware: validates JWT/API-key on every request except PUBLIC_PATHS + OPTIONS.
        // In dev mode (NEXUS_AUTH_DISABLED=true), injects a default dev AuthContext.
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::security::auth::auth_middleware,
        ))
        // Global per-IP rate limiter — runs *before* auth so abusive
        // unauthenticated traffic cannot flood the JWT verifier.
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::rate_limiter::rate_limit_middleware,
        ))
        .layer(cors)
        // NOTE(audit): CatchPanicLayer converts handler panics into clean 500 JSON
        // responses instead of connection resets.
        .layer(CatchPanicLayer::custom(|_| {
            axum::response::IntoResponse::into_response((
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({"error": {"code": "INTERNAL", "message": "An internal error occurred"}})),
            ))
        }))
        // Cap request body size across the whole router. Anything larger is
        // rejected with 413 before the handler runs.
        .layer(DefaultBodyLimit::max(BODY_LIMIT_BYTES))
        // HTTP-level metrics — count + latency histogram per method/status.
        // Runs on every request; `/metrics` exposes the aggregated values.
        .layer(axum::middleware::from_fn(
            crate::http_metrics::track_request,
        ))
        // Tell nginx / Caddy / CloudFront NOT to buffer SSE streams. Without
        // this header, reverse proxies buffer tokens for seconds and the
        // user sees generation hang until the buffer flushes. The header is
        // safe on non-SSE responses (clients ignore it).
        .layer(SetResponseHeaderLayer::if_not_present(
            HeaderName::from_static("x-accel-buffering"),
            HeaderValue::from_static("no"),
        ))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(TraceLayer::new_for_http())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .with_state(state)
}

/// Routes requiring `SystemAdmin` scope (sandbox, MCP, webhooks).
///
/// These get the base `auth_middleware` from the parent router AND an additional
/// `require_admin_middleware` layer that checks for the `SystemAdmin` scope.
/// Non-admin tokens receive 403 on these endpoints.
fn admin_routes() -> Router<Arc<AppState>> {
    Router::new()
        // Sandbox
        .route("/sandbox/create", post(sandbox_handler::create_sandbox))
        .route("/sandbox/:id/exec", post(sandbox_handler::exec_in_sandbox))
        .route(
            "/sandbox/:id/destroy",
            post(sandbox_handler::destroy_sandbox),
        )
        .route("/sandbox/:id/logs", get(sandbox_handler::get_sandbox_logs))
        // WASM sandbox
        .route("/sandbox/wasm/exec", post(sandbox_handler::exec_wasm))
        // MCP
        .route(
            "/mcp/servers",
            post(mcp_handler::register_server).get(mcp_handler::list_servers),
        )
        .route(
            "/mcp/servers/:id/connect",
            post(mcp_handler::connect_server),
        )
        .route(
            "/mcp/servers/:id/tools",
            get(mcp_handler::list_server_tools),
        )
        .route("/mcp/servers/:id/tools/call", post(mcp_handler::call_tool))
        .route("/mcp/servers/:id", delete(mcp_handler::remove_server))
        .route("/mcp/tools", get(mcp_handler::list_all_tools))
        // MCP Proxy and auto-discovery
        .route("/mcp/proxy/call", post(mcp_handler::proxy_call))
        .route("/mcp/discover", post(mcp_handler::discover_local))
        // Webhooks
        .route(
            "/webhooks",
            post(webhook_handler::register_webhook).get(webhook_handler::list_webhooks),
        )
        .route("/webhooks/:id", delete(webhook_handler::delete_webhook))
        .route("/webhooks/:id/test", post(webhook_handler::test_webhook))
        // Governance mutators — kill-switch halts ALL agents across ALL tenants;
        // policy CRUD is platform-wide. Both must be SystemAdmin only.
        .route(
            "/governance/policies",
            post(governance_handler::upsert_policy),
        )
        .route(
            "/governance/policies/:id",
            delete(governance_handler::delete_policy),
        )
        .route(
            "/governance/kill-switch",
            post(governance_handler::kill_switch),
        )
        // Marketplace install/uninstall — arbitrary code execution surface.
        .route("/marketplace/install", post(marketplace::pkg_install))
        .route("/marketplace/uninstall", post(marketplace::pkg_uninstall))
        // Plugins (install / enable / disable / import / export — arbitrary code exec surface)
        .route("/plugins", get(plugins::list_plugins))
        .route("/plugins/import", post(plugins::import_plugin))
        .route("/plugins/:id/enable", post(plugins::enable_plugin))
        .route("/plugins/:id/disable", post(plugins::disable_plugin))
        .route("/plugins/:id/export", get(plugins::export_plugin))
        .route("/plugins/:id", delete(plugins::uninstall_plugin))
        .route("/plugins/install", post(plugin_handler::install_plugin))
        .route("/plugins/all", get(plugin_handler::list_plugins))
        .route("/plugins/validate", post(plugin_handler::validate_plugin))
        .route(
            "/plugins/hooks/available",
            get(plugin_handler::list_available_hooks),
        )
        .route("/plugins/:id/hooks", get(plugin_handler::get_plugin_hooks))
        .route(
            "/projects/:id/plugins/execution-log",
            get(plugin_handler::get_execution_log),
        )
        // Audit chain manual append — forge surface; admin-only.
        .route(
            "/audit/chain",
            post(audit_trail_handler::append_audit_entry),
        )
        // Admin-only middleware layer
        .layer(axum::middleware::from_fn(
            crate::security::auth::require_admin_middleware,
        ))
}

fn cors_methods_and_headers() -> ([axum::http::Method; 5], [axum::http::HeaderName; 5]) {
    (
        [
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PUT,
            axum::http::Method::DELETE,
            axum::http::Method::OPTIONS,
        ],
        [
            axum::http::header::ACCEPT,
            axum::http::header::ACCEPT_LANGUAGE,
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
            axum::http::HeaderName::from_static("x-request-id"),
        ],
    )
}

fn cors_layer_permissive(
    methods: [axum::http::Method; 5],
    headers: [axum::http::HeaderName; 5],
) -> CorsLayer {
    // Wildcard origins and credentialed requests are mutually exclusive per the
    // CORS spec and in tower-http. Force allow_credentials(false) explicitly so
    // that a future refactor cannot silently combine `Any` with credentials.
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(methods)
        .allow_headers(headers)
        .allow_credentials(false)
        .max_age(std::time::Duration::from_secs(3600))
}

/// Build CORS layer from environment configuration.
///
/// Reads `NEXUS_ALLOWED_ORIGINS` (comma-separated). If unset or empty, allows any origin
/// and logs a warning so operators can set an explicit list for production. A value of
/// `*` also allows any origin (no warning). Methods and headers are always applied.
fn build_cors_layer() -> CorsLayer {
    let (methods, headers) = cors_methods_and_headers();
    // In release builds we refuse to fall back to `allow_origin(Any)` silently;
    // operators must opt in explicitly with `NEXUS_ALLOWED_ORIGINS=*`.
    let release = !cfg!(debug_assertions);

    match std::env::var("NEXUS_ALLOWED_ORIGINS") {
        Err(_) => {
            if release {
                warn!(
                    "NEXUS_ALLOWED_ORIGINS is not set in a release build; defaulting to a locked-down CORS policy with no allowed origins. Set NEXUS_ALLOWED_ORIGINS or '*' to allow cross-origin access."
                );
                return CorsLayer::new()
                    .allow_methods(methods)
                    .allow_headers(headers)
                    .max_age(std::time::Duration::from_secs(3600));
            }
            warn!(
                "NEXUS_ALLOWED_ORIGINS is not set; CORS allows any origin (debug only). Set NEXUS_ALLOWED_ORIGINS to a comma-separated list of allowed origins to restrict cross-origin access."
            );
            cors_layer_permissive(methods, headers)
        }
        Ok(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                if release {
                    warn!(
                        "NEXUS_ALLOWED_ORIGINS is empty in a release build; defaulting to a locked-down CORS policy. Set NEXUS_ALLOWED_ORIGINS or '*' to allow cross-origin access."
                    );
                    return CorsLayer::new()
                        .allow_methods(methods)
                        .allow_headers(headers)
                        .max_age(std::time::Duration::from_secs(3600));
                }
                warn!("NEXUS_ALLOWED_ORIGINS is empty; CORS allows any origin (debug only).");
                return cors_layer_permissive(methods, headers);
            }
            if trimmed == "*" {
                return cors_layer_permissive(methods, headers);
            }

            // Fail-closed: parse every comma-separated entry. If any token is
            // unparseable we refuse to boot in release rather than silently
            // dropping it (which previously could turn one typo into a
            // credentialed CORS bypass when the rest of the list looked fine).
            let mut origins: Vec<axum::http::HeaderValue> = Vec::new();
            let mut bad_tokens: Vec<String> = Vec::new();
            for raw_token in trimmed
                .split(',')
                .map(|t| t.trim())
                .filter(|t| !t.is_empty())
            {
                match raw_token.parse::<axum::http::HeaderValue>() {
                    Ok(v) => origins.push(v),
                    Err(_) => bad_tokens.push(raw_token.to_string()),
                }
            }

            if !bad_tokens.is_empty() {
                if release {
                    panic!(
                        "NEXUS_ALLOWED_ORIGINS contained unparseable tokens {bad_tokens:?}. \
                         Refusing to boot a release build with a malformed CORS allow-list."
                    );
                }
                warn!(
                    ?bad_tokens,
                    "NEXUS_ALLOWED_ORIGINS contained unparseable tokens (debug only)"
                );
            }

            if origins.is_empty() {
                if release {
                    panic!(
                        "NEXUS_ALLOWED_ORIGINS contained no valid origins. \
                         Set NEXUS_ALLOWED_ORIGINS to a comma-separated list, or set it to '*' \
                         to explicitly allow any origin."
                    );
                }
                warn!(
                    "NEXUS_ALLOWED_ORIGINS contained no valid origins; CORS allows any origin (debug only)."
                );
                return cors_layer_permissive(methods, headers);
            }

            // Credentialed CORS is opt-in: an explicit allow-list of origins
            // PLUS NEXUS_CORS_CREDENTIALS=true. Without the env var we serve
            // the allow-list with credentials disabled — a typo that adds an
            // attacker-controlled origin still cannot carry tenant cookies.
            let credentials = std::env::var("NEXUS_CORS_CREDENTIALS")
                .map(|v| v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);

            CorsLayer::new()
                .allow_origin(origins)
                .allow_methods(methods)
                .allow_headers(headers)
                .allow_credentials(credentials)
                .max_age(std::time::Duration::from_secs(3600))
        }
    }
}

async fn health(
    axum::extract::State(state): axum::extract::State<Arc<crate::state::AppState>>,
) -> axum::Json<serde_json::Value> {
    let uptime_secs = state.started_at.elapsed().as_secs();
    axum::Json(serde_json::json!({
        "status": "ok",
        "service": "nexus-http",
        "uptime_secs": uptime_secs,
    }))
}
