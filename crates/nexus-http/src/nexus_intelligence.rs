//! Nexus Intelligence — unified facade over all intelligence subsystems.
//!
//! This is the SINGLE entry point that orchestrates:
//! - Intent analysis (intent_engine)
//! - Architecture decisions (decision_engine + decision_learning)
//! - Product brief generation (product_engine)
//! - Personality detection (personality)
//! - Execution mode selection (adaptive_control)
//! - Explainability (explain_engine)
//! - Thinking narrative (thinking_stream)
//! - Predictive preprocessing (predictive_preprocess — via AppState.predictor)
//! - Post-build analysis (post_build_intel)
//! - Quality gate (quality_gate)
//! - Continuous improvement (continuous_improve)
//! - Skill DNA composition (skill_dna)
//!
//! Before this module, each handler independently imported and called 5-8
//! subsystems. Now they call `NexusIntelligence` which handles sequencing,
//! caching, and cross-system wiring.

use std::path::Path;
use std::sync::Arc;

use serde::Serialize;

use crate::adaptive_control::{self, ExecutionMode, PipelineParams};
use crate::decision_engine::{self, ArchitectureDecision};
use crate::explain_engine::{self, ExplainCollector, ExplainTrace, Explanation};
use crate::intent_engine::{self, FlatIntent};
use crate::personality::{self, Personality, PersonalityConfig};
use crate::product_engine::{self, FullProductBrief};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Unified analysis result — everything the pipeline needs in one struct
// ---------------------------------------------------------------------------

/// Complete intelligence analysis for a user's description.
/// Produced once, consumed by the oneshot pipeline, handlers, and SSE events.
#[derive(Debug, Clone, Serialize)]
pub struct IntelligenceReport {
    /// What the user wants.
    pub intent: FlatIntent,
    /// How to build it.
    pub architecture: ArchitectureDecision,
    /// Learning overrides applied to architecture.
    pub learning_notes: Vec<String>,
    /// Full product brief (hero, flows, seed content, agents, delight).
    pub product_brief: FullProductBrief,
    /// Detected personality profile.
    pub personality: Personality,
    /// Personality configuration (tone, design influence, product influence).
    pub personality_config: PersonalityConfig,
    /// Suggested execution mode.
    pub execution_mode: ExecutionMode,
    /// Pipeline parameters for the suggested mode.
    pub pipeline_params: PipelineParams,
    /// Human-readable explanations of every decision.
    pub explanations: Vec<Explanation>,
    /// Full explanation trace (summary, counts, avg confidence).
    pub explain_trace: ExplainTrace,
    /// Whether this analysis was served from the prediction cache.
    pub from_cache: bool,
}

// ---------------------------------------------------------------------------
// Core function — analyze a description completely
// ---------------------------------------------------------------------------

/// Run the full intelligence pipeline on a description.
///
/// This is the one function the oneshot handler calls instead of
/// individually calling intent_engine, decision_engine, product_engine,
/// personality, adaptive_control, and explain_engine.
///
/// If the user was typing and predictions were cached (via `/intelligence/predict`),
/// the cached intent is reused (saving ~1ms of deterministic compute, but more
/// importantly providing consistent results between preview and execution).
pub async fn analyze(app: &Arc<AppState>, description: &str) -> IntelligenceReport {
    // 1. Try to consume cached prediction
    let cached = app.predictor.consume(description).await;
    let from_cache = cached.is_some();

    // 2. Intent analysis (cached or fresh)
    let intent = match &cached {
        Some(c) => c.intent.clone(),
        None => intent_engine::analyze_flat(description),
    };

    // 3. Architecture decisions (always apply learning even if intent was cached)
    let (architecture, learning_notes) =
        decision_engine::decide_with_learning(&intent, app).await;

    // 4. Product brief
    let product_brief = product_engine::generate_full_product_brief(&intent, description);

    // 5. Personality
    let personality_val = match &cached {
        Some(c) => c.personality,
        None => personality::detect_personality(description, &intent.app_type),
    };
    let personality_config = personality::configure(personality_val);

    // 6. Execution mode
    let execution_mode = adaptive_control::suggest_mode(description, &intent.complexity);
    let pipeline_params = adaptive_control::compute_params(execution_mode, &intent.complexity);

    // 7. Explainability — explain every architecture decision
    let explanations = explain_engine::explain_architecture(&architecture, &intent);
    let collector = ExplainCollector::new("analysis");
    for expl in &explanations {
        collector
            .record(
                &expl.decision_id,
                expl.category.clone(),
                &expl.decision,
                &expl.reason,
                expl.confidence,
                expl.alternatives.clone(),
                &expl.source,
            )
            .await;
    }
    collector
        .record_simple(
            "personality",
            explain_engine::ExplainCategory::Design,
            &format!("Using {:?} personality", personality_val),
            "Detected from project description — affects tone, design, and product decisions",
            0.8,
            "personality_engine",
        )
        .await;
    collector
        .record_simple(
            "execution_mode",
            explain_engine::ExplainCategory::Optimization,
            &format!("Suggested {:?} execution mode", execution_mode),
            &pipeline_params.mode_description,
            0.85,
            "adaptive_control",
        )
        .await;
    let explain_trace = collector.finalize().await;

    IntelligenceReport {
        intent,
        architecture,
        learning_notes,
        product_brief,
        personality: personality_val,
        personality_config,
        execution_mode,
        pipeline_params,
        explanations,
        explain_trace,
        from_cache,
    }
}

// ---------------------------------------------------------------------------
// Post-build analysis — run after code generation
// ---------------------------------------------------------------------------

/// Complete post-generation analysis result.
#[derive(Debug, Clone, Serialize)]
pub struct PostBuildReport {
    pub analysis: crate::post_build_intel::PostBuildAnalysis,
    pub improvement: Option<crate::continuous_improve::ImprovementResult>,
    pub quality_gate: Option<crate::quality_gate::GateResult>,
}

/// Run post-build intelligence: analyze → optionally improve → optionally gate.
pub async fn post_build(
    app: &Arc<AppState>,
    project_id: &str,
    project_dir: &Path,
    intent: &FlatIntent,
    auto_improve: bool,
    run_gate: bool,
) -> PostBuildReport {
    let analysis = crate::post_build_intel::analyze(project_dir, intent);

    let improvement = if auto_improve {
        Some(crate::continuous_improve::improve(project_dir, &analysis))
    } else {
        None
    };

    let quality_gate = if run_gate {
        let config = crate::quality_gate::QualityGateConfig::default();
        Some(
            crate::quality_gate::run_quality_gate(app, project_id, project_dir, &config, intent)
                .await,
        )
    } else {
        None
    };

    PostBuildReport {
        analysis,
        improvement,
        quality_gate,
    }
}

// ---------------------------------------------------------------------------
// Skill-enhanced prompt — compose agent prompt with learned skill DNA
// ---------------------------------------------------------------------------

/// Build an agent system prompt enhanced with relevant skill DNA.
pub async fn compose_agent_prompt(
    app: &Arc<AppState>,
    base_prompt: &str,
    task_description: &str,
) -> String {
    crate::skill_dna::compose_prompt(app, base_prompt, task_description).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;

    // Integration test requires a real AppState with DB — use a helper
    async fn test_app() -> Arc<AppState> {
        let dir = tempfile::tempdir().unwrap();
        AppState::init(dir.path()).await.unwrap()
    }

    #[tokio::test]
    async fn analyze_produces_complete_report() {
        let app = test_app().await;
        let report = analyze(&app, "Build a CRM with auth and billing").await;

        // Intent
        assert!(report.intent.needs_auth);
        assert!(report.intent.needs_database);

        // Architecture
        assert!(!format!("{:?}", report.architecture.frontend).is_empty());

        // Product brief
        assert!(!report.product_brief.base.hero.headline.is_empty());

        // Explanations
        assert!(!report.explanations.is_empty());
        assert!(report.explanations.iter().all(|e| e.confidence > 0.0));

        // Personality
        assert!(!format!("{:?}", report.personality).is_empty());

        // Execution mode
        assert!(!report.pipeline_params.mode_description.is_empty());

        // Explain trace
        assert!(report.explain_trace.total_decisions > 0);
    }

    #[tokio::test]
    async fn analyze_uses_cached_prediction() {
        let app = test_app().await;
        let desc = "Build an e-commerce store for shoes";

        // Pre-warm cache
        app.predictor.predict(desc).await;

        // Analyze should consume cache
        let report = analyze(&app, desc).await;
        assert!(report.from_cache);

        // Second call should NOT have cache (consumed)
        let report2 = analyze(&app, desc).await;
        assert!(!report2.from_cache);
    }
}
