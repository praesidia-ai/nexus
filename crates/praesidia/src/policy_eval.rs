//! Evaluate parsed Praesidia policy against ingress text, egress chunks, and tool names.

use regex::Regex;

/// Result returned by policy evaluation hooks.
#[derive(Debug, Clone)]
pub struct HookResult {
    pub allowed: bool,
    pub reason: Option<String>,
    pub modified_payload: Option<String>,
}

use crate::policy::{
    EgressAction, EnforcementLevel, IngressAction, PraesidiaPolicy, ToolAction,
};

fn pattern_matches(pattern: &str, haystack: &str) -> bool {
    if let Ok(re) = Regex::new(pattern) {
        return re.is_match(haystack);
    }
    haystack.contains(pattern)
}

/// Strict + non-empty ingress rules: reject when no rule matches (default deny).
/// Permissive or empty ingress rules: allow when no rule matches.
pub fn eval_ingress(policy: &PraesidiaPolicy, text: &str) -> HookResult {
    if policy.rules.ingress.is_empty() {
        return HookResult {
            allowed: true,
            reason: None,
            modified_payload: None,
        };
    }

    for rule in &policy.rules.ingress {
        if pattern_matches(&rule.rule, text) {
            return match rule.action {
                IngressAction::Reject => HookResult {
                    allowed: false,
                    reason: Some(format!("ingress blocked: pattern `{}`", rule.rule)),
                    modified_payload: None,
                },
                IngressAction::Allow => HookResult {
                    allowed: true,
                    reason: None,
                    modified_payload: None,
                },
                IngressAction::Log => {
                    tracing::info!(target: "praesidia_policy", pattern = %rule.rule, "ingress allowed (log)");
                    HookResult {
                        allowed: true,
                        reason: None,
                        modified_payload: None,
                    }
                }
            };
        }
    }

    match policy.enforcement {
        EnforcementLevel::Strict => HookResult {
            allowed: false,
            reason: Some(
                "strict ingress: no matching rule (default deny when ingress rules are defined)"
                    .into(),
            ),
            modified_payload: None,
        },
        EnforcementLevel::Permissive => HookResult {
            allowed: true,
            reason: None,
            modified_payload: None,
        },
    }
}

/// First matching egress rule wins. No match: allow unchanged output.
pub fn eval_egress(policy: &PraesidiaPolicy, chunk: &str) -> HookResult {
    for rule in &policy.rules.egress {
        let types_ok = rule.types.as_ref().is_none_or(|types| {
            types.iter().any(|t| {
                let t = t.to_lowercase();
                t == "text" || t == "all" || t == "*"
            })
        });
        if !types_ok {
            continue;
        }
        if pattern_matches(&rule.rule, chunk) {
            return match rule.action {
                EgressAction::Reject => HookResult {
                    allowed: false,
                    reason: Some(format!("egress blocked: pattern `{}`", rule.rule)),
                    modified_payload: None,
                },
                EgressAction::Allow => HookResult {
                    allowed: true,
                    reason: None,
                    modified_payload: None,
                },
                EgressAction::Modify => HookResult {
                    allowed: true,
                    reason: None,
                    modified_payload: Some(format!("[modified] {}", chunk)),
                },
            };
        }
    }
    HookResult {
        allowed: true,
        reason: None,
        modified_payload: None,
    }
}

/// First matching tool rule wins. No match: allow.
pub fn eval_tool(policy: &PraesidiaPolicy, tool_name: &str) -> HookResult {
    for rule in &policy.rules.tools {
        if pattern_matches(&rule.match_pattern, tool_name) {
            if rule.require_human_approval == Some(true) {
                return HookResult {
                    allowed: false,
                    reason: Some(format!(
                        "tool `{}` requires human approval per policy",
                        tool_name
                    )),
                    modified_payload: None,
                };
            }
            return match rule.action {
                ToolAction::Reject => HookResult {
                    allowed: false,
                    reason: Some(format!(
                        "tool blocked: pattern `{}`",
                        rule.match_pattern
                    )),
                    modified_payload: None,
                },
                ToolAction::Allow => HookResult {
                    allowed: true,
                    reason: None,
                    modified_payload: None,
                },
            };
        }
    }
    HookResult {
        allowed: true,
        reason: None,
        modified_payload: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{
        EgressAction, EgressRule, EnforcementLevel, IngressAction, IngressRule, PraesidiaRules,
        ToolAction, ToolRule,
    };
    use serde_json::json;

    fn policy_with_ingress(enforcement: EnforcementLevel, rules: Vec<IngressRule>) -> PraesidiaPolicy {
        PraesidiaPolicy {
            schema_version: json!(1),
            enforcement,
            rules: PraesidiaRules {
                ingress: rules,
                ..Default::default()
            },
        }
    }

    #[test]
    fn ingress_reject_on_match() {
        let p = policy_with_ingress(
            EnforcementLevel::Permissive,
            vec![IngressRule {
                rule: "secret".into(),
                action: IngressAction::Reject,
            }],
        );
        let r = eval_ingress(&p, "my secret plan");
        assert!(!r.allowed);
    }

    #[test]
    fn strict_default_deny_when_ingress_rules_exist() {
        let p = policy_with_ingress(
            EnforcementLevel::Strict,
            vec![IngressRule {
                rule: "only-this".into(),
                action: IngressAction::Allow,
            }],
        );
        let r = eval_ingress(&p, "something else");
        assert!(!r.allowed);
    }

    #[test]
    fn permissive_allows_when_no_ingress_match() {
        let p = policy_with_ingress(
            EnforcementLevel::Permissive,
            vec![IngressRule {
                rule: "only-this".into(),
                action: IngressAction::Allow,
            }],
        );
        let r = eval_ingress(&p, "something else");
        assert!(r.allowed);
    }

    #[test]
    fn egress_modify() {
        let p = PraesidiaPolicy {
            schema_version: json!(1),
            enforcement: EnforcementLevel::Permissive,
            rules: PraesidiaRules {
                egress: vec![EgressRule {
                    rule: "password".into(),
                    types: None,
                    action: EgressAction::Modify,
                }],
                ..Default::default()
            },
        };
        let r = eval_egress(&p, "user password here");
        assert!(r.allowed);
        assert_eq!(r.modified_payload, Some("[modified] user password here".into()));
    }

    #[test]
    fn tool_requires_approval() {
        let p = PraesidiaPolicy {
            schema_version: json!(1),
            enforcement: EnforcementLevel::Permissive,
            rules: PraesidiaRules {
                tools: vec![ToolRule {
                    match_pattern: "shell_.*".into(),
                    action: ToolAction::Allow,
                    require_human_approval: Some(true),
                }],
                ..Default::default()
            },
        };
        let r = eval_tool(&p, "shell_exec");
        assert!(!r.allowed);
    }
}
