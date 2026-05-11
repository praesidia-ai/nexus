//! Swarm runtime foundation — `MiniAgent` trait, `Task` / `Result` /
//! `Budget` primitives, and the canonical kind registry.
//!
//! See `docs/NEXUS_MASTER_PLAN.md` §2 for the architectural thesis. In
//! short: the 10 named personas (Nova, Atlas, Kai, …) are **conductors**
//! that fan out to dozens–hundreds of narrow-scope **mini-agents** per
//! run. Each mini-agent has:
//!
//! - a fixed input/output schema (the [`MiniKind`] it implements)
//! - a narrow context window (≤ 2 k tokens of prompt)
//! - a cheap default model (typically a local 7 B on Ollama)
//! - its own golden-set eval (lives in `crates/nexus-eval/`)
//! - a hard per-invocation [`Budget`]
//!
//! This module only defines the *shapes*. Concrete implementations live
//! in `nexus-http/src/mini_agents/` and the `SwarmConductor` that
//! orchestrates them lives in `nexus-http/src/coding_agents/swarm.rs`.

use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Canonical mini-agent kinds. The v1.0 set is enumerated in
/// `docs/NEXUS_MASTER_PLAN.md` §2. New kinds are added here and MUST
/// ship with a matching golden-set eval before they can be promoted
/// into the default swarm.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MiniKind {
    // ── File / code ────────────────────────────────────────────────
    FsLocator,
    FsReader,
    FsPatcher,
    AstExtractor,
    AstRefactorer,
    MergeResolver,
    ImportFixer,

    // ── Test / verify ─────────────────────────────────────────────
    TestWriter,
    TestRunner,
    LintRunner,
    PolicyChecker,

    // ── Docs / copy ───────────────────────────────────────────────
    DocWriter,
    CopyRewriter,
    ReadmeSectioner,

    // ── Planning / reasoning ──────────────────────────────────────
    IntentClassifier,
    DecisionRouter,
    RouteSuggester,
    PromptCritiquer,

    // ── I/O / tools ───────────────────────────────────────────────
    WebFetcher,
    ShellRunner,
    SchemaInferrer,
    CacheLookerUpper,

    // ── Cost / eval ───────────────────────────────────────────────
    CostEstimator,
    EvalScorer,
}

impl MiniKind {
    /// Stable wire name for persistence + telemetry. Must never be
    /// renamed without a migration.
    pub fn as_wire_str(self) -> &'static str {
        match self {
            Self::FsLocator => "fs.locator",
            Self::FsReader => "fs.reader",
            Self::FsPatcher => "fs.patcher",
            Self::AstExtractor => "ast.extractor",
            Self::AstRefactorer => "ast.refactorer",
            Self::MergeResolver => "merge.resolver",
            Self::ImportFixer => "import.fixer",
            Self::TestWriter => "test.writer",
            Self::TestRunner => "test.runner",
            Self::LintRunner => "lint.runner",
            Self::PolicyChecker => "policy.checker",
            Self::DocWriter => "doc.writer",
            Self::CopyRewriter => "copy.rewriter",
            Self::ReadmeSectioner => "readme.sectioner",
            Self::IntentClassifier => "intent.classifier",
            Self::DecisionRouter => "decision.router",
            Self::RouteSuggester => "route.suggester",
            Self::PromptCritiquer => "prompt.critiquer",
            Self::WebFetcher => "web.fetcher",
            Self::ShellRunner => "shell.runner",
            Self::SchemaInferrer => "schema.inferrer",
            Self::CacheLookerUpper => "cache.lookerupper",
            Self::CostEstimator => "cost.estimator",
            Self::EvalScorer => "eval.scorer",
        }
    }

    /// Full v1.0 canonical set, in a stable order.
    pub fn all() -> &'static [MiniKind] {
        use MiniKind::*;
        &[
            FsLocator, FsReader, FsPatcher, AstExtractor, AstRefactorer,
            MergeResolver, ImportFixer,
            TestWriter, TestRunner, LintRunner, PolicyChecker,
            DocWriter, CopyRewriter, ReadmeSectioner,
            IntentClassifier, DecisionRouter, RouteSuggester, PromptCritiquer,
            WebFetcher, ShellRunner, SchemaInferrer, CacheLookerUpper,
            CostEstimator, EvalScorer,
        ]
    }
}

/// Per-invocation resource budget enforced by the `SwarmConductor`.
///
/// Mini-agents that hit any ceiling return [`MiniError::BudgetExceeded`]
/// and their parent conductor decides whether to retry with a larger
/// budget, reroute to a different kind, or fail the parent task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Budget {
    /// Hard cap on input + output tokens this mini-agent may consume.
    pub tokens: u32,
    /// Hard cap on wall-clock duration for this mini-agent.
    #[serde(with = "duration_ms")]
    pub wall_clock: Duration,
    /// Hard cap on dollars this mini-agent may spend (LLM + tool calls).
    pub cost_usd: f64,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            tokens: 2_000,
            wall_clock: Duration::from_secs(30),
            cost_usd: 0.05,
        }
    }
}

/// Narrow input passed to a mini-agent. `input` is opaque `serde_json`;
/// each [`MiniKind`] documents its expected shape. Keeping this loose
/// lets conductors compose mini-agents dynamically at runtime without a
/// combinatorial explosion of typed structs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Stable identifier for this task (used in traces + Trust Certs).
    pub id: String,
    /// Which mini-agent kind should handle this task.
    pub kind: MiniKind,
    /// Opaque input blob, shape defined per-kind.
    pub input: serde_json::Value,
    /// Budget ceiling for this specific invocation.
    #[serde(default)]
    pub budget: Budget,
    /// Optional parent task id — used by the conductor to build the
    /// run's DAG.
    #[serde(default)]
    pub parent_id: Option<String>,
}

/// Structured output from a mini-agent. `output` is opaque; the
/// [`MiniKind`] documents its shape. Conductors merge outputs
/// deterministically — they never feed raw mini-agent transcripts back
/// into an LLM (the "context bleed" guardrail from
/// `docs/NEXUS_MASTER_PLAN.md` §2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiniOutput {
    pub task_id: String,
    pub kind: MiniKind,
    pub output: serde_json::Value,
    /// Tokens actually consumed (input + output).
    pub tokens_used: u32,
    /// Wall-clock time this mini-agent ran.
    #[serde(with = "duration_ms")]
    pub duration: Duration,
    /// Dollars actually spent.
    pub cost_usd: f64,
    /// `true` if the mini-agent flagged its output as needing human or
    /// conductor review (e.g. a `merge.resolver` that found a non-
    /// trivial conflict).
    #[serde(default)]
    pub needs_review: bool,
}

/// Every mini-agent error shape worth distinguishing at the conductor
/// level. Narrow on purpose — the conductor reacts to these; deeper
/// diagnostic detail goes in the span / trace.
#[derive(Debug, Error)]
pub enum MiniError {
    #[error("budget exceeded: {dimension}")]
    BudgetExceeded { dimension: &'static str },
    #[error("bad input for kind {kind}: {reason}", kind = kind.as_wire_str())]
    BadInput { kind: MiniKind, reason: String },
    #[error("upstream provider failed: {0}")]
    Provider(String),
    #[error("mini-agent internal error: {0}")]
    Internal(String),
}

/// The single trait every mini-agent implements. Intentionally tiny —
/// any additional behaviour (retries, caching, budget enforcement) is
/// layered by the `SwarmConductor`, not by individual mini-agents.
#[async_trait]
pub trait MiniAgent: Send + Sync {
    /// Which canonical kind this implementation satisfies.
    fn kind(&self) -> MiniKind;

    /// Run exactly one task. Implementations MUST NOT retain state
    /// between calls beyond what is explicitly supplied via `task`.
    async fn run(&self, task: Task) -> Result<MiniOutput, MiniError>;
}

mod duration_ms {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S: Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
        (d.as_millis() as u64).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        let ms = u64::deserialize(d)?;
        Ok(Duration::from_millis(ms))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_a_stable_wire_name() {
        for k in MiniKind::all() {
            let s = k.as_wire_str();
            assert!(!s.is_empty(), "kind missing wire name: {k:?}");
            assert!(
                s.contains('.'),
                "wire name must be dotted (got {s}): {k:?}"
            );
        }
    }

    #[test]
    fn default_budget_is_narrow() {
        let b = Budget::default();
        assert!(b.tokens <= 2_000);
        assert!(b.wall_clock.as_secs() <= 30);
        assert!(b.cost_usd <= 0.10);
    }

    #[test]
    fn canonical_set_has_v1_count() {
        // Must match docs/NEXUS_MASTER_PLAN.md §2 which pins the v1.0
        // canonical set at 24. Adding a kind requires updating both
        // files together.
        assert_eq!(MiniKind::all().len(), 24);
    }

    #[test]
    fn task_and_output_roundtrip() {
        let t = Task {
            id: "t-1".into(),
            kind: MiniKind::FsReader,
            input: serde_json::json!({"path": "README.md"}),
            budget: Budget::default(),
            parent_id: None,
        };
        let j = serde_json::to_string(&t).unwrap();
        let back: Task = serde_json::from_str(&j).unwrap();
        assert_eq!(back.kind, MiniKind::FsReader);

        let o = MiniOutput {
            task_id: "t-1".into(),
            kind: MiniKind::FsReader,
            output: serde_json::json!({"summary": "ok"}),
            tokens_used: 320,
            duration: std::time::Duration::from_millis(1_234),
            cost_usd: 0.001,
            needs_review: false,
        };
        let j = serde_json::to_string(&o).unwrap();
        let back: MiniOutput = serde_json::from_str(&j).unwrap();
        assert_eq!(back.tokens_used, 320);
    }
}
