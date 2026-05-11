---
name: add-engine
description: Add a new engine to nexus-http following the deterministic-first, LLM-fallback pattern. Use when building a new analysis, scoring, or decision subsystem.
---

# Adding a New Engine to nexus-http

## Core philosophy: "Deterministic intelligence first, LLM generation second"

Every engine must follow the layered approach:
1. **Deterministic layer** — keyword heuristics, rule matching, regex, scoring formulas. No LLM calls. Always fast, always predictable.
2. **Semantic layer (optional)** — LLM fallback only when confidence from deterministic layer is below threshold (typically `< 0.6`).

Study `intent_engine.rs` and `decision_engine.rs` as canonical references.

## File location

`crates/nexus-http/src/<name>_engine.rs`

Export from `crates/nexus-http/src/lib.rs` if other crates need it.

## Engine skeleton

```rust
//! <Name> Engine — <one-line description>.
//!
//! Layered approach:
//! 1. Deterministic heuristics (keyword/rule matching).
//! 2. Optional LLM semantic fallback for low-confidence inputs.

use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::state::AppState;

// ---------------------------------------------------------------------------
// Public output types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MyEngineResult {
    pub primary: MyCategory,
    pub confidence: f32,         // 0.0 – 1.0
    pub source: AnalysisSource,  // Deterministic | Semantic
    pub reasoning: String,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MyCategory {
    CategoryA,
    CategoryB,
    CategoryC,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnalysisSource {
    Deterministic,
    Semantic,
}

// ---------------------------------------------------------------------------
// Deterministic layer
// ---------------------------------------------------------------------------

struct DeterministicSignals {
    score: f32,
    category: MyCategory,
    matched_rules: Vec<&'static str>,
}

fn analyze_deterministic(input: &str) -> DeterministicSignals {
    let input_lower = input.to_lowercase();
    let mut score = 0.0_f32;
    let mut matched = vec![];
    let mut category = MyCategory::Unknown;

    // Rule 1 — keyword matching
    let category_a_keywords = ["keyword1", "keyword2", "keyword3"];
    let a_matches = category_a_keywords.iter()
        .filter(|k| input_lower.contains(*k))
        .count();
    if a_matches > 0 {
        score += (a_matches as f32 * 0.3).min(0.9);
        category = MyCategory::CategoryA;
        matched.push("category_a_keywords");
    }

    // Rule 2 — pattern matching
    if input_lower.contains("specific_phrase") {
        score = score.max(0.85);
        category = MyCategory::CategoryB;
        matched.push("specific_phrase_rule");
    }

    DeterministicSignals { score, category, matched_rules: matched }
}

// ---------------------------------------------------------------------------
// LLM semantic layer (only called when confidence < 0.6)
// ---------------------------------------------------------------------------

async fn analyze_semantic(
    input: &str,
    state: &Arc<AppState>,
) -> anyhow::Result<MyEngineResult> {
    let prompt = format!(
        "Classify the following input into one of: CategoryA, CategoryB, CategoryC.\n\
         Input: {input}\n\
         Respond in JSON: {{\"category\": \"...\", \"confidence\": 0.0-1.0, \"reasoning\": \"...\"}}",
    );

    let response = crate::llm_client::complete_json(state, &prompt).await?;
    // parse response...
    Ok(MyEngineResult {
        primary: MyCategory::CategoryA, // parsed from response
        confidence: 0.7,
        source: AnalysisSource::Semantic,
        reasoning: "LLM classified".into(),
        elapsed_ms: 0,
    })
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub async fn analyze(input: &str, state: &Arc<AppState>) -> MyEngineResult {
    let start = Instant::now();

    let det = analyze_deterministic(input);

    if det.score >= 0.6 {
        return MyEngineResult {
            primary: det.category,
            confidence: det.score,
            source: AnalysisSource::Deterministic,
            reasoning: format!("Deterministic rules matched: {:?}", det.matched_rules),
            elapsed_ms: start.elapsed().as_millis() as u64,
        };
    }

    // Fall through to semantic only when confidence is low
    match analyze_semantic(input, state).await {
        Ok(mut result) => {
            result.elapsed_ms = start.elapsed().as_millis() as u64;
            result
        }
        Err(_) => MyEngineResult {
            primary: MyCategory::Unknown,
            confidence: 0.0,
            source: AnalysisSource::Deterministic,
            reasoning: "Deterministic low-confidence, semantic fallback failed".into(),
            elapsed_ms: start.elapsed().as_millis() as u64,
        },
    }
}
```

## Calling the engine from a handler

```rust
// In your handler
let result = my_engine::analyze(&req.description, &state).await;

// Use confidence to gate downstream behavior
if result.confidence < 0.5 {
    tracing::warn!(confidence = result.confidence, "Low-confidence engine result");
}
```

## Calling the engine from the oneshot pipeline

If this engine should participate in the oneshot flow, add it to `handlers/oneshot.rs` in the appropriate phase and emit an SSE event:

```rust
let engine_result = my_engine::analyze(&req.description, &state).await;
tx.send(OneShotEvent::Phase {
    phase: "my_engine".into(),
    status: format!("category={:?} confidence={:.2}", engine_result.primary, engine_result.confidence),
}).ok();
```

## Plugin hook integration

If the engine result should be observable/overridable by plugins, call the hook after analysis:

```rust
let hook_result = plugin_hooks::run_hook(
    &state,
    HookPoint::OnMyEngineResult,
    serde_json::to_value(&engine_result)?,
).await;
```

## Invariants

- The deterministic layer MUST NOT make LLM calls (no `state` parameter in the deterministic fn)
- Always record `elapsed_ms` — the intent engine does this; follow the pattern
- Return `Unknown` / zero confidence rather than panicking on unexpected input
- All `pub` types must implement `Serialize + Deserialize` for SSE serialization
