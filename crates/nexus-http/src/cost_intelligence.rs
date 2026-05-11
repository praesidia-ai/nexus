//! Cost Intelligence Layer — LLM token tracking, cost estimation,
//! budget enforcement, and optimization recommendations.
//!
//! Tracks every LLM call with:
//! - Token counts (input/output)
//! - Model used
//! - Cost calculation
//! - Budget enforcement (per-project and global)
//! - Optimization signals (model downgrade suggestions)

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Record of a single LLM call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmCallRecord {
    pub id: String,
    pub project_id: Option<String>,
    pub model: String,
    pub provider: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cost_usd: f64,
    pub latency_ms: u64,
    pub purpose: String,
    pub timestamp: String,
}

/// Budget configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostBudget {
    /// Max spend per project per day (USD).
    pub daily_project_limit: f64,
    /// Max spend globally per day (USD).
    pub daily_global_limit: f64,
    /// Max tokens per single call.
    pub max_tokens_per_call: u64,
    /// Warn when daily spend exceeds this fraction (0.0-1.0).
    pub warn_threshold: f64,
}

impl Default for CostBudget {
    fn default() -> Self {
        Self {
            daily_project_limit: 10.0,
            daily_global_limit: 50.0,
            max_tokens_per_call: 200_000,
            warn_threshold: 0.8,
        }
    }
}

/// Cost summary for a time period.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostSummary {
    pub total_cost_usd: f64,
    pub total_tokens: u64,
    pub total_calls: usize,
    pub by_model: HashMap<String, ModelCost>,
    pub by_purpose: HashMap<String, f64>,
    pub by_project: HashMap<String, f64>,
    pub period: String,
    pub budget_remaining: f64,
    pub budget_used_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCost {
    pub calls: usize,
    pub tokens: u64,
    pub cost_usd: f64,
}

/// Optimization recommendation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostOptimization {
    pub recommendation: String,
    pub estimated_savings_usd: f64,
    pub confidence: f64,
    pub action: OptAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptAction {
    /// Use a cheaper model for this purpose.
    DowngradeModel { from: String, to: String },
    /// Cache this type of call.
    EnableCache { purpose: String },
    /// Reduce prompt size.
    TrimPrompt { purpose: String, current_tokens: u64 },
    /// Batch similar calls.
    BatchCalls { purpose: String, count: usize },
}

/// Budget check result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetCheck {
    pub allowed: bool,
    pub reason: Option<String>,
    pub daily_spent: f64,
    pub daily_limit: f64,
    pub remaining: f64,
}

// ---------------------------------------------------------------------------
// Pricing Table
// ---------------------------------------------------------------------------

/// Approximate pricing per 1M tokens (USD).
fn price_per_million(model: &str, direction: &str) -> f64 {
    match (model, direction) {
        // OpenAI — known-real models (current as of 2025)
        ("gpt-4.1", "input") => 2.00,
        ("gpt-4.1", "output") => 8.00,
        ("gpt-4.1-mini", "input") => 0.40,
        ("gpt-4.1-mini", "output") => 1.60,
        ("gpt-4.1-nano", "input") => 0.10,
        ("gpt-4.1-nano", "output") => 0.40,
        ("gpt-4o", "input") => 2.50,
        ("gpt-4o", "output") => 10.00,
        ("gpt-4-turbo", "input") => 10.00,
        ("gpt-4-turbo", "output") => 30.00,
        ("o1", "input") => 15.00,
        ("o1", "output") => 60.00,
        ("o1-mini", "input") => 3.00,
        ("o1-mini", "output") => 12.00,
        ("o3-mini", "input") => 1.10,
        ("o3-mini", "output") => 4.40,
        // Speculative GPT-5 family (only if user's account has access)
        ("gpt-5", "input") => 0.625,
        ("gpt-5", "output") => 5.00,
        ("gpt-5.2", "input") => 0.875,
        ("gpt-5.2", "output") => 7.00,
        ("gpt-5.4", "input") => 2.50,
        ("gpt-5.4", "output") => 10.00,
        ("gpt-5.4-mini", "input") => 0.75,
        ("gpt-5.4-mini", "output") => 4.50,
        ("gpt-5.4-nano", "input") => 0.20,
        ("gpt-5.4-nano", "output") => 1.25,
        // Anthropic
        ("claude-sonnet-4-6", "input") | ("claude-3-5-sonnet-latest", "input") => 3.00,
        ("claude-sonnet-4-6", "output") | ("claude-3-5-sonnet-latest", "output") => 15.00,
        ("claude-opus-4-6", "input") => 15.00,
        ("claude-opus-4-6", "output") => 75.00,
        ("claude-haiku-4-5-20251001", "input") => 0.80,
        ("claude-haiku-4-5-20251001", "output") => 4.00,
        // Google
        ("gemini-2.0-flash", "input") => 0.075,
        ("gemini-2.0-flash", "output") => 0.30,
        ("gemini-2.5-pro", "input") => 1.25,
        ("gemini-2.5-pro", "output") => 10.00,
        // Local (free)
        (m, _) if m.starts_with("ollama") || m.contains("local") => 0.0,
        // Default (estimate as mid-range)
        (_, "input") => 2.00,
        (_, "output") => 8.00,
        _ => 5.00,
    }
}

fn estimate_cost(model: &str, input_tokens: u64, output_tokens: u64) -> f64 {
    let input_cost = (input_tokens as f64 / 1_000_000.0) * price_per_million(model, "input");
    let output_cost = (output_tokens as f64 / 1_000_000.0) * price_per_million(model, "output");
    input_cost + output_cost
}

/// Publicly-exported cost estimator — same rates as the private `estimate_cost`
/// used by `record_call`. Call sites that need to surface per-call cost (e.g.
/// Agent TV's per-agent ticker) use this without having to wait on the cost
/// tracker's record store.
pub fn estimate_cost_usd(model: &str, input_tokens: u64, output_tokens: u64) -> f64 {
    estimate_cost(model, input_tokens, output_tokens)
}

// ---------------------------------------------------------------------------
// Cost Tracker
// ---------------------------------------------------------------------------

pub struct CostTracker {
    records: Arc<Mutex<Vec<LlmCallRecord>>>,
    // Budget is interior-mutable so operators can raise/lower limits at
    // runtime via POST /costs/budget without restarting the server. The
    // previous design took budget by-value, so set_budget updates were
    // silently dropped.
    budget: Arc<Mutex<CostBudget>>,
}

/// Cheaply-cloneable handle to a `CostTracker` — shares the same in-memory record store.
#[derive(Clone)]
pub struct CostTrackerRef {
    records: Arc<Mutex<Vec<LlmCallRecord>>>,
    budget: Arc<Mutex<CostBudget>>,
}

impl CostTrackerRef {
    #[allow(clippy::too_many_arguments)]
    pub async fn record_call(
        &self,
        project_id: Option<&str>,
        model: &str,
        provider: &str,
        input_tokens: u64,
        output_tokens: u64,
        latency_ms: u64,
        purpose: &str,
    ) {
        self.record_call_with_id(
            None,
            project_id,
            model,
            provider,
            input_tokens,
            output_tokens,
            latency_ms,
            purpose,
        )
        .await;
    }

    /// Idempotent variant of [`record_call`]. If `call_id` is provided and a
    /// record with the same id already exists, this is a no-op. Callers that
    /// may retry on the same logical LLM call (429/5xx retries, in-flight
    /// crash recovery) should pass a stable id to avoid inflating spend.
    #[allow(clippy::too_many_arguments)]
    pub async fn record_call_with_id(
        &self,
        call_id: Option<&str>,
        project_id: Option<&str>,
        model: &str,
        provider: &str,
        input_tokens: u64,
        output_tokens: u64,
        latency_ms: u64,
        purpose: &str,
    ) {
        // Cap token counts defensively — bad provider responses occasionally
        // return wildly large numbers. Pricing * u64::MAX would overflow f64
        // silently and poison the daily-budget check.
        const TOKEN_CAP: u64 = 10_000_000;
        let input_tokens = input_tokens.min(TOKEN_CAP);
        let output_tokens = output_tokens.min(TOKEN_CAP);

        let total_tokens = input_tokens.saturating_add(output_tokens);
        let cost = estimate_cost(model, input_tokens, output_tokens);
        let id = call_id
            .map(str::to_string)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        {
            let mut records = self.records.lock().await;
            // Idempotency: if caller-supplied id already present, skip.
            if call_id.is_some() && records.iter().any(|r| r.id == id) {
                return;
            }
            records.push(LlmCallRecord {
                id,
                project_id: project_id.map(String::from),
                model: model.to_string(),
                provider: provider.to_string(),
                input_tokens,
                output_tokens,
                total_tokens,
                cost_usd: cost,
                latency_ms,
                purpose: purpose.to_string(),
                timestamp: Utc::now().to_rfc3339(),
            });
            const MAX_RECORDS: usize = 10_000;
            if records.len() > MAX_RECORDS {
                let drain = records.len() - MAX_RECORDS;
                records.drain(..drain);
            }
        }

        info!(
            model = model,
            tokens = total_tokens,
            cost_usd = format!("{:.4}", cost),
            latency_ms = latency_ms,
            purpose = purpose,
            "LLM call tracked"
        );

        // ADR-005 §4: surface live cost on `/metrics`. Tenant attribution is
        // not yet plumbed through this call site (CostTrackerRef predates
        // ADR-003); use "default" until the trait migration moves through.
        crate::http_metrics::record_llm_call(
            provider,
            model,
            "default",
            crate::budget_brake::dollars_to_micros(cost) as u64,
            input_tokens,
            output_tokens,
            0,
        );

        // Warn if approaching budget. Take a short-lived read snapshot rather
        // than holding both locks at once — see cost_intelligence invariants.
        let daily_total: f64 = {
            let records = self.records.lock().await;
            records.iter().map(|r| r.cost_usd).sum()
        };
        let budget = self.budget.lock().await;
        if daily_total > budget.daily_global_limit * budget.warn_threshold {
            warn!(
                daily_usd = format!("{:.4}", daily_total),
                limit_usd = budget.daily_global_limit,
                "LLM spend approaching daily budget limit"
            );
        }
    }
}

impl CostTracker {
    pub fn new(budget: CostBudget) -> Self {
        Self {
            records: Arc::new(Mutex::new(Vec::new())),
            budget: Arc::new(Mutex::new(budget)),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(CostBudget::default())
    }

    /// Return a cheaply-cloneable handle that shares this tracker's storage
    /// AND its budget — updates via `set_budget` are visible to all refs.
    pub fn clone_ref(&self) -> CostTrackerRef {
        CostTrackerRef {
            records: Arc::clone(&self.records),
            budget: Arc::clone(&self.budget),
        }
    }

    /// Read the current budget (snapshot).
    pub async fn budget(&self) -> CostBudget {
        self.budget.lock().await.clone()
    }

    /// Replace the current budget. Takes effect immediately — subsequent
    /// `check_budget` calls use the new limits, without restarting the server.
    pub async fn set_budget(&self, new: CostBudget) {
        let mut slot = self.budget.lock().await;
        *slot = new;
    }

    /// Record an LLM call.
    #[allow(clippy::too_many_arguments)]
    pub async fn record_call(
        &self,
        project_id: Option<&str>,
        model: &str,
        provider: &str,
        input_tokens: u64,
        output_tokens: u64,
        latency_ms: u64,
        purpose: &str,
    ) -> LlmCallRecord {
        let total_tokens = input_tokens + output_tokens;
        let cost = estimate_cost(model, input_tokens, output_tokens);

        let record = LlmCallRecord {
            id: uuid::Uuid::new_v4().to_string(),
            project_id: project_id.map(String::from),
            model: model.to_string(),
            provider: provider.to_string(),
            input_tokens,
            output_tokens,
            total_tokens,
            cost_usd: cost,
            latency_ms,
            purpose: purpose.to_string(),
            timestamp: Utc::now().to_rfc3339(),
        };

        let mut records = self.records.lock().await;
        records.push(record.clone());

        // Cap in-memory records to prevent unbounded growth (keep last 10,000)
        const MAX_RECORDS: usize = 10_000;
        if records.len() > MAX_RECORDS {
            let drain_count = records.len() - MAX_RECORDS;
            records.drain(..drain_count);
        }

        info!(
            model = model,
            tokens = total_tokens,
            cost_usd = format!("{:.4}", cost),
            purpose = purpose,
            "LLM call recorded"
        );

        record
    }

    /// Check if a call is within budget.
    pub async fn check_budget(&self, project_id: Option<&str>) -> BudgetCheck {
        let records = self.records.lock().await;
        let today = Utc::now().format("%Y-%m-%d").to_string();

        let daily_spent: f64 = records
            .iter()
            .filter(|r| r.timestamp.starts_with(&today))
            .filter(|r| {
                project_id.is_none_or(|pid| {
                    r.project_id.as_deref() == Some(pid)
                })
            })
            .map(|r| r.cost_usd)
            .sum();

        let budget = self.budget.lock().await;
        let limit = if project_id.is_some() {
            budget.daily_project_limit
        } else {
            budget.daily_global_limit
        };

        let remaining = (limit - daily_spent).max(0.0);
        let allowed = daily_spent < limit;

        if !allowed {
            warn!(
                daily_spent = format!("{:.2}", daily_spent),
                limit = format!("{:.2}", limit),
                "Budget limit exceeded"
            );
        } else if daily_spent >= limit * budget.warn_threshold {
            warn!(
                daily_spent = format!("{:.2}", daily_spent),
                limit = format!("{:.2}", limit),
                "Approaching budget limit"
            );
        }

        BudgetCheck {
            allowed,
            reason: if allowed {
                None
            } else {
                Some(format!(
                    "Daily budget exceeded: ${:.2} / ${:.2}",
                    daily_spent, limit
                ))
            },
            daily_spent,
            daily_limit: limit,
            remaining,
        }
    }

    /// Get cost summary for today.
    pub async fn daily_summary(&self) -> CostSummary {
        let records = self.records.lock().await;
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let limit = self.budget.lock().await.daily_global_limit;
        self.build_summary(&records, &today, limit)
    }

    /// Get cost summary for a specific date.
    pub async fn summary_for_date(&self, date: &str) -> CostSummary {
        let records = self.records.lock().await;
        let limit = self.budget.lock().await.daily_global_limit;
        self.build_summary(&records, date, limit)
    }

    /// Generate optimization recommendations.
    pub async fn recommendations(&self) -> Vec<CostOptimization> {
        let records = self.records.lock().await;
        let mut recs = Vec::new();

        // Group by purpose and model
        let mut purpose_stats: HashMap<String, (usize, u64, f64, String)> = HashMap::new();
        for r in records.iter() {
            let entry = purpose_stats
                .entry(r.purpose.clone())
                .or_insert((0, 0, 0.0, r.model.clone()));
            entry.0 += 1;
            entry.1 += r.total_tokens;
            entry.2 += r.cost_usd;
        }

        for (purpose, (count, tokens, cost, model)) in &purpose_stats {
            // Recommend cheaper model for validation/simple tasks
            if (purpose.contains("validate") || purpose.contains("simple"))
                && (model.starts_with("gpt-5") || *model == "gpt-4o" || model.contains("claude-sonnet"))
            {
                let cheaper = if model.starts_with("gpt") {
                    "gpt-4.1-mini"
                } else {
                    "claude-haiku-4-5-20251001"
                };
                let savings = cost * 0.7; // ~70% savings with mini/haiku
                recs.push(CostOptimization {
                    recommendation: format!(
                        "Use {} instead of {} for '{}' tasks",
                        cheaper, model, purpose
                    ),
                    estimated_savings_usd: savings,
                    confidence: 0.8,
                    action: OptAction::DowngradeModel {
                        from: model.clone(),
                        to: cheaper.to_string(),
                    },
                });
            }

            // Recommend batching if many small calls
            if *count > 10 && *tokens / (*count as u64) < 1000 {
                recs.push(CostOptimization {
                    recommendation: format!(
                        "Batch {} small '{}' calls into fewer larger calls",
                        count, purpose
                    ),
                    estimated_savings_usd: cost * 0.3,
                    confidence: 0.6,
                    action: OptAction::BatchCalls {
                        purpose: purpose.clone(),
                        count: *count,
                    },
                });
            }

            // Recommend prompt trimming for large inputs
            if *count > 0 {
                let avg_tokens = tokens / *count as u64;
                if avg_tokens > 50_000 {
                    recs.push(CostOptimization {
                        recommendation: format!(
                            "Trim prompts for '{}' (avg {}K tokens)",
                            purpose,
                            avg_tokens / 1000
                        ),
                        estimated_savings_usd: cost * 0.4,
                        confidence: 0.7,
                        action: OptAction::TrimPrompt {
                            purpose: purpose.clone(),
                            current_tokens: avg_tokens,
                        },
                    });
                }
            }
        }

        // Sort by savings (highest first). Use total_cmp to avoid NaN panics.
        recs.sort_by(|a, b| b.estimated_savings_usd.total_cmp(&a.estimated_savings_usd));
        recs
    }

    /// Persist records to database.
    ///
    /// The records lock is taken briefly to snapshot the outstanding entries,
    /// *then released*, before the DB mutex is acquired. Holding both
    /// `records` and `db` simultaneously (or worse, holding one across an
    /// `await`) risks deadlocking the single-connection SQLite mutex under
    /// load — see the invariant in `CLAUDE.md`.
    pub async fn persist_to_db(
        &self,
        db: &tokio::sync::Mutex<rusqlite::Connection>,
    ) {
        let snapshot: Vec<LlmCallRecord> = {
            let records = self.records.lock().await;
            records.clone()
        };

        let conn = db.lock().await;
        for record in snapshot.iter() {
            let _ = conn.execute(
                "INSERT OR IGNORE INTO audit_entries (id, project_id, action, actor, detail, created_at)
                 VALUES (?1, ?2, 'llm_call', ?3, ?4, ?5)",
                rusqlite::params![
                    record.id,
                    record.project_id,
                    format!("{}:{}", record.provider, record.model),
                    serde_json::to_string(record).unwrap_or_default(),
                    record.timestamp,
                ],
            );
        }
    }

    fn build_summary(
        &self,
        records: &[LlmCallRecord],
        date: &str,
        daily_global_limit: f64,
    ) -> CostSummary {
        let day_records: Vec<&LlmCallRecord> = records
            .iter()
            .filter(|r| r.timestamp.starts_with(date))
            .collect();

        let mut by_model: HashMap<String, ModelCost> = HashMap::new();
        let mut by_purpose: HashMap<String, f64> = HashMap::new();
        let mut by_project: HashMap<String, f64> = HashMap::new();
        let mut total_cost = 0.0f64;
        let mut total_tokens = 0u64;

        for r in &day_records {
            total_cost += r.cost_usd;
            total_tokens += r.total_tokens;

            let model_entry = by_model.entry(r.model.clone()).or_insert(ModelCost {
                calls: 0,
                tokens: 0,
                cost_usd: 0.0,
            });
            model_entry.calls += 1;
            model_entry.tokens += r.total_tokens;
            model_entry.cost_usd += r.cost_usd;

            *by_purpose.entry(r.purpose.clone()).or_default() += r.cost_usd;

            if let Some(ref pid) = r.project_id {
                *by_project.entry(pid.clone()).or_default() += r.cost_usd;
            }
        }

        let budget_remaining = (daily_global_limit - total_cost).max(0.0);
        let budget_used_pct = if daily_global_limit > 0.0 {
            (total_cost / daily_global_limit * 100.0).min(100.0)
        } else {
            0.0
        };

        CostSummary {
            total_cost_usd: total_cost,
            total_tokens,
            total_calls: day_records.len(),
            by_model,
            by_purpose,
            by_project,
            period: date.to_string(),
            budget_remaining,
            budget_used_pct,
        }
    }
}

// ---------------------------------------------------------------------------
// Model Router — select cheapest model that meets quality requirements
// ---------------------------------------------------------------------------

/// Select the most cost-effective model for a given task.
pub fn route_model(purpose: &str, complexity: &str) -> &'static str {
    match (purpose, complexity) {
        // Simple validation tasks -> cheapest model
        (p, _) if p.contains("validate") || p.contains("check") || p.contains("lint") => {
            "gpt-4.1-mini"
        }
        // Code generation -> mid-tier
        (p, "simple") if p.contains("generate") || p.contains("code") => "gpt-4.1-mini",
        (p, "medium") if p.contains("generate") || p.contains("code") => "gpt-4.1-mini",
        (p, "complex") if p.contains("generate") || p.contains("code") => "gpt-4.1",
        // Architecture/planning -> strong model
        (p, _) if p.contains("plan") || p.contains("architect") => "gpt-4.1",
        // Security review -> strong model
        (p, _) if p.contains("security") || p.contains("audit") => "gpt-4.1",
        // Default
        (_, "simple") => "gpt-4.1-mini",
        (_, _) => "gpt-4.1-mini",
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn tracks_cost_correctly() {
        let tracker = CostTracker::with_defaults();
        let record = tracker
            .record_call(Some("proj1"), "gpt-4o", "openai", 1000, 500, 200, "test")
            .await;

        assert!(record.cost_usd > 0.0);
        assert_eq!(record.total_tokens, 1500);
    }

    #[tokio::test]
    async fn budget_check_works() {
        let budget = CostBudget {
            daily_project_limit: 0.001, // Very low for testing
            ..Default::default()
        };
        let tracker = CostTracker::new(budget);

        // Record a call that exceeds budget
        tracker
            .record_call(Some("proj1"), "gpt-4o", "openai", 100_000, 50_000, 200, "test")
            .await;

        let check = tracker.check_budget(Some("proj1")).await;
        assert!(!check.allowed);
    }

    #[tokio::test]
    async fn daily_summary_aggregates() {
        let tracker = CostTracker::with_defaults();
        tracker
            .record_call(Some("p1"), "gpt-4o", "openai", 1000, 500, 100, "gen")
            .await;
        tracker
            .record_call(Some("p1"), "gpt-4.1-mini", "openai", 2000, 1000, 50, "validate")
            .await;

        let summary = tracker.daily_summary().await;
        assert_eq!(summary.total_calls, 2);
        assert!(summary.total_cost_usd > 0.0);
        assert!(summary.by_model.contains_key("gpt-4o"));
        assert!(summary.by_model.contains_key("gpt-4.1-mini"));
    }

    #[test]
    fn model_routing_selects_cheapest() {
        assert_eq!(route_model("validate_spec", "simple"), "gpt-4.1-mini");
        assert_eq!(route_model("generate_code", "complex"), "gpt-4.1");
        assert_eq!(route_model("security_audit", "medium"), "gpt-4.1");
        assert_eq!(route_model("plan_architecture", "complex"), "gpt-4.1");
    }

    #[test]
    fn cost_estimation_correct() {
        // gpt-4o: $2.50/M input, $10/M output
        let cost = estimate_cost("gpt-4o", 1_000_000, 0);
        assert!((cost - 2.50).abs() < 0.01);

        let cost = estimate_cost("gpt-4o", 0, 1_000_000);
        assert!((cost - 10.0).abs() < 0.01);

        // Local models should be free
        let cost = estimate_cost("ollama/llama3", 1_000_000, 1_000_000);
        assert_eq!(cost, 0.0);
    }
}
