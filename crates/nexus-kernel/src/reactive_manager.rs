//! Reactive Agent Manager — orchestrates trigger registration and agent spawning.
//!
//! The manager holds reactive agent definitions and wires up event bus subscriptions
//! for event-based triggers. When a trigger fires, it spawns the agent via the scheduler.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::events::{EventBus, ProcessEvent, Subscription, SubscriptionHandler};
use crate::process::ProcessError;
use crate::reactive::{ReactiveAgentDefinition, ReactiveLifecycle, Trigger};
use crate::scheduler::AgentScheduler;

/// Error type for reactive manager operations.
#[derive(Debug, thiserror::Error)]
pub enum ReactiveError {
    #[error("reactive agent definition {id} not found")]
    NotFound { id: String },

    #[error("reactive agent definition {id} already exists")]
    AlreadyExists { id: String },

    #[error("process error: {0}")]
    Process(#[from] ProcessError),

    #[error("manager error: {0}")]
    Internal(String),
}

/// Tracks subscriptions created for a reactive definition.
struct DefinitionState {
    definition: ReactiveAgentDefinition,
    /// Event bus subscription IDs created for this definition.
    subscription_ids: Vec<String>,
}

/// The reactive agent manager — registers definitions, sets up triggers, spawns agents.
pub struct ReactiveAgentManager {
    scheduler: Arc<AgentScheduler>,
    event_bus: Arc<EventBus>,
    definitions: Arc<RwLock<HashMap<String, DefinitionState>>>,
}

impl ReactiveAgentManager {
    /// Create a new reactive agent manager.
    pub fn new(scheduler: Arc<AgentScheduler>, event_bus: Arc<EventBus>) -> Self {
        Self {
            scheduler,
            event_bus,
            definitions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a reactive agent definition. Sets up event bus subscriptions for
    /// event-based triggers.
    pub async fn register(
        &self,
        definition: ReactiveAgentDefinition,
    ) -> Result<String, ReactiveError> {
        let id = definition.id.clone();

        {
            let defs = self.definitions.read().await;
            if defs.contains_key(&id) {
                return Err(ReactiveError::AlreadyExists { id });
            }
        }

        let mut subscription_ids = Vec::new();

        if definition.enabled {
            subscription_ids = self.setup_triggers(&definition).await;
        }

        {
            let mut defs = self.definitions.write().await;
            defs.insert(
                id.clone(),
                DefinitionState {
                    definition,
                    subscription_ids,
                },
            );
        }

        info!(id = %id, "Reactive agent definition registered");
        Ok(id)
    }

    /// Unregister a reactive agent definition. Removes all event bus subscriptions.
    pub async fn unregister(&self, id: &str) -> Result<ReactiveAgentDefinition, ReactiveError> {
        let state = {
            let mut defs = self.definitions.write().await;
            defs.remove(id)
                .ok_or_else(|| ReactiveError::NotFound { id: id.to_string() })?
        };

        // Remove all subscriptions.
        for sub_id in &state.subscription_ids {
            self.event_bus.unsubscribe(sub_id).await;
        }

        info!(id = %id, "Reactive agent definition unregistered");
        Ok(state.definition)
    }

    /// List all registered reactive agent definitions.
    pub async fn list(&self) -> Vec<ReactiveAgentDefinition> {
        let defs = self.definitions.read().await;
        defs.values().map(|s| s.definition.clone()).collect()
    }

    /// Enable a reactive agent definition. Sets up its triggers.
    pub async fn enable(&self, id: &str) -> Result<(), ReactiveError> {
        let mut defs = self.definitions.write().await;
        let state = defs
            .get_mut(id)
            .ok_or_else(|| ReactiveError::NotFound { id: id.to_string() })?;

        if state.definition.enabled {
            return Ok(());
        }

        state.definition.enabled = true;
        let new_subs = self.setup_triggers(&state.definition).await;
        state.subscription_ids = new_subs;

        info!(id = %id, "Reactive agent definition enabled");
        Ok(())
    }

    /// Disable a reactive agent definition. Removes its triggers.
    pub async fn disable(&self, id: &str) -> Result<(), ReactiveError> {
        let mut defs = self.definitions.write().await;
        let state = defs
            .get_mut(id)
            .ok_or_else(|| ReactiveError::NotFound { id: id.to_string() })?;

        if !state.definition.enabled {
            return Ok(());
        }

        state.definition.enabled = false;

        // Remove all subscriptions.
        for sub_id in state.subscription_ids.drain(..) {
            self.event_bus.unsubscribe(&sub_id).await;
        }

        info!(id = %id, "Reactive agent definition disabled");
        Ok(())
    }

    /// Handle a trigger firing for a specific definition. Spawns the agent.
    pub async fn handle_trigger(
        &self,
        definition_id: &str,
        trigger_context: Option<serde_json::Value>,
    ) -> Result<String, ReactiveError> {
        let def = {
            let defs = self.definitions.read().await;
            let state = defs
                .get(definition_id)
                .ok_or_else(|| ReactiveError::NotFound {
                    id: definition_id.to_string(),
                })?;
            if !state.definition.enabled {
                return Err(ReactiveError::Internal(format!(
                    "reactive agent {} is disabled",
                    definition_id
                )));
            }
            state.definition.clone()
        };

        // Build the task string, optionally including trigger context.
        let task = if let Some(ctx) = trigger_context {
            format!(
                "{}\n\nTrigger context:\n{}",
                def.task_template,
                serde_json::to_string_pretty(&ctx).unwrap_or_default()
            )
        } else {
            def.task_template.clone()
        };

        let pid = self
            .scheduler
            .spawn(
                def.agent.clone(),
                task,
                def.priority,
                def.resources.clone(),
                None,
            )
            .await?;

        info!(
            definition_id = %definition_id,
            pid = %pid,
            "Reactive agent spawned"
        );

        Ok(pid)
    }

    /// Start the background event listener that monitors process events and fires
    /// `AgentCompleted` triggers. Call this once at startup and hold the returned
    /// `JoinHandle`.
    pub fn start_process_event_listener(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let manager = Arc::clone(self);
        let mut rx = manager.scheduler.subscribe();

        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(ProcessEvent::Completed { agent_id, .. }) => {
                        manager.on_agent_completed(&agent_id).await;
                    }
                    Ok(ProcessEvent::Failed { agent_id, .. }) => {
                        manager.on_agent_failed(&agent_id).await;
                    }
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        warn!("Reactive manager event listener lagged by {} events", n);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        info!("Reactive manager event listener stopping — channel closed");
                        break;
                    }
                }
            }
        })
    }

    // ── Internal ────────────────────────────────────────────────────────

    /// Set up event bus subscriptions for the definition's triggers.
    async fn setup_triggers(&self, def: &ReactiveAgentDefinition) -> Vec<String> {
        let mut sub_ids = Vec::new();

        for trigger in &def.triggers {
            match trigger {
                Trigger::Event {
                    channel,
                    event_type_filter,
                    ..
                } => {
                    let sub = Subscription::new(
                        channel.clone(),
                        event_type_filter.clone(),
                        SubscriptionHandler::Callback {
                            name: format!("reactive:{}", def.id),
                        },
                    );
                    let id = self.event_bus.subscribe(sub).await;
                    sub_ids.push(id);
                }
                Trigger::Schedule { cron } => {
                    // Cron scheduling is tracked but actual timer setup happens
                    // in the runtime layer. We store a marker subscription.
                    let sub = Subscription::new(
                        format!("system.cron.{}", def.id),
                        Some("tick".to_string()),
                        SubscriptionHandler::Callback {
                            name: format!("reactive:cron:{}", def.id),
                        },
                    );
                    let id = self.event_bus.subscribe(sub).await;
                    sub_ids.push(id);
                    info!(
                        definition_id = %def.id,
                        cron = %cron,
                        "Cron trigger registered (timer setup deferred to runtime)"
                    );
                }
                Trigger::FileChange { paths, events } => {
                    let sub = Subscription::new(
                        format!("system.files.{}", def.id),
                        None,
                        SubscriptionHandler::Callback {
                            name: format!("reactive:files:{}", def.id),
                        },
                    );
                    let id = self.event_bus.subscribe(sub).await;
                    sub_ids.push(id);
                    info!(
                        definition_id = %def.id,
                        paths = ?paths,
                        events = ?events,
                        "File change trigger registered (watcher setup deferred to runtime)"
                    );
                }
                Trigger::Webhook { path } => {
                    let sub = Subscription::new(
                        format!("system.webhook.{}", def.id),
                        None,
                        SubscriptionHandler::Callback {
                            name: format!("reactive:webhook:{}", def.id),
                        },
                    );
                    let id = self.event_bus.subscribe(sub).await;
                    sub_ids.push(id);
                    info!(
                        definition_id = %def.id,
                        path = %path,
                        "Webhook trigger registered"
                    );
                }
                Trigger::AgentCompleted { agent_id } => {
                    info!(
                        definition_id = %def.id,
                        watched_agent_id = %agent_id,
                        "AgentCompleted trigger registered"
                    );
                }
                Trigger::OnStartup => {
                    info!(
                        definition_id = %def.id,
                        "OnStartup trigger registered (will fire on system start)"
                    );
                }
                Trigger::Persistent => {
                    info!(
                        definition_id = %def.id,
                        "Persistent trigger registered (daemon mode)"
                    );
                }
            }
        }

        sub_ids
    }

    /// Called when an agent completes. Check if any reactive definition has an
    /// `AgentCompleted` trigger for it.
    async fn on_agent_completed(&self, completed_agent_id: &str) {
        let matching_defs: Vec<String> = {
            let defs = self.definitions.read().await;
            defs.values()
                .filter(|state| {
                    state.definition.enabled
                        && state.definition.triggers.iter().any(|t| {
                            matches!(t, Trigger::AgentCompleted { agent_id } if agent_id == completed_agent_id)
                        })
                })
                .map(|state| state.definition.id.clone())
                .collect()
        };

        for def_id in matching_defs {
            let ctx = serde_json::json!({
                "trigger": "agent_completed",
                "completed_agent_id": completed_agent_id,
            });
            if let Err(e) = self.handle_trigger(&def_id, Some(ctx)).await {
                warn!(
                    definition_id = %def_id,
                    error = %e,
                    "Failed to handle AgentCompleted trigger"
                );
            }
        }
    }

    /// Called when an agent fails. For Service/Daemon lifecycle, apply restart policy.
    async fn on_agent_failed(&self, failed_agent_id: &str) {
        let matching_defs: Vec<(String, ReactiveLifecycle)> = {
            let defs = self.definitions.read().await;
            defs.values()
                .filter(|state| {
                    state.definition.enabled && state.definition.agent.id == failed_agent_id
                })
                .map(|state| {
                    (
                        state.definition.id.clone(),
                        state.definition.lifecycle.clone(),
                    )
                })
                .collect()
        };

        for (def_id, lifecycle) in matching_defs {
            let should_restart = match &lifecycle {
                ReactiveLifecycle::Daemon => true,
                ReactiveLifecycle::Service {
                    restart_policy: crate::reactive::RestartPolicy::Always,
                } => true,
                ReactiveLifecycle::Service {
                    restart_policy:
                        crate::reactive::RestartPolicy::OnFailure { .. },
                } => {
                    // Full windowed restart tracking would be implemented in the runtime.
                    true
                }
                _ => false,
            };

            if should_restart {
                info!(definition_id = %def_id, "Agent failed — respawning per lifecycle policy");
                if let Err(e) = self.handle_trigger(&def_id, None).await {
                    warn!(
                        definition_id = %def_id,
                        error = %e,
                        "Failed to respawn agent after failure"
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EventBusConfig;
    use crate::process::{Priority, ResourceAllocation};
    use crate::reactive::{ReactiveAgentDefinition, ReactiveLifecycle, Trigger};
    use crate::scheduler::SchedulerConfig;
    use nexus_agents_core::definition::{AgentDefinition, ExecutionMode};

    fn test_agent(id: &str) -> AgentDefinition {
        AgentDefinition {
            id: id.to_string(),
            name: format!("Test-{}", id),
            system_prompt: "Test".to_string(),
            skills: vec![],
            tools: vec![],
            model_preference: None,
            execution_mode: ExecutionMode::Interactive,
            max_iterations: 10,
            timeout_secs: 60,
        }
    }

    fn test_reactive_def(id: &str, triggers: Vec<Trigger>) -> ReactiveAgentDefinition {
        ReactiveAgentDefinition {
            id: id.to_string(),
            agent: test_agent(id),
            task_template: "Run the task".to_string(),
            triggers,
            lifecycle: ReactiveLifecycle::OnDemand,
            state_persistence: false,
            priority: Priority::Normal,
            resources: ResourceAllocation::default(),
            enabled: true,
        }
    }

    #[tokio::test]
    async fn test_register_and_list() {
        let scheduler = Arc::new(AgentScheduler::new(SchedulerConfig::default()));
        let bus = Arc::new(EventBus::new(EventBusConfig::default()));
        let manager = ReactiveAgentManager::new(scheduler, bus);

        let def = test_reactive_def(
            "watcher",
            vec![Trigger::Event {
                channel: "project.*.build".to_string(),
                event_type_filter: None,
                debounce_ms: None,
            }],
        );

        manager.register(def).await.unwrap();
        let list = manager.list().await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "watcher");
    }

    #[tokio::test]
    async fn test_register_duplicate() {
        let scheduler = Arc::new(AgentScheduler::new(SchedulerConfig::default()));
        let bus = Arc::new(EventBus::new(EventBusConfig::default()));
        let manager = ReactiveAgentManager::new(scheduler, bus);

        let def = test_reactive_def("dup", vec![Trigger::OnStartup]);
        manager.register(def.clone()).await.unwrap();
        let result = manager.register(def).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_unregister() {
        let scheduler = Arc::new(AgentScheduler::new(SchedulerConfig::default()));
        let bus = Arc::new(EventBus::new(EventBusConfig::default()));
        let manager = ReactiveAgentManager::new(scheduler, bus);

        let def = test_reactive_def("to_remove", vec![Trigger::OnStartup]);
        manager.register(def).await.unwrap();
        manager.unregister("to_remove").await.unwrap();
        assert!(manager.list().await.is_empty());
    }

    #[tokio::test]
    async fn test_enable_disable() {
        let scheduler = Arc::new(AgentScheduler::new(SchedulerConfig::default()));
        let bus = Arc::new(EventBus::new(EventBusConfig::default()));
        let manager = ReactiveAgentManager::new(scheduler, bus.clone());

        let mut def = test_reactive_def(
            "toggleable",
            vec![Trigger::Event {
                channel: "test.*".to_string(),
                event_type_filter: None,
                debounce_ms: None,
            }],
        );
        def.enabled = false;
        manager.register(def).await.unwrap();

        // Should have no subscriptions since disabled.
        assert_eq!(bus.subscription_count().await, 0);

        manager.enable("toggleable").await.unwrap();
        assert_eq!(bus.subscription_count().await, 1);

        manager.disable("toggleable").await.unwrap();
        assert_eq!(bus.subscription_count().await, 0);
    }

    #[tokio::test]
    async fn test_handle_trigger_spawns_agent() {
        let scheduler = Arc::new(AgentScheduler::new(SchedulerConfig::default()));
        let bus = Arc::new(EventBus::new(EventBusConfig::default()));
        let manager = ReactiveAgentManager::new(scheduler.clone(), bus);

        let def = test_reactive_def("spawner", vec![Trigger::OnStartup]);
        manager.register(def).await.unwrap();

        let pid = manager.handle_trigger("spawner", None).await.unwrap();
        let info = scheduler.get_process(&pid).await.unwrap();
        assert_eq!(info.agent_id, "spawner");
    }

    #[tokio::test]
    async fn test_handle_trigger_with_context() {
        let scheduler = Arc::new(AgentScheduler::new(SchedulerConfig::default()));
        let bus = Arc::new(EventBus::new(EventBusConfig::default()));
        let manager = ReactiveAgentManager::new(scheduler.clone(), bus);

        let def = test_reactive_def("ctx_agent", vec![Trigger::OnStartup]);
        manager.register(def).await.unwrap();

        let ctx = serde_json::json!({"file": "main.rs", "event": "modified"});
        let pid = manager.handle_trigger("ctx_agent", Some(ctx)).await.unwrap();
        let info = scheduler.get_process(&pid).await.unwrap();
        assert!(info.task.contains("Trigger context"));
    }
}
