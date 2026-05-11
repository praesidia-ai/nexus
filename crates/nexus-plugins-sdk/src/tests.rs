//! Tests for nexus-plugins-sdk types and default hook implementations.

#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use crate::manifest::*;
    use crate::hooks::*;
    use crate::capabilities::*;
    use crate::default_hook::*;

    // -----------------------------------------------------------------------
    // Manifest serialization
    // -----------------------------------------------------------------------

    #[test]
    fn plugin_manifest_serde_roundtrip() {
        let manifest = PluginManifest {
            id: "com.example.my-plugin".into(),
            name: "My Plugin".into(),
            version: "1.0.0".into(),
            description: "A test plugin".into(),
            author: "Test Author".into(),
            license: Some("MIT".into()),
            homepage: Some("https://example.com".into()),
            min_nexus_version: Some("0.1.0".into()),
            capabilities: vec![
                PluginCapability::DomainPack { domain: "fitness".into() },
                PluginCapability::Tools { tool_names: vec!["my_tool".into()] },
            ],
            hooks: vec![HookRegistration {
                hook: HookPoint::PostGenerate,
                priority: 50,
                condition: Some("always".into()),
            }],
            config_schema: Some(serde_json::json!({"type": "object"})),
        };
        let json = serde_json::to_string_pretty(&manifest).unwrap();
        let back: PluginManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "com.example.my-plugin");
        assert_eq!(back.capabilities.len(), 2);
        assert_eq!(back.hooks.len(), 1);
    }

    #[test]
    fn hook_registration_default() {
        let reg = HookRegistration::default();
        assert_eq!(reg.hook, HookPoint::PostGenerate);
        assert_eq!(reg.priority, 100);
        assert!(reg.condition.is_none());
    }

    // -----------------------------------------------------------------------
    // Capabilities serialization
    // -----------------------------------------------------------------------

    #[test]
    fn plugin_capability_all_variants_serde() {
        let variants: Vec<PluginCapability> = vec![
            PluginCapability::DomainPack { domain: "fitness".into() },
            PluginCapability::Tools { tool_names: vec!["read".into(), "write".into()] },
            PluginCapability::Generator { target: "react".into() },
            PluginCapability::QualityDimension { dimension: "performance".into() },
            PluginCapability::DeployTarget { platform: "vercel".into() },
            PluginCapability::LlmProvider { provider: "ollama".into() },
            PluginCapability::Theme { theme_name: "dark-mode".into() },
            PluginCapability::Custom { name: "my-cap".into(), description: "Custom capability".into() },
        ];
        for cap in &variants {
            let json = serde_json::to_string(cap).unwrap();
            let back: PluginCapability = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, cap);
        }
    }

    // -----------------------------------------------------------------------
    // Hook point serialization
    // -----------------------------------------------------------------------

    #[test]
    fn hook_point_all_variants_serde() {
        let points = vec![
            HookPoint::PreIntent, HookPoint::PostIntent,
            HookPoint::PreDecision, HookPoint::PostDecision,
            HookPoint::PreGenerate, HookPoint::PostGenerate,
            HookPoint::PreQualityScore, HookPoint::PostQualityScore,
            HookPoint::PreGuarantee, HookPoint::PostGuarantee,
            HookPoint::PreBuild, HookPoint::PostBuild,
            HookPoint::PreDeploy, HookPoint::PostDeploy,
            HookPoint::AgentTaskStart, HookPoint::AgentTaskComplete,
        ];
        for point in &points {
            let json = serde_json::to_string(point).unwrap();
            let back: HookPoint = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, point);
        }
    }

    // -----------------------------------------------------------------------
    // Hook action and result
    // -----------------------------------------------------------------------

    #[test]
    fn hook_action_all_variants_serde() {
        for action in [HookAction::Continue, HookAction::Modify, HookAction::Abort, HookAction::Skip] {
            let json = serde_json::to_string(&action).unwrap();
            let back: HookAction = serde_json::from_str(&json).unwrap();
            assert_eq!(back, action);
        }
    }

    #[test]
    fn hook_result_ok() {
        let result = HookResult::ok();
        assert_eq!(result.action, HookAction::Continue);
        assert!(result.data.is_none());
        assert!(result.messages.is_empty());
    }

    #[test]
    fn hook_result_modify() {
        let data = serde_json::json!({"modified": true});
        let result = HookResult::modify(data.clone());
        assert_eq!(result.action, HookAction::Modify);
        assert_eq!(result.data, Some(data));
    }

    #[test]
    fn hook_result_abort() {
        let result = HookResult::abort("Something went wrong");
        assert_eq!(result.action, HookAction::Abort);
        assert_eq!(result.messages.len(), 1);
        assert!(result.messages[0].contains("Something went wrong"));
    }

    #[test]
    fn hook_context_serde_roundtrip() {
        let ctx = HookContext {
            project_id: Some("proj-1".into()),
            tenant_id: "tenant-1".into(),
            hook_point: HookPoint::PostGenerate,
            data: serde_json::json!({"files": ["index.html"]}),
            config: None,
        };
        let json = serde_json::to_string(&ctx).unwrap();
        let back: HookContext = serde_json::from_str(&json).unwrap();
        assert_eq!(back.tenant_id, "tenant-1");
    }

    // -----------------------------------------------------------------------
    // Default hook implementations
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn passthrough_hook_returns_continue() {
        let hook = PassthroughHook::new();
        let ctx = HookContext {
            project_id: None,
            tenant_id: "t1".into(),
            hook_point: HookPoint::PreBuild,
            data: serde_json::json!({}),
            config: None,
        };
        let result = hook.execute(&ctx).await.unwrap();
        assert_eq!(result.action, HookAction::Continue);
    }

    #[tokio::test]
    async fn logging_hook_records_to_buffer() {
        let hook = LoggingHook::new("test");
        let buffer = hook.log_buffer();
        let ctx = HookContext {
            project_id: Some("proj-1".into()),
            tenant_id: "t1".into(),
            hook_point: HookPoint::PostDeploy,
            data: serde_json::json!({}),
            config: None,
        };
        let result = hook.execute(&ctx).await.unwrap();
        assert_eq!(result.action, HookAction::Continue);
        assert!(!result.messages.is_empty());

        let log = buffer.lock().unwrap();
        assert_eq!(log.len(), 1);
        assert!(log[0].contains("[test]"));
        assert!(log[0].contains("PostDeploy"));
    }

    #[tokio::test]
    async fn logging_hook_multiple_calls() {
        let hook = LoggingHook::new("multi");
        let buffer = hook.log_buffer();

        for point in [HookPoint::PreIntent, HookPoint::PostIntent, HookPoint::PreGenerate] {
            let ctx = HookContext {
                project_id: None,
                tenant_id: "t1".into(),
                hook_point: point,
                data: serde_json::json!({}),
                config: None,
            };
            hook.execute(&ctx).await.unwrap();
        }

        let log = buffer.lock().unwrap();
        assert_eq!(log.len(), 3);
    }
}
