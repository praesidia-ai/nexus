//! Process events and System Event Bus — Layer 2 of the Nexus Agent OS.
//!
//! The event bus provides a publish/subscribe messaging system with:
//! - Channel-based routing with glob pattern matching
//! - Per-channel broadcast senders for high throughput
//! - Event history ring buffer for late subscribers
//! - Typed subscription handlers (wake agent, deliver, webhook, callback)

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::process::{ProcessId, ProcessState, Priority};

// ── Process Events ──────────────────────────────────────────────────────────

/// Events emitted during a process's lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ProcessEvent {
    /// A new process was spawned.
    Spawned {
        pid: ProcessId,
        agent_id: String,
        agent_name: String,
        parent: Option<ProcessId>,
        priority: Priority,
    },
    /// A process started executing.
    Started {
        pid: ProcessId,
        agent_id: String,
    },
    /// A process changed state.
    StateChanged {
        pid: ProcessId,
        from: ProcessState,
        to: ProcessState,
    },
    /// A process completed successfully.
    Completed {
        pid: ProcessId,
        agent_id: String,
        output: Option<String>,
        duration_secs: f64,
        tokens_used: u64,
        cost_used: f64,
    },
    /// A process failed.
    Failed {
        pid: ProcessId,
        agent_id: String,
        error: String,
        duration_secs: f64,
    },
    /// A process was killed.
    Killed {
        pid: ProcessId,
        agent_id: String,
        reason: String,
    },
    /// A process's priority was changed.
    PriorityChanged {
        pid: ProcessId,
        from: Priority,
        to: Priority,
    },
    /// A process was suspended.
    Suspended {
        pid: ProcessId,
        agent_id: String,
    },
    /// A process was resumed.
    Resumed {
        pid: ProcessId,
        agent_id: String,
    },
    /// A process was preempted by a higher-priority process.
    Preempted {
        pid: ProcessId,
        agent_id: String,
        preempted_by: ProcessId,
    },
    /// A process health status changed.
    HealthChanged {
        pid: ProcessId,
        agent_id: String,
        status: String,
    },
    /// A process exceeded its resource budget.
    ResourceBudgetWarning {
        pid: ProcessId,
        agent_id: String,
        resource: String,
        used: u64,
        limit: u64,
    },
    /// A process group was cancelled.
    GroupCancelled {
        group_id: String,
        group_name: String,
        cancelled_pids: Vec<ProcessId>,
    },
}

// ── System Events ───────────────────────────────────────────────────────────

/// Source of a system event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSource {
    /// Event originated from an agent process.
    Agent { pid: ProcessId, agent_id: String },
    /// Event originated from the system (scheduler, kernel, etc.).
    System { component: String },
    /// Event originated from a user action.
    User { user_id: String },
    /// Event originated from a tool execution.
    Tool { tool_name: String, pid: ProcessId },
    /// Event originated from an external system (webhook, API, etc.).
    External { source: String },
}

/// A system event published to the event bus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemEvent {
    /// Unique event identifier.
    pub id: String,
    /// The channel this event was published to (dot-separated path).
    pub channel: String,
    /// The event type (application-defined).
    pub event_type: String,
    /// The event payload.
    pub payload: serde_json::Value,
    /// Who/what generated this event.
    pub source: EventSource,
    /// When the event was created.
    pub timestamp: DateTime<Utc>,
}

impl SystemEvent {
    /// Create a new system event.
    pub fn new(
        channel: impl Into<String>,
        event_type: impl Into<String>,
        payload: serde_json::Value,
        source: EventSource,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            channel: channel.into(),
            event_type: event_type.into(),
            payload,
            source,
            timestamp: Utc::now(),
        }
    }
}

// ── Subscriptions ───────────────────────────────────────────────────────────

/// What to do when a matching event arrives.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum SubscriptionHandler {
    /// Wake a sleeping agent process.
    WakeAgent { pid: ProcessId },
    /// Deliver the event to an agent's message inbox.
    DeliverToAgent { pid: ProcessId },
    /// Forward the event to a webhook URL.
    Webhook { url: String },
    /// Forward the event to another channel.
    Forward { target_channel: String },
    /// Invoke a named callback (resolved at runtime by the kernel).
    Callback { name: String },
}

/// A subscription to events matching a channel pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    /// Unique subscription identifier.
    pub id: String,
    /// Glob pattern for matching channels (e.g., "project.*.files.*").
    pub channel_pattern: String,
    /// Optional filter on event_type.
    pub event_type_filter: Option<String>,
    /// What to do when a matching event arrives.
    pub handler: SubscriptionHandler,
    /// When this subscription was created.
    pub created_at: DateTime<Utc>,
}

impl Subscription {
    /// Create a new subscription.
    pub fn new(
        channel_pattern: impl Into<String>,
        event_type_filter: Option<String>,
        handler: SubscriptionHandler,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            channel_pattern: channel_pattern.into(),
            event_type_filter,
            handler,
            created_at: Utc::now(),
        }
    }

    /// Check if this subscription matches the given channel and event type.
    pub fn matches(&self, channel: &str, event_type: &str) -> bool {
        if !glob_match(&self.channel_pattern, channel) {
            return false;
        }
        if let Some(ref filter) = self.event_type_filter {
            if !glob_match(filter, event_type) {
                return false;
            }
        }
        true
    }
}

/// Simple glob matching supporting `*` (single segment) and `**` (any number of segments).
///
/// Segments are separated by `.` (dot). Examples:
/// - `"project.*.files"` matches `"project.abc.files"` but not `"project.abc.def.files"`
/// - `"project.**"` matches `"project.abc"`, `"project.abc.def"`, etc.
/// - `"*"` matches any single segment
pub fn glob_match(pattern: &str, input: &str) -> bool {
    let pattern_parts: Vec<&str> = pattern.split('.').collect();
    let input_parts: Vec<&str> = input.split('.').collect();
    glob_match_recursive(&pattern_parts, &input_parts)
}

fn glob_match_recursive(pattern: &[&str], input: &[&str]) -> bool {
    match (pattern.first(), input.first()) {
        // Both exhausted — match.
        (None, None) => true,
        // Pattern has `**` — try consuming zero or more input segments.
        (Some(&"**"), _) => {
            // Try matching the rest of the pattern against every suffix of input.
            for i in 0..=input.len() {
                if glob_match_recursive(&pattern[1..], &input[i..]) {
                    return true;
                }
            }
            false
        }
        // Pattern exhausted but input remains — no match.
        (None, Some(_)) => false,
        // Input exhausted but pattern remains — no match (unless pattern is all **).
        (Some(_), None) => pattern.iter().all(|&p| p == "**"),
        // Both have segments — check current segment.
        (Some(&"*"), Some(_)) => glob_match_recursive(&pattern[1..], &input[1..]),
        (Some(p), Some(i)) => {
            if *p == *i {
                glob_match_recursive(&pattern[1..], &input[1..])
            } else {
                false
            }
        }
    }
}

// ── Event Bus ───────────────────────────────────────────────────────────────

/// Configuration for the event bus.
#[derive(Debug, Clone)]
pub struct EventBusConfig {
    /// Maximum number of events to keep in the history ring buffer per channel.
    pub history_capacity: usize,
    /// Broadcast channel capacity (how many unread events before lagging).
    pub broadcast_capacity: usize,
}

impl Default for EventBusConfig {
    fn default() -> Self {
        Self {
            history_capacity: 1000,
            broadcast_capacity: 256,
        }
    }
}

/// The system event bus — publish/subscribe with channel-based routing.
pub struct EventBus {
    config: EventBusConfig,
    /// Broadcast sender for all events (subscribers receive via cloned receivers).
    sender: broadcast::Sender<SystemEvent>,
    /// Per-subscription registry.
    subscriptions: Arc<RwLock<HashMap<String, Subscription>>>,
    /// Event history ring buffer (most recent events across all channels).
    history: Arc<RwLock<Vec<SystemEvent>>>,
}

impl EventBus {
    /// Create a new event bus with the given configuration.
    pub fn new(config: EventBusConfig) -> Self {
        let (sender, _) = broadcast::channel(config.broadcast_capacity);
        Self {
            config,
            sender,
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Publish an event to the bus. Returns the list of subscription IDs that matched.
    pub async fn publish(&self, event: SystemEvent) -> Vec<String> {
        // Store in history
        {
            let mut history = self.history.write().await;
            history.push(event.clone());
            if history.len() > self.config.history_capacity {
                let drain_count = history.len() - self.config.history_capacity;
                history.drain(..drain_count);
            }
        }

        // Find matching subscriptions
        let matched: Vec<String> = {
            let subs = self.subscriptions.read().await;
            subs.values()
                .filter(|s| s.matches(&event.channel, &event.event_type))
                .map(|s| s.id.clone())
                .collect()
        };

        // Broadcast to all receivers (ignore send errors — means no active receivers)
        let _ = self.sender.send(event);

        matched
    }

    /// Subscribe to events matching the given pattern. Returns the subscription ID.
    pub async fn subscribe(&self, subscription: Subscription) -> String {
        let id = subscription.id.clone();
        let mut subs = self.subscriptions.write().await;
        subs.insert(id.clone(), subscription);
        id
    }

    /// Remove a subscription by ID. Returns the removed subscription if it existed.
    pub async fn unsubscribe(&self, subscription_id: &str) -> Option<Subscription> {
        let mut subs = self.subscriptions.write().await;
        subs.remove(subscription_id)
    }

    /// Get recent events, optionally filtered by channel pattern.
    pub async fn recent_events(
        &self,
        channel_pattern: Option<&str>,
        limit: usize,
    ) -> Vec<SystemEvent> {
        let history = self.history.read().await;
        let iter = history.iter().rev();

        match channel_pattern {
            Some(pattern) => iter
                .filter(|e| glob_match(pattern, &e.channel))
                .take(limit)
                .cloned()
                .collect(),
            None => iter.take(limit).cloned().collect(),
        }
    }

    /// Wait for an event matching the given channel pattern and optional event type filter.
    /// Returns the first matching event, or `None` if the timeout is reached.
    pub async fn wait_for(
        &self,
        channel_pattern: &str,
        event_type_filter: Option<&str>,
        timeout: std::time::Duration,
    ) -> Option<SystemEvent> {
        let mut receiver = self.sender.subscribe();
        let pattern = channel_pattern.to_string();
        let type_filter = event_type_filter.map(|s| s.to_string());

        tokio::select! {
            result = async {
                loop {
                    match receiver.recv().await {
                        Ok(event) => {
                            if !glob_match(&pattern, &event.channel) {
                                continue;
                            }
                            if let Some(ref filter) = type_filter {
                                if !glob_match(filter, &event.event_type) {
                                    continue;
                                }
                            }
                            return Some(event);
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(broadcast::error::RecvError::Closed) => return None,
                    }
                }
            } => result,
            _ = tokio::time::sleep(timeout) => None,
        }
    }

    /// Get a broadcast receiver for all events. Callers filter events themselves.
    pub fn receiver(&self) -> broadcast::Receiver<SystemEvent> {
        self.sender.subscribe()
    }

    /// Get the number of active subscriptions.
    pub async fn subscription_count(&self) -> usize {
        self.subscriptions.read().await.len()
    }

    /// List all subscriptions.
    pub async fn list_subscriptions(&self) -> Vec<Subscription> {
        self.subscriptions.read().await.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_match_exact() {
        assert!(glob_match("project.files", "project.files"));
        assert!(!glob_match("project.files", "project.tests"));
    }

    #[test]
    fn test_glob_match_single_wildcard() {
        assert!(glob_match("project.*.files", "project.abc.files"));
        assert!(!glob_match("project.*.files", "project.abc.def.files"));
        assert!(glob_match("*.files", "project.files"));
    }

    #[test]
    fn test_glob_match_double_wildcard() {
        assert!(glob_match("project.**", "project.abc"));
        assert!(glob_match("project.**", "project.abc.def"));
        assert!(glob_match("project.**", "project.abc.def.ghi"));
        assert!(glob_match("**", "anything.at.all"));
        assert!(glob_match("project.**.files", "project.abc.files"));
        assert!(glob_match("project.**.files", "project.abc.def.files"));
    }

    #[test]
    fn test_glob_match_no_match() {
        assert!(!glob_match("project.files", "other.files"));
        assert!(!glob_match("project.*", "project.abc.def"));
    }

    #[test]
    fn test_glob_match_empty() {
        assert!(glob_match("", ""));
        assert!(!glob_match("", "something"));
        assert!(!glob_match("something", ""));
    }

    #[test]
    fn test_subscription_matches() {
        let sub = Subscription::new(
            "project.*.build",
            Some("failed".to_string()),
            SubscriptionHandler::Callback { name: "on_build_fail".to_string() },
        );
        assert!(sub.matches("project.myapp.build", "failed"));
        assert!(!sub.matches("project.myapp.build", "succeeded"));
        assert!(!sub.matches("project.myapp.test", "failed"));
    }

    #[tokio::test]
    async fn test_event_bus_publish_subscribe() {
        let bus = EventBus::new(EventBusConfig::default());
        let mut rx = bus.receiver();

        let sub = Subscription::new(
            "test.*",
            None,
            SubscriptionHandler::Callback { name: "handler".to_string() },
        );
        let sub_id = bus.subscribe(sub).await;

        let event = SystemEvent::new(
            "test.channel",
            "my_event",
            serde_json::json!({"data": "hello"}),
            EventSource::System { component: "test".to_string() },
        );

        let matched = bus.publish(event.clone()).await;
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0], sub_id);

        let received = rx.recv().await.unwrap();
        assert_eq!(received.channel, "test.channel");
        assert_eq!(received.event_type, "my_event");
    }

    #[tokio::test]
    async fn test_event_bus_recent_events() {
        let bus = EventBus::new(EventBusConfig::default());

        for i in 0..5 {
            let event = SystemEvent::new(
                format!("test.channel.{}", i),
                "event",
                serde_json::Value::Null,
                EventSource::System { component: "test".to_string() },
            );
            bus.publish(event).await;
        }

        let recent = bus.recent_events(None, 3).await;
        assert_eq!(recent.len(), 3);

        let filtered = bus.recent_events(Some("test.channel.2"), 10).await;
        assert_eq!(filtered.len(), 1);
    }

    #[tokio::test]
    async fn test_event_bus_unsubscribe() {
        let bus = EventBus::new(EventBusConfig::default());

        let sub = Subscription::new(
            "test.*",
            None,
            SubscriptionHandler::Callback { name: "h".to_string() },
        );
        let id = bus.subscribe(sub).await;
        assert_eq!(bus.subscription_count().await, 1);

        bus.unsubscribe(&id).await;
        assert_eq!(bus.subscription_count().await, 0);
    }

    #[tokio::test]
    async fn test_event_bus_wait_for() {
        let bus = Arc::new(EventBus::new(EventBusConfig::default()));
        let bus_clone = bus.clone();

        let handle = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let event = SystemEvent::new(
                "test.signal",
                "ready",
                serde_json::Value::Null,
                EventSource::System { component: "test".to_string() },
            );
            bus_clone.publish(event).await;
        });

        let result = bus
            .wait_for("test.*", Some("ready"), std::time::Duration::from_secs(2))
            .await;

        assert!(result.is_some());
        assert_eq!(result.unwrap().event_type, "ready");
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_event_bus_wait_for_timeout() {
        let bus = EventBus::new(EventBusConfig::default());
        let result = bus
            .wait_for("nothing.*", None, std::time::Duration::from_millis(50))
            .await;
        assert!(result.is_none());
    }
}
