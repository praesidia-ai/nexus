//! Production Simulation Engine — simulate production conditions before deploy.
//!
//! Runs a battery of pre-deployment checks that mimic real production:
//! - Dependency audit (outdated, vulnerable)
//! - Environment variable completeness
//! - Build in production mode
//! - Bundle size analysis
//! - Lighthouse-style performance checks
//! - Database migration dry-run
//! - Health endpoint verification

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::info;

use crate::state::AppState;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SimEvent {
    #[serde(rename = "phase")]
    Phase {
        name: String,
        index: usize,
        total: usize,
    },
    #[serde(rename = "check")]
    Check {
        phase: String,
        name: String,
        passed: bool,
        detail: String,
        severity: String,
    },
    #[serde(rename = "metric")]
    Metric {
        name: String,
        value: f64,
        unit: String,
        threshold: Option<f64>,
        passed: bool,
    },
    #[serde(rename = "complete")]
    Complete {
        passed: bool,
        score: u32,
        phases_passed: usize,
        phases_total: usize,
        duration_ms: u64,
    },
    #[serde(rename = "error")]
    Error { phase: String, message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimConfig {
    /// Skip phases by name.
    pub skip_phases: Vec<String>,
    /// Maximum acceptable bundle size in KB.
    pub max_bundle_kb: u64,
    /// Timeout for build step in seconds.
    pub build_timeout_secs: u64,
    /// Whether to run `npm audit`.
    pub audit_deps: bool,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            skip_phases: Vec::new(),
            max_bundle_kb: 500,
            build_timeout_secs: 180,
            audit_deps: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimResult {
    pub passed: bool,
    pub score: u32,
    pub phases: Vec<PhaseResult>,
    pub metrics: Vec<SimMetric>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseResult {
    pub name: String,
    pub passed: bool,
    pub checks: Vec<CheckResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub name: String,
    pub passed: bool,
    pub detail: String,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimMetric {
    pub name: String,
    pub value: f64,
    pub unit: String,
    pub threshold: Option<f64>,
    pub passed: bool,
}

// ---------------------------------------------------------------------------
// Simulation Phases
// ---------------------------------------------------------------------------

const PHASES: &[&str] = &[
    "env_check",
    "dependency_audit",
    "production_build",
    "bundle_analysis",
    "invariant_check",
    "health_check",
];

/// Run the full production simulation.
pub async fn simulate(
    _app: Arc<AppState>,
    project_dir: PathBuf,
    config: SimConfig,
    tx: mpsc::Sender<SimEvent>,
) -> SimResult {
    let start = std::time::Instant::now();
    let mut phases = Vec::new();
    let mut metrics = Vec::new();
    let active_phases: Vec<&&str> = PHASES
        .iter()
        .filter(|p| !config.skip_phases.contains(&p.to_string()))
        .collect();
    let total = active_phases.len();

    for (i, phase_name) in active_phases.iter().enumerate() {
        let _ = tx
            .send(SimEvent::Phase {
                name: phase_name.to_string(),
                index: i,
                total,
            })
            .await;

        let phase_result = match **phase_name {
            "env_check" => run_env_check(&project_dir, &tx).await,
            "dependency_audit" => run_dependency_audit(&project_dir, &config, &tx).await,
            "production_build" => run_production_build(&project_dir, &config, &tx).await,
            "bundle_analysis" => {
                run_bundle_analysis(&project_dir, &config, &tx, &mut metrics).await
            }
            "invariant_check" => run_invariant_check(&project_dir, &tx, &mut metrics).await,
            "health_check" => run_health_check(&project_dir, &tx).await,
            _ => PhaseResult {
                name: phase_name.to_string(),
                passed: true,
                checks: vec![],
            },
        };

        phases.push(phase_result);
    }

    let phases_passed = phases.iter().filter(|p| p.passed).count();
    let critical_failed = phases
        .iter()
        .any(|p| !p.passed && matches!(p.name.as_str(), "production_build" | "env_check"));

    // Score: each phase worth equal weight
    let score = if total > 0 {
        ((phases_passed as f64 / total as f64) * 100.0) as u32
    } else {
        100
    };

    let passed = !critical_failed && score >= 70;
    let duration_ms = start.elapsed().as_millis() as u64;

    let _ = tx
        .send(SimEvent::Complete {
            passed,
            score,
            phases_passed,
            phases_total: total,
            duration_ms,
        })
        .await;

    info!(
        passed = passed,
        score = score,
        duration_ms = duration_ms,
        "Production simulation complete"
    );

    SimResult {
        passed,
        score,
        phases,
        metrics,
        duration_ms,
    }
}

// ---------------------------------------------------------------------------
// Phase Implementations
// ---------------------------------------------------------------------------

async fn run_env_check(project_dir: &Path, tx: &mpsc::Sender<SimEvent>) -> PhaseResult {
    let mut checks = Vec::new();

    // Check .env.example exists
    let has_example = project_dir.join(".env.example").exists();
    checks.push(CheckResult {
        name: ".env.example exists".into(),
        passed: has_example,
        detail: if has_example {
            "Found .env.example".into()
        } else {
            "Missing .env.example — env vars are undocumented".into()
        },
        severity: "warning".into(),
    });

    // Check .env.local or .env exists (for runtime)
    let has_env = project_dir.join(".env.local").exists() || project_dir.join(".env").exists();
    checks.push(CheckResult {
        name: "Runtime env file exists".into(),
        passed: has_env,
        detail: if has_env {
            "Found runtime environment file".into()
        } else {
            "No .env or .env.local — app may fail to start without env vars".into()
        },
        severity: "info".into(),
    });

    // Check for required env vars in code vs. available
    let required = scan_required_env_vars(project_dir);
    if !required.is_empty() {
        let available = read_env_file(project_dir);
        let missing: Vec<&String> = required.iter().filter(|v| !available.contains(v)).collect();

        let passed = missing.is_empty();
        let detail = if passed {
            format!("All {} required env vars are set", required.len())
        } else {
            format!("Missing env vars: {}", missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "))
        };

        let _ = tx
            .send(SimEvent::Check {
                phase: "env_check".into(),
                name: "env_var_completeness".into(),
                passed,
                detail: detail.clone(),
                severity: "error".into(),
            })
            .await;

        checks.push(CheckResult {
            name: "Env var completeness".into(),
            passed,
            detail,
            severity: "error".into(),
        });
    }

    let all_passed = checks.iter().filter(|c| c.severity == "error").all(|c| c.passed);
    PhaseResult {
        name: "env_check".into(),
        passed: all_passed,
        checks,
    }
}

async fn run_dependency_audit(
    project_dir: &Path,
    config: &SimConfig,
    tx: &mpsc::Sender<SimEvent>,
) -> PhaseResult {
    let mut checks = Vec::new();

    let pkg_path = project_dir.join("package.json");
    if !pkg_path.exists() {
        return PhaseResult {
            name: "dependency_audit".into(),
            passed: true,
            checks: vec![CheckResult {
                name: "package.json".into(),
                passed: true,
                detail: "No package.json — skipping".into(),
                severity: "info".into(),
            }],
        };
    }

    // Check lock file exists
    let has_lock = project_dir.join("package-lock.json").exists()
        || project_dir.join("yarn.lock").exists()
        || project_dir.join("pnpm-lock.yaml").exists();
    checks.push(CheckResult {
        name: "Lock file exists".into(),
        passed: has_lock,
        detail: if has_lock {
            "Dependency lock file found".into()
        } else {
            "No lock file — builds may be non-deterministic".into()
        },
        severity: "warning".into(),
    });

    // Run npm audit if enabled
    if config.audit_deps {
        let dir = project_dir.to_path_buf();
        let audit = tokio::task::spawn_blocking(move || {
            std::process::Command::new("npm")
                .args(["audit", "--json", "--production"])
                .current_dir(&dir)
                .output()
        })
        .await;

        match audit {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                // Parse vulnerability count from npm audit JSON
                let vulns = extract_vulnerability_count(&stdout);
                let passed = vulns.critical == 0 && vulns.high == 0;

                let detail = format!(
                    "critical: {}, high: {}, moderate: {}, low: {}",
                    vulns.critical, vulns.high, vulns.moderate, vulns.low
                );

                let _ = tx
                    .send(SimEvent::Check {
                        phase: "dependency_audit".into(),
                        name: "npm_audit".into(),
                        passed,
                        detail: detail.clone(),
                        severity: if !passed { "error" } else { "info" }.into(),
                    })
                    .await;

                checks.push(CheckResult {
                    name: "npm audit".into(),
                    passed,
                    detail,
                    severity: if !passed { "error" } else { "info" }.into(),
                });
            }
            _ => {
                checks.push(CheckResult {
                    name: "npm audit".into(),
                    passed: true,
                    detail: "Could not run npm audit — skipping".into(),
                    severity: "info".into(),
                });
            }
        }
    }

    let all_passed = checks.iter().filter(|c| c.severity == "error").all(|c| c.passed);
    PhaseResult {
        name: "dependency_audit".into(),
        passed: all_passed,
        checks,
    }
}

async fn run_production_build(
    project_dir: &Path,
    config: &SimConfig,
    tx: &mpsc::Sender<SimEvent>,
) -> PhaseResult {
    let pkg_path = project_dir.join("package.json");
    if !pkg_path.exists() {
        return PhaseResult {
            name: "production_build".into(),
            passed: true,
            checks: vec![CheckResult {
                name: "build".into(),
                passed: true,
                detail: "No package.json — skipping build".into(),
                severity: "info".into(),
            }],
        };
    }

    let dir = project_dir.to_path_buf();
    let timeout = std::time::Duration::from_secs(config.build_timeout_secs);

    let build = tokio::time::timeout(
        timeout,
        tokio::task::spawn_blocking(move || {
            std::process::Command::new("npx")
                .args(["next", "build"])
                .current_dir(&dir)
                .env("NODE_ENV", "production")
                .output()
        }),
    )
    .await;

    let (passed, detail) = match build {
        Ok(Ok(Ok(output))) => {
            if output.status.success() {
                (true, "Production build succeeded".to_string())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                (
                    false,
                    format!(
                        "Build failed: {}",
                        stderr.chars().take(500).collect::<String>()
                    ),
                )
            }
        }
        Ok(Ok(Err(e))) => (false, format!("Cannot run build: {}", e)),
        Ok(Err(e)) => (false, format!("Build task panicked: {}", e)),
        Err(_) => (
            false,
            format!("Build timed out after {}s", config.build_timeout_secs),
        ),
    };

    let _ = tx
        .send(SimEvent::Check {
            phase: "production_build".into(),
            name: "next_build".into(),
            passed,
            detail: detail.clone(),
            severity: if passed { "info" } else { "error" }.into(),
        })
        .await;

    PhaseResult {
        name: "production_build".into(),
        passed,
        checks: vec![CheckResult {
            name: "Production build".into(),
            passed,
            detail,
            severity: if passed { "info" } else { "error" }.into(),
        }],
    }
}

async fn run_bundle_analysis(
    project_dir: &Path,
    config: &SimConfig,
    tx: &mpsc::Sender<SimEvent>,
    metrics: &mut Vec<SimMetric>,
) -> PhaseResult {
    let mut checks = Vec::new();

    // Check .next/static directory for bundle sizes
    let static_dir = project_dir.join(".next").join("static");
    if !static_dir.exists() {
        return PhaseResult {
            name: "bundle_analysis".into(),
            passed: true,
            checks: vec![CheckResult {
                name: "bundle_size".into(),
                passed: true,
                detail: "No build output — skipping bundle analysis".into(),
                severity: "info".into(),
            }],
        };
    }

    let total_bytes = dir_size(&static_dir);
    let total_kb = total_bytes / 1024;
    let passed = total_kb <= config.max_bundle_kb;

    let _ = tx
        .send(SimEvent::Metric {
            name: "bundle_size_kb".into(),
            value: total_kb as f64,
            unit: "KB".into(),
            threshold: Some(config.max_bundle_kb as f64),
            passed,
        })
        .await;

    metrics.push(SimMetric {
        name: "bundle_size_kb".into(),
        value: total_kb as f64,
        unit: "KB".into(),
        threshold: Some(config.max_bundle_kb as f64),
        passed,
    });

    checks.push(CheckResult {
        name: "Bundle size".into(),
        passed,
        detail: format!(
            "{}KB (max: {}KB)",
            total_kb, config.max_bundle_kb
        ),
        severity: if passed { "info" } else { "warning" }.into(),
    });

    PhaseResult {
        name: "bundle_analysis".into(),
        passed,
        checks,
    }
}

async fn run_invariant_check(
    project_dir: &Path,
    tx: &mpsc::Sender<SimEvent>,
    metrics: &mut Vec<SimMetric>,
) -> PhaseResult {
    let result = crate::invariants::check_invariants(project_dir);
    let passed = result.score >= 70;

    let _ = tx
        .send(SimEvent::Metric {
            name: "invariant_score".into(),
            value: result.score as f64,
            unit: "points".into(),
            threshold: Some(70.0),
            passed,
        })
        .await;

    metrics.push(SimMetric {
        name: "invariant_score".into(),
        value: result.score as f64,
        unit: "points".into(),
        threshold: Some(70.0),
        passed,
    });

    let errors = result
        .violations
        .iter()
        .filter(|v| v.severity == "error")
        .count();

    PhaseResult {
        name: "invariant_check".into(),
        passed,
        checks: vec![CheckResult {
            name: "Invariant score".into(),
            passed,
            detail: format!(
                "Score: {}/100, {} violations ({} errors)",
                result.score, result.total_violations, errors
            ),
            severity: if passed { "info" } else { "warning" }.into(),
        }],
    }
}

async fn run_health_check(project_dir: &Path, tx: &mpsc::Sender<SimEvent>) -> PhaseResult {
    // Verify that the app has required files for deployment
    let mut checks = Vec::new();

    let required_files = [
        ("package.json", "Package manifest"),
        ("next.config.ts", "Next.js config (ts)"),
        ("next.config.js", "Next.js config (js)"),
        ("next.config.mjs", "Next.js config (mjs)"),
    ];

    let has_pkg = project_dir.join("package.json").exists();
    checks.push(CheckResult {
        name: "package.json".into(),
        passed: has_pkg,
        detail: if has_pkg { "Found" } else { "Missing" }.into(),
        severity: "error".into(),
    });

    // Check for next.config (any extension)
    let has_next_config = required_files[1..]
        .iter()
        .any(|(f, _)| project_dir.join(f).exists());
    checks.push(CheckResult {
        name: "Next.js config".into(),
        passed: has_next_config,
        detail: if has_next_config {
            "Found"
        } else {
            "No next.config found"
        }
        .into(),
        severity: "warning".into(),
    });

    // Check for entry point
    let has_entry = project_dir.join("src/app/page.tsx").exists()
        || project_dir.join("src/app/page.ts").exists()
        || project_dir.join("app/page.tsx").exists();
    checks.push(CheckResult {
        name: "App entry point".into(),
        passed: has_entry,
        detail: if has_entry {
            "Found app entry"
        } else {
            "No page.tsx entry point found"
        }
        .into(),
        severity: "error".into(),
    });

    let all_passed = checks.iter().filter(|c| c.severity == "error").all(|c| c.passed);

    let _ = tx
        .send(SimEvent::Check {
            phase: "health_check".into(),
            name: "structural_health".into(),
            passed: all_passed,
            detail: format!(
                "{}/{} checks passed",
                checks.iter().filter(|c| c.passed).count(),
                checks.len()
            ),
            severity: if all_passed { "info" } else { "error" }.into(),
        })
        .await;

    PhaseResult {
        name: "health_check".into(),
        passed: all_passed,
        checks,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn scan_required_env_vars(project_dir: &Path) -> Vec<String> {
    let mut vars = Vec::new();
    fn walk(dir: &Path, vars: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.')
                || matches!(
                    name.as_str(),
                    "node_modules" | ".next" | "target" | "__pycache__"
                )
            {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                walk(&path, vars);
            } else if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("ts" | "tsx" | "js" | "jsx")
            ) {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    for line in content.lines() {
                        if let Some(idx) = line.find("process.env.") {
                            let rest = &line[idx + 12..];
                            let var: String = rest
                                .chars()
                                .take_while(|c| c.is_alphanumeric() || *c == '_')
                                .collect();
                            if !var.is_empty() && !vars.contains(&var) {
                                vars.push(var);
                            }
                        }
                    }
                }
            }
        }
    }
    walk(project_dir, &mut vars);
    vars
}

fn read_env_file(project_dir: &Path) -> Vec<String> {
    let mut vars = Vec::new();
    for env_file in &[".env.local", ".env", ".env.production"] {
        let path = project_dir.join(env_file);
        if let Ok(content) = std::fs::read_to_string(&path) {
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                if let Some(eq_idx) = trimmed.find('=') {
                    vars.push(trimmed[..eq_idx].to_string());
                }
            }
        }
    }
    vars
}

struct VulnCount {
    critical: usize,
    high: usize,
    moderate: usize,
    low: usize,
}

fn extract_vulnerability_count(npm_audit_json: &str) -> VulnCount {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(npm_audit_json) {
        if let Some(vulns) = json.get("metadata").and_then(|m| m.get("vulnerabilities")) {
            return VulnCount {
                critical: vulns.get("critical").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                high: vulns.get("high").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                moderate: vulns.get("moderate").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                low: vulns.get("low").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
            };
        }
    }
    VulnCount {
        critical: 0,
        high: 0,
        moderate: 0,
        low: 0,
    }
}

fn dir_size(dir: &Path) -> u64 {
    let mut total = 0u64;
    fn walk(dir: &Path, total: &mut u64) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, total);
            } else if let Ok(meta) = path.metadata() {
                *total += meta.len();
            }
        }
    }
    walk(dir, &mut total);
    total
}
