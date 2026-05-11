//! Super Agents — autonomous performance optimization system for Nexus.
//!
//! This module implements a fleet of internal system agents that continuously
//! monitor, analyze, and optimize the Nexus platform. Unlike user-facing agents,
//! these run silently in the background and make the system faster, cheaper, and
//! more reliable over time.
//!
//! # Architecture
//!
//! ```text
//!  +-----------------------+
//!  |   MetricsBus          |  <-- all subsystems push metrics here
//!  +-----------+-----------+
//!              |
//!  +-----------v-----------+
//!  | PerformanceOrchestrator|  <-- schedules agents, resolves conflicts
//!  +-----------+-----------+
//!              |
//!    +---------+----------+----------+--- ... ---+
//!    |         |          |          |           |
//!  Agent1   Agent2     Agent3     Agent4     Agent10
//!  (Latency)(Cost)   (Pipeline) (Cache)    (Build)
//!
//!  Each agent runs: Analyze -> Optimize -> Validate -> (Rollback if needed)
//! ```
//!
//! # Agent Lifecycle
//!
//! 1. **Analyze** — scan metrics and produce findings
//! 2. **Optimize** — apply improvements (idempotent, rollback-safe)
//! 3. **Validate** — verify system health after optimization
//! 4. **Rollback** — revert if validation fails
//!
//! # Conflict Resolution
//!
//! Agents declare conflict groups (Pipeline, LlmCalls, Database, etc.).
//! The orchestrator ensures no two agents sharing a group run simultaneously.

pub mod agents;
pub mod metrics_bus;
pub mod orchestrator;
pub mod traits;
pub mod types;

use std::sync::Arc;

use tracing::info;

use crate::state::AppState;
use metrics_bus::MetricsBus;
use orchestrator::PerformanceOrchestrator;

/// Initialize and start the Super Agents system.
///
/// Call this from server startup after `AppState::init`.
/// Returns the `MetricsBus` handle so other subsystems can push metrics.
pub fn start_super_agents(
    app: Arc<AppState>,
    metrics: Arc<MetricsBus>,
) -> Arc<PerformanceOrchestrator> {
    let agents = orchestrator::create_all_agents();
    let orchestrator = Arc::new(PerformanceOrchestrator::new(
        app,
        metrics,
        agents,
    ));

    let orch_clone = orchestrator.clone();
    tokio::spawn(async move {
        orch_clone.run().await;
    });

    info!("Super Agents system started with 10 performance agents");
    orchestrator
}
