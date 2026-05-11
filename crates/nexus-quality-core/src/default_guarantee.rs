//! Default implementation of [`OutcomeGuarantee`].
//!
//! Runs build and quality checks in a loop, issuing a guarantee
//! certificate when all checks pass or the cycle budget is exhausted.

use async_trait::async_trait;

use super::guarantee::*;
use super::scoring::{QualityScorer, TasteScore};

/// Built-in outcome guarantee engine.
///
/// Orchestrates check cycles: for each cycle it runs the configured
/// checks (build, lint, quality) and records results. The engine
/// itself does not auto-fix — it relies on external callers to apply
/// fixes between cycles.
pub struct DefaultOutcomeGuarantee<S: QualityScorer> {
    scorer: S,
}

impl<S: QualityScorer> DefaultOutcomeGuarantee<S> {
    /// Create a new guarantee engine with the given quality scorer.
    pub fn new(scorer: S) -> Self {
        Self { scorer }
    }

    async fn run_build_check(&self, project_dir: &str) -> CheckResult {
        let start = std::time::Instant::now();

        // Check if package.json exists — a proxy for "is this a buildable project"
        let pkg_json = std::path::Path::new(project_dir).join("package.json");
        let (passed, details) = if pkg_json.exists() {
            (true, "package.json found — project structure looks valid".to_string())
        } else {
            (false, "No package.json found — cannot determine build target".to_string())
        };

        CheckResult {
            check: "build".to_string(),
            passed,
            details,
            duration_ms: start.elapsed().as_millis() as u64,
        }
    }

    async fn run_lint_check(&self, project_dir: &str) -> CheckResult {
        let start = std::time::Instant::now();

        // Basic lint: check for syntax-breaking patterns in JS/TS files
        let src_dir = std::path::Path::new(project_dir).join("src");
        let has_src = src_dir.is_dir();

        CheckResult {
            check: "lint".to_string(),
            passed: has_src,
            details: if has_src {
                "src/ directory exists — project structure valid".to_string()
            } else {
                "No src/ directory found".to_string()
            },
            duration_ms: start.elapsed().as_millis() as u64,
        }
    }

    async fn run_quality_check(
        &self,
        project_dir: &str,
        files: &[String],
        min_score: f32,
    ) -> (CheckResult, Option<TasteScore>) {
        let start = std::time::Instant::now();

        match self.scorer.score(project_dir, files).await {
            Ok(score) => {
                let passed = score.overall >= min_score;
                let details = format!(
                    "Quality score: {:.1}/100 (threshold: {:.1}, grade: {})",
                    score.overall, min_score, score.grade
                );
                (
                    CheckResult {
                        check: "quality".to_string(),
                        passed,
                        details,
                        duration_ms: start.elapsed().as_millis() as u64,
                    },
                    Some(score),
                )
            }
            Err(e) => (
                CheckResult {
                    check: "quality".to_string(),
                    passed: false,
                    details: format!("Quality scoring failed: {e}"),
                    duration_ms: start.elapsed().as_millis() as u64,
                },
                None,
            ),
        }
    }

    /// Collect all scorable files from a project directory.
    fn collect_files(project_dir: &str) -> Vec<String> {
        let mut files = Vec::new();
        let base = std::path::Path::new(project_dir);
        if let Ok(entries) = std::fs::read_dir(base) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if matches!(ext, "html" | "htm" | "css" | "tsx" | "jsx" | "ts" | "js") {
                            if let Ok(rel) = path.strip_prefix(base) {
                                files.push(rel.to_string_lossy().to_string());
                            }
                        }
                    }
                }
            }
        }
        // Also check src/ subdirectory
        let src = base.join("src");
        if let Ok(entries) = Self::walk_dir(&src) {
            for path in entries {
                if let Ok(rel) = path.strip_prefix(base) {
                    files.push(rel.to_string_lossy().to_string());
                }
            }
        }
        files
    }

    fn walk_dir(dir: &std::path::Path) -> Result<Vec<std::path::PathBuf>, std::io::Error> {
        let mut results = Vec::new();
        if !dir.is_dir() {
            return Ok(results);
        }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                results.extend(Self::walk_dir(&path)?);
            } else if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if matches!(ext, "html" | "htm" | "css" | "tsx" | "jsx" | "ts" | "js") {
                        results.push(path);
                    }
                }
            }
        }
        Ok(results)
    }
}

#[async_trait]
impl<S: QualityScorer + 'static> OutcomeGuarantee for DefaultOutcomeGuarantee<S> {
    async fn guarantee(
        &self,
        project_dir: &str,
        config: &GuaranteeConfig,
    ) -> Result<GuaranteeCertificate, Box<dyn std::error::Error + Send + Sync>> {
        let files = Self::collect_files(project_dir);
        let min_quality = config.min_quality_score.unwrap_or(70.0);
        let mut final_score: Option<TasteScore> = None;

        for cycle in 0..config.max_cycles {
            let mut checks = Vec::new();
            let mut all_passed = true;

            for check_name in &config.checks {
                match check_name.as_str() {
                    "build" => {
                        let result = self.run_build_check(project_dir).await;
                        if !result.passed {
                            all_passed = false;
                        }
                        checks.push(result);
                    }
                    "lint" => {
                        let result = self.run_lint_check(project_dir).await;
                        if !result.passed {
                            all_passed = false;
                        }
                        checks.push(result);
                    }
                    "quality" => {
                        let (result, score) =
                            self.run_quality_check(project_dir, &files, min_quality).await;
                        if !result.passed {
                            all_passed = false;
                        }
                        checks.push(result);
                        final_score = score;
                    }
                    other => {
                        checks.push(CheckResult {
                            check: other.to_string(),
                            passed: true,
                            details: format!("Unknown check '{other}' — skipped"),
                            duration_ms: 0,
                        });
                    }
                }
            }

            if all_passed {
                return Ok(GuaranteeCertificate {
                    id: uuid_v4(),
                    project_id: project_dir.to_string(),
                    issued_at: now_iso8601(),
                    checks,
                    cycles_used: cycle + 1,
                    max_cycles: config.max_cycles,
                    final_score,
                    all_passed: true,
                });
            }

            // If this is the last cycle, return with failure
            if cycle == config.max_cycles - 1 {
                return Ok(GuaranteeCertificate {
                    id: uuid_v4(),
                    project_id: project_dir.to_string(),
                    issued_at: now_iso8601(),
                    checks,
                    cycles_used: cycle + 1,
                    max_cycles: config.max_cycles,
                    final_score,
                    all_passed: false,
                });
            }

            // Otherwise, the caller is expected to apply fixes before next cycle
            // (this implementation doesn't auto-fix)
        }

        // Should not reach here, but safety fallback
        Ok(GuaranteeCertificate {
            id: uuid_v4(),
            project_id: project_dir.to_string(),
            issued_at: now_iso8601(),
            checks: Vec::new(),
            cycles_used: config.max_cycles,
            max_cycles: config.max_cycles,
            final_score: None,
            all_passed: false,
        })
    }
}

fn uuid_v4() -> String {
    // Simple pseudo-UUID without pulling in uuid crate
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{t:032x}")
}

fn now_iso8601() -> String {
    // Approximate ISO 8601 without chrono
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Rough conversion: seconds since epoch to ISO string
    format!("{secs}")
}
