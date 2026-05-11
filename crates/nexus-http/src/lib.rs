//! `nexus-http` — Axum HTTP server exposing all Nexus capabilities.
//!
//! # Usage
//! ```rust,ignore
//! use nexus_http::AppState;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let state = AppState::init("~/.nexus").await?;
//!     nexus_http::serve(state, "0.0.0.0:8080").await
//! }
//! ```

pub mod agents;
pub mod agent_routing;
pub mod budget_brake;
pub mod agent_tv_sink;
pub mod adaptive_control;
pub mod anthropic_cache;
pub mod borrowed_agents;
pub mod claude_md;
pub mod claude_md_injector;
pub mod forge_reputation;
pub mod adaptive_runtime;
pub mod agent_designer;
pub mod global_intelligence;
pub mod intelligence_amplifier;
pub mod runtime_feedback;
pub mod variant_engine;
pub mod anticipation_engine;
pub mod autonomous_engine;
pub mod agent_loop;
pub mod agent_orchestrator;
pub mod agent_tools;
pub mod agent_versioning;
pub mod background_executor;
pub mod cache;
pub mod causal_learning;
pub mod code_graph;
pub mod coding_agents;
pub mod collective_intelligence;
pub mod continuous_improve;
pub mod control_plane;
pub mod decision_engine;
pub mod decision_learning;
pub mod design_system;
pub mod error;
pub mod evolution;
pub mod execution_core;
pub mod execution_pipeline;
pub mod generation_event;
pub mod pipeline_engine;
pub mod explain_engine;
pub mod feedback_engine;
pub mod file_utils;
pub mod handlers;
pub mod hitl;
pub mod http_metrics;
pub mod input_limits;
pub mod intent_engine;
pub mod intent_engine_semantic;
pub mod invariant_enforcer;
pub mod invariants;
pub mod live_update;
pub mod llm_client;
pub mod llm_model_defaults;
pub mod log_redact;
pub mod memory_unification;
pub mod mini_agents;
pub mod model_router;
pub mod mutation_engine;
pub mod trust;
pub mod nexus_brain;
pub mod nexus_intelligence;
pub mod nexus_types;
pub mod outcome_guarantee;
pub mod perceived_speed;
pub mod personality;
pub mod pipeline_turbo;
pub mod plugin_hooks;
pub mod plugin_installer;
pub mod plugin_system;
pub mod post_build_intel;
pub mod predictive_intent;
pub mod prompt_evolution;
pub mod predictive_preprocess;
pub mod product_engine;
pub mod production_sim;
pub mod project_brain;
pub mod cost_intelligence;
pub mod quality_gate;
pub mod rate_limiter;
pub mod security;
pub mod runtime_observer;
pub mod runtime_tester;
pub mod self_healing;
pub mod self_improvement;
pub mod skill_dna;
pub mod skill_runtime;
pub mod system_scheduler;
pub mod server;
pub mod state;
pub mod strategy_engine;
pub mod super_agents;
pub mod taste_engine;
pub mod taste_gate;
pub mod taste_redesign;
pub mod thinking_stream;
pub mod learning_memory;
pub mod user_learning;
pub mod user_pattern_detector;
pub mod user_sim;
pub mod observability;
pub mod plugin_sandbox;
pub mod team_event_sink;
pub mod team_orchestrator;
pub mod team_prompting;
pub mod team_templates;
pub mod marketplace;
pub mod graceful_shutdown;
pub mod health_checks;
pub mod smoke_test;
pub mod webhooks;
pub mod self_improvement_engine;
pub mod conversation_engine;
pub mod business_teams;
pub mod app_agent;
pub mod edge;
pub mod governance;
pub mod multimodal;
pub mod telemetry;

pub use state::AppState;

use std::sync::Arc;

/// Start the Nexus HTTP server on the given address.
pub async fn serve(state: Arc<AppState>, addr: &str) -> anyhow::Result<()> {
    use std::net::SocketAddr;

    // Start the Super Agents performance optimization system
    let orchestrator = super_agents::start_super_agents(
        state.clone(),
        state.metrics_bus.clone(),
    );
    let _ = state.orchestrator.set(orchestrator);

    // Start the Background Agent Executor (runs scheduled agents on intervals)
    let _bg_executor = background_executor::start_background_executor(state.clone());

    // Start the System Scheduler (priority-aware task scheduling)
    let _scheduler = system_scheduler::start_scheduler(state.clone());

    // Periodic janitor: evict expired entries from caches and rate limiter
    // so the in-memory footprint can't grow monotonically. Runs every 60s.
    {
        let state_for_janitor = state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            interval.tick().await; // skip immediate first tick
            loop {
                interval.tick().await;
                let cache_evicted = state_for_janitor.llm_cache.evict_expired();
                let ip_evicted = state_for_janitor.rate_limiter.ip_limiter.evict_expired();
                if cache_evicted + ip_evicted > 0 {
                    tracing::debug!(
                        cache_evicted,
                        ip_evicted,
                        "periodic janitor swept expired entries"
                    );
                }
            }
        });
    }

    // Register A2A local agent card
    {
        let base_url = std::env::var("NEXUS_PUBLIC_URL")
            .unwrap_or_else(|_| format!("http://{}", addr));
        let card = handlers::a2a_handler::build_default_card(&base_url);
        state.a2a_registry.set_local(card).await;
    }

    let shutdown_state = state.clone();
    let router = server::create_router(state);
    let addr: SocketAddr = addr.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;

    tracing::info!("Nexus HTTP server listening on http://{}", addr);
    // Passing `into_make_service_with_connect_info::<SocketAddr>()` makes the
    // client's remote address available to middleware (via `ConnectInfo`),
    // which the rate limiter needs to key per-IP.
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(graceful_shutdown::shutdown_signal())
    .await?;

    // Server has stopped accepting — now tear down subsystems.
    graceful_shutdown::graceful_shutdown(shutdown_state).await;
    Ok(())
}
