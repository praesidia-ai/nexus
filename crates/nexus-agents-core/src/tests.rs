//! Tests for nexus-agents-core types.

#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use crate::definition::*;
    use crate::events::*;
    use crate::teams::*;
    use crate::tools::*;
    // -----------------------------------------------------------------------
    // AgentDefinition serialization
    // -----------------------------------------------------------------------

    #[test]
    fn agent_definition_serde_roundtrip() {
        let def = AgentDefinition {
            id: "agent-1".into(),
            name: "Nova".into(),
            system_prompt: "You are a coder".into(),
            skills: vec!["code_review".into(), "debugging".into()],
            tools: vec!["file_read".into(), "file_write".into()],
            model_preference: Some(ModelPreference {
                provider: "openai".into(),
                model: "gpt-4o".into(),
            }),
            execution_mode: ExecutionMode::Interactive,
            max_iterations: 10,
            timeout_secs: 300,
        };
        let json = serde_json::to_string(&def).unwrap();
        let back: AgentDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "agent-1");
        assert_eq!(back.name, "Nova");
        assert_eq!(back.tools.len(), 2);
    }

    #[test]
    fn execution_mode_interactive_serde() {
        let mode = ExecutionMode::Interactive;
        let json = serde_json::to_string(&mode).unwrap();
        let back: ExecutionMode = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, ExecutionMode::Interactive));
    }

    #[test]
    fn execution_mode_background_serde() {
        let mode = ExecutionMode::Background {
            notify_on: vec![NotifyCondition::Completion, NotifyCondition::Failure],
        };
        let json = serde_json::to_string(&mode).unwrap();
        let back: ExecutionMode = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, ExecutionMode::Background { .. }));
    }

    #[test]
    fn execution_mode_parallel_serde() {
        let mode = ExecutionMode::Parallel {
            merge_strategy: MergeStrategy::BestOf,
        };
        let json = serde_json::to_string(&mode).unwrap();
        let back: ExecutionMode = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, ExecutionMode::Parallel { .. }));
    }

    // -----------------------------------------------------------------------
    // Team types serialization
    // -----------------------------------------------------------------------

    #[test]
    fn team_role_display() {
        assert_eq!(TeamRole::Lead.to_string(), "Lead");
        assert_eq!(TeamRole::Reviewer.to_string(), "Reviewer");
        assert_eq!(TeamRole::Coordinator.to_string(), "Coordinator");
        assert_eq!(TeamRole::Observer.to_string(), "Observer");
        assert_eq!(
            TeamRole::Specialist { domain: "security".into() }.to_string(),
            "Specialist(security)"
        );
        assert_eq!(
            TeamRole::Custom { name: "QA".into(), description: "Quality assurance".into() }.to_string(),
            "Custom(QA)"
        );
    }

    #[test]
    fn coordination_protocol_hierarchical_serde() {
        let proto = CoordinationProtocol::Hierarchical { lead_id: "lead-1".into() };
        let json = serde_json::to_string(&proto).unwrap();
        let back: CoordinationProtocol = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, CoordinationProtocol::Hierarchical { .. }));
    }

    #[test]
    fn coordination_protocol_consensus_serde() {
        let proto = CoordinationProtocol::Consensus { quorum: 0.75, max_rounds: 3 };
        let json = serde_json::to_string(&proto).unwrap();
        let back: CoordinationProtocol = serde_json::from_str(&json).unwrap();
        if let CoordinationProtocol::Consensus { quorum, max_rounds } = back {
            assert!((quorum - 0.75).abs() < f32::EPSILON);
            assert_eq!(max_rounds, 3);
        } else {
            panic!("Expected Consensus protocol");
        }
    }

    #[test]
    fn coordination_protocol_all_variants_serialize() {
        let variants: Vec<CoordinationProtocol> = vec![
            CoordinationProtocol::Hierarchical { lead_id: "l".into() },
            CoordinationProtocol::Parallel { coordinator_id: "c".into() },
            CoordinationProtocol::Sequential { order: vec!["a".into(), "b".into()] },
            CoordinationProtocol::Consensus { quorum: 0.5, max_rounds: 2 },
            CoordinationProtocol::Competitive { judge_id: "j".into(), candidates: vec!["a".into()] },
            CoordinationProtocol::Swarm { max_concurrent: 3 },
        ];
        for v in variants {
            let json = serde_json::to_string(&v).unwrap();
            let back: CoordinationProtocol = serde_json::from_str(&json).unwrap();
            let _ = back;
        }
    }

    #[test]
    fn team_budget_serde_roundtrip() {
        let budget = TeamBudget {
            max_total_cost_usd: 10.0,
            max_cost_per_member_usd: 2.0,
            max_total_tokens: 100_000,
            max_duration_secs: 600,
            max_iterations_per_member: 15,
        };
        let json = serde_json::to_string(&budget).unwrap();
        let back: TeamBudget = serde_json::from_str(&json).unwrap();
        assert!((back.max_total_cost_usd - 10.0).abs() < f32::EPSILON);
    }

    #[test]
    fn hitl_checkpoint_serde_roundtrip() {
        let checkpoints = vec![
            HitlCheckpoint::AfterPlanning,
            HitlCheckpoint::AfterEachCompletion,
            HitlCheckpoint::BudgetThreshold { percent: 0.8 },
            HitlCheckpoint::OnConflict,
            HitlCheckpoint::BeforeCommit,
            HitlCheckpoint::Never,
        ];
        for cp in checkpoints {
            let json = serde_json::to_string(&cp).unwrap();
            let back: HitlCheckpoint = serde_json::from_str(&json).unwrap();
            assert_eq!(back, cp);
        }
    }

    // -----------------------------------------------------------------------
    // Event serialization
    // -----------------------------------------------------------------------

    #[test]
    fn agent_event_started_serializes() {
        let event = AgentEvent::Started {
            agent_id: "nova".into(),
            task: "Fix bugs".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"started\""));
    }

    #[test]
    fn agent_event_completed_serializes() {
        let event = AgentEvent::Completed {
            agent_id: "nova".into(),
            result: AgentResult {
                output: "Done".into(),
                files_modified: vec!["src/main.rs".into()],
                iterations_used: 3,
                duration_ms: 5000,
                tool_calls: vec![ToolCallRecord {
                    tool: "file_write".into(),
                    input_summary: "wrote main.rs".into(),
                    output_summary: "ok".into(),
                    duration_ms: 100,
                    success: true,
                }],
            },
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"completed\""));
    }

    #[test]
    fn agent_event_failed_serializes() {
        let event = AgentEvent::Failed {
            agent_id: "nova".into(),
            error: "timeout".into(),
            iteration: 5,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"failed\""));
    }

    // -----------------------------------------------------------------------
    // Tool types
    // -----------------------------------------------------------------------

    #[test]
    fn tool_input_serde_roundtrip() {
        let input = ToolInput {
            parameters: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
        };
        let json = serde_json::to_string(&input).unwrap();
        let back: ToolInput = serde_json::from_str(&json).unwrap();
        assert_eq!(back.parameters["path"], "/tmp/test.txt");
    }

    #[test]
    fn tool_output_success() {
        let output = ToolOutput {
            result: serde_json::json!({"status": "ok"}),
            success: true,
            error: None,
        };
        let json = serde_json::to_string(&output).unwrap();
        let back: ToolOutput = serde_json::from_str(&json).unwrap();
        assert!(back.success);
        assert!(back.error.is_none());
    }

    #[test]
    fn tool_output_failure() {
        let output = ToolOutput {
            result: serde_json::json!(null),
            success: false,
            error: Some("file not found".into()),
        };
        let json = serde_json::to_string(&output).unwrap();
        let back: ToolOutput = serde_json::from_str(&json).unwrap();
        assert!(!back.success);
        assert_eq!(back.error.as_deref(), Some("file not found"));
    }

    #[test]
    fn tool_category_serde_roundtrip() {
        for cat in [
            ToolCategory::FileSystem,
            ToolCategory::CodeExecution,
            ToolCategory::WebAccess,
            ToolCategory::DataProcessing,
            ToolCategory::NexusNative,
            ToolCategory::Communication,
            ToolCategory::External,
        ] {
            let json = serde_json::to_string(&cat).unwrap();
            let back: ToolCategory = serde_json::from_str(&json).unwrap();
            assert_eq!(back, cat);
        }
    }
}
