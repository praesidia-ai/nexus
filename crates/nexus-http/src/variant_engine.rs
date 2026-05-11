//! Variant Engine — generates multiple variants and selects the best one.
//!
//! For high-complexity projects, generates 2–3 variants of:
//! - UI approaches (different layouts, styles, component structures)
//! - API structures (different entity relationships, endpoint patterns)
//!
//! Each variant is scored by:
//! - taste_engine (UX quality)
//! - heuristic scoring (code structure, completeness)
//!
//! The BEST variant is selected and used as the final output.
//!
//! Budget-aware: only triggers for Complex projects to avoid waste.

use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::intent_engine::{Complexity, FlatIntent};
use crate::state::AppState;
use crate::taste_engine;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Configuration for variant generation.
#[derive(Debug, Clone)]
pub struct VariantConfig {
    /// Maximum number of variants to generate (2-3).
    pub max_variants: u32,
    /// What to vary.
    pub variant_type: VariantType,
    /// Budget cap per variant (tokens).
    pub budget_per_variant: u64,
    /// Minimum taste score to accept.
    pub min_taste_score: u32,
}

impl Default for VariantConfig {
    fn default() -> Self {
        Self {
            max_variants: 2,
            variant_type: VariantType::Full,
            budget_per_variant: 100_000,
            min_taste_score: 70,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VariantType {
    Ui,
    Api,
    Full,
}

/// A single generated variant with its scores.
#[derive(Debug, Clone, Serialize)]
pub struct Variant {
    pub index: u32,
    pub variant_type: VariantType,
    pub description: String,
    pub taste_score: u32,
    pub heuristic_score: f64,
    pub combined_score: f64,
    pub files_count: usize,
    pub selected: bool,
}

/// Result of multi-variant generation.
#[derive(Debug, Clone, Serialize)]
pub struct VariantResult {
    pub variants: Vec<Variant>,
    pub selected_index: u32,
    pub reason: String,
    pub total_budget_used: u64,
}

// ---------------------------------------------------------------------------
// Should we generate variants?
// ---------------------------------------------------------------------------

/// Determine if a project is complex enough to warrant multi-variant generation.
pub fn should_generate_variants(intent: &FlatIntent) -> bool {
    // Only for complex projects
    if intent.complexity != Complexity::Complex {
        return false;
    }
    // Must have substantial features
    let feature_count = intent.inferred_features.len() + intent.suggested_pages.len();
    feature_count >= 6
}

// ---------------------------------------------------------------------------
// Generate variant prompts
// ---------------------------------------------------------------------------

/// Generate differentiated prompts for each variant.
pub fn build_variant_prompts(
    base_prompt: &str,
    config: &VariantConfig,
) -> Vec<(u32, String, String)> {
    let mut prompts = Vec::new();

    match config.variant_type {
        VariantType::Ui => {
            prompts.push((
                0,
                "Modern minimal — clean whitespace, large typography, subtle animations".into(),
                format!(
                    "{}\n\n## UI VARIANT: Modern Minimal\n\
                     - Prioritize whitespace and breathing room\n\
                     - Large, bold typography for headers\n\
                     - Subtle fade-in animations only\n\
                     - Monochrome with one accent color\n\
                     - Card-based layouts with rounded corners",
                    base_prompt
                ),
            ));
            prompts.push((
                1,
                "Bold dashboard — data-dense, dark theme option, compact layout".into(),
                format!(
                    "{}\n\n## UI VARIANT: Bold Dashboard\n\
                     - Dense, information-rich layout\n\
                     - Dark theme with vibrant accent colors\n\
                     - Compact spacing for data tables\n\
                     - Chart-heavy with real-time indicators\n\
                     - Sidebar navigation with collapsible sections",
                    base_prompt
                ),
            ));
            if config.max_variants >= 3 {
                prompts.push((
                    2,
                    "Playful consumer — bright gradients, emoji, gamification elements".into(),
                    format!(
                        "{}\n\n## UI VARIANT: Playful Consumer\n\
                         - Bright gradient backgrounds\n\
                         - Rounded, pill-shaped buttons\n\
                         - Progress bars and achievement badges\n\
                         - Conversational UI patterns\n\
                         - Animated transitions between views",
                        base_prompt
                    ),
                ));
            }
        }
        VariantType::Api => {
            prompts.push((
                0,
                "REST-first — traditional REST endpoints, clear resource hierarchy".into(),
                format!(
                    "{}\n\n## API VARIANT: REST-First\n\
                     - Classic REST resource hierarchy\n\
                     - Nested routes for relationships\n\
                     - Standard CRUD operations\n\
                     - Pagination via query params\n\
                     - JSON:API-style responses",
                    base_prompt
                ),
            ));
            prompts.push((
                1,
                "Action-based — RPC-style endpoints, server actions, optimistic updates".into(),
                format!(
                    "{}\n\n## API VARIANT: Action-Based\n\
                     - Server actions for mutations\n\
                     - Optimistic UI updates\n\
                     - Batch operations support\n\
                     - Event-driven patterns\n\
                     - Minimal API surface, rich server functions",
                    base_prompt
                ),
            ));
        }
        VariantType::Full => {
            prompts.push((
                0,
                "Variant A — balanced approach, conventional patterns".into(),
                format!(
                    "{}\n\n## GENERATION VARIANT A\n\
                     Use conventional, well-established patterns.\n\
                     Prioritize readability and maintainability.\n\
                     Standard component library usage.\n\
                     REST API with clear resource hierarchy.",
                    base_prompt
                ),
            ));
            prompts.push((
                1,
                "Variant B — modern patterns, server-first, optimized".into(),
                format!(
                    "{}\n\n## GENERATION VARIANT B\n\
                     Use cutting-edge patterns (server components, server actions).\n\
                     Prioritize performance and minimal client JS.\n\
                     Streaming where possible.\n\
                     Co-located data fetching.",
                    base_prompt
                ),
            ));
        }
    }

    prompts
}

// ---------------------------------------------------------------------------
// Score and select best variant
// ---------------------------------------------------------------------------

/// Score a variant using taste engine + heuristics.
pub fn score_variant(
    project_dir: &Path,
    variant_index: u32,
    variant_type: VariantType,
    description: &str,
) -> Variant {
    let taste = taste_engine::score_project(project_dir);
    let heuristic = compute_heuristic_score(project_dir);
    // Combined: 70% taste + 30% heuristic
    let combined = taste.overall as f64 * 0.7 + heuristic * 30.0;

    let files_count = count_source_files(project_dir);

    Variant {
        index: variant_index,
        variant_type,
        description: description.to_string(),
        taste_score: taste.overall,
        heuristic_score: heuristic,
        combined_score: combined,
        files_count,
        selected: false,
    }
}

/// Select the best variant from scored candidates.
pub fn select_best(variants: &mut [Variant]) -> VariantResult {
    if variants.is_empty() {
        return VariantResult {
            variants: Vec::new(),
            selected_index: 0,
            reason: "No variants generated".into(),
            total_budget_used: 0,
        };
    }

    // Sort by combined score descending
    variants.sort_by(|a, b| {
        b.combined_score
            .partial_cmp(&a.combined_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let best_idx = variants[0].index;
    variants[0].selected = true;

    let reason = format!(
        "Selected variant {} (taste: {}, heuristic: {:.2}, combined: {:.1}). \
         Runner-up: variant {} (combined: {:.1})",
        variants[0].index,
        variants[0].taste_score,
        variants[0].heuristic_score,
        variants[0].combined_score,
        if variants.len() > 1 { variants[1].index } else { 0 },
        if variants.len() > 1 { variants[1].combined_score } else { 0.0 },
    );

    info!(
        selected = best_idx,
        taste = variants[0].taste_score,
        heuristic = variants[0].heuristic_score,
        "Variant selected"
    );

    VariantResult {
        variants: variants.to_vec(),
        selected_index: best_idx,
        reason,
        total_budget_used: 0, // Updated by caller
    }
}

/// Record variant selection in the database for future learning.
pub async fn record_variant_selection(
    app: &Arc<AppState>,
    project_id: &str,
    result: &VariantResult,
) {
    let db = app.db.lock().await;

    for variant in &result.variants {
        let id = uuid::Uuid::new_v4().to_string();
        let vt = match variant.variant_type {
            VariantType::Ui => "ui",
            VariantType::Api => "api",
            VariantType::Full => "full",
        };
        let _ = db.execute(
            "INSERT INTO generation_variants
             (id, project_id, variant_index, variant_type, description,
              taste_score, heuristic_score, selected)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                id,
                project_id,
                variant.index,
                vt,
                variant.description,
                variant.taste_score,
                variant.heuristic_score,
                variant.selected as i64,
            ],
        );
    }
}

// ---------------------------------------------------------------------------
// Heuristic scoring
// ---------------------------------------------------------------------------

/// Compute a heuristic quality score (0.0–1.0) based on code structure.
fn compute_heuristic_score(project_dir: &Path) -> f64 {
    let mut score = 0.0;
    let mut checks = 0;

    // Check 1: Has package.json
    if project_dir.join("package.json").exists() {
        score += 1.0;
    }
    checks += 1;

    // Check 2: Has src directory with structure
    if project_dir.join("src").exists() {
        score += 1.0;
        if project_dir.join("src/app").exists() || project_dir.join("src/pages").exists() {
            score += 1.0;
        }
        checks += 1;
    }
    checks += 1;

    // Check 3: Has TypeScript config
    if project_dir.join("tsconfig.json").exists() {
        score += 1.0;
    }
    checks += 1;

    // Check 4: Has tailwind config
    if project_dir.join("tailwind.config.ts").exists()
        || project_dir.join("tailwind.config.js").exists()
    {
        score += 1.0;
    }
    checks += 1;

    // Check 5: Has multiple source files
    let file_count = count_source_files(project_dir);
    if file_count >= 10 {
        score += 1.0;
    } else if file_count >= 5 {
        score += 0.5;
    }
    checks += 1;

    // Check 6: Has API routes
    let api_dir = project_dir.join("src/app/api");
    if api_dir.exists() {
        score += 1.0;
    }
    checks += 1;

    // Check 7: Has components directory
    if project_dir.join("src/components").exists() {
        score += 1.0;
    }
    checks += 1;

    if checks == 0 {
        return 0.0;
    }
    score / checks as f64
}

fn count_source_files(dir: &Path) -> usize {
    let mut count = 0;
    fn walk(dir: &Path, count: &mut usize) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name == "node_modules" || name == "target" {
                continue;
            }
            if path.is_dir() {
                walk(&path, count);
            } else if let Some(ext) = path.extension() {
                let ext = ext.to_string_lossy();
                if matches!(ext.as_ref(), "ts" | "tsx" | "js" | "jsx" | "css" | "json") {
                    *count += 1;
                }
            }
        }
    }
    walk(dir, &mut count);
    count
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent_engine;

    #[test]
    fn simple_project_skips_variants() {
        let intent = intent_engine::analyze_flat("Build a simple landing page");
        assert!(!should_generate_variants(&intent));
    }

    #[test]
    fn complex_project_enables_variants() {
        let mut intent = intent_engine::analyze_flat(
            "Build a SaaS CRM with auth, billing, team management, analytics, reporting, and API",
        );
        intent.complexity = Complexity::Complex;
        // Ensure enough features
        intent.inferred_features = vec![
            "auth".into(), "billing".into(), "teams".into(),
            "analytics".into(), "reporting".into(), "api".into(),
        ];
        assert!(should_generate_variants(&intent));
    }

    #[test]
    fn builds_ui_variant_prompts() {
        let config = VariantConfig {
            max_variants: 3,
            variant_type: VariantType::Ui,
            ..Default::default()
        };
        let prompts = build_variant_prompts("Build an app", &config);
        assert_eq!(prompts.len(), 3);
        assert!(prompts[0].2.contains("Modern Minimal"));
        assert!(prompts[1].2.contains("Bold Dashboard"));
        assert!(prompts[2].2.contains("Playful Consumer"));
    }

    #[test]
    fn builds_full_variant_prompts() {
        let config = VariantConfig::default();
        let prompts = build_variant_prompts("Build an app", &config);
        assert_eq!(prompts.len(), 2);
        assert!(prompts[0].2.contains("VARIANT A"));
        assert!(prompts[1].2.contains("VARIANT B"));
    }

    #[test]
    fn selects_best_variant() {
        let mut variants = vec![
            Variant {
                index: 0,
                variant_type: VariantType::Full,
                description: "A".into(),
                taste_score: 80,
                heuristic_score: 0.7,
                combined_score: 80.0 * 0.7 + 0.7 * 30.0,
                files_count: 15,
                selected: false,
            },
            Variant {
                index: 1,
                variant_type: VariantType::Full,
                description: "B".into(),
                taste_score: 92,
                heuristic_score: 0.8,
                combined_score: 92.0 * 0.7 + 0.8 * 30.0,
                files_count: 18,
                selected: false,
            },
        ];

        let result = select_best(&mut variants);
        assert_eq!(result.selected_index, 1);
        assert!(result.variants[0].selected);
    }

    #[test]
    fn heuristic_scores_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let score = compute_heuristic_score(dir.path());
        assert!((score).abs() < 0.001, "Empty dir should score 0");
    }
}
