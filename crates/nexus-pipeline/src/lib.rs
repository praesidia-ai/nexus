pub mod blueprint;
pub mod executor;
pub mod gate;
pub mod prefetch;
pub mod templates;

pub use blueprint::{
    Blueprint, BlueprintConfig, BlueprintTrigger, FailureAction, GateCheck, MergeStrategy,
    NotificationTarget, PipelineStep, ResourceBudget,
};
pub use executor::{PipelineRun, PipelineStatus, StepMetrics, StepResult, StepStatus};
pub use gate::{GateResult, GateTier};
pub use prefetch::{PrefetchPlan, ToolDescriptor};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blueprint_toml_roundtrip() {
        let bp = templates::code_fix();
        let toml_str = bp.to_toml().expect("serialize to TOML");
        assert!(toml_str.contains("code_fix"));
        assert!(toml_str.contains("analyze_bug"));

        let parsed = Blueprint::from_toml(&toml_str).expect("parse from TOML");
        assert_eq!(parsed.name, "code_fix");
        assert_eq!(parsed.steps.len(), bp.steps.len());
    }

    #[test]
    fn test_feature_build_template_structure() {
        let bp = templates::feature_build();
        assert_eq!(bp.name, "feature_build");
        assert_eq!(bp.steps.len(), 6);
        assert_eq!(bp.config.resource_budget.max_cost_usd, 5.0);
    }

    #[test]
    fn test_pr_merge_template_has_parallel_step() {
        let bp = templates::pr_merge();
        assert_eq!(bp.name, "pr_merge");

        let has_parallel = bp.steps.iter().any(|s| {
            matches!(s, PipelineStep::Parallel { .. })
        });
        assert!(has_parallel, "pr_merge template must contain a Parallel step");
    }

    #[test]
    fn test_pipeline_run_budget_tracking() {
        let budget = ResourceBudget {
            max_llm_tokens: 1000,
            max_tool_calls: 10,
            max_cost_usd: 1.0,
        };
        let mut run = PipelineRun::new("test");
        assert!(!run.is_budget_exceeded(&budget));

        run.record_step(StepResult {
            step_name: "step1".into(),
            status: StepStatus::Passed,
            started_at: chrono::Utc::now(),
            finished_at: Some(chrono::Utc::now()),
            output: serde_json::Value::Null,
            metrics: StepMetrics {
                duration_ms: 100,
                llm_tokens_used: 500,
                tool_calls: 5,
                cost_usd: 0.5,
            },
        });
        assert!(!run.is_budget_exceeded(&budget));

        run.record_step(StepResult {
            step_name: "step2".into(),
            status: StepStatus::Passed,
            started_at: chrono::Utc::now(),
            finished_at: Some(chrono::Utc::now()),
            output: serde_json::Value::Null,
            metrics: StepMetrics {
                duration_ms: 200,
                llm_tokens_used: 600,
                tool_calls: 6,
                cost_usd: 0.6,
            },
        });
        assert!(run.is_budget_exceeded(&budget));
        assert_eq!(run.total_metrics.llm_tokens_used, 1100);
        assert_eq!(run.total_metrics.tool_calls, 11);
    }

    #[test]
    fn test_pipeline_run_complete() {
        let mut run = PipelineRun::new("test_bp");
        assert_eq!(run.status, PipelineStatus::Running);
        assert!(run.finished_at.is_none());

        run.complete(PipelineStatus::Completed);
        assert_eq!(run.status, PipelineStatus::Completed);
        assert!(run.finished_at.is_some());
    }

    #[test]
    fn test_gate_execute_check_echo() {
        let check = GateCheck::Custom {
            command: "echo hello".into(),
            expected_exit: 0,
        };
        let result = gate::execute_check(&check, "/tmp");
        assert!(result.passed);
        assert!(result.output.contains("hello"));
        assert_eq!(result.tier, GateTier::Tier3);
    }

    #[test]
    fn test_gate_execute_check_failure() {
        let check = GateCheck::Custom {
            command: "exit 1".into(),
            expected_exit: 0,
        };
        let result = gate::execute_check(&check, "/tmp");
        assert!(!result.passed);
    }

    #[test]
    fn test_tiered_gates_abort_on_failure() {
        let checks = vec![
            GateCheck::Lint {
                command: "exit 1".into(),
            },
            GateCheck::Test {
                command: "echo should-not-run".into(),
                selective: false,
            },
        ];
        let results =
            gate::run_tiered_gates(&checks, &blueprint::FailureAction::Abort, "/tmp");
        assert_eq!(results.len(), 1, "Should stop after first failure on Abort");
        assert!(!results[0].passed);
    }

    #[test]
    fn test_tiered_gates_all_pass() {
        let checks = vec![
            GateCheck::Lint {
                command: "echo lint-ok".into(),
            },
            GateCheck::TypeCheck {
                command: "echo check-ok".into(),
            },
        ];
        let results =
            gate::run_tiered_gates(&checks, &blueprint::FailureAction::Abort, "/tmp");
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.passed));
    }

    #[test]
    fn test_prefetch_tools_relevance() {
        let tools = vec![
            ToolDescriptor {
                name: "read_file".into(),
                description: "Read a file from the filesystem".into(),
                category: "filesystem".into(),
                keywords: vec!["read".into(), "file".into(), "open".into()],
            },
            ToolDescriptor {
                name: "run_tests".into(),
                description: "Execute test suite".into(),
                category: "testing".into(),
                keywords: vec!["test".into(), "suite".into(), "check".into()],
            },
            ToolDescriptor {
                name: "deploy_server".into(),
                description: "Deploy to production server".into(),
                category: "deployment".into(),
                keywords: vec!["deploy".into(), "server".into(), "production".into()],
            },
        ];

        let plan = prefetch::prefetch_tools("read the test file and run tests", &tools, 2);
        assert_eq!(plan.selected_tools.len(), 2);
        assert!(
            plan.selected_tools.contains(&"read_file".to_string()),
            "Should select read_file for 'read file' query"
        );
        assert!(
            plan.selected_tools.contains(&"run_tests".to_string()),
            "Should select run_tests for 'run tests' query"
        );
    }

    #[test]
    fn test_step_status_serialization() {
        let status = StepStatus::Failed {
            reason: "compile error".into(),
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("failed"));
        assert!(json.contains("compile error"));

        let deserialized: StepStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, status);
    }
}
