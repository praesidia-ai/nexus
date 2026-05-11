//! Production Observability and Control Plane.
//!
//! Provides structured tracing with span hierarchies, cost tracking with budget
//! enforcement, Prometheus-compatible metrics export, circuit breaker patterns,
//! and a unified health dashboard. Designed for production deployments where
//! fine-grained visibility into generation pipelines, LLM spend, and system
//! health is critical.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::state::AppState;

// ===========================================================================
// 1. Structured Tracing
// ===========================================================================

/// A complete trace representing one generation pipeline execution.
///
/// Traces aggregate multiple [`Span`]s, each corresponding to a discrete phase
/// (intent analysis, code generation, quality gate, etc.). Token usage and cost
/// are rolled up from individual spans.
#[derive(Debug, Clone, Serialize)]
pub struct GenerationTrace {
    /// Unique trace identifier (typically a UUID).
    pub trace_id: String,
    /// The project this trace belongs to.
    pub project_id: String,
    /// The tenant (organization) that owns the project.
    pub tenant_id: String,
    /// ISO-8601 timestamp when the trace started.
    pub started_at: String,
    /// ISO-8601 timestamp when the trace completed (if finished).
    pub completed_at: Option<String>,
    /// Current status of the trace.
    pub status: TraceStatus,
    /// Ordered list of spans within the trace.
    pub spans: Vec<Span>,
    /// Aggregate USD cost across all spans.
    pub total_cost_usd: f32,
    /// Aggregate token usage across all spans.
    pub total_tokens: TokenUsage,
}

/// Status of a generation trace.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum TraceStatus {
    /// The trace is still running.
    InProgress,
    /// The trace completed successfully.
    Completed,
    /// The trace failed with an error.
    Failed,
    /// The trace exceeded its time budget.
    TimedOut,
}

/// A single span within a trace, representing a discrete unit of work.
///
/// Spans form a tree via `parent_id`. Leaf spans typically correspond to LLM
/// calls while intermediate spans group logical phases.
#[derive(Debug, Clone, Serialize)]
pub struct Span {
    /// Unique span identifier.
    pub span_id: String,
    /// Parent span id (None for root spans).
    pub parent_id: Option<String>,
    /// Human-readable name (e.g. "intent_analysis", "code_generation").
    pub name: String,
    /// ISO-8601 timestamp when the span started.
    pub started_at: String,
    /// Duration in milliseconds.
    pub duration_ms: u64,
    /// Arbitrary key-value attributes attached to this span.
    pub attributes: HashMap<String, serde_json::Value>,
    /// Discrete events that occurred during this span.
    pub events: Vec<SpanEvent>,
    /// Terminal status of the span.
    pub status: SpanStatus,
    /// USD cost attributed to this span.
    pub cost_usd: f32,
    /// Token usage if this span involved an LLM call.
    pub tokens: Option<TokenUsage>,
}

/// A point-in-time event within a span (e.g. "retry", "cache_hit").
#[derive(Debug, Clone, Serialize)]
pub struct SpanEvent {
    /// Event name.
    pub name: String,
    /// ISO-8601 timestamp.
    pub timestamp: String,
    /// Additional string attributes.
    pub attributes: HashMap<String, String>,
}

/// Terminal status of a span.
#[derive(Debug, Clone, Serialize)]
pub enum SpanStatus {
    /// Span completed successfully.
    Ok,
    /// Span failed with the given error message.
    Error(String),
}

/// Token usage from a single LLM call.
#[derive(Debug, Clone, Serialize, Default)]
pub struct TokenUsage {
    /// Number of input (prompt) tokens.
    pub input_tokens: u64,
    /// Number of output (completion) tokens.
    pub output_tokens: u64,
    /// Model identifier (e.g. "gpt-4o", "claude-sonnet-4-20250514").
    pub model: String,
    /// Provider name (e.g. "openai", "anthropic").
    pub provider: String,
}

// ===========================================================================
// 2. TraceCollector
// ===========================================================================

/// In-memory collector for generation traces.
///
/// Maintains a map of active and recently completed traces. In a production
/// deployment this would be backed by a persistent store; the in-memory version
/// is suitable for single-node setups and testing.
///
/// A soft cap (`MAX_TRACES`) prevents unbounded memory growth on long-lived
/// servers: when the collector crosses the cap we evict the oldest completed
/// trace. In-progress traces are never evicted.
pub struct TraceCollector {
    /// Map from trace_id to the full trace.
    traces: HashMap<String, GenerationTrace>,
}

const MAX_TRACES: usize = 2_000;

impl Default for TraceCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl TraceCollector {
    /// Create a new empty trace collector.
    pub fn new() -> Self {
        Self {
            traces: HashMap::new(),
        }
    }

    /// Start a new trace and return a mutable reference to it.
    ///
    /// The trace is initialised with `InProgress` status and the current UTC
    /// timestamp.
    pub fn start_trace(
        &mut self,
        trace_id: &str,
        project_id: &str,
        tenant_id: &str,
    ) -> &mut GenerationTrace {
        self.evict_if_needed();
        let trace = GenerationTrace {
            trace_id: trace_id.to_string(),
            project_id: project_id.to_string(),
            tenant_id: tenant_id.to_string(),
            started_at: chrono::Utc::now().to_rfc3339(),
            completed_at: None,
            status: TraceStatus::InProgress,
            spans: Vec::new(),
            total_cost_usd: 0.0,
            total_tokens: TokenUsage::default(),
        };
        self.traces.insert(trace_id.to_string(), trace);
        self.traces.get_mut(trace_id).expect("just inserted")
    }

    /// Drop the oldest completed trace if we are above [`MAX_TRACES`].
    fn evict_if_needed(&mut self) {
        if self.traces.len() < MAX_TRACES {
            return;
        }
        // Find the completed trace with the earliest `started_at`. String
        // comparison on RFC-3339 timestamps is order-preserving, so no
        // parsing needed here.
        let victim = self
            .traces
            .iter()
            .filter(|(_, t)| !matches!(t.status, TraceStatus::InProgress))
            .min_by(|a, b| a.1.started_at.cmp(&b.1.started_at))
            .map(|(k, _)| k.clone());
        if let Some(k) = victim {
            self.traces.remove(&k);
        }
    }

    /// Start a new span within an existing trace.
    ///
    /// The span is created with zero duration and `SpanStatus::Ok` — call
    /// [`end_span`](Self::end_span) when the work completes.
    pub fn start_span(
        &mut self,
        trace_id: &str,
        span_id: &str,
        parent_id: Option<&str>,
        name: &str,
    ) {
        if let Some(trace) = self.traces.get_mut(trace_id) {
            let span = Span {
                span_id: span_id.to_string(),
                parent_id: parent_id.map(|s| s.to_string()),
                name: name.to_string(),
                started_at: chrono::Utc::now().to_rfc3339(),
                duration_ms: 0,
                attributes: HashMap::new(),
                events: Vec::new(),
                status: SpanStatus::Ok,
                cost_usd: 0.0,
                tokens: None,
            };
            trace.spans.push(span);
        }
    }

    /// End a span, recording its final status, cost, and token usage.
    ///
    /// The `duration_ms` is computed from the difference between now and the
    /// span's `started_at` timestamp.
    pub fn end_span(
        &mut self,
        trace_id: &str,
        span_id: &str,
        status: SpanStatus,
        cost_usd: f32,
        tokens: Option<TokenUsage>,
    ) {
        if let Some(trace) = self.traces.get_mut(trace_id) {
            if let Some(span) = trace.spans.iter_mut().find(|s| s.span_id == span_id) {
                let started = chrono::DateTime::parse_from_rfc3339(&span.started_at)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now());
                let elapsed = chrono::Utc::now()
                    .signed_duration_since(started)
                    .num_milliseconds()
                    .max(0) as u64;

                span.duration_ms = elapsed;
                span.status = status;
                span.cost_usd = cost_usd;
                span.tokens = tokens.clone();
            }

            // Roll up totals.
            trace.total_cost_usd += cost_usd;
            if let Some(ref tok) = tokens {
                trace.total_tokens.input_tokens += tok.input_tokens;
                trace.total_tokens.output_tokens += tok.output_tokens;
                // Keep the most recent model/provider on the aggregate.
                if !tok.model.is_empty() {
                    trace.total_tokens.model = tok.model.clone();
                }
                if !tok.provider.is_empty() {
                    trace.total_tokens.provider = tok.provider.clone();
                }
            }
        }
    }

    /// Mark a trace as completed with the given terminal status.
    pub fn complete_trace(&mut self, trace_id: &str, status: TraceStatus) {
        if let Some(trace) = self.traces.get_mut(trace_id) {
            trace.status = status;
            trace.completed_at = Some(chrono::Utc::now().to_rfc3339());
        }
    }

    /// Retrieve a trace by ID.
    pub fn get_trace(&self, trace_id: &str) -> Option<&GenerationTrace> {
        self.traces.get(trace_id)
    }

    /// List traces for a given tenant, most recent first, up to `limit`.
    pub fn list_traces(&self, tenant_id: &str, limit: usize) -> Vec<&GenerationTrace> {
        let mut matching: Vec<&GenerationTrace> = self
            .traces
            .values()
            .filter(|t| t.tenant_id == tenant_id)
            .collect();
        matching.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        matching.truncate(limit);
        matching
    }
}

// ===========================================================================
// 3. Cost Tracker
// ===========================================================================

/// A single cost record representing one LLM call's spend.
#[derive(Debug, Clone, Serialize)]
pub struct CostRecord {
    /// Unique record id.
    pub id: String,
    /// Trace this cost belongs to.
    pub trace_id: String,
    /// Tenant that incurred the cost.
    pub tenant_id: String,
    /// Project context.
    pub project_id: String,
    /// LLM model used.
    pub model: String,
    /// LLM provider.
    pub provider: String,
    /// Input tokens billed.
    pub input_tokens: u64,
    /// Output tokens billed.
    pub output_tokens: u64,
    /// USD cost for this call.
    pub cost_usd: f32,
    /// ISO-8601 timestamp.
    pub timestamp: String,
}

/// Aggregate usage for a tenant over a time window.
#[derive(Debug, Clone, Serialize)]
pub struct TenantUsage {
    /// Total USD spent.
    pub total_cost_usd: f32,
    /// Total tokens consumed.
    pub total_tokens: u64,
    /// Cost broken down by model name.
    pub by_model: HashMap<String, f32>,
    /// Cost broken down by project id.
    pub by_project: HashMap<String, f32>,
}

/// Aggregate usage for a single project.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectUsage {
    /// Total USD spent on this project.
    pub total_cost_usd: f32,
    /// Total tokens consumed.
    pub total_tokens: u64,
    /// Cost broken down by model name.
    pub by_model: HashMap<String, f32>,
}

/// Budget status for a tenant.
#[derive(Debug, Clone, Serialize)]
pub struct BudgetStatus {
    /// Remaining USD before hitting the budget cap.
    pub remaining_usd: f32,
    /// USD already used.
    pub used_usd: f32,
    /// Projected overage based on current burn rate (0.0 if within budget).
    pub projected_overage: f32,
    /// Human-readable recommendation.
    pub recommendation: String,
}

/// Cost tracking backed by SQLite.
///
/// All methods are stateless — they operate directly on the provided database
/// connection, making them safe to call from any async context that holds the
/// `db` lock.
pub struct CostTracker;

impl CostTracker {
    /// Persist a cost record to the `cost_records` table.
    pub fn record(db: &rusqlite::Connection, record: &CostRecord) -> Result<(), String> {
        db.execute(
            "INSERT INTO cost_records (id, trace_id, tenant_id, project_id, model, provider, input_tokens, output_tokens, cost_usd, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                record.id,
                record.trace_id,
                record.tenant_id,
                record.project_id,
                record.model,
                record.provider,
                record.input_tokens,
                record.output_tokens,
                record.cost_usd,
                record.timestamp,
            ],
        )
        .map_err(|e| format!("Failed to insert cost record: {e}"))?;
        Ok(())
    }

    /// Compute aggregate usage for a tenant over the last N days.
    pub fn tenant_usage(
        db: &rusqlite::Connection,
        tenant_id: &str,
        days: u32,
    ) -> TenantUsage {
        let cutoff = chrono::Utc::now()
            .checked_sub_signed(chrono::Duration::days(i64::from(days)))
            .unwrap_or_else(chrono::Utc::now)
            .to_rfc3339();

        let mut total_cost: f32 = 0.0;
        let mut total_tokens: u64 = 0;
        let mut by_model: HashMap<String, f32> = HashMap::new();
        let mut by_project: HashMap<String, f32> = HashMap::new();

        if let Ok(mut stmt) = db.prepare(
            "SELECT model, project_id, input_tokens, output_tokens, cost_usd
             FROM cost_records
             WHERE tenant_id = ?1 AND timestamp >= ?2",
        ) {
            let _ = stmt.query_map(rusqlite::params![tenant_id, cutoff], |row| {
                let model: String = row.get(0)?;
                let project_id: String = row.get(1)?;
                let input: u64 = row.get::<_, i64>(2)? as u64;
                let output: u64 = row.get::<_, i64>(3)? as u64;
                let cost: f32 = row.get::<_, f64>(4)? as f32;
                Ok((model, project_id, input, output, cost))
            }).map(|rows| {
                for row in rows.flatten() {
                    let (model, project_id, input, output, cost) = row;
                    total_cost += cost;
                    total_tokens += input + output;
                    *by_model.entry(model).or_insert(0.0) += cost;
                    *by_project.entry(project_id).or_insert(0.0) += cost;
                }
            });
        }

        TenantUsage {
            total_cost_usd: total_cost,
            total_tokens,
            by_model,
            by_project,
        }
    }

    /// Compute aggregate usage for a single project.
    pub fn project_usage(
        db: &rusqlite::Connection,
        project_id: &str,
    ) -> ProjectUsage {
        let mut total_cost: f32 = 0.0;
        let mut total_tokens: u64 = 0;
        let mut by_model: HashMap<String, f32> = HashMap::new();

        if let Ok(mut stmt) = db.prepare(
            "SELECT model, input_tokens, output_tokens, cost_usd
             FROM cost_records
             WHERE project_id = ?1",
        ) {
            let _ = stmt.query_map(rusqlite::params![project_id], |row| {
                let model: String = row.get(0)?;
                let input: u64 = row.get::<_, i64>(1)? as u64;
                let output: u64 = row.get::<_, i64>(2)? as u64;
                let cost: f32 = row.get::<_, f64>(3)? as f32;
                Ok((model, input, output, cost))
            }).map(|rows| {
                for row in rows.flatten() {
                    let (model, input, output, cost) = row;
                    total_cost += cost;
                    total_tokens += input + output;
                    *by_model.entry(model).or_insert(0.0) += cost;
                }
            });
        }

        ProjectUsage {
            total_cost_usd: total_cost,
            total_tokens,
            by_model,
        }
    }

    /// Check a tenant's spend against a budget cap and project future usage.
    pub fn budget_check(
        db: &rusqlite::Connection,
        tenant_id: &str,
        budget_usd: f32,
    ) -> BudgetStatus {
        let usage = Self::tenant_usage(db, tenant_id, 30);
        let remaining = (budget_usd - usage.total_cost_usd).max(0.0);

        // Simple linear projection: if we spent X in the last 30 days,
        // the daily rate is X/30, and projected overage for the next 30 days
        // is (daily_rate * 30) - remaining.
        let daily_rate = usage.total_cost_usd / 30.0;
        let projected_30d = daily_rate * 30.0;
        let projected_overage = (projected_30d - remaining).max(0.0);

        let recommendation = if remaining <= 0.0 {
            "Budget exhausted. Consider upgrading your plan or reducing model complexity.".to_string()
        } else if projected_overage > 0.0 {
            format!(
                "At current burn rate (${:.2}/day), you will exceed your budget by ${:.2} in the next 30 days. Consider switching to cheaper models for routine tasks.",
                daily_rate, projected_overage
            )
        } else {
            format!(
                "Within budget. ${:.2} remaining of ${:.2} (${:.2}/day burn rate).",
                remaining, budget_usd, daily_rate
            )
        };

        BudgetStatus {
            remaining_usd: remaining,
            used_usd: usage.total_cost_usd,
            projected_overage,
            recommendation,
        }
    }
}

// ===========================================================================
// 4. Prometheus Metrics
// ===========================================================================

/// Generate Prometheus-compatible text metrics from the current application state.
///
/// Returns metrics in the standard Prometheus exposition format, suitable for
/// scraping by a Prometheus server or compatible agent.
pub async fn prometheus_metrics(app: &AppState) -> String {
    let db = app.db.lock().await;
    let mut lines: Vec<String> = Vec::new();

    // -- nexus_generations_total{status} --
    lines.push("# HELP nexus_generations_total Total generation requests by status.".into());
    lines.push("# TYPE nexus_generations_total counter".into());
    for status in &["completed", "failed", "in_progress", "timed_out"] {
        let count = count_traces_by_status(&db, status);
        lines.push(format!("nexus_generations_total{{status=\"{status}\"}} {count}"));
    }

    // -- nexus_generation_duration_seconds{phase} --
    lines.push("# HELP nexus_generation_duration_seconds Generation duration by phase.".into());
    lines.push("# TYPE nexus_generation_duration_seconds gauge".into());
    for phase in &["intent_analysis", "code_generation", "quality_gate", "taste_check"] {
        let avg = avg_phase_duration_seconds(&db, phase);
        lines.push(format!("nexus_generation_duration_seconds{{phase=\"{phase}\"}} {avg:.3}"));
    }

    // -- nexus_taste_score{app_type} --
    lines.push("# HELP nexus_taste_score Average taste score by app type.".into());
    lines.push("# TYPE nexus_taste_score gauge".into());
    lines.push("nexus_taste_score{app_type=\"web\"} 0.0".into());
    lines.push("nexus_taste_score{app_type=\"api\"} 0.0".into());
    lines.push("nexus_taste_score{app_type=\"cli\"} 0.0".into());

    // -- nexus_guarantee_pass_rate --
    lines.push("# HELP nexus_guarantee_pass_rate Fraction of guarantees that passed.".into());
    lines.push("# TYPE nexus_guarantee_pass_rate gauge".into());
    lines.push("nexus_guarantee_pass_rate 0.0".into());

    // -- nexus_llm_cost_usd{provider,model} --
    lines.push("# HELP nexus_llm_cost_usd Total LLM cost in USD.".into());
    lines.push("# TYPE nexus_llm_cost_usd counter".into());
    let cost_rows = llm_cost_by_model(&db);
    for (provider, model, cost) in &cost_rows {
        lines.push(format!(
            "nexus_llm_cost_usd{{provider=\"{provider}\",model=\"{model}\"}} {cost:.4}"
        ));
    }

    // -- nexus_llm_tokens_total{provider,model,direction} --
    lines.push("# HELP nexus_llm_tokens_total Total LLM tokens by direction.".into());
    lines.push("# TYPE nexus_llm_tokens_total counter".into());
    let token_rows = llm_tokens_by_model(&db);
    for (provider, model, input, output) in &token_rows {
        lines.push(format!(
            "nexus_llm_tokens_total{{provider=\"{provider}\",model=\"{model}\",direction=\"input\"}} {input}"
        ));
        lines.push(format!(
            "nexus_llm_tokens_total{{provider=\"{provider}\",model=\"{model}\",direction=\"output\"}} {output}"
        ));
    }

    // -- nexus_active_runtimes --
    lines.push("# HELP nexus_active_runtimes Number of active app runtimes.".into());
    lines.push("# TYPE nexus_active_runtimes gauge".into());
    let active = count_active_runtimes(&db);
    lines.push(format!("nexus_active_runtimes {active}"));

    // -- nexus_agent_executions_total{type,status} --
    lines.push("# HELP nexus_agent_executions_total Agent executions by type and status.".into());
    lines.push("# TYPE nexus_agent_executions_total counter".into());
    lines.push("nexus_agent_executions_total{type=\"coding\",status=\"success\"} 0".into());
    lines.push("nexus_agent_executions_total{type=\"coding\",status=\"failure\"} 0".into());
    lines.push("nexus_agent_executions_total{type=\"background\",status=\"success\"} 0".into());
    lines.push("nexus_agent_executions_total{type=\"background\",status=\"failure\"} 0".into());

    // -- nexus_errors_total{class} --
    lines.push("# HELP nexus_errors_total Total errors by class.".into());
    lines.push("# TYPE nexus_errors_total counter".into());
    lines.push("nexus_errors_total{class=\"llm\"} 0".into());
    lines.push("nexus_errors_total{class=\"db\"} 0".into());
    lines.push("nexus_errors_total{class=\"runtime\"} 0".into());

    // -- HTTP-level metrics (counters + latency histogram) --
    lines.push(String::new());
    lines.push(crate::http_metrics::render_prometheus());

    lines.join("\n")
}

// ── Prometheus helper queries ───────────────────────────────────────────────

fn count_traces_by_status(_db: &rusqlite::Connection, _status: &str) -> u64 {
    // The cost_records table doesn't store trace status directly.
    // In a full implementation, this would query a traces table.
    // For now, return 0 — actual counts come from the in-memory TraceCollector.
    0
}

fn avg_phase_duration_seconds(db: &rusqlite::Connection, _phase: &str) -> f64 {
    // Would query span durations from a spans table or the in-memory collector.
    let _ = db;
    0.0
}

fn llm_cost_by_model(db: &rusqlite::Connection) -> Vec<(String, String, f64)> {
    let mut results = Vec::new();
    if let Ok(mut stmt) = db.prepare(
        "SELECT provider, model, SUM(cost_usd) FROM cost_records GROUP BY provider, model",
    ) {
        let _ = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, f64>(2)?,
            ))
        }).map(|rows| {
            for row in rows.flatten() {
                results.push(row);
            }
        });
    }
    results
}

fn llm_tokens_by_model(db: &rusqlite::Connection) -> Vec<(String, String, i64, i64)> {
    let mut results = Vec::new();
    if let Ok(mut stmt) = db.prepare(
        "SELECT provider, model, SUM(input_tokens), SUM(output_tokens) FROM cost_records GROUP BY provider, model",
    ) {
        let _ = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        }).map(|rows| {
            for row in rows.flatten() {
                results.push(row);
            }
        });
    }
    results
}

fn count_active_runtimes(db: &rusqlite::Connection) -> u64 {
    db.query_row(
        "SELECT COUNT(*) FROM app_instances WHERE status = 'running'",
        [],
        |row| row.get::<_, i64>(0),
    )
    .unwrap_or(0) as u64
}

// ===========================================================================
// 5. Control Plane
// ===========================================================================

/// Operating mode for the control plane, encoded as a `u8` for atomic storage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ControlMode {
    /// Full autonomy — all actions auto-approved.
    Autonomous,
    /// AI proposes, human approves high-risk actions.
    Supervised,
    /// Human drives, AI assists on request.
    Manual,
    /// All modifications disabled — emergency lockdown.
    Locked,
}

impl ControlMode {
    fn to_u8(&self) -> u8 {
        match self {
            Self::Autonomous => 0,
            Self::Supervised => 1,
            Self::Manual => 2,
            Self::Locked => 3,
        }
    }

    fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Autonomous,
            1 => Self::Supervised,
            2 => Self::Manual,
            3 => Self::Locked,
            _ => Self::Supervised,
        }
    }
}

/// A circuit breaker for a named subsystem.
///
/// When tripped, the subsystem is considered unhealthy and operations that
/// depend on it should degrade gracefully or return an error.
#[derive(Debug, Clone, Serialize)]
pub struct CircuitBreaker {
    /// Name of the subsystem (e.g. "llm", "database", "runtime").
    pub subsystem: String,
    /// Whether the breaker is currently tripped.
    pub tripped: bool,
    /// Human-readable reason the breaker was tripped.
    pub reason: Option<String>,
    /// ISO-8601 timestamp when the breaker was tripped.
    pub tripped_at: Option<String>,
}

/// The decision returned by the control plane for a given action.
#[derive(Debug, Clone, Serialize)]
pub enum ControlDecision {
    /// Action is allowed to proceed.
    Allow,
    /// Action requires human confirmation before proceeding.
    RequireConfirmation(String),
    /// Action is denied outright.
    Deny(String),
}

/// Production control plane with atomic mode switching and circuit breakers.
///
/// Thread-safe: the mode is stored atomically and breakers are behind a mutex,
/// so this can be shared across Tokio tasks without an outer async lock.
pub struct ControlPlane {
    /// Atomic operating mode (see [`ControlMode::to_u8`]).
    mode: AtomicU8,
    /// Circuit breakers keyed by subsystem name.
    circuit_breakers: Mutex<HashMap<String, CircuitBreaker>>,
}

impl Default for ControlPlane {
    fn default() -> Self {
        Self::new()
    }
}

impl ControlPlane {
    /// Create a new control plane in `Supervised` mode with no tripped breakers.
    pub fn new() -> Self {
        Self {
            mode: AtomicU8::new(ControlMode::Supervised.to_u8()),
            circuit_breakers: Mutex::new(HashMap::new()),
        }
    }

    /// Get the current operating mode.
    pub fn mode(&self) -> ControlMode {
        ControlMode::from_u8(self.mode.load(Ordering::SeqCst))
    }

    /// Set the operating mode atomically.
    pub fn set_mode(&self, mode: ControlMode) {
        self.mode.store(mode.to_u8(), Ordering::SeqCst);
    }

    /// Check whether an action is permitted under the current mode.
    ///
    /// Actions are classified by name — destructive actions (deploy, delete,
    /// bash) require confirmation in `Supervised` mode and are denied in
    /// `Locked` mode.
    pub fn check_permission(&self, action: &str) -> ControlDecision {
        let mode = self.mode();

        // If any breaker related to this action is tripped, deny.
        if let Ok(breakers) = self.circuit_breakers.lock() {
            for (_, cb) in breakers.iter() {
                if cb.tripped && action.contains(&cb.subsystem) {
                    return ControlDecision::Deny(format!(
                        "Circuit breaker tripped for subsystem '{}': {}",
                        cb.subsystem,
                        cb.reason.as_deref().unwrap_or("unknown")
                    ));
                }
            }
        }

        match mode {
            ControlMode::Autonomous => ControlDecision::Allow,
            ControlMode::Supervised => {
                let risky = matches!(
                    action,
                    "deploy" | "file_delete" | "bash_exec" | "shell_command"
                        | "push_to_github" | "deploy_to_server"
                );
                if risky {
                    ControlDecision::RequireConfirmation(format!(
                        "Action '{action}' requires confirmation in Supervised mode"
                    ))
                } else {
                    ControlDecision::Allow
                }
            }
            ControlMode::Manual => {
                ControlDecision::RequireConfirmation(format!(
                    "Manual mode: all actions require confirmation (action: '{action}')"
                ))
            }
            ControlMode::Locked => {
                ControlDecision::Deny(format!(
                    "System is locked — action '{action}' is denied"
                ))
            }
        }
    }

    /// Trip a circuit breaker for the given subsystem.
    pub fn trip_breaker(&self, subsystem: &str, reason: &str) {
        if let Ok(mut breakers) = self.circuit_breakers.lock() {
            breakers.insert(
                subsystem.to_string(),
                CircuitBreaker {
                    subsystem: subsystem.to_string(),
                    tripped: true,
                    reason: Some(reason.to_string()),
                    tripped_at: Some(chrono::Utc::now().to_rfc3339()),
                },
            );
        }
    }

    /// Reset (close) a circuit breaker for the given subsystem.
    pub fn reset_breaker(&self, subsystem: &str) {
        if let Ok(mut breakers) = self.circuit_breakers.lock() {
            if let Some(cb) = breakers.get_mut(subsystem) {
                cb.tripped = false;
                cb.reason = None;
                cb.tripped_at = None;
            }
        }
    }

    /// List all known circuit breakers.
    pub fn breakers(&self) -> Vec<CircuitBreaker> {
        self.circuit_breakers
            .lock()
            .map(|b| b.values().cloned().collect())
            .unwrap_or_default()
    }
}

// ===========================================================================
// 6. Health Dashboard
// ===========================================================================

/// Overall health status of the Nexus system.
#[derive(Debug, Clone, Serialize)]
pub struct HealthStatus {
    /// Top-level status: "healthy", "degraded", or "unhealthy".
    pub status: String,
    /// Health of individual subsystems.
    pub subsystems: HashMap<String, SubsystemHealth>,
    /// Current control mode as a string.
    pub control_mode: String,
    /// All circuit breakers (including non-tripped).
    pub circuit_breakers: Vec<CircuitBreaker>,
}

/// Health status of a single subsystem.
#[derive(Debug, Clone, Serialize)]
pub struct SubsystemHealth {
    /// "healthy", "degraded", or "unhealthy".
    pub status: String,
    /// Latency of the last health probe in milliseconds.
    pub latency_ms: Option<u64>,
    /// Error message if the subsystem is unhealthy.
    pub error: Option<String>,
    /// Arbitrary details about the subsystem.
    pub details: HashMap<String, serde_json::Value>,
}

/// Probe all subsystems and produce a unified health dashboard.
///
/// Checks the database, LLM configuration, and app runner status. The overall
/// status is "unhealthy" if any subsystem is unhealthy, "degraded" if any
/// subsystem is degraded, and "healthy" otherwise.
pub async fn health_dashboard(app: &AppState) -> HealthStatus {
    let mut subsystems = HashMap::new();

    // Database health
    let db_start = std::time::Instant::now();
    let db_health = {
        let db = app.db.lock().await;
        match db.query_row("SELECT 1", [], |_| Ok(())) {
            Ok(_) => SubsystemHealth {
                status: "healthy".into(),
                latency_ms: Some(db_start.elapsed().as_millis() as u64),
                error: None,
                details: HashMap::new(),
            },
            Err(e) => SubsystemHealth {
                status: "unhealthy".into(),
                latency_ms: Some(db_start.elapsed().as_millis() as u64),
                error: Some(e.to_string()),
                details: HashMap::new(),
            },
        }
    };
    subsystems.insert("database".into(), db_health);

    // LLM configuration health
    let llm_health = if app.openai_api_key.is_empty()
        || app.openai_api_key == "sk-placeholder"
    {
        SubsystemHealth {
            status: "degraded".into(),
            latency_ms: None,
            error: Some("No valid OpenAI API key configured".into()),
            details: {
                let mut d = HashMap::new();
                d.insert("model".into(), serde_json::Value::String(app.model.clone()));
                d
            },
        }
    } else {
        SubsystemHealth {
            status: "healthy".into(),
            latency_ms: None,
            error: None,
            details: {
                let mut d = HashMap::new();
                d.insert("model".into(), serde_json::Value::String(app.model.clone()));
                d.insert(
                    "anthropic_configured".into(),
                    serde_json::Value::Bool(app.anthropic_api_key.is_some()),
                );
                d
            },
        }
    };
    subsystems.insert("llm".into(), llm_health);

    // App runner health (count running instances)
    let runner_health = {
        let db = app.db.lock().await;
        let running_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM app_instances WHERE status = 'running'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let mut details = HashMap::new();
        details.insert(
            "running_instances".into(),
            serde_json::Value::Number(serde_json::Number::from(running_count)),
        );
        SubsystemHealth {
            status: "healthy".into(),
            latency_ms: None,
            error: None,
            details,
        }
    };
    subsystems.insert("app_runner".into(), runner_health);

    // Determine overall status
    let has_unhealthy = subsystems.values().any(|s| s.status == "unhealthy");
    let has_degraded = subsystems.values().any(|s| s.status == "degraded");
    let overall = if has_unhealthy {
        "unhealthy"
    } else if has_degraded {
        "degraded"
    } else {
        "healthy"
    };

    HealthStatus {
        status: overall.into(),
        subsystems,
        control_mode: "supervised".into(), // default — would read from a shared ControlPlane
        circuit_breakers: Vec::new(),
    }
}

// ===========================================================================
// 7. Database Initialization
// ===========================================================================

/// Create the `cost_records` table and associated indices if they do not exist.
///
/// Call this during application startup (e.g. from `AppState::init`) to ensure
/// the schema is present.
pub fn init_tables(db: &rusqlite::Connection) -> Result<(), String> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS cost_records (
            id TEXT PRIMARY KEY,
            trace_id TEXT,
            tenant_id TEXT NOT NULL,
            project_id TEXT,
            model TEXT NOT NULL,
            provider TEXT NOT NULL,
            input_tokens INTEGER NOT NULL DEFAULT 0,
            output_tokens INTEGER NOT NULL DEFAULT 0,
            cost_usd REAL NOT NULL DEFAULT 0.0,
            timestamp TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_cost_tenant ON cost_records(tenant_id);
        CREATE INDEX IF NOT EXISTS idx_cost_project ON cost_records(project_id);
        CREATE INDEX IF NOT EXISTS idx_cost_timestamp ON cost_records(timestamp);",
    )
    .map_err(|e| format!("Failed to init observability tables: {e}"))?;
    Ok(())
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ── Trace lifecycle ─────────────────────────────────────────────────

    #[test]
    fn trace_lifecycle_start_span_end_complete() {
        let mut collector = TraceCollector::new();

        // Start a trace
        let trace = collector.start_trace("t1", "proj-1", "tenant-1");
        assert_eq!(trace.status, TraceStatus::InProgress);
        assert!(trace.completed_at.is_none());

        // Start and end a span
        collector.start_span("t1", "s1", None, "intent_analysis");
        collector.end_span(
            "t1",
            "s1",
            SpanStatus::Ok,
            0.002,
            Some(TokenUsage {
                input_tokens: 500,
                output_tokens: 100,
                model: "gpt-4o".into(),
                provider: "openai".into(),
            }),
        );

        // Start a child span
        collector.start_span("t1", "s2", Some("s1"), "code_generation");
        collector.end_span(
            "t1",
            "s2",
            SpanStatus::Error("timeout".into()),
            0.05,
            Some(TokenUsage {
                input_tokens: 2000,
                output_tokens: 800,
                model: "gpt-4o".into(),
                provider: "openai".into(),
            }),
        );

        // Complete the trace
        collector.complete_trace("t1", TraceStatus::Completed);

        let trace = collector.get_trace("t1").unwrap();
        assert_eq!(trace.status, TraceStatus::Completed);
        assert!(trace.completed_at.is_some());
        assert_eq!(trace.spans.len(), 2);
        assert!((trace.total_cost_usd - 0.052).abs() < 0.001);
        assert_eq!(trace.total_tokens.input_tokens, 2500);
        assert_eq!(trace.total_tokens.output_tokens, 900);

        // Child span has parent
        assert_eq!(trace.spans[1].parent_id.as_deref(), Some("s1"));
    }

    #[test]
    fn trace_list_filters_by_tenant() {
        let mut collector = TraceCollector::new();
        collector.start_trace("t1", "p1", "tenant-a");
        collector.start_trace("t2", "p2", "tenant-b");
        collector.start_trace("t3", "p3", "tenant-a");

        let a_traces = collector.list_traces("tenant-a", 10);
        assert_eq!(a_traces.len(), 2);
        for t in &a_traces {
            assert_eq!(t.tenant_id, "tenant-a");
        }

        let b_traces = collector.list_traces("tenant-b", 10);
        assert_eq!(b_traces.len(), 1);
    }

    #[test]
    fn trace_list_respects_limit() {
        let mut collector = TraceCollector::new();
        for i in 0..10 {
            collector.start_trace(&format!("t{i}"), "p", "tenant");
        }
        let limited = collector.list_traces("tenant", 3);
        assert_eq!(limited.len(), 3);
    }

    #[test]
    fn span_on_nonexistent_trace_is_noop() {
        let mut collector = TraceCollector::new();
        // Should not panic.
        collector.start_span("nonexistent", "s1", None, "test");
        collector.end_span("nonexistent", "s1", SpanStatus::Ok, 0.0, None);
        collector.complete_trace("nonexistent", TraceStatus::Completed);
    }

    // ── Cost recording and querying ─────────────────────────────────────

    fn in_memory_db() -> rusqlite::Connection {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        init_tables(&db).unwrap();
        db
    }

    #[test]
    fn cost_record_insert_and_tenant_usage() {
        let db = in_memory_db();

        let record = CostRecord {
            id: "cr-1".into(),
            trace_id: "t-1".into(),
            tenant_id: "tenant-a".into(),
            project_id: "proj-1".into(),
            model: "gpt-4o".into(),
            provider: "openai".into(),
            input_tokens: 1000,
            output_tokens: 200,
            cost_usd: 0.03,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        CostTracker::record(&db, &record).unwrap();

        let record2 = CostRecord {
            id: "cr-2".into(),
            trace_id: "t-2".into(),
            tenant_id: "tenant-a".into(),
            project_id: "proj-2".into(),
            model: "claude-sonnet-4-20250514".into(),
            provider: "anthropic".into(),
            input_tokens: 500,
            output_tokens: 300,
            cost_usd: 0.02,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        CostTracker::record(&db, &record2).unwrap();

        let usage = CostTracker::tenant_usage(&db, "tenant-a", 30);
        assert!((usage.total_cost_usd - 0.05).abs() < 0.001);
        assert_eq!(usage.total_tokens, 2000);
        assert_eq!(usage.by_model.len(), 2);
        assert_eq!(usage.by_project.len(), 2);
    }

    #[test]
    fn cost_project_usage() {
        let db = in_memory_db();

        for i in 0..3 {
            let record = CostRecord {
                id: format!("cr-{i}"),
                trace_id: format!("t-{i}"),
                tenant_id: "t".into(),
                project_id: "proj-1".into(),
                model: "gpt-4o".into(),
                provider: "openai".into(),
                input_tokens: 100,
                output_tokens: 50,
                cost_usd: 0.01,
                timestamp: chrono::Utc::now().to_rfc3339(),
            };
            CostTracker::record(&db, &record).unwrap();
        }

        let usage = CostTracker::project_usage(&db, "proj-1");
        assert!((usage.total_cost_usd - 0.03).abs() < 0.001);
        assert_eq!(usage.total_tokens, 450);
    }

    #[test]
    fn cost_budget_check_within_budget() {
        let db = in_memory_db();

        let record = CostRecord {
            id: "cr-1".into(),
            trace_id: "t-1".into(),
            tenant_id: "tenant-a".into(),
            project_id: "proj-1".into(),
            model: "gpt-4o".into(),
            provider: "openai".into(),
            input_tokens: 1000,
            output_tokens: 200,
            cost_usd: 1.0,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        CostTracker::record(&db, &record).unwrap();

        let status = CostTracker::budget_check(&db, "tenant-a", 100.0);
        assert!(status.remaining_usd > 0.0);
        assert!((status.used_usd - 1.0).abs() < 0.01);
        assert!(status.recommendation.contains("Within budget"));
    }

    #[test]
    fn cost_budget_check_exhausted() {
        let db = in_memory_db();

        let record = CostRecord {
            id: "cr-1".into(),
            trace_id: "t-1".into(),
            tenant_id: "tenant-b".into(),
            project_id: "proj-1".into(),
            model: "gpt-4o".into(),
            provider: "openai".into(),
            input_tokens: 1000,
            output_tokens: 200,
            cost_usd: 150.0,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        CostTracker::record(&db, &record).unwrap();

        let status = CostTracker::budget_check(&db, "tenant-b", 100.0);
        assert_eq!(status.remaining_usd, 0.0);
        assert!(status.recommendation.contains("exhausted"));
    }

    #[test]
    fn cost_duplicate_id_fails() {
        let db = in_memory_db();

        let record = CostRecord {
            id: "dup-1".into(),
            trace_id: "t".into(),
            tenant_id: "t".into(),
            project_id: "p".into(),
            model: "m".into(),
            provider: "p".into(),
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        CostTracker::record(&db, &record).unwrap();
        assert!(CostTracker::record(&db, &record).is_err());
    }

    // ── Control mode transitions ────────────────────────────────────────

    #[test]
    fn control_mode_defaults_to_supervised() {
        let cp = ControlPlane::new();
        assert_eq!(cp.mode(), ControlMode::Supervised);
    }

    #[test]
    fn control_mode_transitions() {
        let cp = ControlPlane::new();

        cp.set_mode(ControlMode::Autonomous);
        assert_eq!(cp.mode(), ControlMode::Autonomous);

        cp.set_mode(ControlMode::Manual);
        assert_eq!(cp.mode(), ControlMode::Manual);

        cp.set_mode(ControlMode::Locked);
        assert_eq!(cp.mode(), ControlMode::Locked);

        cp.set_mode(ControlMode::Supervised);
        assert_eq!(cp.mode(), ControlMode::Supervised);
    }

    #[test]
    fn control_autonomous_allows_all() {
        let cp = ControlPlane::new();
        cp.set_mode(ControlMode::Autonomous);
        assert!(matches!(cp.check_permission("deploy"), ControlDecision::Allow));
        assert!(matches!(cp.check_permission("file_read"), ControlDecision::Allow));
    }

    #[test]
    fn control_supervised_requires_confirmation_for_risky() {
        let cp = ControlPlane::new();
        cp.set_mode(ControlMode::Supervised);

        assert!(matches!(
            cp.check_permission("deploy"),
            ControlDecision::RequireConfirmation(_)
        ));
        assert!(matches!(cp.check_permission("file_read"), ControlDecision::Allow));
    }

    #[test]
    fn control_manual_requires_confirmation_for_all() {
        let cp = ControlPlane::new();
        cp.set_mode(ControlMode::Manual);

        assert!(matches!(
            cp.check_permission("file_read"),
            ControlDecision::RequireConfirmation(_)
        ));
    }

    #[test]
    fn control_locked_denies_all() {
        let cp = ControlPlane::new();
        cp.set_mode(ControlMode::Locked);

        assert!(matches!(
            cp.check_permission("file_read"),
            ControlDecision::Deny(_)
        ));
        assert!(matches!(
            cp.check_permission("deploy"),
            ControlDecision::Deny(_)
        ));
    }

    // ── Circuit breaker behavior ────────────────────────────────────────

    #[test]
    fn circuit_breaker_trip_and_reset() {
        let cp = ControlPlane::new();

        assert!(cp.breakers().is_empty());

        cp.trip_breaker("llm", "Rate limit exceeded");
        let breakers = cp.breakers();
        assert_eq!(breakers.len(), 1);
        assert!(breakers[0].tripped);
        assert_eq!(breakers[0].reason.as_deref(), Some("Rate limit exceeded"));
        assert!(breakers[0].tripped_at.is_some());

        cp.reset_breaker("llm");
        let breakers = cp.breakers();
        assert_eq!(breakers.len(), 1);
        assert!(!breakers[0].tripped);
        assert!(breakers[0].reason.is_none());
    }

    #[test]
    fn tripped_breaker_denies_related_action() {
        let cp = ControlPlane::new();
        cp.set_mode(ControlMode::Autonomous);

        // Without breaker, action is allowed.
        assert!(matches!(cp.check_permission("llm_call"), ControlDecision::Allow));

        // Trip the llm breaker.
        cp.trip_breaker("llm", "Provider down");

        // Now the action is denied because "llm_call" contains "llm".
        assert!(matches!(
            cp.check_permission("llm_call"),
            ControlDecision::Deny(_)
        ));

        // Unrelated actions are still allowed.
        assert!(matches!(
            cp.check_permission("file_read"),
            ControlDecision::Allow
        ));

        // Reset and action is allowed again.
        cp.reset_breaker("llm");
        assert!(matches!(cp.check_permission("llm_call"), ControlDecision::Allow));
    }

    #[test]
    fn multiple_breakers_independent() {
        let cp = ControlPlane::new();
        cp.trip_breaker("llm", "down");
        cp.trip_breaker("database", "slow");

        assert_eq!(cp.breakers().len(), 2);

        cp.reset_breaker("llm");
        let breakers = cp.breakers();
        let llm = breakers.iter().find(|b| b.subsystem == "llm").unwrap();
        let db = breakers.iter().find(|b| b.subsystem == "database").unwrap();
        assert!(!llm.tripped);
        assert!(db.tripped);
    }

    // ── Health status ───────────────────────────────────────────────────

    #[test]
    fn health_status_serializes() {
        let health = HealthStatus {
            status: "healthy".into(),
            subsystems: {
                let mut m = HashMap::new();
                m.insert(
                    "database".into(),
                    SubsystemHealth {
                        status: "healthy".into(),
                        latency_ms: Some(2),
                        error: None,
                        details: HashMap::new(),
                    },
                );
                m
            },
            control_mode: "supervised".into(),
            circuit_breakers: vec![],
        };
        let json = serde_json::to_string(&health).unwrap();
        assert!(json.contains("\"status\":\"healthy\""));
        assert!(json.contains("\"database\""));
    }

    // ── Prometheus metric format ────────────────────────────────────────

    #[test]
    fn prometheus_metric_format_validity() {
        // Verify basic formatting rules without needing a full AppState.
        let lines = vec![
            "# HELP nexus_generations_total Total generation requests by status.",
            "# TYPE nexus_generations_total counter",
            "nexus_generations_total{status=\"completed\"} 42",
            "nexus_llm_cost_usd{provider=\"openai\",model=\"gpt-4o\"} 1.2345",
            "nexus_active_runtimes 3",
        ];
        for line in &lines {
            // HELP and TYPE lines start with #.
            if line.starts_with('#') {
                assert!(line.starts_with("# HELP") || line.starts_with("# TYPE"));
            } else {
                // Metric lines must have a space separating name{labels} from value.
                assert!(line.contains(' '), "Metric line must have space: {line}");
                let parts: Vec<&str> = line.rsplitn(2, ' ').collect();
                // The value part should be parseable as a number.
                assert!(
                    parts[0].parse::<f64>().is_ok(),
                    "Value should be numeric: {}",
                    parts[0]
                );
            }
        }
    }

    // ── DB init ─────────────────────────────────────────────────────────

    #[test]
    fn init_tables_idempotent() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        init_tables(&db).unwrap();
        init_tables(&db).unwrap(); // second call should not fail
    }
}
