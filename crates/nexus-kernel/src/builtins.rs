//! Built-in reactive agent definitions.
//!
//! These factory functions create pre-configured [`ReactiveAgentDefinition`]s for
//! common development workflows. Each definition includes the agent, triggers,
//! lifecycle, and resource settings needed to run out of the box.

use nexus_agents_core::definition::{AgentDefinition, ExecutionMode};

use crate::process::{
    FilePermissions, NetworkPermissions, Priority, ResourceAllocation, ToolPermissions,
};
use crate::reactive::{
    FileEvent, ReactiveAgentDefinition, ReactiveLifecycle, Trigger,
};

/// Create a test watcher agent that watches source files and runs tests on change.
///
/// Triggers on file modifications in `src/` and `tests/` directories. Uses debouncing
/// to avoid running tests multiple times for rapid saves.
pub fn test_watcher_agent(project_root: &str) -> ReactiveAgentDefinition {
    ReactiveAgentDefinition {
        id: "builtin:test-watcher".to_string(),
        agent: AgentDefinition {
            id: "test-watcher".to_string(),
            name: "Test Watcher".to_string(),
            system_prompt: concat!(
                "You are a test runner agent. When source files change, run the relevant ",
                "test suite and report results. Focus on the files that changed and any ",
                "tests that directly test them. If tests fail, provide a concise summary ",
                "of which tests failed and why."
            )
            .to_string(),
            skills: vec!["testing".to_string(), "diagnostics".to_string()],
            tools: vec![
                "shell".to_string(),
                "file_read".to_string(),
                "file_search".to_string(),
            ],
            model_preference: None,
            execution_mode: ExecutionMode::Background {
                notify_on: vec![
                    nexus_agents_core::definition::NotifyCondition::Completion,
                    nexus_agents_core::definition::NotifyCondition::Failure,
                ],
            },
            max_iterations: 20,
            timeout_secs: 300,
        },
        task_template: "Source files have changed. Run the relevant test suite and report results. \
                        Focus on tests related to the changed files."
            .to_string(),
        triggers: vec![Trigger::FileChange {
            paths: vec![
                format!("{}/src/**/*.rs", project_root),
                format!("{}/tests/**/*.rs", project_root),
            ],
            events: vec![FileEvent::Modified, FileEvent::Created],
        }],
        lifecycle: ReactiveLifecycle::OnDemand,
        state_persistence: false,
        priority: Priority::Normal,
        resources: ResourceAllocation {
            max_tokens: 50_000,
            max_cost_usd: 0.50,
            max_memory_bytes: 128 * 1024 * 1024,
            max_duration_secs: 300,
            max_iterations: 20,
            tool_permissions: ToolPermissions {
                allowed: vec![
                    "shell".to_string(),
                    "file_read".to_string(),
                    "file_search".to_string(),
                ],
                denied: vec![],
                allow_shell: true,
            },
            network_permissions: NetworkPermissions::default(),
            file_permissions: FilePermissions {
                read_paths: vec![project_root.to_string()],
                write_paths: vec![],
                allow_create: false,
                allow_delete: false,
            },
        },
        enabled: true,
    }
}

/// Create a build monitor agent that reacts to build failures and diagnoses them.
///
/// Subscribes to the `project.*.build` event channel and triggers when a `failed`
/// event is published.
pub fn build_monitor_agent() -> ReactiveAgentDefinition {
    ReactiveAgentDefinition {
        id: "builtin:build-monitor".to_string(),
        agent: AgentDefinition {
            id: "build-monitor".to_string(),
            name: "Build Monitor".to_string(),
            system_prompt: concat!(
                "You are a build diagnostics agent. When a build fails, analyze the error ",
                "output, identify the root cause, and suggest fixes. Be concise and ",
                "actionable. If the fix is straightforward, provide the exact code change."
            )
            .to_string(),
            skills: vec!["diagnostics".to_string(), "coding".to_string()],
            tools: vec![
                "shell".to_string(),
                "file_read".to_string(),
                "file_write".to_string(),
                "file_search".to_string(),
            ],
            model_preference: None,
            execution_mode: ExecutionMode::Background {
                notify_on: vec![
                    nexus_agents_core::definition::NotifyCondition::Completion,
                    nexus_agents_core::definition::NotifyCondition::Failure,
                ],
            },
            max_iterations: 30,
            timeout_secs: 600,
        },
        task_template: "A build has failed. Analyze the build output, identify the root cause, \
                        and suggest or apply fixes."
            .to_string(),
        triggers: vec![Trigger::Event {
            channel: "project.*.build".to_string(),
            event_type_filter: Some("failed".to_string()),
            debounce_ms: Some(5000),
        }],
        lifecycle: ReactiveLifecycle::OnDemand,
        state_persistence: false,
        priority: Priority::High,
        resources: ResourceAllocation {
            max_tokens: 100_000,
            max_cost_usd: 1.0,
            max_memory_bytes: 256 * 1024 * 1024,
            max_duration_secs: 600,
            max_iterations: 30,
            tool_permissions: ToolPermissions {
                allowed: vec![
                    "shell".to_string(),
                    "file_read".to_string(),
                    "file_write".to_string(),
                    "file_search".to_string(),
                ],
                denied: vec![],
                allow_shell: true,
            },
            network_permissions: NetworkPermissions {
                allowed_urls: vec![],
                allow_outbound: false,
                allow_listen: false,
            },
            file_permissions: FilePermissions {
                read_paths: vec![".".to_string()],
                write_paths: vec![".".to_string()],
                allow_create: true,
                allow_delete: false,
            },
        },
        enabled: true,
    }
}

/// Create a daily report agent that runs at 9:00 AM UTC and summarizes project activity.
///
/// Uses a cron trigger to run on a fixed schedule.
pub fn daily_report_agent() -> ReactiveAgentDefinition {
    ReactiveAgentDefinition {
        id: "builtin:daily-report".to_string(),
        agent: AgentDefinition {
            id: "daily-report".to_string(),
            name: "Daily Report".to_string(),
            system_prompt: concat!(
                "You are a project activity reporter. Summarize what happened in the project ",
                "over the last 24 hours: commits, build status, test results, open issues, ",
                "and any notable changes. Keep the report concise and highlight anything ",
                "that needs attention."
            )
            .to_string(),
            skills: vec!["reporting".to_string(), "analysis".to_string()],
            tools: vec![
                "shell".to_string(),
                "file_read".to_string(),
                "file_search".to_string(),
            ],
            model_preference: None,
            execution_mode: ExecutionMode::Background {
                notify_on: vec![nexus_agents_core::definition::NotifyCondition::Completion],
            },
            max_iterations: 15,
            timeout_secs: 180,
        },
        task_template: "Generate a daily project activity report. Summarize commits, build status, \
                        test results, and any notable changes from the last 24 hours."
            .to_string(),
        triggers: vec![Trigger::Schedule {
            cron: "0 9 * * *".to_string(),
        }],
        lifecycle: ReactiveLifecycle::OnDemand,
        state_persistence: false,
        priority: Priority::Low,
        resources: ResourceAllocation {
            max_tokens: 30_000,
            max_cost_usd: 0.30,
            max_memory_bytes: 128 * 1024 * 1024,
            max_duration_secs: 180,
            max_iterations: 15,
            tool_permissions: ToolPermissions {
                allowed: vec![
                    "shell".to_string(),
                    "file_read".to_string(),
                    "file_search".to_string(),
                ],
                denied: vec![],
                allow_shell: true,
            },
            network_permissions: NetworkPermissions::default(),
            file_permissions: FilePermissions {
                read_paths: vec![".".to_string()],
                write_paths: vec![],
                allow_create: false,
                allow_delete: false,
            },
        },
        enabled: true,
    }
}

/// Create a security scanner agent that runs weekly to audit the codebase.
///
/// Runs every Sunday at 2:00 AM UTC via cron trigger.
pub fn security_scanner_agent() -> ReactiveAgentDefinition {
    ReactiveAgentDefinition {
        id: "builtin:security-scanner".to_string(),
        agent: AgentDefinition {
            id: "security-scanner".to_string(),
            name: "Security Scanner".to_string(),
            system_prompt: concat!(
                "You are a security auditor agent. Scan the codebase for common security ",
                "issues: hardcoded secrets, SQL injection vulnerabilities, insecure ",
                "dependencies, missing input validation, unsafe deserialization, and ",
                "exposed sensitive endpoints. Report findings with severity levels and ",
                "remediation steps."
            )
            .to_string(),
            skills: vec!["security".to_string(), "auditing".to_string()],
            tools: vec![
                "shell".to_string(),
                "file_read".to_string(),
                "file_search".to_string(),
            ],
            model_preference: None,
            execution_mode: ExecutionMode::Background {
                notify_on: vec![nexus_agents_core::definition::NotifyCondition::Completion],
            },
            max_iterations: 40,
            timeout_secs: 900,
        },
        task_template: "Run a security audit of the codebase. Check for hardcoded secrets, \
                        vulnerable dependencies, injection risks, and other common security \
                        issues. Produce a report with severity levels and remediation steps."
            .to_string(),
        triggers: vec![Trigger::Schedule {
            cron: "0 2 * * 0".to_string(),
        }],
        lifecycle: ReactiveLifecycle::OnDemand,
        state_persistence: false,
        priority: Priority::Normal,
        resources: ResourceAllocation {
            max_tokens: 100_000,
            max_cost_usd: 1.0,
            max_memory_bytes: 256 * 1024 * 1024,
            max_duration_secs: 900,
            max_iterations: 40,
            tool_permissions: ToolPermissions {
                allowed: vec![
                    "shell".to_string(),
                    "file_read".to_string(),
                    "file_search".to_string(),
                ],
                denied: vec![],
                allow_shell: true,
            },
            network_permissions: NetworkPermissions {
                allowed_urls: vec!["https://crates.io/*".to_string()],
                allow_outbound: true,
                allow_listen: false,
            },
            file_permissions: FilePermissions {
                read_paths: vec![".".to_string()],
                write_paths: vec![],
                allow_create: false,
                allow_delete: false,
            },
        },
        enabled: true,
    }
}

/// Create a dependency updater agent that checks for outdated dependencies weekly.
///
/// Runs every Wednesday at 3:00 AM UTC via cron trigger.
pub fn dependency_updater_agent() -> ReactiveAgentDefinition {
    ReactiveAgentDefinition {
        id: "builtin:dependency-updater".to_string(),
        agent: AgentDefinition {
            id: "dependency-updater".to_string(),
            name: "Dependency Updater".to_string(),
            system_prompt: concat!(
                "You are a dependency management agent. Check for outdated dependencies, ",
                "review changelogs for breaking changes, and recommend updates. For safe ",
                "minor/patch updates, prepare the update. For major updates, provide a ",
                "migration assessment."
            )
            .to_string(),
            skills: vec![
                "dependency_management".to_string(),
                "analysis".to_string(),
            ],
            tools: vec![
                "shell".to_string(),
                "file_read".to_string(),
                "file_write".to_string(),
                "file_search".to_string(),
            ],
            model_preference: None,
            execution_mode: ExecutionMode::Background {
                notify_on: vec![nexus_agents_core::definition::NotifyCondition::Completion],
            },
            max_iterations: 25,
            timeout_secs: 600,
        },
        task_template: "Check for outdated dependencies. For each outdated dependency, assess \
                        the risk of updating, review the changelog, and recommend whether to \
                        update now or defer."
            .to_string(),
        triggers: vec![Trigger::Schedule {
            cron: "0 3 * * 3".to_string(),
        }],
        lifecycle: ReactiveLifecycle::OnDemand,
        state_persistence: false,
        priority: Priority::Low,
        resources: ResourceAllocation {
            max_tokens: 50_000,
            max_cost_usd: 0.50,
            max_memory_bytes: 128 * 1024 * 1024,
            max_duration_secs: 600,
            max_iterations: 25,
            tool_permissions: ToolPermissions {
                allowed: vec![
                    "shell".to_string(),
                    "file_read".to_string(),
                    "file_write".to_string(),
                    "file_search".to_string(),
                ],
                denied: vec![],
                allow_shell: true,
            },
            network_permissions: NetworkPermissions {
                allowed_urls: vec![
                    "https://crates.io/*".to_string(),
                    "https://registry.npmjs.org/*".to_string(),
                    "https://pypi.org/*".to_string(),
                ],
                allow_outbound: true,
                allow_listen: false,
            },
            file_permissions: FilePermissions {
                read_paths: vec![".".to_string()],
                write_paths: vec![".".to_string()],
                allow_create: false,
                allow_delete: false,
            },
        },
        enabled: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_watcher_agent_creation() {
        let agent = test_watcher_agent("/tmp/project");
        assert_eq!(agent.id, "builtin:test-watcher");
        assert!(agent.enabled);
        assert_eq!(agent.triggers.len(), 1);
        match &agent.triggers[0] {
            Trigger::FileChange { paths, events } => {
                assert_eq!(paths.len(), 2);
                assert!(paths[0].contains("/tmp/project"));
                assert!(events.contains(&FileEvent::Modified));
            }
            _ => panic!("expected FileChange trigger"),
        }
    }

    #[test]
    fn test_build_monitor_creation() {
        let agent = build_monitor_agent();
        assert_eq!(agent.id, "builtin:build-monitor");
        assert_eq!(agent.priority, Priority::High);
        match &agent.triggers[0] {
            Trigger::Event {
                channel,
                event_type_filter,
                debounce_ms,
            } => {
                assert_eq!(channel, "project.*.build");
                assert_eq!(event_type_filter.as_deref(), Some("failed"));
                assert_eq!(*debounce_ms, Some(5000));
            }
            _ => panic!("expected Event trigger"),
        }
    }

    #[test]
    fn test_daily_report_creation() {
        let agent = daily_report_agent();
        assert_eq!(agent.id, "builtin:daily-report");
        assert_eq!(agent.priority, Priority::Low);
        match &agent.triggers[0] {
            Trigger::Schedule { cron } => {
                assert_eq!(cron, "0 9 * * *");
            }
            _ => panic!("expected Schedule trigger"),
        }
    }

    #[test]
    fn test_security_scanner_creation() {
        let agent = security_scanner_agent();
        assert_eq!(agent.id, "builtin:security-scanner");
        assert!(agent.resources.network_permissions.allow_outbound);
        match &agent.triggers[0] {
            Trigger::Schedule { cron } => {
                assert_eq!(cron, "0 2 * * 0");
            }
            _ => panic!("expected Schedule trigger"),
        }
    }

    #[test]
    fn test_dependency_updater_creation() {
        let agent = dependency_updater_agent();
        assert_eq!(agent.id, "builtin:dependency-updater");
        assert!(agent
            .resources
            .network_permissions
            .allowed_urls
            .iter()
            .any(|u| u.contains("crates.io")));
        match &agent.triggers[0] {
            Trigger::Schedule { cron } => {
                assert_eq!(cron, "0 3 * * 3");
            }
            _ => panic!("expected Schedule trigger"),
        }
    }

    #[test]
    fn test_all_builtins_are_serializable() {
        let agents: Vec<ReactiveAgentDefinition> = vec![
            test_watcher_agent("/tmp"),
            build_monitor_agent(),
            daily_report_agent(),
            security_scanner_agent(),
            dependency_updater_agent(),
        ];
        for agent in agents {
            let json = serde_json::to_string(&agent).unwrap();
            let _: ReactiveAgentDefinition = serde_json::from_str(&json).unwrap();
        }
    }
}
