use crate::blueprint::*;

/// Blueprint template: fix a bug.
///
/// Pipeline: analyze → patch → Tier1 lint gate → Tier2 test gate → self-heal loop if failed.
#[allow(dead_code)]
pub fn code_fix() -> Blueprint {
    Blueprint {
        name: "code_fix".into(),
        version: "1.0.0".into(),
        description: "Fix a bug: analyze, patch, lint gate, test gate, self-heal if failed".into(),
        triggers: vec![BlueprintTrigger::Manual],
        steps: vec![
            PipelineStep::Agent {
                name: "analyze_bug".into(),
                model: None,
                system_prompt: concat!(
                    "You are a senior debugger. Analyze the bug report and codebase context. ",
                    "Identify root cause, affected files, and propose a minimal fix. ",
                    "Output a structured analysis with: root_cause, affected_files, proposed_fix."
                )
                .into(),
                tools: vec![
                    "read_file".into(),
                    "search_code".into(),
                    "list_directory".into(),
                ],
                max_iterations: 10,
                timeout_secs: 120,
            },
            PipelineStep::Agent {
                name: "apply_patch".into(),
                model: None,
                system_prompt: concat!(
                    "Apply the proposed fix from the analysis step. ",
                    "Make minimal, targeted changes. Do not refactor unrelated code."
                )
                .into(),
                tools: vec![
                    "read_file".into(),
                    "write_file".into(),
                    "search_code".into(),
                ],
                max_iterations: 15,
                timeout_secs: 180,
            },
            PipelineStep::Gate {
                name: "lint_gate".into(),
                checks: vec![GateCheck::Lint {
                    command: "cargo fmt -- --check".into(),
                }],
                on_failure: FailureAction::SelfHeal { max_rounds: 2 },
            },
            PipelineStep::Gate {
                name: "test_gate".into(),
                checks: vec![
                    GateCheck::TypeCheck {
                        command: "cargo check".into(),
                    },
                    GateCheck::Test {
                        command: "cargo test".into(),
                        selective: true,
                    },
                ],
                on_failure: FailureAction::SelfHeal { max_rounds: 3 },
            },
        ],
        config: BlueprintConfig {
            max_total_duration_secs: 600,
            resource_budget: ResourceBudget {
                max_llm_tokens: 200_000,
                max_tool_calls: 50,
                max_cost_usd: 2.0,
            },
            notifications: vec![],
        },
    }
}

/// Blueprint template: build a feature.
///
/// Pipeline: plan → implement → lint → type-check → test → security scan.
#[allow(dead_code)]
pub fn feature_build() -> Blueprint {
    Blueprint {
        name: "feature_build".into(),
        version: "1.0.0".into(),
        description: "Build a feature: plan, implement, lint, type-check, test, security scan"
            .into(),
        triggers: vec![BlueprintTrigger::Manual],
        steps: vec![
            PipelineStep::Agent {
                name: "plan_feature".into(),
                model: None,
                system_prompt: concat!(
                    "You are a software architect. Given the feature request, produce a detailed ",
                    "implementation plan: files to create/modify, data structures, API surface, ",
                    "test strategy. Output structured JSON."
                )
                .into(),
                tools: vec![
                    "read_file".into(),
                    "search_code".into(),
                    "list_directory".into(),
                ],
                max_iterations: 10,
                timeout_secs: 120,
            },
            PipelineStep::Agent {
                name: "implement_feature".into(),
                model: None,
                system_prompt: concat!(
                    "Implement the feature according to the plan from the previous step. ",
                    "Write production-quality code with proper error handling. ",
                    "Include unit tests alongside the implementation."
                )
                .into(),
                tools: vec![
                    "read_file".into(),
                    "write_file".into(),
                    "search_code".into(),
                    "run_command".into(),
                ],
                max_iterations: 30,
                timeout_secs: 300,
            },
            PipelineStep::Gate {
                name: "lint_gate".into(),
                checks: vec![GateCheck::Lint {
                    command: "cargo fmt -- --check".into(),
                }],
                on_failure: FailureAction::SelfHeal { max_rounds: 2 },
            },
            PipelineStep::Gate {
                name: "typecheck_gate".into(),
                checks: vec![GateCheck::TypeCheck {
                    command: "cargo check".into(),
                }],
                on_failure: FailureAction::SelfHeal { max_rounds: 2 },
            },
            PipelineStep::Gate {
                name: "test_gate".into(),
                checks: vec![GateCheck::Test {
                    command: "cargo test".into(),
                    selective: false,
                }],
                on_failure: FailureAction::SelfHeal { max_rounds: 3 },
            },
            PipelineStep::Gate {
                name: "security_gate".into(),
                checks: vec![GateCheck::SecurityScan {
                    rules: vec!["no-unsafe".into(), "no-unwrap-in-prod".into()],
                }],
                on_failure: FailureAction::Escalate {
                    channel: "security-review".into(),
                },
            },
        ],
        config: BlueprintConfig {
            max_total_duration_secs: 1200,
            resource_budget: ResourceBudget {
                max_llm_tokens: 500_000,
                max_tool_calls: 100,
                max_cost_usd: 5.0,
            },
            notifications: vec![],
        },
    }
}

/// Blueprint template: autonomous PR merge.
///
/// Pipeline: fetch changes → review → lint+test gates (parallel) → approval → merge.
#[allow(dead_code)]
pub fn pr_merge() -> Blueprint {
    Blueprint {
        name: "pr_merge".into(),
        version: "1.0.0".into(),
        description: "Autonomous PR: fetch, review, lint+test gates, approval, merge".into(),
        triggers: vec![
            BlueprintTrigger::Event {
                channel: "github".into(),
                pattern: "pull_request.opened".into(),
            },
            BlueprintTrigger::Manual,
        ],
        steps: vec![
            PipelineStep::Command {
                name: "fetch_changes".into(),
                command: "git".into(),
                args: vec!["fetch".into(), "origin".into()],
                timeout_secs: 30,
                working_dir: None,
            },
            PipelineStep::Agent {
                name: "code_review".into(),
                model: None,
                system_prompt: concat!(
                    "You are a senior code reviewer. Review the PR diff for: ",
                    "correctness, security issues, performance problems, style violations, ",
                    "missing tests, and documentation gaps. ",
                    "Produce a structured review with severity levels."
                )
                .into(),
                tools: vec![
                    "read_file".into(),
                    "search_code".into(),
                    "run_command".into(),
                ],
                max_iterations: 15,
                timeout_secs: 180,
            },
            PipelineStep::Parallel {
                name: "validation_gates".into(),
                branches: vec![
                    PipelineStep::Gate {
                        name: "lint_and_format".into(),
                        checks: vec![GateCheck::Lint {
                            command: "cargo fmt -- --check && cargo clippy -- -D warnings".into(),
                        }],
                        on_failure: FailureAction::SelfHeal { max_rounds: 2 },
                    },
                    PipelineStep::Gate {
                        name: "tests".into(),
                        checks: vec![
                            GateCheck::TypeCheck {
                                command: "cargo check".into(),
                            },
                            GateCheck::Test {
                                command: "cargo test".into(),
                                selective: false,
                            },
                        ],
                        on_failure: FailureAction::Abort,
                    },
                ],
                merge_strategy: MergeStrategy::WaitAll,
            },
            PipelineStep::Approval {
                name: "human_approval".into(),
                approvers: vec!["tech-lead".into(), "security-team".into()],
                timeout_secs: 86400,
            },
            PipelineStep::Command {
                name: "merge_pr".into(),
                command: "gh".into(),
                args: vec![
                    "pr".into(),
                    "merge".into(),
                    "--squash".into(),
                    "--auto".into(),
                ],
                timeout_secs: 60,
                working_dir: None,
            },
        ],
        config: BlueprintConfig {
            max_total_duration_secs: 86400,
            resource_budget: ResourceBudget {
                max_llm_tokens: 300_000,
                max_tool_calls: 75,
                max_cost_usd: 3.0,
            },
            notifications: vec![NotificationTarget::Slack {
                webhook_url: "${SLACK_WEBHOOK_URL}".into(),
            }],
        },
    }
}
