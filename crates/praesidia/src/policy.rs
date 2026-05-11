use serde::{Deserialize, Serialize};
use serde_yaml;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum EnforcementLevel {
    #[default]
    Strict,
    Permissive,
}


#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum IngressAction {
    Reject,
    Allow,
    Log,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngressRule {
    pub rule: String,
    pub action: IngressAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EgressAction {
    Modify,
    Reject,
    Allow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EgressRule {
    pub rule: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub types: Option<Vec<String>>,
    pub action: EgressAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ToolAction {
    Allow,
    Reject,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRule {
    pub match_pattern: String,
    pub action: ToolAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_human_approval: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PraesidiaRules {
    #[serde(default)]
    pub ingress: Vec<IngressRule>,
    #[serde(default)]
    pub egress: Vec<EgressRule>,
    #[serde(default)]
    pub tools: Vec<ToolRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PraesidiaPolicy {
    pub schema_version: serde_json::Value, // Can be number or string
    #[serde(default)]
    pub enforcement: EnforcementLevel,
    #[serde(default)]
    pub rules: PraesidiaRules,
}

pub fn parse_policy(yaml_content: &str) -> Result<PraesidiaPolicy, serde_yaml::Error> {
    serde_yaml::from_str(yaml_content)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_POLICY_YAML: &str = r#"
schema_version: 1
enforcement: strict
rules:
  ingress:
    - rule: "secret"
      action: reject
    - rule: ".*"
      action: allow
  egress:
    - rule: "password"
      types: ["text"]
      action: modify
    - rule: "internal_only"
      action: reject
  tools:
    - match_pattern: "shell_.*"
      action: allow
      require_human_approval: true
    - match_pattern: "dangerous_tool"
      action: reject
"#;

    #[test]
    fn parse_full_policy() {
        let policy = parse_policy(FULL_POLICY_YAML).unwrap();
        assert_eq!(policy.enforcement, EnforcementLevel::Strict);
        assert_eq!(policy.rules.ingress.len(), 2);
        assert_eq!(policy.rules.egress.len(), 2);
        assert_eq!(policy.rules.tools.len(), 2);
    }

    #[test]
    fn parse_ingress_rules() {
        let policy = parse_policy(FULL_POLICY_YAML).unwrap();
        assert_eq!(policy.rules.ingress[0].rule, "secret");
        assert_eq!(policy.rules.ingress[0].action, IngressAction::Reject);
        assert_eq!(policy.rules.ingress[1].action, IngressAction::Allow);
    }

    #[test]
    fn parse_egress_rules() {
        let policy = parse_policy(FULL_POLICY_YAML).unwrap();
        assert_eq!(policy.rules.egress[0].rule, "password");
        assert_eq!(policy.rules.egress[0].types.as_ref().unwrap(), &vec!["text".to_string()]);
        assert_eq!(policy.rules.egress[0].action, EgressAction::Modify);
        assert_eq!(policy.rules.egress[1].action, EgressAction::Reject);
    }

    #[test]
    fn parse_tool_rules() {
        let policy = parse_policy(FULL_POLICY_YAML).unwrap();
        assert_eq!(policy.rules.tools[0].match_pattern, "shell_.*");
        assert_eq!(policy.rules.tools[0].action, ToolAction::Allow);
        assert_eq!(policy.rules.tools[0].require_human_approval, Some(true));
        assert_eq!(policy.rules.tools[1].action, ToolAction::Reject);
        assert!(policy.rules.tools[1].require_human_approval.is_none());
    }

    #[test]
    fn parse_minimal_policy() {
        let yaml = r#"
schema_version: "1.0"
"#;
        let policy = parse_policy(yaml).unwrap();
        assert_eq!(policy.enforcement, EnforcementLevel::Strict); // default
        assert!(policy.rules.ingress.is_empty());
        assert!(policy.rules.egress.is_empty());
        assert!(policy.rules.tools.is_empty());
    }

    #[test]
    fn enforcement_level_serialization() {
        assert_eq!(serde_json::to_string(&EnforcementLevel::Strict).unwrap(), "\"strict\"");
        assert_eq!(serde_json::to_string(&EnforcementLevel::Permissive).unwrap(), "\"permissive\"");
    }

    #[test]
    fn ingress_action_serialization() {
        assert_eq!(serde_json::to_string(&IngressAction::Reject).unwrap(), "\"reject\"");
        assert_eq!(serde_json::to_string(&IngressAction::Allow).unwrap(), "\"allow\"");
        assert_eq!(serde_json::to_string(&IngressAction::Log).unwrap(), "\"log\"");
    }

    #[test]
    fn egress_action_serialization() {
        assert_eq!(serde_json::to_string(&EgressAction::Modify).unwrap(), "\"modify\"");
        assert_eq!(serde_json::to_string(&EgressAction::Reject).unwrap(), "\"reject\"");
        assert_eq!(serde_json::to_string(&EgressAction::Allow).unwrap(), "\"allow\"");
    }

    #[test]
    fn tool_action_serialization() {
        assert_eq!(serde_json::to_string(&ToolAction::Allow).unwrap(), "\"allow\"");
        assert_eq!(serde_json::to_string(&ToolAction::Reject).unwrap(), "\"reject\"");
    }

    #[test]
    fn egress_rule_omits_none_types() {
        let rule = EgressRule {
            rule: "test".into(),
            types: None,
            action: EgressAction::Allow,
        };
        let json = serde_json::to_string(&rule).unwrap();
        assert!(!json.contains("types"));
    }

    #[test]
    fn tool_rule_omits_none_approval() {
        let rule = ToolRule {
            match_pattern: "test".into(),
            action: ToolAction::Allow,
            require_human_approval: None,
        };
        let json = serde_json::to_string(&rule).unwrap();
        assert!(!json.contains("require_human_approval"));
    }

    #[test]
    fn parse_invalid_yaml_errors() {
        assert!(parse_policy("not: [valid: yaml:").is_err());
    }

    #[test]
    fn schema_version_can_be_number_or_string() {
        let num = parse_policy("schema_version: 1").unwrap();
        assert_eq!(num.schema_version, serde_json::json!(1));

        let str_ver = parse_policy("schema_version: \"2.0\"").unwrap();
        assert_eq!(str_ver.schema_version, serde_json::json!("2.0"));
    }
}
