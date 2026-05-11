//! Strategy Engine — Nexus as product strategist, not just builder.
//!
//! Analyzes the current project and suggests the next best features to build,
//! prioritized by estimated impact. Integrates with nexus_brain, autonomous_engine,
//! and anticipation_engine.

use std::path::Path;
use std::sync::Arc;

use serde::Serialize;

use crate::intent_engine::{AppType, FlatIntent};
use crate::state::AppState;
use crate::taste_engine;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct StrategyPlan {
    /// Suggested features to build next, in priority order.
    pub suggested_features: Vec<FeatureSuggestion>,
    /// Overall project maturity assessment.
    pub maturity: ProjectMaturity,
    /// Strategic reasoning.
    pub reasoning: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeatureSuggestion {
    /// Feature name.
    pub name: String,
    /// Why this feature should be built next.
    pub rationale: String,
    /// Estimated impact (1-10).
    pub estimated_impact: u32,
    /// Estimated effort (1-10).
    pub estimated_effort: u32,
    /// Category.
    pub category: FeatureCategory,
    /// Priority score (impact * 10 / effort). Higher = build first.
    pub priority_score: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureCategory {
    Core,
    Growth,
    Retention,
    Quality,
    Infrastructure,
    Monetization,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectMaturity {
    /// No code generated yet.
    Idea,
    /// Code generated but untested.
    Prototype,
    /// Running and tested, basic features.
    Mvp,
    /// Polished, multiple features, good taste score.
    Product,
    /// Optimized, scaled, production-ready.
    Mature,
}

// ---------------------------------------------------------------------------
// Strategy analysis
// ---------------------------------------------------------------------------

/// Analyze a project and suggest the next best features to build.
pub async fn suggest_strategy(
    app: &Arc<AppState>,
    project_id: &str,
    intent: &FlatIntent,
) -> StrategyPlan {
    let project_dir = app
        .data_dir
        .join("projects")
        .join(project_id)
        .join("generated");

    let has_code = project_dir.exists() && project_dir.join("package.json").exists();
    let taste_score = if has_code {
        Some(taste_engine::score_project(&project_dir).overall)
    } else {
        None
    };
    let maturity = assess_maturity(has_code, taste_score, intent);

    let mut suggestions = Vec::new();
    let mut reasoning = Vec::new();

    // ── Stage-appropriate suggestions ──────────────────────────────────

    match maturity {
        ProjectMaturity::Idea => {
            reasoning.push("Project is in idea stage — focus on core generation".into());
            suggestions.push(FeatureSuggestion {
                name: "Generate initial application".into(),
                rationale: "No code exists yet — generate the full app from description".into(),
                estimated_impact: 10,
                estimated_effort: 3,
                category: FeatureCategory::Core,
                priority_score: 33,
            });
        }
        ProjectMaturity::Prototype => {
            reasoning.push("Prototype exists — focus on quality and testing".into());
            suggestions.push(FeatureSuggestion {
                name: "Run quality gate and auto-improve".into(),
                rationale: "Prototype needs quality validation before adding features".into(),
                estimated_impact: 8,
                estimated_effort: 2,
                category: FeatureCategory::Quality,
                priority_score: 40,
            });
            suggestions.push(FeatureSuggestion {
                name: "Add runtime tests".into(),
                rationale: "Validate that all pages and API endpoints work".into(),
                estimated_impact: 7,
                estimated_effort: 2,
                category: FeatureCategory::Quality,
                priority_score: 35,
            });
        }
        ProjectMaturity::Mvp => {
            reasoning.push("MVP is functional — focus on growth features".into());
            add_growth_suggestions(&mut suggestions, intent);
            add_retention_suggestions(&mut suggestions, intent);
        }
        ProjectMaturity::Product => {
            reasoning.push("Product is polished — focus on monetization and scale".into());
            add_monetization_suggestions(&mut suggestions, intent);
            add_infrastructure_suggestions(&mut suggestions, intent);
        }
        ProjectMaturity::Mature => {
            reasoning.push("Product is mature — focus on optimization and expansion".into());
            suggestions.push(FeatureSuggestion {
                name: "A/B testing framework".into(),
                rationale: "Mature product should validate changes with data".into(),
                estimated_impact: 7,
                estimated_effort: 5,
                category: FeatureCategory::Growth,
                priority_score: 14,
            });
            suggestions.push(FeatureSuggestion {
                name: "Analytics dashboard".into(),
                rationale: "Track user behavior, conversion funnels, and retention".into(),
                estimated_impact: 8,
                estimated_effort: 4,
                category: FeatureCategory::Growth,
                priority_score: 20,
            });
        }
    }

    // ── App-type-specific suggestions (always relevant) ────────────────
    add_type_specific_suggestions(&mut suggestions, intent, &maturity);

    // ── Missing feature detection ──────────────────────────────────────
    if has_code {
        add_missing_feature_suggestions(&mut suggestions, &project_dir, intent);
    }

    // Sort by priority score descending
    suggestions.sort_by(|a, b| b.priority_score.cmp(&a.priority_score));
    suggestions.truncate(8); // Top 8 suggestions

    // Add causal insights if available
    let overrides = crate::causal_learning::infer_best_decisions(app).await;
    for o in &overrides {
        reasoning.push(format!("Causal insight: {} (confidence: {:.0}%)", o.reasoning, o.confidence * 100.0));
    }

    StrategyPlan {
        suggested_features: suggestions,
        maturity,
        reasoning,
    }
}

/// Quick helper: suggest the single next best feature.
pub async fn suggest_next_best_feature(
    app: &Arc<AppState>,
    project_id: &str,
    intent: &FlatIntent,
) -> Option<FeatureSuggestion> {
    let plan = suggest_strategy(app, project_id, intent).await;
    plan.suggested_features.into_iter().next()
}

// ---------------------------------------------------------------------------
// Maturity assessment
// ---------------------------------------------------------------------------

fn assess_maturity(
    has_code: bool,
    taste_score: Option<u32>,
    _intent: &FlatIntent,
) -> ProjectMaturity {
    if !has_code {
        return ProjectMaturity::Idea;
    }
    match taste_score {
        None => ProjectMaturity::Prototype,
        Some(s) if s < 70 => ProjectMaturity::Prototype,
        Some(s) if s < 85 => ProjectMaturity::Mvp,
        Some(s) if s < 95 => ProjectMaturity::Product,
        Some(_) => ProjectMaturity::Mature,
    }
}

// ---------------------------------------------------------------------------
// Suggestion generators
// ---------------------------------------------------------------------------

fn add_growth_suggestions(suggestions: &mut Vec<FeatureSuggestion>, intent: &FlatIntent) {
    if intent.needs_auth {
        suggestions.push(FeatureSuggestion {
            name: "Social login (Google, GitHub)".into(),
            rationale: "Reduces signup friction by 40-60%".into(),
            estimated_impact: 7,
            estimated_effort: 3,
            category: FeatureCategory::Growth,
            priority_score: 23,
        });
        suggestions.push(FeatureSuggestion {
            name: "Invite/referral system".into(),
            rationale: "Viral growth loop — each user brings 1-2 more".into(),
            estimated_impact: 8,
            estimated_effort: 4,
            category: FeatureCategory::Growth,
            priority_score: 20,
        });
    }
    suggestions.push(FeatureSuggestion {
        name: "SEO optimization (meta tags, sitemap, structured data)".into(),
        rationale: "Organic traffic is free and compounds over time".into(),
        estimated_impact: 6,
        estimated_effort: 2,
        category: FeatureCategory::Growth,
        priority_score: 30,
    });
}

fn add_retention_suggestions(suggestions: &mut Vec<FeatureSuggestion>, intent: &FlatIntent) {
    suggestions.push(FeatureSuggestion {
        name: "Email notification system".into(),
        rationale: "Re-engage users who haven't visited in 7+ days".into(),
        estimated_impact: 7,
        estimated_effort: 4,
        category: FeatureCategory::Retention,
        priority_score: 17,
    });
    if matches!(intent.app_type, AppType::SaasApp | AppType::Dashboard) {
        suggestions.push(FeatureSuggestion {
            name: "Weekly summary email".into(),
            rationale: "Keeps users informed and gives them a reason to return".into(),
            estimated_impact: 6,
            estimated_effort: 3,
            category: FeatureCategory::Retention,
            priority_score: 20,
        });
    }
}

fn add_monetization_suggestions(suggestions: &mut Vec<FeatureSuggestion>, intent: &FlatIntent) {
    if intent.needs_payments {
        suggestions.push(FeatureSuggestion {
            name: "Subscription management portal".into(),
            rationale: "Let users upgrade/downgrade/cancel without support tickets".into(),
            estimated_impact: 8,
            estimated_effort: 4,
            category: FeatureCategory::Monetization,
            priority_score: 20,
        });
        suggestions.push(FeatureSuggestion {
            name: "Usage-based billing metering".into(),
            rationale: "Align revenue with value delivered — higher ARPU".into(),
            estimated_impact: 7,
            estimated_effort: 5,
            category: FeatureCategory::Monetization,
            priority_score: 14,
        });
    }
}

fn add_infrastructure_suggestions(suggestions: &mut Vec<FeatureSuggestion>, _intent: &FlatIntent) {
    suggestions.push(FeatureSuggestion {
        name: "Rate limiting and abuse prevention".into(),
        rationale: "Protect APIs from abuse before scaling".into(),
        estimated_impact: 6,
        estimated_effort: 2,
        category: FeatureCategory::Infrastructure,
        priority_score: 30,
    });
    suggestions.push(FeatureSuggestion {
        name: "Error monitoring (Sentry integration)".into(),
        rationale: "Catch production errors before users report them".into(),
        estimated_impact: 7,
        estimated_effort: 2,
        category: FeatureCategory::Infrastructure,
        priority_score: 35,
    });
}

fn add_type_specific_suggestions(
    suggestions: &mut Vec<FeatureSuggestion>,
    intent: &FlatIntent,
    maturity: &ProjectMaturity,
) {
    if matches!(maturity, ProjectMaturity::Idea | ProjectMaturity::Prototype) {
        return; // Too early for type-specific features
    }
    match intent.app_type {
        AppType::ECommerce => {
            suggestions.push(FeatureSuggestion {
                name: "Product recommendations engine".into(),
                rationale: "Personalized suggestions increase AOV by 15-30%".into(),
                estimated_impact: 9,
                estimated_effort: 5,
                category: FeatureCategory::Growth,
                priority_score: 18,
            });
        }
        AppType::SaasApp => {
            suggestions.push(FeatureSuggestion {
                name: "Team workspace with role-based access".into(),
                rationale: "Multi-seat = higher ARPU + stickiness".into(),
                estimated_impact: 8,
                estimated_effort: 5,
                category: FeatureCategory::Growth,
                priority_score: 16,
            });
        }
        AppType::Marketplace => {
            suggestions.push(FeatureSuggestion {
                name: "Seller analytics dashboard".into(),
                rationale: "Empowered sellers list more → more supply → more buyers".into(),
                estimated_impact: 7,
                estimated_effort: 4,
                category: FeatureCategory::Growth,
                priority_score: 17,
            });
        }
        _ => {}
    }
}

fn add_missing_feature_suggestions(
    suggestions: &mut Vec<FeatureSuggestion>,
    project_dir: &Path,
    intent: &FlatIntent,
) {
    let check = |pattern: &str| -> bool {
        crate::file_utils::contains_pattern(project_dir, pattern)
    };

    if intent.needs_auth && !check("loading") {
        suggestions.push(FeatureSuggestion {
            name: "Loading states and skeleton screens".into(),
            rationale: "Missing loading states create perceived slowness".into(),
            estimated_impact: 5,
            estimated_effort: 2,
            category: FeatureCategory::Quality,
            priority_score: 25,
        });
    }

    if !check("dark") {
        suggestions.push(FeatureSuggestion {
            name: "Dark mode support".into(),
            rationale: "Highly requested feature — improves accessibility and preference".into(),
            estimated_impact: 4,
            estimated_effort: 3,
            category: FeatureCategory::Quality,
            priority_score: 13,
        });
    }

    if !check("404") && !check("not-found") {
        suggestions.push(FeatureSuggestion {
            name: "Custom 404 page".into(),
            rationale: "Professional error handling improves trust".into(),
            estimated_impact: 3,
            estimated_effort: 1,
            category: FeatureCategory::Quality,
            priority_score: 30,
        });
    }
}

// walkcheck removed — use crate::file_utils::contains_pattern instead

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent_engine::analyze_flat;

    async fn test_app() -> Arc<AppState> {
        let dir = tempfile::tempdir().unwrap();
        AppState::init(dir.path()).await.unwrap()
    }

    #[tokio::test]
    async fn idea_stage_suggests_generation() {
        let app = test_app().await;
        let intent = analyze_flat("Build a CRM");
        let plan = suggest_strategy(&app, "nonexistent", &intent).await;
        assert!(matches!(plan.maturity, ProjectMaturity::Idea));
        assert!(plan.suggested_features.iter().any(|f| f.name.to_lowercase().contains("generate")));
    }

    #[tokio::test]
    async fn next_best_feature_returns_something() {
        let app = test_app().await;
        let intent = analyze_flat("Build a CRM");
        let next = suggest_next_best_feature(&app, "nonexistent", &intent).await;
        assert!(next.is_some());
    }

    #[test]
    fn maturity_assessment() {
        let intent = analyze_flat("SaaS app");
        assert!(matches!(assess_maturity(false, None, &intent), ProjectMaturity::Idea));
        assert!(matches!(assess_maturity(true, Some(60), &intent), ProjectMaturity::Prototype));
        assert!(matches!(assess_maturity(true, Some(80), &intent), ProjectMaturity::Mvp));
        assert!(matches!(assess_maturity(true, Some(90), &intent), ProjectMaturity::Product));
        assert!(matches!(assess_maturity(true, Some(96), &intent), ProjectMaturity::Mature));
    }
}
