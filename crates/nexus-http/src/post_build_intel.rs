//! Post-Build Intelligence — automatic analysis after app generation.
//!
//! Runs after the pipeline completes to identify:
//! - Missing features the user probably wants
//! - UX improvements that would make the app feel more polished
//! - Performance issues that should be addressed
//!
//! Returns actionable suggestions, not vague advice.
//! Each suggestion is tagged with priority, effort, and category.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::intent_engine::{AppType, FlatIntent};
use crate::taste_engine;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Complete post-build analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostBuildAnalysis {
    /// Missing features the user likely wants.
    pub missing_features: Vec<Suggestion>,
    /// UX improvements to make the app feel polished.
    pub ux_improvements: Vec<Suggestion>,
    /// Performance issues detected in generated code.
    pub performance_issues: Vec<Suggestion>,
    /// Overall readiness score (0–100).
    pub readiness_score: u32,
    /// One-line summary.
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    pub id: String,
    pub category: SuggestionCategory,
    pub title: String,
    pub description: String,
    pub priority: Priority,
    pub effort: Effort,
    /// If true, the continuous improvement loop can auto-apply this.
    pub auto_fixable: bool,
    /// Files that would be affected.
    pub affected_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionCategory {
    MissingFeature,
    UxImprovement,
    Performance,
    Accessibility,
    Security,
    Seo,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Critical,
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    Trivial,  // < 5 min, auto-fixable
    Small,    // 5-15 min
    Medium,   // 15-60 min
    Large,    // > 1 hour
}

// ---------------------------------------------------------------------------
// Analysis Engine
// ---------------------------------------------------------------------------

/// Run post-build analysis on a generated project.
pub fn analyze(project_dir: &Path, intent: &FlatIntent) -> PostBuildAnalysis {
    let mut missing = detect_missing_features(project_dir, intent);
    let mut ux = detect_ux_improvements(project_dir, intent);
    let mut perf = detect_performance_issues(project_dir);

    // Sort by priority
    missing.sort_by_key(|s| priority_rank(s.priority));
    ux.sort_by_key(|s| priority_rank(s.priority));
    perf.sort_by_key(|s| priority_rank(s.priority));

    let total_issues = missing.len() + ux.len() + perf.len();
    let critical_count = missing.iter().chain(ux.iter()).chain(perf.iter())
        .filter(|s| s.priority == Priority::Critical)
        .count();

    let readiness = if critical_count > 0 {
        50
    } else if total_issues > 10 {
        65
    } else if total_issues > 5 {
        75
    } else if total_issues > 0 {
        85
    } else {
        95
    };

    let summary = format!(
        "Found {} suggestions ({} critical). Readiness: {}%",
        total_issues, critical_count, readiness
    );

    PostBuildAnalysis {
        missing_features: missing,
        ux_improvements: ux,
        performance_issues: perf,
        readiness_score: readiness,
        summary,
    }
}

// ---------------------------------------------------------------------------
// Missing Feature Detection
// ---------------------------------------------------------------------------

fn detect_missing_features(project_dir: &Path, intent: &FlatIntent) -> Vec<Suggestion> {
    let mut suggestions = Vec::new();
    let files = collect_file_names(project_dir);

    // Check for 404 / not-found page
    let has_not_found = files.iter().any(|f| f.contains("not-found") || f.contains("404"));
    if !has_not_found {
        suggestions.push(Suggestion {
            id: "missing_404".into(),
            category: SuggestionCategory::MissingFeature,
            title: "Add a custom 404 page".into(),
            description: "Users who hit a broken link will see a default error. A custom 404 page keeps them engaged.".into(),
            priority: Priority::Medium,
            effort: Effort::Trivial,
            auto_fixable: true,
            affected_files: vec!["app/not-found.tsx".into()],
        });
    }

    // Check for loading states
    let has_loading = files.iter().any(|f| f.contains("loading."));
    if !has_loading && intent.needs_database {
        suggestions.push(Suggestion {
            id: "missing_loading".into(),
            category: SuggestionCategory::MissingFeature,
            title: "Add loading states".into(),
            description: "Pages that fetch data need loading indicators to prevent blank screens.".into(),
            priority: Priority::High,
            effort: Effort::Trivial,
            auto_fixable: true,
            affected_files: vec!["app/loading.tsx".into()],
        });
    }

    // Check for error boundary
    let has_error = files.iter().any(|f| f.contains("error."));
    if !has_error {
        suggestions.push(Suggestion {
            id: "missing_error_boundary".into(),
            category: SuggestionCategory::MissingFeature,
            title: "Add an error boundary".into(),
            description: "Unhandled errors will crash the page. An error boundary shows a friendly recovery screen.".into(),
            priority: Priority::High,
            effort: Effort::Trivial,
            auto_fixable: true,
            affected_files: vec!["app/error.tsx".into()],
        });
    }

    // Check for favicon
    let has_favicon = files.iter().any(|f| f.contains("favicon") || f.contains("icon."));
    if !has_favicon {
        suggestions.push(Suggestion {
            id: "missing_favicon".into(),
            category: SuggestionCategory::Seo,
            title: "Add a favicon".into(),
            description: "The browser tab shows a generic icon. A favicon reinforces brand identity.".into(),
            priority: Priority::Low,
            effort: Effort::Trivial,
            auto_fixable: true,
            affected_files: vec!["app/favicon.ico".into()],
        });
    }

    // If auth is present, check for protected routes
    if intent.needs_auth {
        let has_middleware = files.iter().any(|f| f.contains("middleware."));
        if !has_middleware {
            suggestions.push(Suggestion {
                id: "missing_auth_middleware".into(),
                category: SuggestionCategory::Security,
                title: "Add authentication middleware".into(),
                description: "Protected routes need middleware to redirect unauthenticated users.".into(),
                priority: Priority::Critical,
                effort: Effort::Small,
                auto_fixable: false,
                affected_files: vec!["middleware.ts".into()],
            });
        }
    }

    // E-commerce specific
    if matches!(intent.app_type, AppType::ECommerce | AppType::Marketplace) {
        let has_cart = files.iter().any(|f| f.contains("cart"));
        if !has_cart {
            suggestions.push(Suggestion {
                id: "missing_cart".into(),
                category: SuggestionCategory::MissingFeature,
                title: "Add a shopping cart".into(),
                description: "E-commerce stores need a cart to collect items before checkout.".into(),
                priority: Priority::Critical,
                effort: Effort::Medium,
                auto_fixable: false,
                affected_files: vec![
                    "app/cart/page.tsx".into(),
                    "lib/cart-store.ts".into(),
                ],
            });
        }
    }

    suggestions
}

// ---------------------------------------------------------------------------
// UX Improvement Detection
// ---------------------------------------------------------------------------

fn detect_ux_improvements(project_dir: &Path, intent: &FlatIntent) -> Vec<Suggestion> {
    let mut suggestions = Vec::new();

    // Run taste scoring for detailed signals
    let taste = taste_engine::score_project(project_dir);

    // Low accessibility score
    if taste.accessibility.score < 60 {
        for missing in &taste.accessibility.signals_missing {
            suggestions.push(Suggestion {
                id: format!("a11y_{}", missing.replace(' ', "_").to_lowercase()),
                category: SuggestionCategory::Accessibility,
                title: format!("Improve accessibility: {}", missing),
                description: format!("The app is missing {}. This affects users with disabilities and hurts SEO.", missing),
                priority: Priority::High,
                effort: Effort::Small,
                auto_fixable: missing.contains("alt") || missing.contains("aria"),
                affected_files: vec![],
            });
        }
    }

    // Low modern UI score
    if taste.modern_ui.score < 50 {
        suggestions.push(Suggestion {
            id: "ux_animations".into(),
            category: SuggestionCategory::UxImprovement,
            title: "Add entrance animations".into(),
            description: "Subtle animations on page load make the app feel polished and modern.".into(),
            priority: Priority::Medium,
            effort: Effort::Trivial,
            auto_fixable: true,
            affected_files: vec![],
        });
    }

    // Low conversion score (for public-facing apps)
    if taste.conversion.score < 50 && matches!(intent.app_type, AppType::LandingPage | AppType::SaasApp | AppType::ECommerce)
        && taste.conversion.signals_missing.iter().any(|s| s.contains("trust")) {
            suggestions.push(Suggestion {
                id: "ux_trust_signals".into(),
                category: SuggestionCategory::UxImprovement,
                title: "Add trust signals".into(),
                description: "Testimonials, star ratings, or partner logos increase user confidence.".into(),
                priority: Priority::Medium,
                effort: Effort::Small,
                auto_fixable: true,
                affected_files: vec![],
            });
        }

    suggestions
}

// ---------------------------------------------------------------------------
// Performance Detection
// ---------------------------------------------------------------------------

fn detect_performance_issues(project_dir: &Path) -> Vec<Suggestion> {
    let mut suggestions = Vec::new();
    let files = collect_files_with_content(project_dir);

    for (path, content) in &files {
        // Detect unoptimized images (using <img> instead of next/image)
        if content.contains("<img ") && path.ends_with(".tsx") && !content.contains("next/image") {
            suggestions.push(Suggestion {
                id: format!("perf_unoptimized_img_{}", sanitize_id(path)),
                category: SuggestionCategory::Performance,
                title: format!("Use next/image in {}", short_path(path)),
                description: "Using <img> instead of next/image skips automatic optimization, lazy loading, and responsive sizing.".into(),
                priority: Priority::Medium,
                effort: Effort::Trivial,
                auto_fixable: true,
                affected_files: vec![path.clone()],
            });
        }

        // Detect missing React.memo / useMemo opportunities
        // (large components that re-render on parent state change)
        if path.ends_with(".tsx") && content.contains("useState") {
            let component_count = content.matches("export function").count()
                + content.matches("export default function").count();
            if component_count > 3 {
                suggestions.push(Suggestion {
                    id: format!("perf_many_components_{}", sanitize_id(path)),
                    category: SuggestionCategory::Performance,
                    title: format!("Consider splitting {}", short_path(path)),
                    description: "This file has many components with state. Splitting into separate files improves code splitting and reduces re-renders.".into(),
                    priority: Priority::Low,
                    effort: Effort::Small,
                    auto_fixable: false,
                    affected_files: vec![path.clone()],
                });
            }
        }

        // Detect large inline data
        if content.len() > 10_000 && (content.contains("const data = [") || content.contains("const items = [")) {
            suggestions.push(Suggestion {
                id: format!("perf_inline_data_{}", sanitize_id(path)),
                category: SuggestionCategory::Performance,
                title: format!("Extract inline data from {}", short_path(path)),
                description: "Large inline arrays increase bundle size. Move them to a separate JSON file or API endpoint.".into(),
                priority: Priority::Low,
                effort: Effort::Trivial,
                auto_fixable: true,
                affected_files: vec![path.clone()],
            });
        }
    }

    suggestions
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn collect_file_names(dir: &Path) -> Vec<String> {
    crate::file_utils::collect_file_names(dir)
}

fn collect_files_with_content(dir: &Path) -> Vec<(String, String)> {
    crate::file_utils::collect_files_with_content(dir, &["ts", "tsx", "js", "jsx"])
}

fn priority_rank(p: Priority) -> u32 {
    match p {
        Priority::Critical => 0,
        Priority::High => 1,
        Priority::Medium => 2,
        Priority::Low => 3,
    }
}

fn sanitize_id(path: &str) -> String {
    path.replace(['/', '.'], "_").to_lowercase()
}

fn short_path(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}
