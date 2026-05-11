//! Invariant Enforcement System — runtime enforcement engine with categories,
//! severity-based blocking, configurable rule sets, and pipeline gate integration.
//!
//! Extends the basic `invariants` module with:
//! - Rule categories (Security, Quality, Accessibility, Performance, Correctness)
//! - Configurable enforcement levels (Block, Warn, Info)
//! - Pre-commit / pre-deploy gates
//! - Enforcement history tracking
//! - Custom rule definitions per project

use std::collections::HashMap;
use std::path::Path;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::invariants::{self, Violation};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Category of invariant rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleCategory {
    Security,
    Quality,
    Accessibility,
    Performance,
    Correctness,
}

/// Enforcement action when a rule is violated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnforcementAction {
    /// Hard block — operation cannot proceed.
    Block,
    /// Warn — operation proceeds but violation is logged.
    Warn,
    /// Info — silent logging only.
    Info,
}

/// Gate context — when enforcement is checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateContext {
    /// Before writing files to disk.
    PreCommit,
    /// Before deploying / starting the app.
    PreDeploy,
    /// Continuous background check.
    Continuous,
}

/// A single enforcement rule with category and action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnforcementRule {
    pub id: String,
    pub category: RuleCategory,
    pub action: EnforcementAction,
    pub description: String,
    /// Which gates this rule applies to.
    pub gates: Vec<GateContext>,
    /// Whether auto-fix is available.
    pub auto_fixable: bool,
}

/// Project-level enforcement configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnforcementConfig {
    /// Whether enforcement is enabled at all.
    pub enabled: bool,
    /// Minimum score required to pass pre-deploy gate (0-100).
    pub min_deploy_score: u32,
    /// Minimum score required to pass pre-commit gate (0-100).
    pub min_commit_score: u32,
    /// Per-rule overrides (rule_id -> action).
    pub overrides: HashMap<String, EnforcementAction>,
    /// Custom rules defined for this project.
    pub custom_rules: Vec<CustomRule>,
}

impl Default for EnforcementConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_deploy_score: 70,
            min_commit_score: 50,
            overrides: HashMap::new(),
            custom_rules: Vec::new(),
        }
    }
}

/// User-defined custom rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomRule {
    pub id: String,
    pub pattern: String,       // regex pattern to match
    pub file_glob: String,     // file pattern (e.g., "*.ts")
    pub message: String,
    pub category: RuleCategory,
    pub action: EnforcementAction,
}

/// Result of enforcement gate check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnforcementResult {
    pub passed: bool,
    pub gate: GateContext,
    pub score: u32,
    pub required_score: u32,
    pub blocking_violations: Vec<EnrichedViolation>,
    pub warnings: Vec<EnrichedViolation>,
    pub info: Vec<EnrichedViolation>,
    pub auto_fixed: usize,
    pub summary: EnforcementSummary,
    pub timestamp: String,
}

/// Enriched violation with category and action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichedViolation {
    pub rule_id: String,
    pub category: RuleCategory,
    pub action: EnforcementAction,
    pub severity: String,
    pub file: String,
    pub line: Option<usize>,
    pub message: String,
    pub fix: Option<String>,
}

/// Aggregated summary by category.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnforcementSummary {
    pub by_category: HashMap<String, CategorySummary>,
    pub total_violations: usize,
    pub total_blocking: usize,
    pub total_auto_fixed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategorySummary {
    pub count: usize,
    pub blocking: usize,
}

// ---------------------------------------------------------------------------
// Default Rule Registry
// ---------------------------------------------------------------------------

fn default_rules() -> Vec<EnforcementRule> {
    vec![
        // Security rules (always block)
        EnforcementRule {
            id: "no-hardcoded-secrets".into(),
            category: RuleCategory::Security,
            action: EnforcementAction::Block,
            description: "No hardcoded API keys, tokens, or passwords".into(),
            gates: vec![GateContext::PreCommit, GateContext::PreDeploy],
            auto_fixable: false,
        },
        EnforcementRule {
            id: "no-empty-catch".into(),
            category: RuleCategory::Correctness,
            action: EnforcementAction::Block,
            description: "Catch blocks must not be empty".into(),
            gates: vec![GateContext::PreCommit, GateContext::PreDeploy],
            auto_fixable: true,
        },
        EnforcementRule {
            id: "fetch-has-try-catch".into(),
            category: RuleCategory::Correctness,
            action: EnforcementAction::Block,
            description: "Every fetch/API call must have error handling".into(),
            gates: vec![GateContext::PreDeploy],
            auto_fixable: false,
        },
        EnforcementRule {
            id: "api-has-error-handling".into(),
            category: RuleCategory::Correctness,
            action: EnforcementAction::Block,
            description: "Every API route must handle errors".into(),
            gates: vec![GateContext::PreDeploy],
            auto_fixable: false,
        },
        // Quality rules (warn)
        EnforcementRule {
            id: "no-any-type".into(),
            category: RuleCategory::Quality,
            action: EnforcementAction::Warn,
            description: "No TypeScript `any` type".into(),
            gates: vec![GateContext::PreCommit, GateContext::PreDeploy, GateContext::Continuous],
            auto_fixable: false,
        },
        EnforcementRule {
            id: "no-console-log".into(),
            category: RuleCategory::Quality,
            action: EnforcementAction::Warn,
            description: "No console.log in production code".into(),
            gates: vec![GateContext::PreDeploy],
            auto_fixable: true,
        },
        EnforcementRule {
            id: "form-has-validation".into(),
            category: RuleCategory::Quality,
            action: EnforcementAction::Warn,
            description: "Forms should validate input".into(),
            gates: vec![GateContext::PreDeploy],
            auto_fixable: false,
        },
        EnforcementRule {
            id: "async-has-loading-state".into(),
            category: RuleCategory::Quality,
            action: EnforcementAction::Warn,
            description: "Async operations should show loading state".into(),
            gates: vec![GateContext::Continuous],
            auto_fixable: false,
        },
        // Accessibility rules (warn)
        EnforcementRule {
            id: "img-has-alt".into(),
            category: RuleCategory::Accessibility,
            action: EnforcementAction::Warn,
            description: "Images must have alt text".into(),
            gates: vec![GateContext::PreDeploy, GateContext::Continuous],
            auto_fixable: false,
        },
        // Info rules
        EnforcementRule {
            id: "no-todo-comments".into(),
            category: RuleCategory::Quality,
            action: EnforcementAction::Info,
            description: "No TODO/FIXME/HACK comments".into(),
            gates: vec![GateContext::Continuous],
            auto_fixable: false,
        },
        EnforcementRule {
            id: "page-has-metadata".into(),
            category: RuleCategory::Quality,
            action: EnforcementAction::Info,
            description: "Pages should have title/metadata".into(),
            gates: vec![GateContext::PreDeploy],
            auto_fixable: false,
        },
        EnforcementRule {
            id: "env-vars-documented".into(),
            category: RuleCategory::Quality,
            action: EnforcementAction::Info,
            description: "Environment variables should be documented".into(),
            gates: vec![GateContext::PreDeploy],
            auto_fixable: true,
        },
    ]
}

// ---------------------------------------------------------------------------
// Enforcer
// ---------------------------------------------------------------------------

pub struct InvariantEnforcer {
    rules: Vec<EnforcementRule>,
    config: EnforcementConfig,
}

impl InvariantEnforcer {
    pub fn new(config: EnforcementConfig) -> Self {
        Self {
            rules: default_rules(),
            config,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(EnforcementConfig::default())
    }

    /// Load config from project directory (.nexus/enforcement.json).
    pub fn load_for_project(project_dir: &Path) -> Self {
        let config_path = project_dir.join(".nexus").join("enforcement.json");
        let config = if config_path.exists() {
            std::fs::read_to_string(&config_path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            EnforcementConfig::default()
        };
        Self::new(config)
    }

    /// Save config to project directory.
    pub fn save_config(&self, project_dir: &Path) -> Result<(), String> {
        let config_dir = project_dir.join(".nexus");
        std::fs::create_dir_all(&config_dir)
            .map_err(|e| format!("Cannot create .nexus dir: {}", e))?;
        let json = serde_json::to_string_pretty(&self.config)
            .map_err(|e| format!("Cannot serialize config: {}", e))?;
        std::fs::write(config_dir.join("enforcement.json"), json)
            .map_err(|e| format!("Cannot write config: {}", e))?;
        Ok(())
    }

    /// Run the enforcement gate for a given context.
    pub fn enforce(&self, project_dir: &Path, gate: GateContext) -> EnforcementResult {
        if !self.config.enabled {
            return EnforcementResult {
                passed: true,
                gate,
                score: 100,
                required_score: 0,
                blocking_violations: Vec::new(),
                warnings: Vec::new(),
                info: Vec::new(),
                auto_fixed: 0,
                summary: EnforcementSummary {
                    by_category: HashMap::new(),
                    total_violations: 0,
                    total_blocking: 0,
                    total_auto_fixed: 0,
                },
                timestamp: Utc::now().to_rfc3339(),
            };
        }

        // Run base invariant checks
        let inv_result = invariants::check_invariants(project_dir);

        // Auto-fix what we can
        let auto_fixed = if inv_result.auto_fixable > 0 {
            invariants::auto_fix(project_dir, &inv_result)
        } else {
            0
        };

        // Re-check after auto-fix
        let inv_result = if auto_fixed > 0 {
            invariants::check_invariants(project_dir)
        } else {
            inv_result
        };

        // Check custom rules
        let custom_violations = self.check_custom_rules(project_dir);

        // Enrich violations with enforcement metadata
        let mut blocking = Vec::new();
        let mut warnings = Vec::new();
        let mut info_list = Vec::new();

        for violation in &inv_result.violations {
            let enriched = self.enrich_violation(violation, gate);
            match enriched.action {
                EnforcementAction::Block => blocking.push(enriched),
                EnforcementAction::Warn => warnings.push(enriched),
                EnforcementAction::Info => info_list.push(enriched),
            }
        }

        // Add custom rule violations
        for cv in custom_violations {
            match cv.action {
                EnforcementAction::Block => blocking.push(cv),
                EnforcementAction::Warn => warnings.push(cv),
                EnforcementAction::Info => info_list.push(cv),
            }
        }

        // Build category summary
        let mut by_category: HashMap<String, CategorySummary> = HashMap::new();
        for v in blocking.iter().chain(warnings.iter()).chain(info_list.iter()) {
            let cat = format!("{:?}", v.category).to_lowercase();
            let entry = by_category.entry(cat).or_insert(CategorySummary {
                count: 0,
                blocking: 0,
            });
            entry.count += 1;
            if v.action == EnforcementAction::Block {
                entry.blocking += 1;
            }
        }

        let required_score = match gate {
            GateContext::PreCommit => self.config.min_commit_score,
            GateContext::PreDeploy => self.config.min_deploy_score,
            GateContext::Continuous => 0,
        };

        let total_violations = blocking.len() + warnings.len() + info_list.len();
        let passed = blocking.is_empty() && inv_result.score >= required_score;

        if passed {
            info!(
                gate = ?gate,
                score = inv_result.score,
                violations = total_violations,
                "Enforcement gate PASSED"
            );
        } else {
            warn!(
                gate = ?gate,
                score = inv_result.score,
                blocking = blocking.len(),
                required = required_score,
                "Enforcement gate FAILED"
            );
        }

        let total_blocking = blocking.len();

        EnforcementResult {
            passed,
            gate,
            score: inv_result.score,
            required_score,
            blocking_violations: blocking,
            warnings,
            info: info_list,
            auto_fixed,
            summary: EnforcementSummary {
                by_category,
                total_violations,
                total_blocking,
                total_auto_fixed: auto_fixed,
            },
            timestamp: Utc::now().to_rfc3339(),
        }
    }

    /// Enrich a base violation with enforcement metadata.
    fn enrich_violation(&self, violation: &Violation, gate: GateContext) -> EnrichedViolation {
        // Find matching rule
        let rule = self.rules.iter().find(|r| r.id == violation.rule);

        let (category, action) = if let Some(rule) = rule {
            let action = self
                .config
                .overrides
                .get(&rule.id)
                .copied()
                .unwrap_or_else(|| {
                    // Only enforce if this gate is in the rule's gate list
                    if rule.gates.contains(&gate) {
                        rule.action
                    } else {
                        EnforcementAction::Info
                    }
                });
            (rule.category, action)
        } else {
            // Unknown rule — default to quality/info
            (RuleCategory::Quality, EnforcementAction::Info)
        };

        EnrichedViolation {
            rule_id: violation.rule.clone(),
            category,
            action,
            severity: violation.severity.clone(),
            file: violation.file.clone(),
            line: violation.line,
            message: violation.message.clone(),
            fix: violation.fix.clone(),
        }
    }

    /// Check custom rules defined in the project config.
    fn check_custom_rules(&self, project_dir: &Path) -> Vec<EnrichedViolation> {
        let mut violations = Vec::new();

        for rule in &self.config.custom_rules {
            let files = collect_matching_files(project_dir, &rule.file_glob);
            for file_path in files {
                let rel_path = file_path
                    .strip_prefix(project_dir)
                    .unwrap_or(&file_path)
                    .display()
                    .to_string();

                let Ok(content) = std::fs::read_to_string(&file_path) else {
                    continue;
                };

                for (line_num, line) in content.lines().enumerate() {
                    if line.contains(&rule.pattern) {
                        violations.push(EnrichedViolation {
                            rule_id: rule.id.clone(),
                            category: rule.category,
                            action: rule.action,
                            severity: match rule.action {
                                EnforcementAction::Block => "error".into(),
                                EnforcementAction::Warn => "warning".into(),
                                EnforcementAction::Info => "info".into(),
                            },
                            file: rel_path.clone(),
                            line: Some(line_num + 1),
                            message: rule.message.clone(),
                            fix: None,
                        });
                    }
                }
            }
        }

        violations
    }

    /// List all rules with their current enforcement action.
    pub fn list_rules(&self, gate: Option<GateContext>) -> Vec<EnforcementRule> {
        self.rules
            .iter()
            .filter(|r| {
                gate.is_none_or(|g| r.gates.contains(&g))
            })
            .cloned()
            .collect()
    }
}

/// Collect files matching a simple glob pattern (e.g., "*.ts", "**/*.tsx").
fn collect_matching_files(dir: &Path, glob: &str) -> Vec<std::path::PathBuf> {
    let ext = glob.trim_start_matches("**/*.").trim_start_matches("*.");
    crate::file_utils::collect_files_by_ext(dir, &[ext])
}

// ---------------------------------------------------------------------------
// Persistence — store enforcement results in DB
// ---------------------------------------------------------------------------

/// Store an enforcement result in the project's observability tables.
pub async fn store_enforcement_result(
    db: &tokio::sync::Mutex<rusqlite::Connection>,
    project_id: &str,
    result: &EnforcementResult,
) {
    let conn = db.lock().await;
    let id = uuid::Uuid::new_v4().to_string();
    let gate = serde_json::to_string(&result.gate).unwrap_or_default();
    let detail = serde_json::to_string(result).unwrap_or_default();

    let _ = conn.execute(
        "INSERT INTO audit_entries (id, project_id, action, actor, detail, created_at)
         VALUES (?1, ?2, ?3, 'enforcer', ?4, ?5)",
        rusqlite::params![
            id,
            project_id,
            format!("enforcement_{}", gate.trim_matches('"')),
            detail,
            result.timestamp,
        ],
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn default_config_blocks_secrets() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("config.ts"),
            "const key = 'sk-abc123def456';\n",
        )
        .unwrap();

        let enforcer = InvariantEnforcer::with_defaults();
        let result = enforcer.enforce(dir.path(), GateContext::PreCommit);

        assert!(!result.passed);
        assert!(!result.blocking_violations.is_empty());
        assert!(result
            .blocking_violations
            .iter()
            .any(|v| v.rule_id == "no-hardcoded-secrets"));
    }

    #[test]
    fn passes_clean_code() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("index.ts"),
            "export const greeting: string = 'hello';\n",
        )
        .unwrap();

        let enforcer = InvariantEnforcer::with_defaults();
        let result = enforcer.enforce(dir.path(), GateContext::PreCommit);

        assert!(result.passed);
        assert!(result.score >= 90);
    }

    #[test]
    fn override_downgrades_action() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("config.ts"),
            "const key = 'sk-abc123def456';\n",
        )
        .unwrap();

        let mut config = EnforcementConfig::default();
        config
            .overrides
            .insert("no-hardcoded-secrets".into(), EnforcementAction::Warn);

        let enforcer = InvariantEnforcer::new(config);
        let result = enforcer.enforce(dir.path(), GateContext::PreCommit);

        // Should pass because the secret rule was downgraded to warn
        assert!(result.passed);
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn disabled_enforcement_always_passes() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("config.ts"),
            "const key = 'sk-abc123def456';\n",
        )
        .unwrap();

        let config = EnforcementConfig {
            enabled: false,
            ..Default::default()
        };
        let enforcer = InvariantEnforcer::new(config);
        let result = enforcer.enforce(dir.path(), GateContext::PreDeploy);

        assert!(result.passed);
        assert_eq!(result.score, 100);
    }

    #[test]
    fn custom_rule_detects_pattern() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("app.ts"),
            "const x = eval('bad code');\n",
        )
        .unwrap();

        let config = EnforcementConfig {
            custom_rules: vec![CustomRule {
                id: "no-eval".into(),
                pattern: "eval(".into(),
                file_glob: "*.ts".into(),
                message: "eval() is forbidden".into(),
                category: RuleCategory::Security,
                action: EnforcementAction::Block,
            }],
            ..Default::default()
        };

        let enforcer = InvariantEnforcer::new(config);
        let result = enforcer.enforce(dir.path(), GateContext::PreCommit);

        assert!(!result.passed);
        assert!(result
            .blocking_violations
            .iter()
            .any(|v| v.rule_id == "no-eval"));
    }

    #[test]
    fn score_threshold_enforced() {
        let dir = tempdir().unwrap();
        // Create many warning-level violations to drop score below threshold
        let mut code = String::new();
        for i in 0..20 {
            code.push_str(&format!("const x{}: any = {};\n", i, i));
        }
        fs::write(dir.path().join("bad.ts"), &code).unwrap();

        let config = EnforcementConfig {
            min_commit_score: 80,
            ..Default::default()
        };
        let enforcer = InvariantEnforcer::new(config);
        let result = enforcer.enforce(dir.path(), GateContext::PreCommit);

        // Many `any` type warnings should drop the score
        assert!(result.score < 80);
        assert!(!result.passed);
    }
}
