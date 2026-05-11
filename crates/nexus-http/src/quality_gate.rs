//! Pipeline Quality Gate — enforce minimum quality with auto-retry.
//!
//! After the execution pipeline generates an app, the quality gate checks:
//! 1. Taste score >= threshold (default 85)
//! 2. No build errors
//! 3. No invariant violations
//!
//! If the gate fails, it triggers automatic improvement and retries
//! up to `max_retries` times (default 3).
//!
//! Each attempt is logged to `quality_gate_log` for observability.

use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::state::AppState;
use crate::taste_engine;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityGateConfig {
    /// Minimum taste score to pass (0–100).
    pub min_taste_score: u32,
    /// Maximum retry attempts.
    pub max_retries: u32,
    /// Whether to auto-apply improvements between retries.
    pub auto_improve: bool,
    /// Whether build must succeed.
    pub require_build_pass: bool,
}

impl Default for QualityGateConfig {
    fn default() -> Self {
        Self {
            min_taste_score: 85,
            max_retries: 3,
            auto_improve: true,
            require_build_pass: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateResult {
    pub passed: bool,
    pub attempts: Vec<GateAttempt>,
    pub final_taste_score: Option<u32>,
    pub total_improvements: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateAttempt {
    pub attempt_number: u32,
    pub taste_score: Option<u32>,
    pub build_passed: bool,
    pub invariant_violations: Vec<String>,
    pub improvements_applied: Vec<String>,
    pub gate_passed: bool,
    pub duration_ms: u64,
}

// ---------------------------------------------------------------------------
// Gate Execution
// ---------------------------------------------------------------------------

/// Run the quality gate on a generated project.
/// Returns the result after all attempts.
pub async fn run_quality_gate(
    app: &Arc<AppState>,
    project_id: &str,
    project_dir: &Path,
    config: &QualityGateConfig,
    intent: &crate::intent_engine::FlatIntent,
) -> GateResult {
    let mut attempts = Vec::new();
    let mut passed = false;

    for attempt_num in 1..=config.max_retries + 1 {
        let start = std::time::Instant::now();

        // 1. Run taste scoring
        let taste_score = taste_engine::score_project(project_dir);
        let taste_passed = taste_score.overall >= config.min_taste_score;

        // 2. Check build (lightweight — check for common errors)
        let build_passed = check_build_health(project_dir);

        // 3. Check invariants
        let violations = check_invariants(project_dir);

        // 4. Determine if gate passes
        let gate_passed = taste_passed
            && (!config.require_build_pass || build_passed)
            && violations.is_empty();

        let mut improvements = Vec::new();

        if gate_passed {
            info!(
                project = project_id,
                attempt = attempt_num,
                taste = taste_score.overall,
                "Quality gate PASSED"
            );
            passed = true;
        } else if attempt_num <= config.max_retries && config.auto_improve {
            // Apply automatic improvements
            info!(
                project = project_id,
                attempt = attempt_num,
                taste = taste_score.overall,
                build = build_passed,
                violations = violations.len(),
                "Quality gate FAILED — applying improvements"
            );

            // Run taste redesign if score is below threshold
            if !taste_passed {
                let redesign_config = crate::taste_redesign::RedesignConfig {
                    threshold: config.min_taste_score,
                    max_mutations: 5,
                    target_axes: vec![],
                    dry_run: false,
                };
                if let Ok(redesign_result) = crate::taste_redesign::redesign(app, project_id, project_dir, &redesign_config).await {
                    for change in &redesign_result.changes {
                        if change.applied {
                            improvements.push(format!("{}: {}", change.axis, change.improvement));
                        }
                    }
                }
            }

            // Run continuous improvement for missing features
            let analysis = crate::post_build_intel::analyze(project_dir, intent);
            let improve_result = crate::continuous_improve::improve(project_dir, &analysis);
            for applied in &improve_result.applied {
                improvements.push(applied.title.clone());
            }
        } else if !gate_passed {
            warn!(
                project = project_id,
                attempt = attempt_num,
                taste = taste_score.overall,
                "Quality gate FAILED — max retries reached"
            );
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        // Log to database
        log_attempt(app, project_id, &GateAttempt {
            attempt_number: attempt_num,
            taste_score: Some(taste_score.overall),
            build_passed,
            invariant_violations: violations.clone(),
            improvements_applied: improvements.clone(),
            gate_passed,
            duration_ms,
        }).await;

        attempts.push(GateAttempt {
            attempt_number: attempt_num,
            taste_score: Some(taste_score.overall),
            build_passed,
            invariant_violations: violations,
            improvements_applied: improvements,
            gate_passed,
            duration_ms,
        });

        if passed {
            break;
        }
    }

    let final_score = attempts.last().and_then(|a| a.taste_score);

    GateResult {
        passed,
        attempts: attempts.clone(),
        final_taste_score: final_score,
        total_improvements: attempts.iter()
            .flat_map(|a| &a.improvements_applied)
            .count(),
    }
}

// ---------------------------------------------------------------------------
// Checks
// ---------------------------------------------------------------------------

/// Lightweight build health check (no actual build — checks for common issues).
fn check_build_health(project_dir: &Path) -> bool {
    if !project_dir.join("package.json").exists() {
        return false;
    }

    let src_dir = project_dir.join("src");
    let app_dir = if src_dir.exists() { src_dir.join("app") } else { project_dir.join("app") };

    let key_files = [
        app_dir.join("layout.tsx"),
        app_dir.join("layout.jsx"),
        project_dir.join("next.config.ts"),
        project_dir.join("next.config.mjs"),
    ];

    for path in &key_files {
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(path) {
                if content.contains("<<<<<<") || content.contains(">>>>>>") {
                    return false;
                }
                // Check for obviously incomplete JSX (unclosed tags are a reliable signal)
                if content.contains("// TODO: implement") || content.contains("throw new Error(\"Not implemented\")") {
                    return false;
                }
            }
        }
    }

    // Verify globals.css has the design token system
    let css_paths = [
        app_dir.join("globals.css"),
        project_dir.join("src/app/globals.css"),
        project_dir.join("app/globals.css"),
    ];
    let has_design_tokens = css_paths.iter().any(|p| {
        p.exists() && std::fs::read_to_string(p)
            .map(|c| c.contains("--primary") && c.contains("--background"))
            .unwrap_or(false)
    });
    if !has_design_tokens {
        return false;
    }

    true
}

/// Check for invariant violations in generated code.
fn check_invariants(project_dir: &Path) -> Vec<String> {
    let mut violations = Vec::new();
    let files = collect_code_files(project_dir);

    for (path, content) in &files {
        // Security: hardcoded secrets
        if (content.contains("sk-") && content.contains("openai")) && !path.contains(".env") {
            violations.push(format!("Hardcoded OpenAI key in {}", path));
        }
        if content.contains("password") && (content.contains("\"123\"") || content.contains("\"password\"")) {
            violations.push(format!("Weak hardcoded password in {}", path));
        }

        // Quality: alert() usage
        if (content.contains("alert(") || content.contains("window.alert(")) && !path.contains(".md") {
            violations.push(format!("Browser alert() in {} — use toast notifications instead", path));
        }

        // Quality: Lorem ipsum
        let lower = content.to_lowercase();
        if lower.contains("lorem ipsum") {
            violations.push(format!("Placeholder text (Lorem ipsum) in {}", path));
        }

        // Quality: broken placeholder hrefs
        if content.contains("href=\"#\"") {
            violations.push(format!("Broken placeholder href in {}", path));
        }
    }

    // Check for missing essential files
    let src_app = project_dir.join("src/app");
    let app_dir = if src_app.exists() { src_app } else { project_dir.join("app") };
    if !app_dir.join("layout.tsx").exists() && !app_dir.join("layout.jsx").exists() {
        violations.push("Missing root layout (src/app/layout.tsx)".into());
    }
    if !project_dir.join("tailwind.config.ts").exists()
        && !project_dir.join("tailwind.config.js").exists()
    {
        violations.push("Missing tailwind.config.ts".into());
    }

    violations
}

fn collect_code_files(dir: &Path) -> Vec<(String, String)> {
    crate::file_utils::collect_files_with_content(dir, &["ts", "tsx", "js", "jsx"])
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

async fn log_attempt(app: &Arc<AppState>, project_id: &str, attempt: &GateAttempt) {
    let db = app.db.lock().await;
    let id = uuid::Uuid::new_v4().to_string();
    let violations_json = serde_json::to_string(&attempt.invariant_violations).unwrap_or_default();
    let improvements_json = serde_json::to_string(&attempt.improvements_applied).unwrap_or_default();

    let _ = db.execute(
        "INSERT INTO quality_gate_log (id, project_id, attempt, taste_score, build_passed,
            invariant_violations, gate_passed, improvements, duration_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            id,
            project_id,
            attempt.attempt_number,
            attempt.taste_score,
            attempt.build_passed as i32,
            violations_json,
            attempt.gate_passed as i32,
            improvements_json,
            attempt.duration_ms as i64,
        ],
    );
}

/// Get the quality gate history for a project.
pub async fn get_gate_history(app: &Arc<AppState>, project_id: &str) -> Vec<GateAttempt> {
    let db = app.db.lock().await;
    let mut stmt = match db.prepare(
        "SELECT attempt, taste_score, build_passed, invariant_violations,
                gate_passed, improvements, duration_ms
         FROM quality_gate_log
         WHERE project_id = ?1
         ORDER BY created_at DESC
         LIMIT 20"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let rows = stmt.query_map(rusqlite::params![project_id], |row| {
        Ok(GateAttempt {
            attempt_number: row.get(0)?,
            taste_score: row.get(1)?,
            build_passed: row.get::<_, i32>(2)? != 0,
            invariant_violations: serde_json::from_str(&row.get::<_, String>(3).unwrap_or_default())
                .unwrap_or_default(),
            gate_passed: row.get::<_, i32>(4)? != 0,
            improvements_applied: serde_json::from_str(&row.get::<_, String>(5).unwrap_or_default())
                .unwrap_or_default(),
            duration_ms: row.get::<_, i64>(6)? as u64,
        })
    }).ok();

    rows.map(|iter| iter.filter_map(|r| r.ok()).collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_strict() {
        let config = QualityGateConfig::default();
        assert_eq!(config.min_taste_score, 85);
        assert_eq!(config.max_retries, 3);
        assert!(config.auto_improve);
        assert!(config.require_build_pass);
    }
}
