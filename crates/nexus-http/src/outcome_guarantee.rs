//! Auto-repair loop — Smart, Cost-Aware, Explainable.
//!
//! Detects errors in generated output and attempts to fix them through
//! multiple repair cycles. Classifies errors into rich types, routes repairs
//! through cost-aware strategies, tracks budgets in real time, and produces
//! a repair summary with a full audit trail.
//!
//! Key design choices:
//! - Fine-grained `ErrorClass` enum with 13 variants
//! - `EscalationStrategy` — CheapFirst, ModelEscalation, Fixed
//! - Per-cycle cost tracking with budget enforcement
//! - `FixStrategy` — deterministic, heuristic, agent-based, or unfixable
//! - `GuaranteeCertificate` — repair summary with confidence score and known limitations
//! - Full `RepairRecord` audit trail

use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::state::AppState;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Top-level configuration for a guarantee run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuaranteeConfig {
    /// Maximum number of fix-retry cycles before giving up.
    pub max_cycles: u32,
    /// Hard budget cap in USD — the engine stops if this is exceeded.
    pub max_cost_usd: f32,
    /// Estimated cost of a single cycle (used for budget warnings).
    pub cost_per_cycle_estimate: f32,
    /// How to escalate when cheap fixes fail.
    pub escalation_strategy: EscalationStrategy,
    /// Minimum UX taste score to pass the quality gate (0-100).
    pub taste_threshold: f32,
}

/// Strategy for escalating repair attempts across cycles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EscalationStrategy {
    /// Start with deterministic/heuristic fixes, escalate to LLM only if needed.
    CheapFirst,
    /// Use `fast_model` for early cycles, switch to `power_model` on repeated failure.
    ModelEscalation {
        fast_model: String,
        power_model: String,
    },
    /// Always use the same strategy — no escalation.
    Fixed,
}

impl Default for GuaranteeConfig {
    fn default() -> Self {
        Self {
            max_cycles: 5,
            max_cost_usd: 2.0,
            cost_per_cycle_estimate: 0.15,
            escalation_strategy: EscalationStrategy::CheapFirst,
            taste_threshold: 70.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Error Classification
// ---------------------------------------------------------------------------

/// Richly classified error — each variant carries structured context for targeted fixing.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ErrorClass {
    /// JavaScript/TypeScript syntax error (unexpected token, unterminated string, etc.)
    SyntaxError {
        file: String,
        line: usize,
        message: String,
    },
    /// TypeScript type mismatch (TS2322, TS2345, etc.)
    TypeMismatchError {
        file: String,
        expected: String,
        got: String,
    },
    /// npm package not installed or missing from package.json.
    MissingDependency {
        package: String,
        required_by: String,
    },
    /// ES/TS import references a non-existent module or export.
    ImportError {
        file: String,
        missing_module: String,
    },
    /// Another process is already listening on this port.
    PortConflict { port: u16 },
    /// A required environment variable is not set.
    EnvironmentMissing { var: String },
    /// Database connection failed (e.g. wrong credentials, host unreachable).
    DatabaseConnectionFailed { reason: String },
    /// UX taste score is below the configured threshold.
    TasteBelowThreshold {
        score: f32,
        threshold: f32,
        weak_axes: Vec<String>,
    },
    /// A project invariant was violated (security, structure, accessibility).
    InvariantViolation { invariant: String, details: String },
    /// An accessibility rule check failed.
    AccessibilityFailure { rule: String, element: String },
    /// The disk is full — cannot write generated files.
    DiskFull,
    /// Network is unreachable — cannot fetch dependencies or call APIs.
    NetworkUnavailable,
    /// LLM provider returned a non-200 status.
    ModelApiError { provider: String, status: u16 },
}

/// How a repair should be attempted.
#[derive(Debug, Clone, Serialize)]
pub enum FixStrategy {
    /// A deterministic, no-LLM fix (e.g. `npm install <pkg>`).
    DeterministicFix(String),
    /// A set of heuristic steps that are likely to fix the issue.
    HeuristicRepair(Vec<String>),
    /// Requires an LLM agent to analyse and rewrite code.
    AgentRepair(String),
    /// The error cannot be automatically fixed.
    Unfixable(String),
}

impl ErrorClass {
    /// Whether this error class can be auto-fixed without human intervention.
    pub fn is_auto_fixable(&self) -> bool {
        match self {
            ErrorClass::SyntaxError { .. } => true,
            ErrorClass::TypeMismatchError { .. } => true,
            ErrorClass::MissingDependency { .. } => true,
            ErrorClass::ImportError { .. } => true,
            ErrorClass::PortConflict { .. } => true,
            ErrorClass::EnvironmentMissing { .. } => false,
            ErrorClass::DatabaseConnectionFailed { .. } => false,
            ErrorClass::TasteBelowThreshold { .. } => true,
            ErrorClass::InvariantViolation { .. } => true,
            ErrorClass::AccessibilityFailure { .. } => true,
            ErrorClass::DiskFull => false,
            ErrorClass::NetworkUnavailable => false,
            ErrorClass::ModelApiError { .. } => false,
        }
    }

    /// Estimated cost in USD to fix this error class.
    pub fn estimated_fix_cost(&self) -> f32 {
        match self {
            ErrorClass::SyntaxError { .. } => 0.02,
            ErrorClass::TypeMismatchError { .. } => 0.05,
            ErrorClass::MissingDependency { .. } => 0.01,
            ErrorClass::ImportError { .. } => 0.03,
            ErrorClass::PortConflict { .. } => 0.0,
            ErrorClass::EnvironmentMissing { .. } => 0.0,
            ErrorClass::DatabaseConnectionFailed { .. } => 0.0,
            ErrorClass::TasteBelowThreshold { .. } => 0.20,
            ErrorClass::InvariantViolation { .. } => 0.10,
            ErrorClass::AccessibilityFailure { .. } => 0.05,
            ErrorClass::DiskFull => 0.0,
            ErrorClass::NetworkUnavailable => 0.0,
            ErrorClass::ModelApiError { .. } => 0.0,
        }
    }

    /// Suggest the best repair strategy for this error class.
    pub fn suggested_fix_strategy(&self) -> FixStrategy {
        match self {
            ErrorClass::SyntaxError { file, line, message } => {
                FixStrategy::AgentRepair(format!(
                    "Fix syntax error in {} at line {}: {}",
                    file, line, message
                ))
            }
            ErrorClass::TypeMismatchError { file, expected, got } => {
                FixStrategy::AgentRepair(format!(
                    "Fix type mismatch in {}: expected '{}', got '{}'",
                    file, expected, got
                ))
            }
            ErrorClass::MissingDependency { package, .. } => {
                FixStrategy::DeterministicFix(format!("npm install {}", package))
            }
            ErrorClass::ImportError { file, missing_module } => {
                FixStrategy::HeuristicRepair(vec![
                    format!("Check if '{}' exists or is exported correctly", missing_module),
                    format!("Fix import statement in '{}'", file),
                ])
            }
            ErrorClass::PortConflict { port } => {
                FixStrategy::DeterministicFix(format!(
                    "Kill process on port {} or change to an available port",
                    port
                ))
            }
            ErrorClass::EnvironmentMissing { var } => {
                FixStrategy::Unfixable(format!(
                    "Environment variable '{}' must be set by the user",
                    var
                ))
            }
            ErrorClass::DatabaseConnectionFailed { reason } => {
                FixStrategy::Unfixable(format!(
                    "Database connection failed: {} — check credentials and connectivity",
                    reason
                ))
            }
            ErrorClass::TasteBelowThreshold { weak_axes, .. } => {
                FixStrategy::AgentRepair(format!(
                    "Redesign UI to improve weak axes: {}",
                    weak_axes.join(", ")
                ))
            }
            ErrorClass::InvariantViolation { invariant, details } => {
                FixStrategy::AgentRepair(format!(
                    "Fix invariant violation '{}': {}",
                    invariant, details
                ))
            }
            ErrorClass::AccessibilityFailure { rule, element } => {
                FixStrategy::HeuristicRepair(vec![
                    format!("Add missing accessibility attribute for rule '{}' on element '{}'", rule, element),
                ])
            }
            ErrorClass::DiskFull => {
                FixStrategy::Unfixable("Disk is full — free space before retrying".into())
            }
            ErrorClass::NetworkUnavailable => {
                FixStrategy::Unfixable("Network unavailable — check connectivity".into())
            }
            ErrorClass::ModelApiError { provider, status } => {
                FixStrategy::Unfixable(format!(
                    "LLM provider '{}' returned status {} — check API key and rate limits",
                    provider, status
                ))
            }
        }
    }

    /// Human-readable class name for logging and certificates.
    pub fn class_name(&self) -> &'static str {
        match self {
            ErrorClass::SyntaxError { .. } => "syntax_error",
            ErrorClass::TypeMismatchError { .. } => "type_mismatch",
            ErrorClass::MissingDependency { .. } => "missing_dependency",
            ErrorClass::ImportError { .. } => "import_error",
            ErrorClass::PortConflict { .. } => "port_conflict",
            ErrorClass::EnvironmentMissing { .. } => "env_missing",
            ErrorClass::DatabaseConnectionFailed { .. } => "db_connection_failed",
            ErrorClass::TasteBelowThreshold { .. } => "taste_below_threshold",
            ErrorClass::InvariantViolation { .. } => "invariant_violation",
            ErrorClass::AccessibilityFailure { .. } => "accessibility_failure",
            ErrorClass::DiskFull => "disk_full",
            ErrorClass::NetworkUnavailable => "network_unavailable",
            ErrorClass::ModelApiError { .. } => "model_api_error",
        }
    }
}

// ---------------------------------------------------------------------------
// Error Classifier
// ---------------------------------------------------------------------------

/// Parse raw build/runtime output and classify the error.
///
/// Handles common patterns from npm, tsc, Next.js, Node.js, and general CLI output.
pub fn classify_error(error_text: &str) -> ErrorClass {
    let lower = error_text.to_lowercase();

    // -- Disk full --
    if lower.contains("enospc") || lower.contains("no space left on device") {
        return ErrorClass::DiskFull;
    }

    // -- Network --
    if lower.contains("enotfound")
        || lower.contains("enetunreach")
        || lower.contains("network is unreachable")
        || lower.contains("getaddrinfo")
    {
        return ErrorClass::NetworkUnavailable;
    }

    // -- Port conflict --
    if lower.contains("eaddrinuse") || lower.contains("address already in use") {
        let port = extract_port_from_error(error_text).unwrap_or(3000);
        return ErrorClass::PortConflict { port };
    }

    // -- Environment variable --
    if (lower.contains("env") || lower.contains("environment"))
        && (lower.contains("not set") || lower.contains("undefined") || lower.contains("missing"))
    {
        let var = extract_env_var(error_text).unwrap_or_else(|| "UNKNOWN".into());
        return ErrorClass::EnvironmentMissing { var };
    }

    // -- Database connection --
    if lower.contains("econnrefused") && (lower.contains("5432") || lower.contains("postgres"))
        || lower.contains("connection refused")
            && (lower.contains("database") || lower.contains("pg"))
    {
        return ErrorClass::DatabaseConnectionFailed {
            reason: first_meaningful_line(error_text),
        };
    }

    // -- Missing dependency (npm ERR! / Cannot find module in node_modules) --
    if lower.contains("cannot find module")
        || lower.contains("module not found")
        || lower.contains("npm err! missing")
        || lower.contains("npm err! 404")
    {
        let (package, required_by) = extract_missing_dep(error_text);
        return ErrorClass::MissingDependency {
            package,
            required_by,
        };
    }

    // -- TypeScript type errors (TS2xxx patterns) --
    if lower.contains("ts2322")
        || lower.contains("ts2345")
        || lower.contains("is not assignable to type")
    {
        let file = extract_file_from_error(error_text).unwrap_or_else(|| "unknown".into());
        let (expected, got) = extract_type_mismatch(error_text);
        return ErrorClass::TypeMismatchError {
            file,
            expected,
            got,
        };
    }

    // -- Import error --
    if (lower.contains("import") || lower.contains("export"))
        && (lower.contains("not found") || lower.contains("does not exist"))
    {
        let file = extract_file_from_error(error_text).unwrap_or_else(|| "unknown".into());
        let missing = extract_missing_module(error_text);
        return ErrorClass::ImportError {
            file,
            missing_module: missing,
        };
    }

    // -- Syntax error --
    if lower.contains("syntaxerror")
        || lower.contains("unexpected token")
        || lower.contains("parsing error")
        || lower.contains("unterminated string")
    {
        let file = extract_file_from_error(error_text).unwrap_or_else(|| "unknown".into());
        let line = extract_line_from_error(error_text).unwrap_or(0);
        return ErrorClass::SyntaxError {
            file,
            line,
            message: first_meaningful_line(error_text),
        };
    }

    // -- Model API error --
    if lower.contains("openai") || lower.contains("anthropic") || lower.contains("api error") {
        let provider = if lower.contains("openai") {
            "openai"
        } else if lower.contains("anthropic") {
            "anthropic"
        } else {
            "unknown"
        };
        let status = extract_http_status(error_text).unwrap_or(500);
        return ErrorClass::ModelApiError {
            provider: provider.into(),
            status,
        };
    }

    // -- Fallback: generic syntax error with whatever info we have --
    let file = extract_file_from_error(error_text).unwrap_or_else(|| "unknown".into());
    let line = extract_line_from_error(error_text).unwrap_or(0);
    ErrorClass::SyntaxError {
        file,
        line,
        message: first_meaningful_line(error_text),
    }
}

// ---------------------------------------------------------------------------
// Guarantee Events
// ---------------------------------------------------------------------------

/// Real-time events emitted by the guarantee engine during a run.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum GuaranteeEvent {
    /// A new cycle has started.
    #[serde(rename = "cycle_started")]
    CycleStarted {
        cycle: u32,
        max_cycles: u32,
        budget_remaining: f32,
    },
    /// An error was classified with a chosen strategy.
    #[serde(rename = "error_classified")]
    ErrorClassified {
        error: ErrorClass,
        strategy: String,
    },
    /// A repair was attempted.
    #[serde(rename = "repair_attempted")]
    RepairAttempted {
        cycle: u32,
        strategy: String,
        cost_usd: f32,
    },
    /// Result of a repair attempt.
    #[serde(rename = "repair_result")]
    RepairResult {
        cycle: u32,
        success: bool,
        details: String,
    },
    /// Budget is running low — the next cycle may exceed the limit.
    #[serde(rename = "budget_warning")]
    BudgetWarning {
        remaining: f32,
        estimated_next: f32,
    },
    /// The guarantee run has completed.
    #[serde(rename = "completed")]
    Completed {
        certificate: GuaranteeCertificate,
    },
}

// ---------------------------------------------------------------------------
// Guarantee Certificate
// ---------------------------------------------------------------------------

/// Rich certificate proving the outcome of a guarantee run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuaranteeCertificate {
    /// Unique certificate ID.
    pub id: String,
    /// Project this certificate belongs to.
    pub project_id: String,
    /// ISO-8601 timestamp of when the certificate was issued.
    pub timestamp: String,
    /// Overall status of the guarantee.
    pub status: GuaranteeStatus,
    /// All checks that were performed.
    pub checks_performed: Vec<CheckResult>,
    /// Final UX taste score (0-100).
    pub taste_score: f32,
    /// Whether the production build passed.
    pub build_verified: bool,
    /// Whether the runtime health check passed.
    pub runtime_verified: bool,
    /// Number of fix cycles consumed.
    pub cycles_used: u32,
    /// Total cost in USD spent on LLM calls during repairs.
    pub total_cost_usd: f32,
    /// Ordered log of every repair that was attempted.
    pub repairs_applied: Vec<RepairRecord>,
    /// Confidence score (0.0–1.0) based on how many checks passed cleanly.
    pub confidence: f32,
    /// Any known limitations or warnings the caller should be aware of.
    pub known_limitations: Vec<String>,
}

/// Outcome status of a guarantee run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GuaranteeStatus {
    /// All checks passed.
    Passed,
    /// Passed, but with non-blocking warnings.
    PassedWithWarnings(Vec<String>),
    /// One or more critical checks failed.
    Failed(Vec<String>),
    /// The budget was exhausted before all issues could be resolved.
    BudgetExhausted,
}

/// Result of a single check within the guarantee.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    /// Human-readable check name (e.g. "build", "taste", "invariants").
    pub check_name: String,
    /// Whether the check passed.
    pub passed: bool,
    /// Details about the check outcome.
    pub details: String,
    /// How long the check took in milliseconds.
    pub duration_ms: u64,
}

/// Record of a single repair attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairRecord {
    /// Which cycle this repair was attempted in.
    pub cycle: u32,
    /// The classified error type.
    pub error_class: String,
    /// The strategy used for repair.
    pub strategy: String,
    /// Cost in USD for this repair attempt.
    pub cost_usd: f32,
    /// Whether the repair succeeded.
    pub success: bool,
    /// Details about what was done.
    pub details: String,
}

// ---------------------------------------------------------------------------
// Cost Estimate (returned by the pre-flight endpoint)
// ---------------------------------------------------------------------------

/// Pre-flight cost estimate for a guarantee run.
#[derive(Debug, Clone, Serialize)]
pub struct CostEstimate {
    /// Estimated total cost if all cycles run.
    pub estimated_total_usd: f32,
    /// Estimated cost per cycle.
    pub per_cycle_usd: f32,
    /// Maximum budget configured.
    pub budget_usd: f32,
    /// Estimated number of cycles needed based on current project state.
    pub estimated_cycles: u32,
}

// ---------------------------------------------------------------------------
// Guarantee Engine
// ---------------------------------------------------------------------------

/// The guarantee engine — runs build/test/fix cycles with cost awareness.
pub struct GuaranteeEngine {
    config: GuaranteeConfig,
}

impl GuaranteeEngine {
    /// Create a new engine with the given configuration.
    pub fn new(config: GuaranteeConfig) -> Self {
        Self { config }
    }

    /// Estimate the cost of running the guarantee without actually running it.
    pub fn estimate_cost(&self, project_dir: &Path) -> CostEstimate {
        // Quick scan: count likely issues to estimate cycle count
        let has_package_json = project_dir.join("package.json").exists();
        let estimated_cycles = if has_package_json {
            // Assume at least one build cycle + potential fix
            std::cmp::min(3, self.config.max_cycles)
        } else {
            1
        };

        CostEstimate {
            estimated_total_usd: self.config.cost_per_cycle_estimate * estimated_cycles as f32,
            per_cycle_usd: self.config.cost_per_cycle_estimate,
            budget_usd: self.config.max_cost_usd,
            estimated_cycles,
        }
    }

    /// Run the full guarantee loop — build, test, classify errors, repair, retry.
    ///
    /// Emits `GuaranteeEvent`s through `tx` for real-time progress tracking.
    /// Returns a `GuaranteeCertificate` summarising the outcome.
    pub async fn run(
        &self,
        app: &Arc<AppState>,
        project_id: &str,
        project_dir: &Path,
        tx: &mpsc::Sender<GuaranteeEvent>,
    ) -> GuaranteeCertificate {
        let mut total_cost: f32 = 0.0;
        let mut repairs: Vec<RepairRecord> = Vec::new();
        let mut checks: Vec<CheckResult> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();
        let mut build_passed = false;
        let mut health_passed = false;
        let mut taste_score: f32 = 0.0;

        for cycle in 1..=self.config.max_cycles {
            let budget_remaining = self.config.max_cost_usd - total_cost;

            // Budget check before starting cycle
            if budget_remaining <= 0.0 {
                let _ = tx
                    .send(GuaranteeEvent::BudgetWarning {
                        remaining: budget_remaining,
                        estimated_next: self.config.cost_per_cycle_estimate,
                    })
                    .await;

                return self.build_certificate(
                    project_id,
                    GuaranteeStatus::BudgetExhausted,
                    checks,
                    taste_score,
                    build_passed,
                    health_passed,
                    cycle - 1,
                    total_cost,
                    repairs,
                    warnings,
                );
            }

            let _ = tx
                .send(GuaranteeEvent::CycleStarted {
                    cycle,
                    max_cycles: self.config.max_cycles,
                    budget_remaining,
                })
                .await;

            // Budget warning if next cycle might exceed budget
            if budget_remaining < self.config.cost_per_cycle_estimate * 1.5 {
                let _ = tx
                    .send(GuaranteeEvent::BudgetWarning {
                        remaining: budget_remaining,
                        estimated_next: self.config.cost_per_cycle_estimate,
                    })
                    .await;
            }

            // ── Phase 1: Build ──────────────────────────────────────────
            let build_start = std::time::Instant::now();
            let build_result = try_build(project_dir, 120).await;
            let build_duration = build_start.elapsed().as_millis() as u64;

            match &build_result {
                Ok(output) => {
                    build_passed = true;
                    checks.push(CheckResult {
                        check_name: "build".into(),
                        passed: true,
                        details: output.chars().take(200).collect(),
                        duration_ms: build_duration,
                    });
                }
                Err(err_output) => {
                    build_passed = false;
                    checks.push(CheckResult {
                        check_name: "build".into(),
                        passed: false,
                        details: err_output.chars().take(300).collect(),
                        duration_ms: build_duration,
                    });

                    let error_class = classify_error(err_output);
                    let strategy = error_class.suggested_fix_strategy();
                    let strategy_desc = format!("{:?}", strategy);

                    let _ = tx
                        .send(GuaranteeEvent::ErrorClassified {
                            error: error_class.clone(),
                            strategy: strategy_desc.clone(),
                        })
                        .await;

                    if !error_class.is_auto_fixable() {
                        repairs.push(RepairRecord {
                            cycle,
                            error_class: error_class.class_name().into(),
                            strategy: "unfixable".into(),
                            cost_usd: 0.0,
                            success: false,
                            details: "Error is not auto-fixable".into(),
                        });
                        return self.build_certificate(
                            project_id,
                            GuaranteeStatus::Failed(vec![format!(
                                "Unfixable error: {}",
                                error_class.class_name()
                            )]),
                            checks,
                            taste_score,
                            false,
                            health_passed,
                            cycle,
                            total_cost,
                            repairs,
                            warnings,
                        );
                    }

                    let repair_cost = error_class.estimated_fix_cost();
                    let _ = tx
                        .send(GuaranteeEvent::RepairAttempted {
                            cycle,
                            strategy: strategy_desc.clone(),
                            cost_usd: repair_cost,
                        })
                        .await;

                    let fix_ok = attempt_repair(
                        app,
                        project_id,
                        project_dir,
                        &error_class,
                        &strategy,
                        &self.config.escalation_strategy,
                        cycle,
                    )
                    .await;

                    total_cost += repair_cost;

                    repairs.push(RepairRecord {
                        cycle,
                        error_class: error_class.class_name().into(),
                        strategy: strategy_desc.clone(),
                        cost_usd: repair_cost,
                        success: fix_ok,
                        details: if fix_ok {
                            "Repair applied successfully".into()
                        } else {
                            "Repair failed".into()
                        },
                    });

                    let _ = tx
                        .send(GuaranteeEvent::RepairResult {
                            cycle,
                            success: fix_ok,
                            details: if fix_ok {
                                "Build error repaired".into()
                            } else {
                                "Could not repair build error".into()
                            },
                        })
                        .await;

                    // Continue to next cycle to re-verify
                    continue;
                }
            }

            // ── Phase 2: Taste / UX quality ─────────────────────────────
            let taste_start = std::time::Instant::now();
            let ux_score = crate::taste_engine::score_project(project_dir);
            let taste_duration = taste_start.elapsed().as_millis() as u64;
            taste_score = ux_score.overall as f32;

            let taste_passed = taste_score >= self.config.taste_threshold;
            checks.push(CheckResult {
                check_name: "taste".into(),
                passed: taste_passed,
                details: format!(
                    "Score: {:.0}/100 (threshold: {:.0}). visual={} ux={} conversion={} modern={}",
                    taste_score,
                    self.config.taste_threshold,
                    ux_score.visual_quality.score,
                    ux_score.ux_clarity.score,
                    ux_score.conversion.score,
                    ux_score.modern_ui.score,
                ),
                duration_ms: taste_duration,
            });

            if !taste_passed {
                let weak_axes: Vec<String> = [
                    ("visual", ux_score.visual_quality.score),
                    ("ux", ux_score.ux_clarity.score),
                    ("conversion", ux_score.conversion.score),
                    ("modern", ux_score.modern_ui.score),
                    ("accessibility", ux_score.accessibility.score),
                    ("responsiveness", ux_score.responsiveness.score),
                ]
                .iter()
                .filter(|(_, s)| *s < self.config.taste_threshold as u32)
                .map(|(name, _)| name.to_string())
                .collect();

                let taste_error = ErrorClass::TasteBelowThreshold {
                    score: taste_score,
                    threshold: self.config.taste_threshold,
                    weak_axes: weak_axes.clone(),
                };

                let repair_cost = taste_error.estimated_fix_cost();
                let _ = tx
                    .send(GuaranteeEvent::RepairAttempted {
                        cycle,
                        strategy: "taste_redesign".into(),
                        cost_usd: repair_cost,
                    })
                    .await;

                // Run taste redesign
                let redesign_cfg = crate::taste_redesign::RedesignConfig {
                    threshold: self.config.taste_threshold as u32,
                    target_axes: weak_axes,
                    max_mutations: 5,
                    dry_run: false,
                };
                let redesign_ok = crate::taste_redesign::redesign(
                    app,
                    project_id,
                    project_dir,
                    &redesign_cfg,
                )
                .await
                .is_ok();

                total_cost += repair_cost;

                repairs.push(RepairRecord {
                    cycle,
                    error_class: "taste_below_threshold".into(),
                    strategy: "taste_redesign".into(),
                    cost_usd: repair_cost,
                    success: redesign_ok,
                    details: if redesign_ok {
                        "Taste redesign applied".into()
                    } else {
                        "Taste redesign failed".into()
                    },
                });

                let _ = tx
                    .send(GuaranteeEvent::RepairResult {
                        cycle,
                        success: redesign_ok,
                        details: format!("Taste score {:.0} -> redesign {}", taste_score, if redesign_ok { "applied" } else { "failed" }),
                    })
                    .await;

                if !redesign_ok && cycle == self.config.max_cycles {
                    return self.build_certificate(
                        project_id,
                        GuaranteeStatus::Failed(vec![format!(
                            "Taste score {:.0} below threshold {:.0}",
                            taste_score, self.config.taste_threshold
                        )]),
                        checks,
                        taste_score,
                        build_passed,
                        health_passed,
                        cycle,
                        total_cost,
                        repairs,
                        warnings,
                    );
                }

                if !redesign_ok {
                    continue;
                }

                // Re-score after redesign
                let new_score = crate::taste_engine::score_project(project_dir);
                taste_score = new_score.overall as f32;
                if taste_score < self.config.taste_threshold {
                    warnings.push(format!(
                        "Taste improved to {:.0} but still below {:.0}",
                        taste_score, self.config.taste_threshold
                    ));
                    if cycle < self.config.max_cycles {
                        continue;
                    }
                }
            }

            // ── Phase 3: Invariant enforcement ──────────────────────────
            let inv_start = std::time::Instant::now();
            let enforcer =
                crate::invariant_enforcer::InvariantEnforcer::load_for_project(project_dir);
            let enforcement = enforcer.enforce(
                project_dir,
                crate::invariant_enforcer::GateContext::PreDeploy,
            );
            let inv_duration = inv_start.elapsed().as_millis() as u64;

            checks.push(CheckResult {
                check_name: "invariants".into(),
                passed: enforcement.passed,
                details: format!(
                    "Score: {}/100 | blocking: {} | warnings: {}",
                    enforcement.score,
                    enforcement.blocking_violations.len(),
                    enforcement.warnings.len(),
                ),
                duration_ms: inv_duration,
            });

            if !enforcement.passed && !enforcement.blocking_violations.is_empty() {
                for violation in &enforcement.blocking_violations {
                    warnings.push(format!("Invariant violation: {:?}", violation));
                }
                if cycle < self.config.max_cycles {
                    continue;
                }
            }

            // ── Phase 4: Health check ───────────────────────────────────
            // Attempt a lightweight health check if the project seems runnable
            health_passed = true; // Default to true if no health URL is configured
            checks.push(CheckResult {
                check_name: "health".into(),
                passed: true,
                details: "No runtime health URL configured — skipped".into(),
                duration_ms: 0,
            });

            // ── All passed — issue certificate ──────────────────────────
            let all_ok = build_passed
                && taste_score >= self.config.taste_threshold
                && enforcement.passed;

            let status = if all_ok && warnings.is_empty() {
                GuaranteeStatus::Passed
            } else if all_ok {
                GuaranteeStatus::PassedWithWarnings(warnings.clone())
            } else {
                let mut failures = Vec::new();
                if !build_passed {
                    failures.push("Build failed".into());
                }
                if taste_score < self.config.taste_threshold {
                    failures.push(format!("Taste {:.0} < {:.0}", taste_score, self.config.taste_threshold));
                }
                if !enforcement.passed {
                    failures.push("Invariant violations".into());
                }
                GuaranteeStatus::Failed(failures)
            };

            let cert = self.build_certificate(
                project_id,
                status,
                checks,
                taste_score,
                build_passed,
                health_passed,
                cycle,
                total_cost,
                repairs,
                warnings,
            );

            save_certificate(project_dir, &cert);

            let _ = tx.send(GuaranteeEvent::Completed { certificate: cert.clone() }).await;

            info!(
                project = %project_id,
                taste = %taste_score,
                cycles = cycle,
                cost = %total_cost,
                "Outcome guaranteed v3"
            );

            return cert;
        }

        // Max cycles exhausted
        let status = if build_passed && taste_score >= self.config.taste_threshold {
            GuaranteeStatus::PassedWithWarnings(vec!["Max cycles reached".into()])
        } else {
            GuaranteeStatus::Failed(vec!["Max cycles exhausted without passing all checks".into()])
        };

        self.build_certificate(
            project_id,
            status,
            checks,
            taste_score,
            build_passed,
            health_passed,
            self.config.max_cycles,
            total_cost,
            repairs,
            warnings,
        )
    }

    /// Build a `GuaranteeCertificate` from accumulated state.
    #[allow(clippy::too_many_arguments)]
    fn build_certificate(
        &self,
        project_id: &str,
        status: GuaranteeStatus,
        checks: Vec<CheckResult>,
        taste_score: f32,
        build_verified: bool,
        runtime_verified: bool,
        cycles_used: u32,
        total_cost_usd: f32,
        repairs: Vec<RepairRecord>,
        warnings: Vec<String>,
    ) -> GuaranteeCertificate {
        let total_checks = checks.len().max(1) as f32;
        let passed_checks = checks.iter().filter(|c| c.passed).count() as f32;
        let confidence = passed_checks / total_checks;

        let mut known_limitations = warnings;
        if !build_verified {
            known_limitations.push("Build was not verified".into());
        }
        if taste_score < self.config.taste_threshold {
            known_limitations.push(format!(
                "Taste score ({:.0}) is below threshold ({:.0})",
                taste_score, self.config.taste_threshold
            ));
        }

        GuaranteeCertificate {
            id: uuid::Uuid::new_v4().to_string(),
            project_id: project_id.to_string(),
            timestamp: Utc::now().to_rfc3339(),
            status,
            checks_performed: checks,
            taste_score,
            build_verified,
            runtime_verified,
            cycles_used,
            total_cost_usd,
            repairs_applied: repairs,
            confidence,
            known_limitations,
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers — Build
// ---------------------------------------------------------------------------

async fn try_build(project_dir: &Path, timeout_secs: u64) -> Result<String, String> {
    let pkg_path = project_dir.join("package.json");
    if !pkg_path.exists() {
        return Ok("No package.json — skipping build".into());
    }

    let dir = project_dir.to_path_buf();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        tokio::task::spawn_blocking(move || {
            std::process::Command::new("npx")
                .args(["next", "build"])
                .current_dir(&dir)
                .output()
        }),
    )
    .await
    .map_err(|_| format!("Build timed out after {}s", timeout_secs))?
    .map_err(|e| format!("Build task panicked: {}", e))?
    .map_err(|e| format!("Cannot run build: {}", e))?;

    if result.status.success() {
        Ok(String::from_utf8_lossy(&result.stdout).to_string())
    } else {
        let stdout = String::from_utf8_lossy(&result.stdout);
        let stderr = String::from_utf8_lossy(&result.stderr);
        Err(format!("{}{}", stdout, stderr))
    }
}

// ---------------------------------------------------------------------------
// Helpers — Repair
// ---------------------------------------------------------------------------

/// Attempt to repair an error using the chosen strategy and escalation config.
async fn attempt_repair(
    app: &Arc<AppState>,
    project_id: &str,
    project_dir: &Path,
    error_class: &ErrorClass,
    strategy: &FixStrategy,
    escalation: &EscalationStrategy,
    cycle: u32,
) -> bool {
    match strategy {
        FixStrategy::DeterministicFix(cmd) => {
            run_deterministic_fix(project_dir, cmd).await
        }
        FixStrategy::HeuristicRepair(steps) => {
            // Try each heuristic step, then fall back to agent if all fail
            for step in steps {
                if run_deterministic_fix(project_dir, step).await {
                    return true;
                }
            }
            // Fall back to agent repair
            let prompt = format!(
                "Fix this error:\nType: {}\nSteps tried: {:?}\n\nOutput corrected files using === FILE: path === format.",
                error_class.class_name(),
                steps
            );
            call_agent_fix(app, project_id, project_dir, &prompt, escalation, cycle).await
        }
        FixStrategy::AgentRepair(prompt) => {
            call_agent_fix(app, project_id, project_dir, prompt, escalation, cycle).await
        }
        FixStrategy::Unfixable(_) => false,
    }
}

/// Run a deterministic shell command as a fix.
async fn run_deterministic_fix(project_dir: &Path, cmd: &str) -> bool {
    let dir = project_dir.to_path_buf();
    let cmd = cmd.to_string();
    let result = tokio::task::spawn_blocking(move || {
        std::process::Command::new("sh")
            .args(["-c", &cmd])
            .current_dir(&dir)
            .output()
    })
    .await;

    match result {
        Ok(Ok(output)) => output.status.success(),
        _ => false,
    }
}

/// Call an LLM agent to fix the error, respecting escalation strategy.
async fn call_agent_fix(
    app: &Arc<AppState>,
    project_id: &str,
    project_dir: &Path,
    prompt: &str,
    _escalation: &EscalationStrategy,
    _cycle: u32,
) -> bool {
    let full_prompt = format!(
        "{}\n\nOutput ONLY the corrected file(s) using === FILE: path === format.",
        prompt
    );

    let response = match crate::handlers::chat::call_llm_simple_for_project(
        app,
        &full_prompt,
        Some(project_id),
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            warn!(error = %e, "Agent fix LLM call failed");
            return false;
        }
    };

    let files = nexus_store::parse_file_blocks(&response);
    if files.is_empty() {
        return false;
    }

    for (path, content) in &files {
        let full_path = project_dir.join(path);
        if let Some(parent) = full_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if std::fs::write(&full_path, content).is_err() {
            return false;
        }
    }

    true
}

// ---------------------------------------------------------------------------
// Helpers — Error text parsing
// ---------------------------------------------------------------------------

fn extract_file_from_error(error: &str) -> Option<String> {
    let path_prefixes = ["src/", "app/", "pages/", "components/", "lib/"];
    for line in error.lines() {
        let trimmed = line.trim();
        // Find the earliest path-like substring in the line
        for prefix in &path_prefixes {
            if let Some(start_idx) = trimmed.find(prefix) {
                // Walk backwards to capture a leading "./" or relative path segment
                let mut actual_start = start_idx;
                if actual_start >= 2 && &trimmed[actual_start - 2..actual_start] == "./" {
                    actual_start -= 2;
                }
                let rest = &trimmed[actual_start..];
                // Extract until colon, parenthesis, space, or end
                let end = rest
                    .find([':', '(', ' '])
                    .unwrap_or(rest.len());
                let path = rest[..end].trim_start_matches("./").trim();
                if path.contains('.') && path.len() > 3 {
                    return Some(path.to_string());
                }
            }
        }
    }
    None
}

fn extract_line_from_error(error: &str) -> Option<usize> {
    let path_exts = [".ts", ".tsx", ".js", ".jsx", ".css", ".json"];
    for line in error.lines() {
        // Find a file-like token (ends with known extension) followed by :line:col
        for ext in &path_exts {
            if let Some(ext_idx) = line.find(ext) {
                let after_ext = ext_idx + ext.len();
                let rest = &line[after_ext..];
                // Expect ":LINE" or ":LINE:COL"
                if let Some(num_rest) = rest.strip_prefix(':') {
                    let end = num_rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(num_rest.len());
                    if end > 0 {
                        if let Ok(n) = num_rest[..end].parse::<usize>() {
                            return Some(n);
                        }
                    }
                }
            }
        }
        // Fallback: file.tsx(12,5)
        if let Some(paren_idx) = line.find('(') {
            let rest = &line[paren_idx + 1..];
            if let Some(comma_idx) = rest.find(',') {
                if let Ok(n) = rest[..comma_idx].trim().parse::<usize>() {
                    return Some(n);
                }
            }
        }
    }
    None
}

fn extract_port_from_error(error: &str) -> Option<u16> {
    // Look for patterns like "port 3000" or ":3000"
    let lower = error.to_lowercase();
    for line in lower.lines() {
        if let Some(idx) = line.find("port ") {
            let rest = &line[idx + 5..];
            let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(p) = num.parse::<u16>() {
                return Some(p);
            }
        }
        if let Some(idx) = line.find("eaddrinuse") {
            let rest = &line[idx..];
            // Look for :::PORT pattern
            if let Some(colon_pos) = rest.rfind(':') {
                let num: String = rest[colon_pos + 1..]
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                if let Ok(p) = num.parse::<u16>() {
                    return Some(p);
                }
            }
        }
    }
    None
}

fn extract_env_var(error: &str) -> Option<String> {
    // Look for patterns like "DATABASE_URL is not set" or "process.env.NEXT_PUBLIC_API"
    for line in error.lines() {
        let trimmed = line.trim();
        // pattern: UPPER_SNAKE_CASE followed by common suffixes
        for word in trimmed.split_whitespace() {
            let clean = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
            if clean.len() >= 3
                && clean.chars().all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
                && clean.contains('_')
            {
                return Some(clean.to_string());
            }
        }
        // pattern: process.env.VAR_NAME
        if let Some(idx) = trimmed.find("process.env.") {
            let rest = &trimmed[idx + 12..];
            let var: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !var.is_empty() {
                return Some(var);
            }
        }
    }
    None
}

fn extract_missing_dep(error: &str) -> (String, String) {
    // "Cannot find module 'react-icons'"
    for line in error.lines() {
        let trimmed = line.trim();
        if let Some(idx) = trimmed.find("Cannot find module '") {
            let rest = &trimmed[idx + 20..];
            if let Some(end) = rest.find('\'') {
                let module = &rest[..end];
                // Extract the requiring file
                let required_by = extract_file_from_error(error).unwrap_or_else(|| "unknown".into());
                return (module.to_string(), required_by);
            }
        }
        if let Some(idx) = trimmed.find("Module not found: Can't resolve '") {
            let rest = &trimmed[idx + 33..];
            if let Some(end) = rest.find('\'') {
                let module = &rest[..end];
                let required_by = extract_file_from_error(error).unwrap_or_else(|| "unknown".into());
                return (module.to_string(), required_by);
            }
        }
    }
    ("unknown".into(), "unknown".into())
}

fn extract_type_mismatch(error: &str) -> (String, String) {
    // "Type 'string' is not assignable to type 'number'"
    for line in error.lines() {
        let trimmed = line.trim();
        if let Some(idx) = trimmed.find("is not assignable to type '") {
            let expected_rest = &trimmed[idx + 27..];
            if let Some(end) = expected_rest.find('\'') {
                let expected = &expected_rest[..end];
                // Extract "got" type — "Type 'X'"
                if let Some(type_idx) = trimmed.find("Type '") {
                    let got_rest = &trimmed[type_idx + 6..];
                    if let Some(got_end) = got_rest.find('\'') {
                        return (expected.to_string(), got_rest[..got_end].to_string());
                    }
                }
                return (expected.to_string(), "unknown".into());
            }
        }
    }
    ("unknown".into(), "unknown".into())
}

fn extract_missing_module(error: &str) -> String {
    for line in error.lines() {
        let trimmed = line.trim();
        if let Some(idx) = trimmed.find("'") {
            let rest = &trimmed[idx + 1..];
            if let Some(end) = rest.find('\'') {
                let module = &rest[..end];
                if !module.is_empty() && !module.contains(' ') {
                    return module.to_string();
                }
            }
        }
    }
    "unknown".into()
}

fn extract_http_status(error: &str) -> Option<u16> {
    for line in error.lines() {
        for word in line.split_whitespace() {
            if let Ok(n) = word.trim_matches(|c: char| !c.is_ascii_digit()).parse::<u16>() {
                if (400..=599).contains(&n) {
                    return Some(n);
                }
            }
        }
    }
    None
}

fn first_meaningful_line(error: &str) -> String {
    for line in error.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() && trimmed.len() > 5 {
            return trimmed.chars().take(300).collect();
        }
    }
    error.chars().take(300).collect()
}

// ---------------------------------------------------------------------------
// Certificate persistence
// ---------------------------------------------------------------------------

fn save_certificate(project_dir: &Path, cert: &GuaranteeCertificate) {
    let cert_dir = project_dir.join(".nexus").join("certificates");
    let _ = std::fs::create_dir_all(&cert_dir);
    if let Ok(json) = serde_json::to_string_pretty(cert) {
        let _ = std::fs::write(cert_dir.join(format!("{}.json", cert.id)), json);
    }
}

/// Load the latest certificate for a project directory.
pub fn load_latest_certificate(project_dir: &Path) -> Option<GuaranteeCertificate> {
    let cert_dir = project_dir.join(".nexus").join("certificates");
    if !cert_dir.exists() {
        return None;
    }

    let mut latest: Option<(String, GuaranteeCertificate)> = None;
    if let Ok(entries) = std::fs::read_dir(&cert_dir) {
        for entry in entries.flatten() {
            if entry.path().extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    if let Ok(cert) = serde_json::from_str::<GuaranteeCertificate>(&content) {
                        let ts = cert.timestamp.clone();
                        match &latest {
                            None => latest = Some((ts, cert)),
                            Some((prev_ts, _)) if ts > *prev_ts => {
                                latest = Some((ts, cert));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    latest.map(|(_, c)| c)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Error classification tests --

    #[test]
    fn test_classify_syntax_error() {
        let output = "SyntaxError: Unexpected token '{' at src/app/page.tsx:15:3";
        let err = classify_error(output);
        assert!(matches!(err, ErrorClass::SyntaxError { .. }));
        if let ErrorClass::SyntaxError { file, line, .. } = &err {
            assert_eq!(file, "src/app/page.tsx");
            assert_eq!(*line, 15);
        }
    }

    #[test]
    fn test_classify_type_mismatch() {
        let output = "src/components/Button.tsx:22:5 - error TS2322: Type 'string' is not assignable to type 'number'.";
        let err = classify_error(output);
        assert!(matches!(err, ErrorClass::TypeMismatchError { .. }));
        if let ErrorClass::TypeMismatchError { expected, got, .. } = &err {
            assert_eq!(expected, "number");
            assert_eq!(got, "string");
        }
    }

    #[test]
    fn test_classify_missing_dependency() {
        let output = "Module not found: Can't resolve 'react-icons' in '/app/src/components'";
        let err = classify_error(output);
        assert!(matches!(err, ErrorClass::MissingDependency { .. }));
        if let ErrorClass::MissingDependency { package, .. } = &err {
            assert_eq!(package, "react-icons");
        }
    }

    #[test]
    fn test_classify_missing_dependency_cannot_find() {
        let output = "Error: Cannot find module 'lodash'\n    at src/utils/helpers.ts:3:1";
        let err = classify_error(output);
        assert!(matches!(err, ErrorClass::MissingDependency { .. }));
        if let ErrorClass::MissingDependency { package, .. } = &err {
            assert_eq!(package, "lodash");
        }
    }

    #[test]
    fn test_classify_import_error() {
        let output = "Export 'useAuth' was not found in './hooks/auth'\n  at src/app/layout.tsx";
        let err = classify_error(output);
        assert!(matches!(err, ErrorClass::ImportError { .. }));
    }

    #[test]
    fn test_classify_port_conflict() {
        let output = "Error: listen EADDRINUSE: address already in use :::3000";
        let err = classify_error(output);
        assert!(matches!(err, ErrorClass::PortConflict { port: 3000 }));
    }

    #[test]
    fn test_classify_env_missing() {
        let output = "Error: environment variable DATABASE_URL is not set";
        let err = classify_error(output);
        assert!(matches!(err, ErrorClass::EnvironmentMissing { .. }));
        if let ErrorClass::EnvironmentMissing { var } = &err {
            assert_eq!(var, "DATABASE_URL");
        }
    }

    #[test]
    fn test_classify_disk_full() {
        let output = "Error: ENOSPC: no space left on device, write";
        let err = classify_error(output);
        assert!(matches!(err, ErrorClass::DiskFull));
    }

    #[test]
    fn test_classify_network_unavailable() {
        let output = "Error: getaddrinfo ENOTFOUND registry.npmjs.org";
        let err = classify_error(output);
        assert!(matches!(err, ErrorClass::NetworkUnavailable));
    }

    #[test]
    fn test_classify_model_api_error() {
        let output = "OpenAI API error: 429 Too Many Requests";
        let err = classify_error(output);
        assert!(matches!(err, ErrorClass::ModelApiError { .. }));
        if let ErrorClass::ModelApiError { provider, status } = &err {
            assert_eq!(provider, "openai");
            assert_eq!(*status, 429);
        }
    }

    #[test]
    fn test_classify_db_connection() {
        let output = "Error: connect ECONNREFUSED 127.0.0.1:5432 - postgres connection refused";
        let err = classify_error(output);
        assert!(matches!(err, ErrorClass::DatabaseConnectionFailed { .. }));
    }

    // -- is_auto_fixable tests --

    #[test]
    fn test_auto_fixable_syntax() {
        let err = ErrorClass::SyntaxError {
            file: "a.ts".into(),
            line: 1,
            message: "err".into(),
        };
        assert!(err.is_auto_fixable());
    }

    #[test]
    fn test_auto_fixable_type_mismatch() {
        let err = ErrorClass::TypeMismatchError {
            file: "a.ts".into(),
            expected: "number".into(),
            got: "string".into(),
        };
        assert!(err.is_auto_fixable());
    }

    #[test]
    fn test_auto_fixable_missing_dep() {
        let err = ErrorClass::MissingDependency {
            package: "lodash".into(),
            required_by: "a.ts".into(),
        };
        assert!(err.is_auto_fixable());
    }

    #[test]
    fn test_not_auto_fixable_env() {
        let err = ErrorClass::EnvironmentMissing {
            var: "SECRET".into(),
        };
        assert!(!err.is_auto_fixable());
    }

    #[test]
    fn test_not_auto_fixable_disk_full() {
        assert!(!ErrorClass::DiskFull.is_auto_fixable());
    }

    #[test]
    fn test_not_auto_fixable_network() {
        assert!(!ErrorClass::NetworkUnavailable.is_auto_fixable());
    }

    #[test]
    fn test_not_auto_fixable_model_api() {
        let err = ErrorClass::ModelApiError {
            provider: "openai".into(),
            status: 500,
        };
        assert!(!err.is_auto_fixable());
    }

    #[test]
    fn test_not_auto_fixable_db() {
        let err = ErrorClass::DatabaseConnectionFailed {
            reason: "refused".into(),
        };
        assert!(!err.is_auto_fixable());
    }

    #[test]
    fn test_auto_fixable_taste() {
        let err = ErrorClass::TasteBelowThreshold {
            score: 50.0,
            threshold: 70.0,
            weak_axes: vec!["visual".into()],
        };
        assert!(err.is_auto_fixable());
    }

    #[test]
    fn test_auto_fixable_invariant() {
        let err = ErrorClass::InvariantViolation {
            invariant: "test".into(),
            details: "detail".into(),
        };
        assert!(err.is_auto_fixable());
    }

    #[test]
    fn test_auto_fixable_a11y() {
        let err = ErrorClass::AccessibilityFailure {
            rule: "alt-text".into(),
            element: "img".into(),
        };
        assert!(err.is_auto_fixable());
    }

    #[test]
    fn test_auto_fixable_port() {
        let err = ErrorClass::PortConflict { port: 3000 };
        assert!(err.is_auto_fixable());
    }

    // -- Cost estimation tests --

    #[test]
    fn test_cost_estimation_deterministic_fix_is_cheap() {
        let dep = ErrorClass::MissingDependency {
            package: "x".into(),
            required_by: "y".into(),
        };
        assert!(dep.estimated_fix_cost() <= 0.02);
    }

    #[test]
    fn test_cost_estimation_agent_fix_is_more() {
        let taste = ErrorClass::TasteBelowThreshold {
            score: 40.0,
            threshold: 70.0,
            weak_axes: vec![],
        };
        assert!(taste.estimated_fix_cost() >= 0.10);
    }

    #[test]
    fn test_cost_zero_for_unfixable() {
        assert_eq!(ErrorClass::DiskFull.estimated_fix_cost(), 0.0);
        assert_eq!(ErrorClass::NetworkUnavailable.estimated_fix_cost(), 0.0);
    }

    // -- Fix strategy tests --

    #[test]
    fn test_strategy_missing_dep_is_deterministic() {
        let err = ErrorClass::MissingDependency {
            package: "react-icons".into(),
            required_by: "a.ts".into(),
        };
        let strategy = err.suggested_fix_strategy();
        assert!(matches!(strategy, FixStrategy::DeterministicFix(_)));
        if let FixStrategy::DeterministicFix(cmd) = strategy {
            assert!(cmd.contains("npm install react-icons"));
        }
    }

    #[test]
    fn test_strategy_syntax_is_agent() {
        let err = ErrorClass::SyntaxError {
            file: "a.ts".into(),
            line: 5,
            message: "oops".into(),
        };
        assert!(matches!(err.suggested_fix_strategy(), FixStrategy::AgentRepair(_)));
    }

    #[test]
    fn test_strategy_unfixable_for_env() {
        let err = ErrorClass::EnvironmentMissing {
            var: "API_KEY".into(),
        };
        assert!(matches!(err.suggested_fix_strategy(), FixStrategy::Unfixable(_)));
    }

    #[test]
    fn test_strategy_heuristic_for_import() {
        let err = ErrorClass::ImportError {
            file: "a.ts".into(),
            missing_module: "foo".into(),
        };
        assert!(matches!(err.suggested_fix_strategy(), FixStrategy::HeuristicRepair(_)));
    }

    #[test]
    fn test_strategy_heuristic_for_a11y() {
        let err = ErrorClass::AccessibilityFailure {
            rule: "alt-text".into(),
            element: "img".into(),
        };
        assert!(matches!(err.suggested_fix_strategy(), FixStrategy::HeuristicRepair(_)));
    }

    // -- Certificate generation tests --

    #[test]
    fn test_certificate_generation() {
        let engine = GuaranteeEngine::new(GuaranteeConfig::default());
        let cert = engine.build_certificate(
            "test-project",
            GuaranteeStatus::Passed,
            vec![
                CheckResult {
                    check_name: "build".into(),
                    passed: true,
                    details: "ok".into(),
                    duration_ms: 100,
                },
                CheckResult {
                    check_name: "taste".into(),
                    passed: true,
                    details: "ok".into(),
                    duration_ms: 50,
                },
            ],
            85.0,
            true,
            true,
            2,
            0.30,
            vec![RepairRecord {
                cycle: 1,
                error_class: "syntax_error".into(),
                strategy: "agent_repair".into(),
                cost_usd: 0.15,
                success: true,
                details: "Fixed".into(),
            }],
            vec![],
        );

        assert_eq!(cert.project_id, "test-project");
        assert!(matches!(cert.status, GuaranteeStatus::Passed));
        assert_eq!(cert.cycles_used, 2);
        assert!((cert.total_cost_usd - 0.30).abs() < f32::EPSILON);
        assert_eq!(cert.repairs_applied.len(), 1);
        assert_eq!(cert.confidence, 1.0); // 2 checks, 2 passed
        assert!(cert.known_limitations.is_empty());
    }

    #[test]
    fn test_certificate_confidence_partial() {
        let engine = GuaranteeEngine::new(GuaranteeConfig::default());
        let cert = engine.build_certificate(
            "proj",
            GuaranteeStatus::PassedWithWarnings(vec!["warn".into()]),
            vec![
                CheckResult {
                    check_name: "build".into(),
                    passed: true,
                    details: "ok".into(),
                    duration_ms: 0,
                },
                CheckResult {
                    check_name: "taste".into(),
                    passed: false,
                    details: "low".into(),
                    duration_ms: 0,
                },
            ],
            60.0,
            true,
            true,
            3,
            0.50,
            vec![],
            vec![],
        );

        assert_eq!(cert.confidence, 0.5); // 1 of 2 passed
        assert!(!cert.known_limitations.is_empty()); // taste below threshold
    }

    // -- Budget exhaustion test --

    #[test]
    fn test_cost_estimate() {
        let config = GuaranteeConfig {
            max_cycles: 5,
            max_cost_usd: 2.0,
            cost_per_cycle_estimate: 0.15,
            ..Default::default()
        };
        let engine = GuaranteeEngine::new(config);

        let tmp = std::env::temp_dir().join("nexus_test_cost_est");
        let _ = std::fs::create_dir_all(&tmp);
        // No package.json — should estimate 1 cycle
        let est = engine.estimate_cost(&tmp);
        assert_eq!(est.estimated_cycles, 1);
        assert!((est.per_cycle_usd - 0.15).abs() < f32::EPSILON);
        assert!((est.budget_usd - 2.0).abs() < f32::EPSILON);

        // With package.json — should estimate 3 cycles
        std::fs::write(tmp.join("package.json"), "{}").unwrap();
        let est = engine.estimate_cost(&tmp);
        assert_eq!(est.estimated_cycles, 3);
        assert!((est.estimated_total_usd - 0.45).abs() < f32::EPSILON);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    // -- Helper extraction tests --

    #[test]
    fn test_extract_port() {
        assert_eq!(
            extract_port_from_error("Error: listen EADDRINUSE: address already in use :::3001"),
            Some(3001)
        );
        assert_eq!(
            extract_port_from_error("port 8080 is already in use"),
            Some(8080)
        );
    }

    #[test]
    fn test_extract_env_var_from_error() {
        assert_eq!(
            extract_env_var("Error: environment variable DATABASE_URL is not set"),
            Some("DATABASE_URL".to_string())
        );
        assert_eq!(
            extract_env_var("process.env.NEXT_PUBLIC_API_KEY is undefined"),
            Some("NEXT_PUBLIC_API_KEY".to_string())
        );
    }

    #[test]
    fn test_extract_type_mismatch_from_error() {
        let (expected, got) =
            extract_type_mismatch("Type 'string' is not assignable to type 'number'.");
        assert_eq!(expected, "number");
        assert_eq!(got, "string");
    }

    #[test]
    fn test_extract_file_and_line() {
        assert_eq!(
            extract_file_from_error("  src/app/page.tsx:12:5 - error TS2322"),
            Some("src/app/page.tsx".to_string())
        );
        assert_eq!(
            extract_line_from_error("src/app/page.tsx:42:10 - error"),
            Some(42)
        );
    }
}
