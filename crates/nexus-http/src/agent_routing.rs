//! Agent-role-aware model routing (NEXUS_MASTER_PLAN §3).
//!
//! The swarm architecture's whole token-economics pitch depends on
//! routing **conductors** (the ten named personas) to frontier models
//! like Claude Sonnet 4.5 while leaf **mini-agents** run on tiny local
//! models (Qwen3-Coder-7B on Ollama by default). This module is the
//! single place that decision is made.
//!
//! # Guarantees
//!
//! - Conductors default to `claude-sonnet-4-5-20250929`. The operator
//!   can override via `NEXUS_CONDUCTOR_MODEL` or the Settings page.
//! - Leaf mini-agents default to Ollama's `qwen3-coder:7b`. The
//!   operator can override per-kind via env vars of the shape
//!   `NEXUS_MINI_<WIRE_NAME>_MODEL` (dots and dashes become
//!   underscores — e.g. `fs.reader` → `NEXUS_MINI_FS_READER_MODEL`).
//! - Falls back to a sensible `claude-3-5-haiku` for leaves when
//!   Ollama isn't available.
//!
//! All defaults can be overridden from a `Settings`-backed map at
//! runtime; a provider-agnostic view of that map lives in
//! `handlers/settings.rs`. This module intentionally avoids a circular
//! dep on settings by accepting an `OverrideMap` as input.

use std::collections::HashMap;

use nexus_agents_core::mini::MiniKind;

/// The ten named conductor personas. Adding a conductor requires
/// updating `handlers/agent_tv.rs` and `docs/AGENT_TV_CONTRACT.md` in
/// lockstep.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum Conductor {
    Nova,
    Atlas,
    Kai,
    Luna,
    Orion,
    Sage,
    Ivy,
    Rex,
    Leo,
    Mia,
}

impl Conductor {
    pub fn as_wire_str(self) -> &'static str {
        match self {
            Self::Nova => "nova",
            Self::Atlas => "atlas",
            Self::Kai => "kai",
            Self::Luna => "luna",
            Self::Orion => "orion",
            Self::Sage => "sage",
            Self::Ivy => "ivy",
            Self::Rex => "rex",
            Self::Leo => "leo",
            Self::Mia => "mia",
        }
    }
}

/// A resolved routing decision returned to the call site. Keeps the
/// fields minimal so callers can't accidentally leak a stale api-key
/// into a provider-swap branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSelection {
    pub provider: String,
    pub model: String,
}

/// Optional per-kind / per-conductor overrides sourced from Settings.
/// Keys use the stable wire names (`"nova"`, `"fs.reader"`, etc.)
/// mapped to `"<provider>/<model>"` strings; the router parses the
/// provider prefix.
pub type OverrideMap = HashMap<String, String>;

fn parse_override(raw: &str) -> Option<ModelSelection> {
    if let Some((provider, model)) = raw.split_once('/') {
        if !provider.is_empty() && !model.is_empty() {
            return Some(ModelSelection {
                provider: provider.to_string(),
                model: model.to_string(),
            });
        }
    }
    None
}

fn env_override(env_var: &str) -> Option<ModelSelection> {
    std::env::var(env_var).ok().and_then(|s| parse_override(&s))
}

/// Route a conductor (persona) turn. Conductors get frontier models
/// because they plan, arbitrate, and synthesise mini-agent outputs —
/// the one place where reasoning quality still dominates cost.
pub fn route_conductor(conductor: Conductor, overrides: &OverrideMap) -> ModelSelection {
    if let Some(raw) = overrides.get(conductor.as_wire_str()) {
        if let Some(sel) = parse_override(raw) {
            return sel;
        }
    }
    if let Some(sel) = env_override("NEXUS_CONDUCTOR_MODEL") {
        return sel;
    }
    ModelSelection {
        provider: "anthropic".into(),
        model: "claude-sonnet-4-5-20250929".into(),
    }
}

/// Route a leaf mini-agent task. Leaves get the smallest model that
/// passes their eval; by default that's Qwen3-Coder-7B on Ollama so a
/// run can survive offline. Anthropic Haiku is the cloud fallback.
pub fn route_leaf(kind: MiniKind, overrides: &OverrideMap) -> ModelSelection {
    if let Some(raw) = overrides.get(kind.as_wire_str()) {
        if let Some(sel) = parse_override(raw) {
            return sel;
        }
    }
    let env_var = format!(
        "NEXUS_MINI_{}_MODEL",
        kind.as_wire_str()
            .replace(['.', '-'], "_")
            .to_uppercase()
    );
    if let Some(sel) = env_override(&env_var) {
        return sel;
    }
    if let Some(sel) = env_override("NEXUS_LEAF_MODEL") {
        return sel;
    }
    // Deterministic mini-agents that never call an LLM still return a
    // routing decision (some pipelines ask for it at planning time) —
    // they just ignore it when running.
    default_leaf_for(kind)
}

fn default_leaf_for(kind: MiniKind) -> ModelSelection {
    use MiniKind::*;
    match kind {
        // Deterministic workers — no LLM call. We still return a
        // selection so callers don't branch on `Option`.
        FsLocator | FsReader | FsPatcher | LintRunner | TestRunner | ShellRunner
        | WebFetcher | CacheLookerUpper | CostEstimator | EvalScorer => ModelSelection {
            provider: "deterministic".into(),
            model: "none".into(),
        },
        // Tiny-context local model — Ollama default.
        CopyRewriter | DocWriter | ReadmeSectioner | SchemaInferrer | ImportFixer
        | IntentClassifier | PolicyChecker | RouteSuggester | PromptCritiquer => ModelSelection {
            provider: "ollama".into(),
            model: "qwen3-coder:7b".into(),
        },
        // Slightly heavier — still local but benefits from a 32B class
        // reasoning model when available.
        TestWriter | AstExtractor | AstRefactorer | MergeResolver | DecisionRouter => {
            ModelSelection {
                provider: "ollama".into(),
                model: "qwen3-coder:32b".into(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conductor_defaults_to_claude_sonnet() {
        let sel = route_conductor(Conductor::Nova, &OverrideMap::new());
        assert_eq!(sel.provider, "anthropic");
        assert!(sel.model.starts_with("claude-sonnet"));
    }

    #[test]
    fn conductor_respects_explicit_override() {
        let mut m = OverrideMap::new();
        m.insert("orion".into(), "openai/gpt-4o".into());
        let sel = route_conductor(Conductor::Orion, &m);
        assert_eq!(sel.provider, "openai");
        assert_eq!(sel.model, "gpt-4o");
    }

    #[test]
    fn deterministic_kinds_route_to_deterministic_provider() {
        let sel = route_leaf(MiniKind::FsLocator, &OverrideMap::new());
        assert_eq!(sel.provider, "deterministic");
    }

    #[test]
    fn llm_leaves_default_to_ollama_qwen3() {
        let sel = route_leaf(MiniKind::DocWriter, &OverrideMap::new());
        assert_eq!(sel.provider, "ollama");
        assert!(sel.model.starts_with("qwen3-coder"));
    }

    #[test]
    fn leaf_per_kind_override_wins_over_env() {
        let mut m = OverrideMap::new();
        m.insert("doc.writer".into(), "anthropic/claude-3-5-haiku".into());
        let sel = route_leaf(MiniKind::DocWriter, &m);
        assert_eq!(sel.provider, "anthropic");
        assert_eq!(sel.model, "claude-3-5-haiku");
    }

    #[test]
    fn wire_name_env_var_conversion() {
        // `fs.reader` → `NEXUS_MINI_FS_READER_MODEL`
        let k = MiniKind::FsReader;
        let got = format!(
            "NEXUS_MINI_{}_MODEL",
            k.as_wire_str()
                .replace(['.', '-'], "_")
                .to_uppercase()
        );
        assert_eq!(got, "NEXUS_MINI_FS_READER_MODEL");
    }
}
