//! Reactive Agent System — Layer 3 of the Nexus Agent OS.
//!
//! Reactive agents are agent definitions paired with triggers that cause them to
//! spawn automatically. Triggers include cron schedules, event patterns, file
//! changes, webhooks, and process completion events.

use serde::{Deserialize, Serialize};

use crate::process::{Priority, ResourceAllocation};
use nexus_agents_core::definition::AgentDefinition;

/// A file system event type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileEvent {
    /// A file was created.
    Created,
    /// A file was modified.
    Modified,
    /// A file was deleted.
    Deleted,
}

/// A trigger that causes a reactive agent to spawn.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Trigger {
    /// Spawn on a cron schedule.
    Schedule {
        /// Cron expression (5-field standard format).
        cron: String,
    },
    /// Spawn when an event matching the pattern is published.
    Event {
        /// Channel pattern to subscribe to (supports glob).
        channel: String,
        /// Optional event type filter.
        event_type_filter: Option<String>,
        /// Debounce duration in milliseconds. If multiple matching events arrive
        /// within this window, only one agent spawn occurs.
        debounce_ms: Option<u64>,
    },
    /// Spawn when watched files change.
    FileChange {
        /// File paths or glob patterns to watch.
        paths: Vec<String>,
        /// Which file events to react to.
        events: Vec<FileEvent>,
    },
    /// Spawn when a webhook is received at the given path.
    Webhook {
        /// The URL path to listen on (e.g., "/hooks/deploy").
        path: String,
    },
    /// Spawn when a specific agent completes.
    AgentCompleted {
        /// The agent definition ID to watch for.
        agent_id: String,
    },
    /// Spawn once when the system starts.
    OnStartup,
    /// Run continuously as a daemon (respawned if it exits).
    Persistent,
}

/// Lifecycle management mode for a reactive agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ReactiveLifecycle {
    /// Spawn only when triggered, terminate when done.
    OnDemand,
    /// Run continuously, restart on exit.
    Daemon,
    /// Run as a service with a configurable restart policy.
    Service {
        /// Restart policy for the service.
        restart_policy: RestartPolicy,
    },
}

#[allow(clippy::derivable_impls)]
impl Default for ReactiveLifecycle {
    fn default() -> Self {
        Self::OnDemand
    }
}

/// Restart policy for service-mode reactive agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "policy", rename_all = "snake_case")]
pub enum RestartPolicy {
    /// Always restart when the process exits.
    Always,
    /// Restart on failure, up to `max_restarts` within `window_secs`.
    OnFailure {
        /// Maximum number of restarts allowed within the window.
        max_restarts: u32,
        /// Time window in seconds for counting restarts.
        window_secs: u64,
    },
    /// Never restart.
    Never,
}

/// A reactive agent definition — an agent paired with its triggers and lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactiveAgentDefinition {
    /// Unique identifier for this reactive agent definition.
    pub id: String,
    /// The agent definition to spawn.
    pub agent: AgentDefinition,
    /// The task/prompt to give the agent when spawned.
    pub task_template: String,
    /// Triggers that cause the agent to spawn.
    pub triggers: Vec<Trigger>,
    /// Lifecycle management mode.
    pub lifecycle: ReactiveLifecycle,
    /// Whether state should be persisted across restarts.
    pub state_persistence: bool,
    /// Scheduling priority for spawned processes.
    pub priority: Priority,
    /// Resource allocation for spawned processes.
    pub resources: ResourceAllocation,
    /// Whether this reactive definition is currently enabled.
    pub enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_serialization() {
        let trigger = Trigger::Schedule {
            cron: "0 9 * * *".to_string(),
        };
        let json = serde_json::to_string(&trigger).unwrap();
        let deserialized: Trigger = serde_json::from_str(&json).unwrap();
        match deserialized {
            Trigger::Schedule { cron } => assert_eq!(cron, "0 9 * * *"),
            _ => panic!("expected Schedule trigger"),
        }
    }

    #[test]
    fn test_lifecycle_default() {
        let lifecycle = ReactiveLifecycle::default();
        match lifecycle {
            ReactiveLifecycle::OnDemand => {}
            _ => panic!("expected OnDemand"),
        }
    }

    #[test]
    fn test_restart_policy_serialization() {
        let policy = RestartPolicy::OnFailure {
            max_restarts: 3,
            window_secs: 300,
        };
        let json = serde_json::to_string(&policy).unwrap();
        assert!(json.contains("on_failure"));
        let deserialized: RestartPolicy = serde_json::from_str(&json).unwrap();
        match deserialized {
            RestartPolicy::OnFailure {
                max_restarts,
                window_secs,
            } => {
                assert_eq!(max_restarts, 3);
                assert_eq!(window_secs, 300);
            }
            _ => panic!("expected OnFailure"),
        }
    }
}
