//! Core trait that every Super Agent must implement.

use std::sync::Arc;

use async_trait::async_trait;

use super::metrics_bus::MetricsBus;
use super::types::*;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Contexts passed to agent lifecycle methods
// ---------------------------------------------------------------------------

pub struct AnalysisContext {
    pub app: Arc<AppState>,
    pub metrics: Arc<MetricsBus>,
    pub snapshot: SystemSnapshot,
    pub mode: OrchestratorMode,
}

pub struct OptimizationContext {
    pub app: Arc<AppState>,
    pub metrics: Arc<MetricsBus>,
    pub mode: OrchestratorMode,
    pub dry_run: bool,
}

pub struct ValidationContext {
    pub app: Arc<AppState>,
    pub metrics: Arc<MetricsBus>,
    pub snapshot_before: SystemSnapshot,
}

// ---------------------------------------------------------------------------
// The SuperAgent trait
// ---------------------------------------------------------------------------

#[async_trait]
pub trait SuperAgent: Send + Sync + 'static {
    /// Human-readable name.
    fn name(&self) -> &str;

    /// Agent type discriminant.
    fn kind(&self) -> SuperAgentKind;

    /// When this agent should be triggered.
    fn trigger(&self) -> TriggerMode;

    /// Higher priority agents are scheduled first.
    fn priority(&self) -> u8 {
        self.kind().default_priority()
    }

    /// Resource conflict groups — the orchestrator will not run two agents
    /// that share a conflict group at the same time.
    fn conflict_groups(&self) -> &[ConflictGroup] {
        self.kind().conflict_groups()
    }

    /// Maximum risk level this agent can introduce.
    /// `Observe` mode will skip agents that return >= Medium.
    fn risk_level(&self) -> Severity {
        Severity::Low
    }

    /// Phase 1: Analyze the system and produce findings.
    async fn analyze(&self, ctx: &AnalysisContext) -> anyhow::Result<AnalysisReport>;

    /// Phase 2: Apply optimizations for actionable findings.
    /// Implementations MUST be idempotent.
    async fn optimize(
        &self,
        ctx: &OptimizationContext,
        report: &AnalysisReport,
    ) -> anyhow::Result<OptimizationResult>;

    /// Phase 3: Validate that the system is still healthy after optimization.
    async fn validate(&self, ctx: &ValidationContext) -> anyhow::Result<ValidationOutcome>;

    /// Rollback a specific optimization if validation fails.
    async fn rollback(
        &self,
        app: &Arc<AppState>,
        rollback_key: &str,
    ) -> anyhow::Result<()>;
}
