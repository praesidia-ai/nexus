//! Taste Redesign — surgical UI improvement from taste scores.
//!
//! Takes taste scoring results and generates targeted mutations:
//! - Improved layouts (spacing, alignment, visual hierarchy)
//! - Rewritten UI components (better patterns, modern code)
//! - Improved copy (headlines, CTAs, descriptions)
//!
//! Uses the mutation engine for surgical updates — no full regen.

use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::mutation_engine;
use crate::state::AppState;
use crate::taste_engine::{TasteImprovement, TasteScore};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedesignResult {
    pub mutations_applied: usize,
    pub mutations_failed: usize,
    pub before_score: u32,
    pub after_score: u32,
    pub changes: Vec<RedesignChange>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedesignChange {
    pub axis: String,
    pub improvement: String,
    pub file: String,
    pub applied: bool,
    pub error: Option<String>,
}

/// Configuration for what to redesign.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedesignConfig {
    /// Only redesign axes below this threshold (0-100).
    #[serde(default = "default_threshold")]
    pub threshold: u32,
    /// Maximum mutations to apply in one pass.
    #[serde(default = "default_max_mutations")]
    pub max_mutations: usize,
    /// Which axes to target (empty = all below threshold).
    #[serde(default)]
    pub target_axes: Vec<String>,
    /// Dry run — generate mutations but don't apply them.
    #[serde(default)]
    pub dry_run: bool,
}

fn default_threshold() -> u32 { 70 }
fn default_max_mutations() -> usize { 5 }

impl Default for RedesignConfig {
    fn default() -> Self {
        Self {
            threshold: default_threshold(),
            max_mutations: default_max_mutations(),
            target_axes: Vec::new(),
            dry_run: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Core redesign logic
// ---------------------------------------------------------------------------

/// Run taste-driven redesign: score → identify weaknesses → mutate → rescore.
pub async fn redesign(
    app: &Arc<AppState>,
    project_id: &str,
    project_dir: &Path,
    config: &RedesignConfig,
) -> Result<RedesignResult, String> {
    let start = std::time::Instant::now();

    // 1. Score current state
    let before = crate::taste_engine::score_project(project_dir);
    let before_score = before.overall;

    // 2. Select improvements to apply
    let mutations = select_mutations(&before, config);

    if mutations.is_empty() {
        return Ok(RedesignResult {
            mutations_applied: 0,
            mutations_failed: 0,
            before_score,
            after_score: before_score,
            changes: Vec::new(),
            duration_ms: start.elapsed().as_millis() as u64,
        });
    }

    // 3. Apply each mutation via the mutation engine
    let mut changes = Vec::new();
    let mut applied = 0usize;
    let mut failed = 0usize;

    for (axis, improvement) in &mutations {
        let mutation_prompt = build_mutation_prompt(axis, improvement);

        if config.dry_run {
            changes.push(RedesignChange {
                axis: axis.clone(),
                improvement: improvement.description.clone(),
                file: improvement.file.clone().unwrap_or_default(),
                applied: false,
                error: Some("dry_run".into()),
            });
            continue;
        }

        let request = mutation_engine::MutationRequest {
            change: mutation_prompt,
            target_file: improvement.file.clone(),
        };

        match mutation_engine::mutate(app, project_id, project_dir, &request).await {
            Ok(result) if result.applied => {
                applied += 1;
                for fc in &result.files_changed {
                    changes.push(RedesignChange {
                        axis: axis.clone(),
                        improvement: improvement.description.clone(),
                        file: fc.path.clone(),
                        applied: true,
                        error: None,
                    });
                }
            }
            Ok(_) => {
                failed += 1;
                changes.push(RedesignChange {
                    axis: axis.clone(),
                    improvement: improvement.description.clone(),
                    file: improvement.file.clone().unwrap_or_default(),
                    applied: false,
                    error: Some("Mutation generated but not applied".into()),
                });
            }
            Err(e) => {
                failed += 1;
                changes.push(RedesignChange {
                    axis: axis.clone(),
                    improvement: improvement.description.clone(),
                    file: improvement.file.clone().unwrap_or_default(),
                    applied: false,
                    error: Some(e),
                });
            }
        }
    }

    // 4. Rescore
    let after = crate::taste_engine::score_project(project_dir);

    // 5. Persist taste history
    persist_taste_score(app, project_id, &after, applied).await;

    Ok(RedesignResult {
        mutations_applied: applied,
        mutations_failed: failed,
        before_score,
        after_score: after.overall,
        changes,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn select_mutations<'a>(
    score: &'a TasteScore,
    config: &RedesignConfig,
) -> Vec<(String, &'a TasteImprovement)> {
    let mut selected: Vec<(String, &TasteImprovement)> = Vec::new();

    for imp in &score.improvements {
        // Filter by target axes
        if !config.target_axes.is_empty() && !config.target_axes.contains(&imp.axis) {
            continue;
        }

        // Filter by threshold (only fix weak axes)
        let axis_score = match imp.axis.as_str() {
            "visual_quality" => score.visual_quality.score,
            "ux_clarity" => score.ux_clarity.score,
            "conversion" => score.conversion.score,
            "modern_ui" => score.modern_ui.score,
            "accessibility" => score.accessibility.score,
            "responsiveness" => score.responsiveness.score,
            _ => continue,
        };

        if axis_score >= config.threshold {
            continue;
        }

        selected.push((imp.axis.clone(), imp));

        if selected.len() >= config.max_mutations {
            break;
        }
    }

    selected
}

fn build_mutation_prompt(axis: &str, improvement: &TasteImprovement) -> String {
    match axis {
        "visual_quality" => format!(
            "Improve visual quality: {}. Add proper design tokens (CSS variables for colors, spacing, radius). \
             Use consistent spacing (gap-4, p-6). Add subtle shadows for depth. \
             Ensure color contrast meets WCAG AA.",
            improvement.description
        ),
        "ux_clarity" => format!(
            "Improve UX clarity: {}. Add loading states with skeleton placeholders. \
             Add empty states with helpful messages. Add error boundaries with retry buttons. \
             Ensure navigation is clear with active state indicators.",
            improvement.description
        ),
        "conversion" => format!(
            "Improve conversion: {}. Add a clear call-to-action button with action-oriented text. \
             Add trust signals (security badges, testimonials). Add a hero section with benefit-driven headline.",
            improvement.description
        ),
        "modern_ui" => format!(
            "Modernize the UI: {}. Add smooth CSS transitions (transition-all duration-200). \
             Add hover effects on interactive elements. Add a dark mode toggle if missing. \
             Use modern patterns like glass-morphism or gradients.",
            improvement.description
        ),
        "accessibility" => format!(
            "Fix accessibility: {}. Add alt text to all images. Add aria-label to icon buttons. \
             Add focus-visible ring styles. Ensure form inputs have associated labels. \
             Add sr-only text for screen readers on icon-only elements.",
            improvement.description
        ),
        "responsiveness" => format!(
            "Fix responsiveness: {}. Add sm:/md:/lg: breakpoint variants. \
             Use flex-col on mobile, flex-row on desktop. Add container mx-auto max-w-7xl. \
             Ensure text doesn't overflow on small screens.",
            improvement.description
        ),
        _ => improvement.suggestion.clone(),
    }
}

async fn persist_taste_score(app: &Arc<AppState>, project_id: &str, score: &TasteScore, improvements_applied: usize) {
    let db = app.db.lock().await;
    let id = uuid::Uuid::new_v4().to_string();
    let _ = db.execute(
        "INSERT INTO taste_history (id, project_id, overall, visual, ux, conversion, modern_ui, accessibility, responsiveness, improvements_applied)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![
            id,
            project_id,
            score.overall,
            score.visual_quality.score,
            score.ux_clarity.score,
            score.conversion.score,
            score.modern_ui.score,
            score.accessibility.score,
            score.responsiveness.score,
            improvements_applied as i64,
        ],
    );
}
