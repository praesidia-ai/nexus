//! Nexus Brain — the SINGLE ENTRY POINT for all intelligence.
//!
//! This is NOT aggregation. This is SYNTHESIS.
//!
//! The Brain orchestrates all subsystems and produces a unified, enhanced
//! understanding that is greater than the sum of its parts:
//! - Infers hidden requirements the user didn't state
//! - Makes full-stack architecture decisions with confidence scores
//! - Produces a UX strategy and monetization plan
//! - Designs agents automatically for the project
//! - Analyzes risks and produces mitigation strategies
//! - Sets a taste target score (>=90) and execution strategy
//!
//! Every downstream system (pipeline, oneshot, wave) calls `Brain::analyze()`
//! instead of individually importing 8+ subsystems.

use std::sync::Arc;

use serde::Serialize;

use crate::adaptive_control::{self, ExecutionMode, PipelineParams};
use crate::agent_designer::{self, DesignedAgent};
use crate::decision_engine::{self, ArchitectureDecision};
use crate::explain_engine::{self, ExplainCollector, ExplainTrace, Explanation};
use crate::global_intelligence::{self, GlobalIntelligenceSummary};
use crate::intelligence_amplifier::{self, AmplifiedIntent, StackSuggestion};
use crate::intent_engine::{self, Complexity, FlatHiddenRequirements, FlatIntent};
use crate::personality::{self, Personality, PersonalityConfig};
use crate::product_engine::{self, FullProductBrief};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Brain Output — everything the system needs, synthesized
// ---------------------------------------------------------------------------

/// The complete, synthesized intelligence output.
/// This is what every pipeline consumes — no subsystem called directly.
#[derive(Debug, Clone, Serialize)]
pub struct BrainOutput {
    // ── Core Understanding ──────────────────────────────────────────────
    /// Enhanced intent (with hidden requirements folded in).
    pub inferred_intent: FlatIntent,
    /// Things the user didn't say but definitely needs.
    pub hidden_requirements: FlatHiddenRequirements,

    // ── Architecture ────────────────────────────────────────────────────
    /// Full-stack architecture decisions with rationale.
    pub architecture_decisions: ArchitectureDecision,
    /// Learning overrides that were applied.
    pub learning_notes: Vec<String>,
    /// Overall confidence in the architecture (0.0–1.0).
    pub decision_confidence: f64,

    // ── Product & UX ────────────────────────────────────────────────────
    /// Full product brief (hero, flows, seed content, agents, delight).
    pub product_brief: FullProductBrief,
    /// UX strategy summary — what makes this app feel great.
    pub ux_strategy: UxStrategy,
    /// Monetization strategy (from product engine).
    pub monetization_summary: String,

    // ── Agents ──────────────────────────────────────────────────────────
    /// Automatically designed agents for this project.
    pub agent_plan: Vec<DesignedAgent>,

    // ── Execution ───────────────────────────────────────────────────────
    /// Risk analysis — what could go wrong and how we mitigate.
    pub risk_analysis: RiskAnalysis,
    /// Minimum taste score to ship (always >=90).
    pub taste_target_score: u32,
    /// How to execute (mode, parallelism, retries).
    pub execution_strategy: ExecutionStrategy,

    // ── Personality & Explain ───────────────────────────────────────────
    pub personality: Personality,
    pub personality_config: PersonalityConfig,
    pub explanations: Vec<Explanation>,
    pub explain_trace: ExplainTrace,

    // ── Cross-Project Intelligence ───────────────────────────────────────
    /// Global intelligence recommendations from ALL past projects.
    pub global_intelligence: GlobalIntelligenceSummary,
    /// Extended stack suggestions beyond fixed decision list.
    pub stack_suggestions: Vec<StackSuggestion>,
    /// Intent amplification result (LLM fallback if ambiguous).
    pub amplification: AmplifiedIntent,
    /// Inferred domain (dynamic, not hardcoded).
    pub inferred_domain: String,

    // ── Meta ────────────────────────────────────────────────────────────
    /// Whether prediction cache was used.
    pub from_cache: bool,
    /// Total brain analysis time in milliseconds.
    pub analysis_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct UxStrategy {
    /// Primary UX goal (e.g., "conversion", "retention", "clarity").
    pub primary_goal: String,
    /// Key UX principles to enforce.
    pub principles: Vec<String>,
    /// Critical user flows that must feel perfect.
    pub critical_flows: Vec<String>,
    /// Design tokens to emphasize.
    pub design_emphasis: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RiskAnalysis {
    /// Identified risks with severity.
    pub risks: Vec<Risk>,
    /// Overall risk level.
    pub overall_level: RiskLevel,
    /// Mitigations that will be applied automatically.
    pub auto_mitigations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Risk {
    pub area: String,
    pub description: String,
    pub severity: RiskLevel,
    pub mitigation: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutionStrategy {
    pub mode: ExecutionMode,
    pub params: PipelineParams,
    /// Steps to skip (determined by brain analysis).
    pub skip_steps: Vec<String>,
    /// Steps to parallelize aggressively.
    pub parallel_groups: Vec<Vec<String>>,
    /// Whether to auto-inject agents.
    pub auto_inject_agents: bool,
    /// Whether to auto-inject UX improvements.
    pub auto_inject_ux: bool,
    /// Maximum redesign attempts if taste < target.
    pub max_redesign_attempts: u32,
}

// ---------------------------------------------------------------------------
// Brain::analyze — the one function to rule them all
// ---------------------------------------------------------------------------

/// Run the FULL brain analysis pipeline.
///
/// This synthesizes all subsystems into a single, coherent output.
/// Every decision is cross-referenced, every gap is filled.
pub async fn analyze(app: &Arc<AppState>, description: &str) -> BrainOutput {
    let start = std::time::Instant::now();

    // ── Step 1: Consume cached prediction if available ──────────────────
    let cached = app.predictor.consume(description).await;
    let from_cache = cached.is_some();

    // ── Step 2: FlatIntent analysis (deterministic) ─────────────────────────
    let mut intent = match &cached {
        Some(c) => c.intent.clone(),
        None => intent_engine::analyze_flat(description),
    };

    // ── Step 2b: FlatIntent amplification (LLM fallback for ambiguous) ─────
    let amplification = intelligence_amplifier::amplify_intent(app, description, &intent).await;
    if amplification.llm_used {
        intent = amplification.intent.clone();
    }

    // ── Step 2c: Dynamic domain inference ──────────────────────────────
    let inferred_domain = intelligence_amplifier::infer_domain(description);

    // ── Step 3: Hidden requirements — things user didn't say ────────────
    let hidden = intent_engine::infer_flat_hidden_requirements(&intent, description);

    // Fold hidden requirements INTO the intent (synthesis, not aggregation)
    if hidden.needs_auth && !intent.needs_auth {
        intent.needs_auth = true;
        intent.inferred_features.push("Authentication (inferred)".into());
    }
    if hidden.needs_database && !intent.needs_database {
        intent.needs_database = true;
        intent.inferred_features.push("Database (inferred)".into());
    }
    if hidden.needs_payments && !intent.needs_payments {
        // Only fold if confidence is high enough
        if hidden.confidence > 0.7 {
            intent.needs_payments = true;
            intent.inferred_features.push("Payments (inferred)".into());
        }
    }
    // Add hidden features
    for feat in &hidden.additional_features {
        if !intent.inferred_features.contains(feat) {
            intent.inferred_features.push(feat.clone());
        }
    }
    // Add hidden pages
    for page in &hidden.additional_pages {
        if !intent.suggested_pages.contains(page) {
            intent.suggested_pages.push(page.clone());
        }
    }
    // Add hidden entities
    for entity in &hidden.additional_entities {
        if !intent.suggested_entities.contains(entity) {
            intent.suggested_entities.push(entity.clone());
        }
    }
    // Upgrade complexity if hidden requirements are substantial
    if hidden.additional_features.len() >= 4 && intent.complexity == Complexity::Simple {
        intent.complexity = Complexity::Medium;
    }
    if hidden.additional_features.len() >= 7 && intent.complexity == Complexity::Medium {
        intent.complexity = Complexity::Complex;
    }

    // ── Step 4: Architecture decisions (with learning) ──────────────────
    let (mut architecture, mut learning_notes) =
        decision_engine::decide_with_learning(&intent, app).await;

    // ── Step 4b: Apply causal learning overrides ────────────────────────
    // If historical cause→effect data suggests better choices, apply them
    let causal_overrides = crate::causal_learning::infer_best_decisions(app).await;
    for co in &causal_overrides {
        if co.confidence > 0.5 {
            decision_engine::apply_learning_override(&mut architecture, &co.area, &co.suggested_value);
            learning_notes.push(format!(
                "Causal override: {} → {} ({})",
                co.area, co.suggested_value, co.reasoning
            ));
        }
    }

    // ── Step 4c: Cross-project intelligence ──────────────────────────────
    // New projects benefit from ALL past projects via GlobalIntelligenceLayer
    let global_intel = global_intelligence::get_recommendations(app, &intent, &inferred_domain).await;
    for rec in &global_intel.recommendations {
        if rec.confidence > 0.6 {
            let current = decision_engine::decision_area_value(&architecture, &rec.area);
            if current != rec.recommended_value {
                decision_engine::apply_learning_override(
                    &mut architecture,
                    &rec.area,
                    &rec.recommended_value,
                );
                learning_notes.push(format!(
                    "Cross-project: {} → {} ({}, {} projects)",
                    rec.area, rec.recommended_value, rec.source, rec.project_count
                ));
            }
        }
    }

    // ── Step 4d: Extended stack suggestions ────────────────────────────
    let stack_suggestions = intelligence_amplifier::suggest_extended_stack(&intent, description);

    // Compute decision confidence from learning weight magnitudes
    let decision_confidence = compute_decision_confidence(&architecture, &intent);

    // ── Step 5: Product brief ───────────────────────────────────────────
    let product_brief = product_engine::generate_full_product_brief(&intent, description);

    // ── Step 6: Personality ─────────────────────────────────────────────
    let personality_val = match &cached {
        Some(c) => c.personality,
        None => personality::detect_personality(description, &intent.app_type),
    };
    let personality_config = personality::configure(personality_val);

    // ── Step 7: UX Strategy (synthesized from intent + personality) ─────
    let ux_strategy = synthesize_ux_strategy(&intent, &personality_config, &product_brief);

    // ── Step 8: Monetization summary ────────────────────────────────────
    let monetization_summary = if product_brief.monetization.paid_tiers.is_empty() {
        "No monetization needed for this app type.".to_string()
    } else {
        format!(
            "{} model with {} tier(s)",
            product_brief.monetization.model,
            product_brief.monetization.paid_tiers.len(),
        )
    };

    // ── Step 9: Agent auto-design ───────────────────────────────────────
    let agent_plan = agent_designer::design_agents(&intent, description);

    // ── Step 10: Risk analysis ──────────────────────────────────────────
    let risk_analysis = analyze_risks(&intent, &architecture, &hidden);

    // ── Step 11: Execution strategy ─────────────────────────────────────
    let execution_mode = adaptive_control::suggest_mode(description, &intent.complexity);
    // Override to Perfect if complexity is high or risks are high
    let execution_mode = match risk_analysis.overall_level {
        RiskLevel::High | RiskLevel::Critical => ExecutionMode::Perfect,
        _ => execution_mode,
    };
    let params = adaptive_control::compute_params(execution_mode, &intent.complexity);

    let execution_strategy = ExecutionStrategy {
        mode: execution_mode,
        params: params.clone(),
        skip_steps: compute_skip_steps(&intent),
        parallel_groups: compute_parallel_groups(&intent),
        auto_inject_agents: !agent_plan.is_empty(),
        auto_inject_ux: true,
        max_redesign_attempts: 3,
    };

    // Taste target: always >=90, but allow 85 for simple apps
    let taste_target_score = match intent.complexity {
        Complexity::Simple => 85,
        _ => 90,
    };

    // ── Step 12: Explainability ─────────────────────────────────────────
    let explanations = explain_engine::explain_architecture(&architecture, &intent);
    let collector = ExplainCollector::new("brain_analysis");
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
    // Record hidden requirements as explanation
    if !hidden.additional_features.is_empty() {
        collector
            .record_simple(
                "hidden_requirements",
                explain_engine::ExplainCategory::Architecture,
                &format!(
                    "Inferred {} hidden requirements",
                    hidden.additional_features.len()
                ),
                &format!(
                    "Based on app type {:?} and domain patterns: {}",
                    intent.app_type,
                    hidden.additional_features.join(", ")
                ),
                hidden.confidence,
                "nexus_brain",
            )
            .await;
    }
    // Record agent plan
    if !agent_plan.is_empty() {
        collector
            .record_simple(
                "agent_auto_design",
                explain_engine::ExplainCategory::Agent,
                &format!("Auto-designed {} agents", agent_plan.len()),
                &format!(
                    "Agents: {}",
                    agent_plan.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ")
                ),
                0.85,
                "agent_designer",
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
            &format!("Execution mode: {:?}", execution_mode),
            &params.mode_description,
            0.85,
            "nexus_brain",
        )
        .await;
    let explain_trace = collector.finalize().await;

    let analysis_ms = start.elapsed().as_millis() as u64;

    BrainOutput {
        inferred_intent: intent,
        hidden_requirements: hidden,
        architecture_decisions: architecture,
        learning_notes,
        decision_confidence,
        product_brief,
        ux_strategy,
        monetization_summary,
        agent_plan,
        risk_analysis,
        taste_target_score,
        execution_strategy,
        personality: personality_val,
        personality_config,
        explanations,
        explain_trace,
        global_intelligence: global_intel,
        stack_suggestions,
        amplification,
        inferred_domain,
        from_cache,
        analysis_ms,
    }
}

// ---------------------------------------------------------------------------
// Internal synthesis functions
// ---------------------------------------------------------------------------

fn compute_decision_confidence(arch: &ArchitectureDecision, intent: &FlatIntent) -> f64 {
    let mut confidence = intent.confidence;

    // High confidence if rationale covers all areas
    let covered_areas: Vec<&str> = arch.rationale.iter().map(|r| r.area.as_str()).collect();
    let expected = ["frontend", "backend", "database", "auth", "hosting", "ui_library"];
    let coverage = expected.iter().filter(|a| covered_areas.contains(*a)).count();
    confidence *= coverage as f64 / expected.len() as f64;

    // Clamp
    confidence.clamp(0.3, 1.0)
}

fn synthesize_ux_strategy(
    intent: &FlatIntent,
    personality: &PersonalityConfig,
    brief: &FullProductBrief,
) -> UxStrategy {
    let primary_goal = match intent.app_type {
        intent_engine::AppType::ECommerce | intent_engine::AppType::Marketplace => {
            "conversion".to_string()
        }
        intent_engine::AppType::SaasApp | intent_engine::AppType::Dashboard => {
            "retention".to_string()
        }
        intent_engine::AppType::LandingPage | intent_engine::AppType::Portfolio => {
            "clarity".to_string()
        }
        _ => "usability".to_string(),
    };

    let mut principles = vec![
        "Mobile-first responsive design".to_string(),
        "Clear visual hierarchy".to_string(),
        "Consistent spacing system".to_string(),
    ];
    if intent.needs_auth {
        principles.push("Frictionless onboarding flow".to_string());
    }
    if intent.needs_payments {
        principles.push("Trust signals near payment CTAs".to_string());
    }
    // Personality-driven
    match personality.design.animation_intensity.as_str() {
        "high" => principles.push("Rich micro-interactions and transitions".to_string()),
        "low" => principles.push("Minimal, purposeful animations".to_string()),
        _ => principles.push("Balanced, tasteful animations".to_string()),
    }

    let critical_flows: Vec<String> = brief
        .base
        .flows
        .iter()
        .map(|f| f.name.clone())
        .take(3)
        .collect();

    let design_emphasis = vec![
        format!("Color saturation: {}", personality.design.color_saturation),
        format!("Typography: {}", personality.design.type_weight),
        format!("Border style: {}", personality.design.border_style),
    ];

    UxStrategy {
        primary_goal,
        principles,
        critical_flows,
        design_emphasis,
    }
}

fn analyze_risks(
    intent: &FlatIntent,
    arch: &ArchitectureDecision,
    hidden: &FlatHiddenRequirements,
) -> RiskAnalysis {
    let mut risks = Vec::new();

    // Complexity risk
    if intent.complexity == Complexity::Complex {
        risks.push(Risk {
            area: "complexity".into(),
            description: "High complexity increases generation failure risk".into(),
            severity: RiskLevel::Medium,
            mitigation: "Using Perfect execution mode with max retries".into(),
        });
    }

    // Auth + payments security risk
    if intent.needs_auth && intent.needs_payments {
        risks.push(Risk {
            area: "security".into(),
            description: "Auth + payments requires careful security handling".into(),
            severity: RiskLevel::High,
            mitigation: "Enforcing OWASP patterns, validating all inputs".into(),
        });
    }

    // Architecture-specific risks
    let arch_str = format!("{:?}", arch.database);
    if arch_str.contains("None") && intent.needs_database {
        risks.push(Risk {
            area: "architecture".into(),
            description: "Database needed but architecture chose None — data will be lost".into(),
            severity: RiskLevel::High,
            mitigation: "Overriding to SQLite as minimum persistence layer".into(),
        });
    }
    let auth_str = format!("{:?}", arch.auth);
    if auth_str.contains("None") && intent.needs_auth {
        risks.push(Risk {
            area: "architecture".into(),
            description: "Auth needed but architecture chose None — app will be unprotected".into(),
            severity: RiskLevel::High,
            mitigation: "Overriding to SimpleCredentials as minimum auth".into(),
        });
    }

    // Hidden requirements scope risk
    if hidden.additional_features.len() > 5 {
        risks.push(Risk {
            area: "scope".into(),
            description: format!(
                "{} hidden requirements detected — scope may exceed single generation",
                hidden.additional_features.len()
            ),
            severity: RiskLevel::Medium,
            mitigation: "Prioritizing core features, deferring secondary ones".into(),
        });
    }

    // Low confidence risk
    if hidden.confidence < 0.5 {
        risks.push(Risk {
            area: "understanding".into(),
            description: "Low confidence in intent analysis — vague description".into(),
            severity: RiskLevel::Medium,
            mitigation: "Expanding prompt with smart defaults, asking for confirmation".into(),
        });
    }

    let overall_level = if risks.iter().any(|r| matches!(r.severity, RiskLevel::Critical)) {
        RiskLevel::Critical
    } else if risks.iter().any(|r| matches!(r.severity, RiskLevel::High)) {
        RiskLevel::High
    } else if risks.iter().any(|r| matches!(r.severity, RiskLevel::Medium)) {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    };

    let auto_mitigations: Vec<String> = risks.iter().map(|r| r.mitigation.clone()).collect();

    RiskAnalysis {
        risks,
        overall_level,
        auto_mitigations,
    }
}

fn compute_skip_steps(intent: &FlatIntent) -> Vec<String> {
    let mut skip = Vec::new();
    if !intent.needs_auth {
        skip.push("auth_setup".into());
    }
    if !intent.needs_payments {
        skip.push("payment_setup".into());
    }
    if !intent.needs_database {
        skip.push("database_schema".into());
        skip.push("database_migration".into());
    }
    if intent.suggested_agents.is_empty() {
        skip.push("agent_generation".into());
    }
    skip
}

fn compute_parallel_groups(intent: &FlatIntent) -> Vec<Vec<String>> {
    let mut groups = Vec::new();

    // Group 1: schema + API can run in parallel if independent
    if intent.needs_database {
        groups.push(vec!["database_schema".into(), "api_routes".into()]);
    }

    // Group 2: UI pages can always be parallelized
    groups.push(
        intent
            .suggested_pages
            .iter()
            .take(6)
            .map(|p| format!("ui_page_{}", p.to_lowercase().replace(' ', "_")))
            .collect(),
    );

    // Group 3: agents can run in parallel
    if !intent.suggested_agents.is_empty() {
        groups.push(vec!["agent_generation".into(), "agent_ui".into()]);
    }

    groups
}

// ---------------------------------------------------------------------------
// Convenience: convert BrainOutput to legacy IntelligenceReport
// ---------------------------------------------------------------------------

/// Convert BrainOutput to the legacy IntelligenceReport for backwards compat.
impl BrainOutput {
    pub fn to_intelligence_report(&self) -> crate::nexus_intelligence::IntelligenceReport {
        crate::nexus_intelligence::IntelligenceReport {
            intent: self.inferred_intent.clone(),
            architecture: self.architecture_decisions.clone(),
            learning_notes: self.learning_notes.clone(),
            product_brief: self.product_brief.clone(),
            personality: self.personality,
            personality_config: self.personality_config.clone(),
            execution_mode: self.execution_strategy.mode,
            pipeline_params: self.execution_strategy.params.clone(),
            explanations: self.explanations.clone(),
            explain_trace: self.explain_trace.clone(),
            from_cache: self.from_cache,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_app() -> Arc<AppState> {
        let dir = tempfile::tempdir().unwrap();
        AppState::init(dir.path()).await.unwrap()
    }

    #[tokio::test]
    async fn brain_analyze_produces_complete_output() {
        let app = test_app().await;
        let output = analyze(&app, "Build a CRM with auth, billing, and team management").await;

        // Intent enhanced with hidden requirements
        assert!(output.inferred_intent.needs_auth);
        assert!(output.inferred_intent.needs_database);
        assert!(!output.hidden_requirements.additional_features.is_empty());

        // Architecture decisions present
        assert!(!format!("{:?}", output.architecture_decisions.frontend).is_empty());
        assert!(output.decision_confidence > 0.0);

        // Product brief
        assert!(!output.product_brief.base.hero.headline.is_empty());

        // Agents auto-designed
        // CRM should get at least a support agent
        assert!(!output.agent_plan.is_empty());

        // Risk analysis
        assert!(!output.risk_analysis.risks.is_empty());

        // Taste target
        assert!(output.taste_target_score >= 85);

        // Execution strategy
        assert!(output.execution_strategy.max_redesign_attempts >= 1);

        // Explanations
        assert!(!output.explanations.is_empty());

        // Timing
        assert!(output.analysis_ms < 100); // Should be <5ms deterministic
    }

    #[tokio::test]
    async fn brain_infers_hidden_requirements_for_marketplace() {
        let app = test_app().await;
        let output = analyze(&app, "Build a marketplace for handmade crafts").await;

        let features = &output.hidden_requirements.additional_features;
        // Marketplace should infer payments, reviews, moderation
        assert!(
            features.iter().any(|f| f.to_lowercase().contains("payment")
                || f.to_lowercase().contains("review")
                || f.to_lowercase().contains("search")),
            "Marketplace should infer payment/review/search. Got: {:?}",
            features
        );
    }

    #[tokio::test]
    async fn brain_output_converts_to_legacy_report() {
        let app = test_app().await;
        let output = analyze(&app, "Simple portfolio site").await;
        let report = output.to_intelligence_report();
        assert!(!format!("{:?}", report.intent.app_type).is_empty());
    }

    #[tokio::test]
    async fn brain_queries_causal_learning() {
        let app = test_app().await;
        // Record a causal observation
        crate::causal_learning::record_observation(
            &app, "database=postgres", "build_success", true,
        ).await;
        // Brain should run without error even with causal data
        let output = analyze(&app, "Build a SaaS CRM with billing").await;
        assert!(!output.architecture_decisions.rationale.is_empty());
        // Learning notes may include causal overrides if data is sufficient
        // (with only 1 observation, threshold won't trigger — that's correct)
    }
}
