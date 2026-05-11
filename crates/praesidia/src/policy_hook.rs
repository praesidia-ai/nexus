//! [`PraesidiaPolicyHook`] — policy-based ingress/egress/tool gate.

use std::sync::Arc;

use crate::policy::PraesidiaPolicy;
use crate::policy_eval::{eval_egress, eval_ingress, eval_tool, HookResult};

/// Hash is SHA-256 hex of the raw policy YAML.
pub struct PraesidiaPolicyHook {
    policy: Arc<PraesidiaPolicy>,
    policy_hash: Option<String>,
}

impl PraesidiaPolicyHook {
    pub fn new(policy: PraesidiaPolicy, policy_hash: Option<String>) -> Self {
        Self {
            policy: Arc::new(policy),
            policy_hash,
        }
    }

    pub fn from_yaml(yaml: &str) -> Result<Self, serde_yaml::Error> {
        let hash = crate::policy_hash::sha256_hex(yaml.as_bytes());
        let policy = crate::parse_policy(yaml)?;
        Ok(Self::new(policy, Some(hash)))
    }

    pub fn get_policy_hash(&self) -> Option<String> {
        self.policy_hash.clone()
    }

    pub async fn check_ingress(&self, payload: &str) -> HookResult {
        eval_ingress(&self.policy, payload)
    }

    pub async fn check_egress(&self, chunk: &str) -> HookResult {
        eval_egress(&self.policy, chunk)
    }

    pub async fn check_tool(&self, tool_name: &str) -> HookResult {
        eval_tool(&self.policy, tool_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_POLICY_YAML: &str = r#"
schema_version: 1
enforcement: strict
rules:
  ingress:
    - rule: "forbidden"
      action: reject
    - rule: ".*"
      action: allow
  egress:
    - rule: "secret_data"
      action: reject
  tools:
    - match_pattern: "dangerous_.*"
      action: reject
"#;

    #[test]
    fn from_yaml_parses_successfully() {
        let hook = PraesidiaPolicyHook::from_yaml(TEST_POLICY_YAML).unwrap();
        assert!(hook.get_policy_hash().is_some());
    }

    #[test]
    fn from_yaml_produces_sha256_hash() {
        let hook = PraesidiaPolicyHook::from_yaml(TEST_POLICY_YAML).unwrap();
        let hash = hook.get_policy_hash().unwrap();
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn from_yaml_invalid_errors() {
        assert!(PraesidiaPolicyHook::from_yaml("not: [valid: yaml:").is_err());
    }

    #[tokio::test]
    async fn check_ingress_allows_normal_input() {
        let hook = PraesidiaPolicyHook::from_yaml(TEST_POLICY_YAML).unwrap();
        let result = hook.check_ingress("hello world").await;
        assert!(result.allowed);
    }

    #[tokio::test]
    async fn check_ingress_rejects_forbidden() {
        let hook = PraesidiaPolicyHook::from_yaml(TEST_POLICY_YAML).unwrap();
        let result = hook.check_ingress("this is forbidden content").await;
        assert!(!result.allowed);
        assert!(result.reason.is_some());
    }

    #[tokio::test]
    async fn check_egress_allows_clean_output() {
        let hook = PraesidiaPolicyHook::from_yaml(TEST_POLICY_YAML).unwrap();
        let result = hook.check_egress("normal output").await;
        assert!(result.allowed);
    }

    #[tokio::test]
    async fn check_egress_rejects_secret_data() {
        let hook = PraesidiaPolicyHook::from_yaml(TEST_POLICY_YAML).unwrap();
        let result = hook.check_egress("contains secret_data leak").await;
        assert!(!result.allowed);
    }

    #[tokio::test]
    async fn check_tool_allows_safe_tools() {
        let hook = PraesidiaPolicyHook::from_yaml(TEST_POLICY_YAML).unwrap();
        let result = hook.check_tool("web_search").await;
        assert!(result.allowed);
    }

    #[tokio::test]
    async fn check_tool_rejects_dangerous_tools() {
        let hook = PraesidiaPolicyHook::from_yaml(TEST_POLICY_YAML).unwrap();
        let result = hook.check_tool("dangerous_exec").await;
        assert!(!result.allowed);
    }

    #[test]
    fn new_with_explicit_hash() {
        let policy = crate::parse_policy(TEST_POLICY_YAML).unwrap();
        let hook = PraesidiaPolicyHook::new(policy, Some("custom-hash".into()));
        assert_eq!(hook.get_policy_hash().as_deref(), Some("custom-hash"));
    }

    #[test]
    fn new_with_no_hash() {
        let policy = crate::parse_policy(TEST_POLICY_YAML).unwrap();
        let hook = PraesidiaPolicyHook::new(policy, None);
        assert!(hook.get_policy_hash().is_none());
    }
}
