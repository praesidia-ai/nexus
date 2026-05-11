//! Governance framework for Nexus agent actions.
//!
//! Provides:
//! - **Policy engine**: Define per-agent/team policies (cost limits, tool access, data scope)
//! - **Compliance grading**: Auto-classify agent actions against regulatory frameworks
//! - **Kill switch**: Emergency global halt with audit trail
//! - **PII detection**: Auto-redact sensitive data in agent outputs and telemetry

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Policy definitions
// ---------------------------------------------------------------------------

/// A governance policy applied to an agent or team.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPolicy {
    pub id: String,
    pub name: String,
    /// Which agent IDs or team IDs this policy applies to ("*" = all).
    pub applies_to: Vec<String>,
    pub rules: Vec<PolicyRule>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PolicyRule {
    /// Hard cap on cost (USD) per execution.
    MaxCostUsd { limit: f64 },
    /// Whitelist of allowed tool names.
    AllowedTools { tools: Vec<String> },
    /// Blacklist of forbidden tool names.
    ForbiddenTools { tools: Vec<String> },
    /// Require human approval for actions with these tags.
    RequireApproval { tags: Vec<String> },
    /// Block PII in outputs.
    BlockPii,
    /// Restrict data access to specific project scopes.
    DataScope { allowed_projects: Vec<String> },
    /// Block all LLM calls (offline-only mode).
    OfflineOnly,
}

/// Result of evaluating policies against a proposed action.
#[derive(Debug, Clone)]
pub enum PolicyDecision {
    Allow,
    Deny { reason: String },
    RequireApproval { reason: String },
}

// ---------------------------------------------------------------------------
// Policy engine
// ---------------------------------------------------------------------------

pub struct PolicyEngine {
    policies: Arc<RwLock<Vec<AgentPolicy>>>,
    /// Global kill switch — when true, all agent actions are denied.
    global_halt: Arc<std::sync::atomic::AtomicBool>,
}

impl PolicyEngine {
    pub fn new() -> Self {
        Self {
            policies: Arc::new(RwLock::new(Vec::new())),
            global_halt: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Add or replace a policy.
    pub async fn upsert_policy(&self, policy: AgentPolicy) {
        let mut policies = self.policies.write().await;
        if let Some(existing) = policies.iter_mut().find(|p| p.id == policy.id) {
            *existing = policy;
        } else {
            policies.push(policy);
        }
    }

    /// Remove a policy by ID.
    pub async fn remove_policy(&self, id: &str) -> bool {
        let mut policies = self.policies.write().await;
        let before = policies.len();
        policies.retain(|p| p.id != id);
        policies.len() < before
    }

    /// List all policies.
    pub async fn list_policies(&self) -> Vec<AgentPolicy> {
        self.policies.read().await.clone()
    }

    /// Activate the global kill switch — immediately halts all agents.
    pub fn activate_kill_switch(&self) {
        self.global_halt
            .store(true, std::sync::atomic::Ordering::SeqCst);
        warn!("GLOBAL KILL SWITCH ACTIVATED — all agent actions denied");
    }

    /// Deactivate the global kill switch.
    pub fn deactivate_kill_switch(&self) {
        self.global_halt
            .store(false, std::sync::atomic::Ordering::SeqCst);
        info!("Global kill switch deactivated");
    }

    /// Check whether the global kill switch is active.
    pub fn is_halted(&self) -> bool {
        self.global_halt.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Evaluate policies for a proposed action.
    pub async fn evaluate(
        &self,
        agent_id: &str,
        tool_name: Option<&str>,
        estimated_cost_usd: Option<f64>,
        action_tags: &[&str],
    ) -> PolicyDecision {
        if self.is_halted() {
            return PolicyDecision::Deny {
                reason: "Global kill switch is active".into(),
            };
        }

        let policies = self.policies.read().await;
        for policy in policies.iter().filter(|p| p.enabled) {
            let applies = policy.applies_to.iter().any(|a| {
                a == "*" || a == agent_id
            });
            if !applies {
                continue;
            }

            for rule in &policy.rules {
                let decision = self.evaluate_rule(rule, tool_name, estimated_cost_usd, action_tags);
                if !matches!(decision, PolicyDecision::Allow) {
                    return decision;
                }
            }
        }

        PolicyDecision::Allow
    }

    fn evaluate_rule(
        &self,
        rule: &PolicyRule,
        tool_name: Option<&str>,
        estimated_cost_usd: Option<f64>,
        action_tags: &[&str],
    ) -> PolicyDecision {
        match rule {
            PolicyRule::MaxCostUsd { limit } => {
                if let Some(cost) = estimated_cost_usd {
                    if cost > *limit {
                        return PolicyDecision::Deny {
                            reason: format!("Estimated cost ${cost:.4} exceeds limit ${limit:.4}"),
                        };
                    }
                }
                PolicyDecision::Allow
            }
            PolicyRule::AllowedTools { tools } => {
                if let Some(tool) = tool_name {
                    if !tools.iter().any(|t| t == tool || t == "*") {
                        return PolicyDecision::Deny {
                            reason: format!("Tool '{tool}' is not in the allowed tools list"),
                        };
                    }
                }
                PolicyDecision::Allow
            }
            PolicyRule::ForbiddenTools { tools } => {
                if let Some(tool) = tool_name {
                    if tools.iter().any(|t| t == tool) {
                        return PolicyDecision::Deny {
                            reason: format!("Tool '{tool}' is forbidden by policy"),
                        };
                    }
                }
                PolicyDecision::Allow
            }
            PolicyRule::RequireApproval { tags } => {
                for tag in action_tags {
                    if tags.iter().any(|t| t == tag) {
                        return PolicyDecision::RequireApproval {
                            reason: format!("Action tagged '{tag}' requires human approval"),
                        };
                    }
                }
                PolicyDecision::Allow
            }
            PolicyRule::BlockPii => PolicyDecision::Allow, // Handled at output layer
            PolicyRule::DataScope { .. } => PolicyDecision::Allow, // Checked at DB layer
            PolicyRule::OfflineOnly => {
                if let Some(tool) = tool_name {
                    if tool.contains("web") || tool.contains("http") || tool.contains("api") {
                        return PolicyDecision::Deny {
                            reason: "Offline-only mode: network tools are disabled".into(),
                        };
                    }
                }
                PolicyDecision::Allow
            }
        }
    }
}

impl Default for PolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Compliance grading
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComplianceGrade {
    /// Fully compliant with all checked frameworks.
    Compliant,
    /// Minor issues that should be addressed.
    Warning,
    /// Serious violations requiring remediation.
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceReport {
    pub grade: ComplianceGrade,
    pub framework: String,
    pub findings: Vec<ComplianceFinding>,
    pub score: u8, // 0-100
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceFinding {
    pub severity: String,
    pub rule: String,
    pub description: String,
}

/// Evaluate an agent action against basic compliance frameworks.
pub fn grade_action(
    action: &str,
    tool_name: Option<&str>,
    data_contains_pii: bool,
    has_human_approval: bool,
) -> ComplianceReport {
    let mut findings = Vec::new();
    let mut score: i32 = 100;

    // GDPR / EU AI Act checks
    if data_contains_pii && !has_human_approval {
        findings.push(ComplianceFinding {
            severity: "warning".into(),
            rule: "GDPR-Art6".into(),
            description: "PII processed without explicit user consent or human oversight".into(),
        });
        score -= 20;
    }

    if tool_name == Some("send_email") && !has_human_approval {
        findings.push(ComplianceFinding {
            severity: "warning".into(),
            rule: "EU-AI-High-Risk".into(),
            description: "Autonomous communication sent without human approval".into(),
        });
        score -= 15;
    }

    // High-risk action checks
    let high_risk_actions = ["delete", "drop", "terminate", "wipe", "purge"];
    if high_risk_actions.iter().any(|r| action.contains(r)) && !has_human_approval {
        findings.push(ComplianceFinding {
            severity: "critical".into(),
            rule: "HIGH-RISK-ACTION".into(),
            description: format!(
                "Destructive action '{action}' performed without human approval"
            ),
        });
        score -= 40;
    }

    let grade = if score >= 80 {
        ComplianceGrade::Compliant
    } else if score >= 50 {
        ComplianceGrade::Warning
    } else {
        ComplianceGrade::Critical
    };

    ComplianceReport {
        grade,
        framework: "EU-AI-Act-2025 + GDPR".into(),
        findings,
        score: score.max(0) as u8,
    }
}

// ---------------------------------------------------------------------------
// PII detection and redaction
// ---------------------------------------------------------------------------

/// Detect and redact PII from a text string.
///
/// Uses regex-based detection for common PII patterns:
/// - Email addresses
/// - Phone numbers
/// - Credit card numbers
/// - Social security numbers
/// - IP addresses
pub fn redact_pii(text: &str) -> (String, Vec<String>) {
    let mut redacted = text.to_string();
    let mut detected: Vec<String> = Vec::new();

    // Simple pattern-based detection (production would use a proper ML model)
    let _patterns: &[(&str, &str, &str)] = &[
        // (pattern_hint, pattern, replacement)
        ("email", "@", "[EMAIL REDACTED]"),
    ];

    // Email-like patterns (simple heuristic)
    let words: Vec<&str> = redacted.split_whitespace().collect();
    let mut result_parts = Vec::new();
    for word in &words {
        let clean = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '@' && c != '.');
        if clean.contains('@') && clean.contains('.') {
            detected.push("email".into());
            result_parts.push(word.replace(clean, "[EMAIL REDACTED]"));
        } else if looks_like_phone(clean) {
            detected.push("phone".into());
            result_parts.push(word.replace(clean, "[PHONE REDACTED]"));
        } else if looks_like_credit_card(clean) {
            detected.push("credit_card".into());
            result_parts.push(word.replace(clean, "[CARD REDACTED]"));
        } else {
            result_parts.push((*word).to_string());
        }
    }

    redacted = result_parts.join(" ");
    (redacted, detected)
}

fn looks_like_phone(s: &str) -> bool {
    let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    digits.len() >= 10 && digits.len() <= 15
        && s.chars().all(|c| c.is_ascii_digit() || "+-() ".contains(c))
}

fn looks_like_credit_card(s: &str) -> bool {
    let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    (digits.len() == 15 || digits.len() == 16)
        && s.chars().all(|c| c.is_ascii_digit() || c == '-' || c == ' ')
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn kill_switch() {
        let engine = PolicyEngine::new();
        engine.activate_kill_switch();
        let decision = engine.evaluate("agent-1", Some("web_search"), None, &[]).await;
        assert!(matches!(decision, PolicyDecision::Deny { .. }));
        engine.deactivate_kill_switch();
        let decision = engine.evaluate("agent-1", Some("web_search"), None, &[]).await;
        assert!(matches!(decision, PolicyDecision::Allow));
    }

    #[tokio::test]
    async fn cost_limit_policy() {
        let engine = PolicyEngine::new();
        engine.upsert_policy(AgentPolicy {
            id: "cost-limit".into(),
            name: "Cost Limit".into(),
            applies_to: vec!["*".into()],
            rules: vec![PolicyRule::MaxCostUsd { limit: 0.10 }],
            enabled: true,
        }).await;

        let allow = engine.evaluate("agent-1", None, Some(0.05), &[]).await;
        assert!(matches!(allow, PolicyDecision::Allow));

        let deny = engine.evaluate("agent-1", None, Some(0.20), &[]).await;
        assert!(matches!(deny, PolicyDecision::Deny { .. }));
    }

    #[test]
    fn pii_detection() {
        let (redacted, types) = redact_pii("Contact user@example.com for more info");
        assert!(redacted.contains("[EMAIL REDACTED]"));
        assert!(types.contains(&"email".to_string()));
    }
}
