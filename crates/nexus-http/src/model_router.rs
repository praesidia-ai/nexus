//! Model Router — chooses the BEST model per task automatically.
//!
//! Inputs: task type, complexity, latency target, cost budget.
//! Output: provider + model + reasoning.
//!
//! Rules:
//! - Heavy reasoning → strongest model (claude-opus, gpt-4.1)
//! - UI generation → fast model (gpt-4.1-mini, claude-haiku)
//! - Retries → cheaper model
//! - High cost usage → automatic downgrade
//! - Validation → cheaper model
//! - Codegen / UI generation prefer OpenAI + Anthropic only when those API keys exist (no Groq/DeepSeek for app generation).

use serde::Serialize;

use crate::llm_model_defaults::OPENAI_DEFAULT_MODEL;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ModelSelection {
    pub provider: String,
    pub model: String,
    pub reasoning: String,
    /// Estimated cost per 1K tokens (input + output averaged).
    pub estimated_cost_per_1k: f64,
    /// Expected latency tier.
    pub latency_tier: LatencyTier,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LatencyTier {
    /// <3s first token
    Fast,
    /// 3-10s first token
    Medium,
    /// 10s+ first token
    Slow,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TaskType {
    /// Complex reasoning, architecture decisions, spec generation.
    Reasoning,
    /// Code generation (API routes, services, components).
    Codegen,
    /// UI page generation (HTML/JSX/CSS heavy).
    UiGeneration,
    /// Validation, linting, checking output quality.
    Validation,
    /// Retry of a failed step (use cheaper model).
    Retry,
    /// Context compression, summarization.
    Compression,
    /// Agent tool-use loop.
    AgentLoop,
    /// Simple extraction, parsing.
    Extraction,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TaskComplexity {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone)]
pub struct RoutingContext {
    pub task_type: TaskType,
    pub complexity: TaskComplexity,
    /// Target latency in ms (0 = no preference).
    pub latency_target_ms: u64,
    /// Remaining daily budget as fraction (0.0 = empty, 1.0 = full).
    pub budget_remaining_pct: f64,
    /// Whether this is a retry attempt.
    pub is_retry: bool,
    /// Preferred provider (empty = any).
    pub preferred_provider: String,
    /// Available API keys.
    pub has_anthropic: bool,
    pub has_openai: bool,
}

// ---------------------------------------------------------------------------
// Model catalog (static, deterministic)
// ---------------------------------------------------------------------------

struct ModelOption {
    provider: &'static str,
    model: &'static str,
    reasoning_score: u32, // 0-100
    codegen_score: u32,   // 0-100
    speed_score: u32,       // 0-100 (higher = faster)
    cost_per_1k: f64,       // approximate USD per 1K tokens
    latency_tier: LatencyTier,
}

const MODELS: &[ModelOption] = &[
    // OpenAI — prefer gpt-4.1 family over deprecated weak mini tiers
    ModelOption { provider: "openai", model: "gpt-4.1", reasoning_score: 92, codegen_score: 94, speed_score: 68, cost_per_1k: 0.008, latency_tier: LatencyTier::Medium },
    ModelOption { provider: "openai", model: "gpt-4.1-mini", reasoning_score: 78, codegen_score: 82, speed_score: 90, cost_per_1k: 0.0012, latency_tier: LatencyTier::Fast },
    ModelOption { provider: "openai", model: "o3-mini", reasoning_score: 88, codegen_score: 85, speed_score: 60, cost_per_1k: 0.005, latency_tier: LatencyTier::Medium },
    // Anthropic
    ModelOption { provider: "anthropic", model: "claude-opus-4-6", reasoning_score: 98, codegen_score: 97, speed_score: 45, cost_per_1k: 0.022, latency_tier: LatencyTier::Slow },
    ModelOption { provider: "anthropic", model: "claude-sonnet-4-6", reasoning_score: 93, codegen_score: 95, speed_score: 65, cost_per_1k: 0.009, latency_tier: LatencyTier::Medium },
    ModelOption { provider: "anthropic", model: "claude-haiku-4-5-20251001", reasoning_score: 76, codegen_score: 80, speed_score: 91, cost_per_1k: 0.001, latency_tier: LatencyTier::Fast },
    // Groq (fast inference)
    ModelOption { provider: "groq", model: "llama-3.3-70b-versatile", reasoning_score: 75, codegen_score: 72, speed_score: 98, cost_per_1k: 0.0002, latency_tier: LatencyTier::Fast },
    // DeepSeek (cost-effective)
    ModelOption { provider: "deepseek", model: "deepseek-chat", reasoning_score: 82, codegen_score: 84, speed_score: 75, cost_per_1k: 0.0007, latency_tier: LatencyTier::Fast },
];

fn codegen_ui_prefer_frontier_only(ctx: &RoutingContext) -> bool {
    matches!(
        ctx.task_type,
        TaskType::Codegen | TaskType::UiGeneration
    ) && (ctx.has_openai || ctx.has_anthropic)
}

/// Select candidate models for routing (OpenAI + Anthropic only for codegen/UI when keys exist).
fn candidate_models(ctx: &RoutingContext) -> Vec<&ModelOption> {
    let available: Vec<&ModelOption> = MODELS
        .iter()
        .filter(|m| {
            match m.provider {
                "anthropic" => ctx.has_anthropic,
                "openai" => ctx.has_openai,
                _ => ctx.has_openai,
            }
        })
        .filter(|m| {
            ctx.preferred_provider.is_empty() || m.provider == ctx.preferred_provider
        })
        .collect();

    if codegen_ui_prefer_frontier_only(ctx) {
        let filtered: Vec<&ModelOption> = available
            .iter()
            .copied()
            .filter(|m| m.provider == "openai" || m.provider == "anthropic")
            .collect();
        if !filtered.is_empty() {
            return filtered;
        }
    }

    available
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Select the best model for a given task and context.
pub fn route(ctx: &RoutingContext) -> ModelSelection {
    let available = candidate_models(ctx);

    if available.is_empty() {
        return ModelSelection {
            provider: "openai".into(),
            model: OPENAI_DEFAULT_MODEL.into(),
            reasoning: "No models available matching constraints — using fallback".into(),
            estimated_cost_per_1k: 0.008,
            latency_tier: LatencyTier::Medium,
        };
    }

    // Score each model for this specific task
    let scored: Vec<(&ModelOption, f64)> = available
        .iter()
        .map(|m| {
            let score = score_model(m, ctx);
            (*m, score)
        })
        .collect();

    // Pick the highest scored
    let best = scored
        .iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(m, _)| *m)
        .unwrap_or(available[0]);

    let reasoning = build_reasoning(best, ctx);

    ModelSelection {
        provider: best.provider.to_string(),
        model: best.model.to_string(),
        reasoning,
        estimated_cost_per_1k: best.cost_per_1k,
        latency_tier: best.latency_tier,
    }
}

fn score_model(m: &ModelOption, ctx: &RoutingContext) -> f64 {
    let mut score: f64 = 0.0;

    // Task type weights
    let (reasoning_w, codegen_w, speed_w, cost_w) = match ctx.task_type {
        TaskType::Reasoning => (0.5, 0.1, 0.1, 0.3),
        TaskType::Codegen => (0.2, 0.4, 0.2, 0.2),
        TaskType::UiGeneration => (0.1, 0.3, 0.3, 0.3),
        TaskType::Validation => (0.1, 0.1, 0.4, 0.4),
        TaskType::Retry => (0.1, 0.2, 0.3, 0.4),
        TaskType::Compression => (0.2, 0.1, 0.3, 0.4),
        TaskType::AgentLoop => (0.3, 0.3, 0.2, 0.2),
        TaskType::Extraction => (0.1, 0.1, 0.4, 0.4),
    };

    score += m.reasoning_score as f64 * reasoning_w;
    score += m.codegen_score as f64 * codegen_w;
    score += m.speed_score as f64 * speed_w;
    // Cost: invert (lower cost = higher score)
    let cost_score = (1.0 - (m.cost_per_1k / 0.025).min(1.0)) * 100.0;
    score += cost_score * cost_w;

    // Complexity multiplier
    match ctx.complexity {
        TaskComplexity::High => {
            // Boost reasoning-heavy models
            score += m.reasoning_score as f64 * 0.15;
        }
        TaskComplexity::Low => {
            // Boost fast/cheap models
            score += m.speed_score as f64 * 0.15;
            score += cost_score * 0.1;
        }
        TaskComplexity::Medium => {}
    }

    // Budget pressure: if budget is low, heavily favor cheap models
    if ctx.budget_remaining_pct < 0.3 {
        score += cost_score * 0.3;
    }

    // Retry: always favor cheap + fast
    if ctx.is_retry {
        score += m.speed_score as f64 * 0.2;
        score += cost_score * 0.2;
    }

    // Latency target
    if ctx.latency_target_ms > 0 {
        match (ctx.latency_target_ms, m.latency_tier) {
            (0..=3000, LatencyTier::Fast) => score += 15.0,
            (0..=3000, _) => score -= 20.0,
            (3001..=10000, LatencyTier::Fast | LatencyTier::Medium) => score += 10.0,
            _ => {}
        }
    }

    score
}

fn build_reasoning(m: &ModelOption, ctx: &RoutingContext) -> String {
    let task_desc = match ctx.task_type {
        TaskType::Reasoning => "complex reasoning",
        TaskType::Codegen => "code generation",
        TaskType::UiGeneration => "UI generation",
        TaskType::Validation => "validation",
        TaskType::Retry => "retry attempt",
        TaskType::Compression => "context compression",
        TaskType::AgentLoop => "agent tool-use loop",
        TaskType::Extraction => "data extraction",
    };

    let mut parts = vec![format!(
        "Selected {}/{} for {} (complexity: {:?})",
        m.provider, m.model, task_desc, ctx.complexity
    )];

    if codegen_ui_prefer_frontier_only(ctx) {
        parts.push("Codegen/UI — frontier providers only (OpenAI/Anthropic)".into());
    }
    if ctx.budget_remaining_pct < 0.3 {
        parts.push(format!(
            "Budget at {:.0}% — favoring cost-effective model",
            ctx.budget_remaining_pct * 100.0
        ));
    }
    if ctx.is_retry {
        parts.push("Retry — using faster/cheaper model".into());
    }

    parts.join(". ")
}

/// Quick helper: route a task type to a model name string (for simple integration).
pub fn route_model_name(
    task_type: TaskType,
    complexity: TaskComplexity,
    has_anthropic: bool,
    has_openai: bool,
    budget_pct: f64,
) -> String {
    let ctx = RoutingContext {
        task_type,
        complexity,
        latency_target_ms: 0,
        budget_remaining_pct: budget_pct,
        is_retry: false,
        preferred_provider: String::new(),
        has_anthropic,
        has_openai,
    };
    route(&ctx).model
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(task: TaskType, complexity: TaskComplexity) -> RoutingContext {
        RoutingContext {
            task_type: task,
            complexity,
            latency_target_ms: 0,
            budget_remaining_pct: 1.0,
            is_retry: false,
            preferred_provider: String::new(),
            has_anthropic: true,
            has_openai: true,
        }
    }

    #[test]
    fn reasoning_selects_strong_model() {
        let sel = route(&ctx(TaskType::Reasoning, TaskComplexity::High));
        assert!(
            sel.model.contains("sonnet")
                || sel.model.contains("gpt-4.1")
                || sel.model.contains("o3")
                || sel.model.contains("opus")
                || sel.model.contains("deepseek"),
            "Expected strong model for reasoning, got: {}",
            sel.model
        );
    }

    #[test]
    fn validation_selects_cheap_model() {
        let sel = route(&ctx(TaskType::Validation, TaskComplexity::Low));
        assert!(
            sel.estimated_cost_per_1k < 0.006,
            "Expected cheap model for validation, got cost: {}",
            sel.estimated_cost_per_1k
        );
    }

    #[test]
    fn retry_favors_fast_cheap() {
        let mut c = ctx(TaskType::Retry, TaskComplexity::Medium);
        c.is_retry = true;
        let sel = route(&c);
        assert!(
            sel.latency_tier == LatencyTier::Fast || sel.estimated_cost_per_1k < 0.004,
            "Retry should use fast/cheap model, got: {} (${:.4}/1k)",
            sel.model,
            sel.estimated_cost_per_1k
        );
    }

    #[test]
    fn low_budget_downgrades() {
        let mut c = ctx(TaskType::Codegen, TaskComplexity::High);
        c.budget_remaining_pct = 0.1;
        let sel = route(&c);
        assert!(
            sel.estimated_cost_per_1k < 0.012,
            "Low budget should favor cheaper models, got: {} (${:.4}/1k)",
            sel.model,
            sel.estimated_cost_per_1k
        );
    }

    #[test]
    fn openai_only_excludes_anthropic() {
        let mut c = ctx(TaskType::Codegen, TaskComplexity::High);
        c.has_anthropic = false;
        let sel = route(&c);
        assert_ne!(sel.provider, "anthropic");
    }

    #[test]
    fn codegen_with_both_keys_stays_on_frontier_providers() {
        let sel = route(&ctx(TaskType::Codegen, TaskComplexity::High));
        assert!(
            sel.provider == "openai" || sel.provider == "anthropic",
            "Expected OpenAI or Anthropic for codegen, got {}",
            sel.provider
        );
        assert_ne!(sel.provider, "groq");
    }
}
