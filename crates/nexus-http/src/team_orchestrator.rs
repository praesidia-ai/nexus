//! Team orchestrator — manages lifecycle of multi-agent team execution.
//!
//! The orchestrator receives a [`TeamDefinition`] and a task string, then
//! dispatches to the appropriate coordination protocol handler. Each member
//! runs through the existing [`AgentRuntime`], but with team-specific tools
//! (send_message, read_messages, create_task, publish_artifact) injected
//! into the agent context.
//!
//! All progress is emitted as [`TeamEvent`]s on a broadcast channel that
//! HTTP handlers can stream as SSE.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{info, warn};

use nexus_agents_core::communication::*;
use nexus_agents_core::teams::*;
use nexus_agents_core::workspace::*;

use crate::agents::events::AgentResult;
use crate::agents::{AgentContext, AgentRuntime, ToolRegistry};
use crate::state::AppState;
use crate::team_prompting;

// ── Team Events ─────────────────────────────────────────────────────────────

/// Events emitted during team execution (streamed via SSE).
///
/// Every significant state change in the team lifecycle produces an event.
/// Frontends subscribe to these for real-time progress display.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TeamEvent {
    /// The team has started working on a task.
    TeamStarted {
        /// Unique ID for this team run.
        team_id: String,
        /// The task being worked on.
        task: String,
        /// Names of team members participating.
        members: Vec<String>,
    },
    /// The team completed the task successfully.
    TeamCompleted {
        /// Team run ID.
        team_id: String,
        /// Total wall-clock time in milliseconds.
        duration_ms: u64,
        /// Total estimated cost in USD.
        cost_usd: f32,
    },
    /// The team failed to complete the task.
    TeamFailed {
        /// Team run ID.
        team_id: String,
        /// Reason for failure.
        reason: String,
    },
    /// A team member started working.
    MemberStarted {
        /// The member's ID.
        member_id: String,
        /// The member's role description.
        role: String,
    },
    /// A team member is reasoning/thinking.
    MemberThinking {
        /// The member's ID.
        member_id: String,
        /// The thinking content.
        content: String,
    },
    /// A team member invoked a tool.
    MemberToolCall {
        /// The member's ID.
        member_id: String,
        /// The tool name.
        tool: String,
    },
    /// A team member completed their work.
    MemberCompleted {
        /// The member's ID.
        member_id: String,
        /// Summary of what was accomplished.
        summary: String,
    },
    /// A team member's execution failed.
    MemberFailed {
        /// The member's ID.
        member_id: String,
        /// Error message.
        error: String,
    },
    /// A message was sent between team members.
    MessageSent {
        /// Sender member ID.
        from: String,
        /// Recipient description.
        to: String,
        /// Type of message content.
        content_type: String,
        /// Brief summary of the message.
        summary: String,
    },
    /// A task was created on the workspace board.
    TaskCreated {
        /// The task ID.
        task_id: String,
        /// Task title.
        title: String,
        /// Who the task is assigned to.
        assigned_to: Option<String>,
    },
    /// A task's status changed.
    TaskStatusChanged {
        /// The task ID.
        task_id: String,
        /// Previous status.
        old_status: String,
        /// New status.
        new_status: String,
    },
    /// An artifact was published to the workspace.
    ArtifactPublished {
        /// Artifact ID.
        artifact_id: String,
        /// Artifact name.
        name: String,
        /// Who published it.
        by: String,
    },
    /// The orchestrator paused for human review.
    HitlPauseRequested {
        /// Why the pause was triggered.
        reason: String,
        /// Which checkpoint triggered it.
        checkpoint: String,
    },
    /// The orchestrator resumed after human approval.
    HitlResumed,
    /// Budget status update.
    BudgetUpdate {
        /// Amount spent so far in USD.
        spent_usd: f32,
        /// Amount remaining in USD.
        remaining_usd: f32,
        /// Percentage of budget used.
        percent_used: f32,
    },
}

// ── Team Bus ────────────────────────────────────────────────────────────────

/// The team communication bus — routes messages between agents.
///
/// Messages are stored for history and delivered to per-member inboxes.
/// The bus also exposes a broadcast channel for [`TeamEvent`]s.
///
/// Per ADR-002 §4: when a [`TeamEventSink`] is attached, every message and
/// team event is shadow-written to the SQLite event log so a server restart
/// can resume the run from the last persisted event. Without a sink the bus
/// behaves exactly like before — useful for tests.
pub struct TeamBus {
    /// All messages ever sent through the bus.
    messages: Arc<RwLock<Vec<AgentMessage>>>,
    /// Per-member inbox senders keyed by member ID.
    subscribers: Arc<RwLock<HashMap<String, mpsc::Sender<AgentMessage>>>>,
    /// Broadcast channel for team events (SSE consumers subscribe here).
    event_tx: broadcast::Sender<TeamEvent>,
    /// Optional durable event log — when present, every message + emit
    /// shadow-writes to SQLite via [`crate::team_event_sink::TeamEventSink`].
    event_sink: Option<Arc<crate::team_event_sink::TeamEventSink>>,
}

impl Default for TeamBus {
    fn default() -> Self {
        Self::new()
    }
}

impl TeamBus {
    /// Create a new team bus with the given event broadcast capacity.
    pub fn new() -> Self {
        // Token-streaming agents emit hundreds of events in seconds; the old
        // 256 cap caused slow SSE clients to lag instantly. 4096 absorbs
        // realistic bursts; downstream still surfaces `lag` events so the
        // UI can show what was dropped instead of silently freezing.
        let (event_tx, _) = broadcast::channel(4096);
        Self {
            messages: Arc::new(RwLock::new(Vec::new())),
            subscribers: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            event_sink: None,
        }
    }

    /// Attach a durable event sink. Returns `self` so it can chain after
    /// `TeamBus::new()`. Calling this after the bus has been used is safe;
    /// only future messages are persisted.
    pub fn with_event_sink(mut self, sink: Arc<crate::team_event_sink::TeamEventSink>) -> Self {
        self.event_sink = Some(sink);
        self
    }

    /// True when the bus is durably persisting messages.
    pub fn has_event_sink(&self) -> bool {
        self.event_sink.is_some()
    }

    /// Number of active SSE subscribers on this bus.
    pub fn receiver_count(&self) -> usize {
        self.event_tx.receiver_count()
    }

    /// Register a member's inbox. Returns the receiving end of the channel.
    pub async fn register_member(&self, member_id: &str) -> mpsc::Receiver<AgentMessage> {
        let (tx, rx) = mpsc::channel(64);
        self.subscribers
            .write()
            .await
            .insert(member_id.to_string(), tx);
        rx
    }

    /// Send a message through the bus.
    ///
    /// The message is stored in history and routed to the appropriate
    /// recipient(s) based on [`MessageTarget`].
    pub async fn send(&self, message: AgentMessage) {
        // Emit a TeamEvent for SSE consumers.
        let content_type = match &message.content {
            MessageContent::TaskAssignment { .. } => "task_assignment",
            MessageContent::TaskCompletion { .. } => "task_completion",
            MessageContent::Review { .. } => "review",
            MessageContent::ReviewVerdict { .. } => "review_verdict",
            MessageContent::Proposal { .. } => "proposal",
            MessageContent::Vote { .. } => "vote",
            MessageContent::Escalation { .. } => "escalation",
            MessageContent::ContextShare { .. } => "context_share",
            MessageContent::StatusUpdate { .. } => "status_update",
            MessageContent::Discussion { .. } => "discussion",
        };

        let to_str = match &message.to {
            MessageTarget::Direct { member_id } => member_id.clone(),
            MessageTarget::Broadcast => "broadcast".to_string(),
            MessageTarget::Role { role } => format!("role:{}", role),
            MessageTarget::Human => "human".to_string(),
        };

        let summary = message_summary(&message.content);

        let _ = self.event_tx.send(TeamEvent::MessageSent {
            from: message.from.clone(),
            to: to_str.clone(),
            content_type: content_type.to_string(),
            summary,
        });

        // Route to subscribers.
        // IMPORTANT: collect senders under the read lock, then drop the lock
        // before awaiting on channel sends to avoid holding an RwLock across
        // await points (which can cause deadlocks with Tokio's cooperative
        // scheduling).
        let targets: Vec<mpsc::Sender<AgentMessage>> = {
            let subs = self.subscribers.read().await;
            match &message.to {
                MessageTarget::Direct { member_id } => {
                    subs.get(member_id).cloned().into_iter().collect()
                }
                MessageTarget::Broadcast => {
                    subs.iter()
                        .filter(|(id, _)| **id != message.from)
                        .map(|(_, tx)| tx.clone())
                        .collect()
                }
                MessageTarget::Role { .. } | MessageTarget::Human => {
                    // Role-based routing and human escalation are handled at a
                    // higher level by the orchestrator, which knows member roles.
                    // For now, store in history for retrieval.
                    Vec::new()
                }
            }
        }; // read lock dropped here

        for tx in targets {
            let _ = tx.send(message.clone()).await;
        }

        // Store in history (cap at 10,000 messages to prevent unbounded growth).
        const MAX_MESSAGES: usize = 10_000;
        let from_for_sink = message.from.clone();
        let to_for_sink = to_str;
        let content_type_for_sink = content_type;
        let summary_for_sink = message_summary(&message.content);
        {
            let mut msgs = self.messages.write().await;
            msgs.push(message);
            if msgs.len() > MAX_MESSAGES {
                let drain = msgs.len() - MAX_MESSAGES;
                msgs.drain(..drain);
            }
        }

        // Durable shadow-write (ADR-002). Best-effort: a SQLite failure logs
        // a warning but does NOT block the in-memory path.
        if let Some(sink) = &self.event_sink {
            let payload = serde_json::json!({
                "from": from_for_sink,
                "to": to_for_sink,
                "content_type": content_type_for_sink,
                "summary": summary_for_sink,
            });
            sink.append_message(Some(&from_for_sink), &payload).await;
        }
    }

    /// Retrieve all messages sent to a specific member (by direct target or broadcast).
    pub async fn receive(&self, member_id: &str) -> Vec<AgentMessage> {
        let msgs = self.messages.read().await;
        msgs.iter()
            .filter(|m| match &m.to {
                MessageTarget::Direct { member_id: id } => id == member_id,
                MessageTarget::Broadcast => m.from != member_id,
                _ => false,
            })
            .cloned()
            .collect()
    }

    /// Subscribe to the team event broadcast channel.
    pub fn subscribe_events(&self) -> broadcast::Receiver<TeamEvent> {
        self.event_tx.subscribe()
    }

    /// Emit a team event (used by the orchestrator directly).
    ///
    /// Lifecycle-shaping events (`TeamStarted`/`TeamCompleted`/`TeamFailed`)
    /// are also persisted as `phase` events when an event sink is attached.
    /// Per-message events are persisted by [`TeamBus::send`] instead.
    pub fn emit(&self, event: TeamEvent) {
        let _ = self.event_tx.send(event.clone());
        if let Some(sink) = &self.event_sink {
            // Best-effort durable mirror. We dispatch a fire-and-forget task so
            // emitters that aren't async (callers that hold synchronous locks)
            // can keep working unchanged.
            let sink = Arc::clone(sink);
            tokio::spawn(async move {
                let payload = serde_json::to_value(&event)
                    .unwrap_or_else(|_| serde_json::json!({"unserializable": true}));
                let kind = match &event {
                    TeamEvent::TeamStarted { .. }
                    | TeamEvent::TeamCompleted { .. }
                    | TeamEvent::TeamFailed { .. } => "phase",
                    TeamEvent::ArtifactPublished { .. } => "artifact",
                    TeamEvent::TaskCreated { .. } | TeamEvent::TaskStatusChanged { .. } => "task",
                    _ => "phase",
                };
                if kind == "phase" {
                    sink.append_phase(None, &payload).await;
                }
            });
        }
    }

    /// Get the full message history.
    pub async fn history(&self) -> Vec<AgentMessage> {
        self.messages.read().await.clone()
    }
}

// ── Team Workspace ──────────────────────────────────────────────────────────

/// Shared team workspace — task board, artifacts, key-value state, scratchpad.
///
/// Thread-safe: all inner collections are behind `RwLock`.
///
/// Per ADR-002 §4: when a [`TeamEventSink`] is attached, every state write,
/// task creation, and artifact publication is shadow-written to the SQLite
/// event log so a restart can rebuild the workspace from the durable log.
pub struct TeamWorkspace {
    /// Key-value state shared across the team.
    state: Arc<RwLock<HashMap<String, serde_json::Value>>>,
    /// Task board.
    tasks: Arc<RwLock<Vec<Task>>>,
    /// Published artifacts.
    artifacts: Arc<RwLock<Vec<Artifact>>>,
    /// Scratchpad entries: (author, topic, content).
    scratchpad: Arc<RwLock<Vec<(String, String, String)>>>,
    /// Optional durable event log for state/task/artifact writes.
    event_sink: Option<Arc<crate::team_event_sink::TeamEventSink>>,
}

impl Default for TeamWorkspace {
    fn default() -> Self {
        Self::new()
    }
}

impl TeamWorkspace {
    /// Create a new empty workspace.
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(HashMap::new())),
            tasks: Arc::new(RwLock::new(Vec::new())),
            artifacts: Arc::new(RwLock::new(Vec::new())),
            scratchpad: Arc::new(RwLock::new(Vec::new())),
            event_sink: None,
        }
    }

    /// Attach a durable event sink. See [`TeamBus::with_event_sink`].
    pub fn with_event_sink(mut self, sink: Arc<crate::team_event_sink::TeamEventSink>) -> Self {
        self.event_sink = Some(sink);
        self
    }

    /// True when state writes are durably persisted.
    pub fn has_event_sink(&self) -> bool {
        self.event_sink.is_some()
    }

    /// Get a value from the shared state.
    pub async fn get_state(&self, key: &str) -> Option<serde_json::Value> {
        self.state.read().await.get(key).cloned()
    }

    /// Set a value in the shared state.
    ///
    /// Capped at [`MAX_STATE_KEYS`] entries so an agent that writes with
    /// unbounded keys (e.g. timestamped keys derived from user input) cannot
    /// grow the map forever. Existing keys always succeed; new keys beyond
    /// the cap are silently dropped to avoid destabilising the run.
    pub async fn set_state(&self, key: &str, value: serde_json::Value) {
        const MAX_STATE_KEYS: usize = 1_000;
        // Update in-memory cache first.
        {
            let mut state = self.state.write().await;
            if state.len() >= MAX_STATE_KEYS && !state.contains_key(key) {
                tracing::warn!(
                    key = %key,
                    current = state.len(),
                    cap = MAX_STATE_KEYS,
                    "team orchestrator state cap reached; dropping new key"
                );
                return;
            }
            state.insert(key.to_string(), value.clone());
        }
        // Durable shadow-write (ADR-002 §3 — atomic with the workspace cache).
        if let Some(sink) = &self.event_sink {
            sink.put_state(None, key, &value).await;
        }
    }

    /// Create a task on the board (capped at 1,000 tasks).
    pub async fn create_task(&self, task: Task) {
        const MAX_TASKS: usize = 1_000;
        // Capture fields needed for persistence before moving into history.
        let task_id = task.id.clone();
        let task_assignee = task.assigned_to.clone();
        let task_status = format!("{:?}", task.status);
        {
            let mut tasks = self.tasks.write().await;
            tasks.push(task);
            if tasks.len() > MAX_TASKS {
                let drain = tasks.len() - MAX_TASKS;
                tasks.drain(..drain);
            }
        }
        if let Some(sink) = &self.event_sink {
            let payload = serde_json::json!({
                "op": "create",
                "task_id": task_id,
                "assignee": task_assignee,
                "status": task_status,
            });
            sink.append_task(task_assignee.as_deref(), &payload).await;
        }
    }

    /// Update a task's status by ID.
    ///
    /// Returns the old status if the task was found, or `None` if not.
    pub async fn update_task_status(&self, task_id: &str, status: TaskStatus) -> Option<TaskStatus> {
        let new_status_str = format!("{:?}", status);
        let result = {
            let mut tasks = self.tasks.write().await;
            let mut found_old = None;
            for t in tasks.iter_mut() {
                if t.id == task_id {
                    found_old = Some(t.status.clone());
                    t.status = status;
                    break;
                }
            }
            found_old
        };
        if result.is_some() {
            if let Some(sink) = &self.event_sink {
                let payload = serde_json::json!({
                    "op": "status_update",
                    "task_id": task_id,
                    "status": new_status_str,
                });
                sink.append_task(None, &payload).await;
            }
        }
        result
    }

    /// Get all tasks assigned to a specific member.
    pub async fn my_tasks(&self, member_id: &str) -> Vec<Task> {
        self.tasks
            .read()
            .await
            .iter()
            .filter(|t| t.assigned_to.as_deref() == Some(member_id))
            .cloned()
            .collect()
    }

    /// Get all tasks that are in "Ready" status — dependencies satisfied.
    pub async fn ready_tasks(&self) -> Vec<Task> {
        let tasks = self.tasks.read().await;
        let done_ids: Vec<&str> = tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Done)
            .map(|t| t.id.as_str())
            .collect();

        tasks
            .iter()
            .filter(|t| {
                t.status == TaskStatus::Ready
                    || (t.status == TaskStatus::Backlog
                        && t.dependencies.iter().all(|d| done_ids.contains(&d.as_str())))
            })
            .cloned()
            .collect()
    }

    /// Publish an artifact to the workspace (capped at 1,000 artifacts).
    pub async fn publish_artifact(&self, artifact: Artifact) {
        const MAX_ARTIFACTS: usize = 1_000;
        let aid = artifact.id.clone();
        let aname = artifact.name.clone();
        let producer = artifact.produced_by.clone();
        let kind = format!("{:?}", artifact.artifact_type);
        {
            let mut artifacts = self.artifacts.write().await;
            artifacts.push(artifact);
            if artifacts.len() > MAX_ARTIFACTS {
                let drain = artifacts.len() - MAX_ARTIFACTS;
                artifacts.drain(..drain);
            }
        }
        if let Some(sink) = &self.event_sink {
            let payload = serde_json::json!({
                "artifact_id": aid,
                "name": aname,
                "kind": kind,
                "produced_by": producer,
            });
            sink.append_artifact(Some(&producer), &payload).await;
        }
    }

    /// Get all published artifacts.
    pub async fn get_artifacts(&self) -> Vec<Artifact> {
        self.artifacts.read().await.clone()
    }

    /// Get artifacts produced by a specific member.
    pub async fn get_artifacts_by_member(&self, member_id: &str) -> Vec<Artifact> {
        self.artifacts
            .read()
            .await
            .iter()
            .filter(|a| a.produced_by == member_id)
            .cloned()
            .collect()
    }

    /// Add a scratchpad entry (capped at 1,000 entries).
    pub async fn add_scratchpad(&self, author: &str, topic: &str, content: &str) {
        const MAX_SCRATCHPAD: usize = 1_000;
        let mut scratchpad = self.scratchpad.write().await;
        scratchpad.push((
            author.to_string(),
            topic.to_string(),
            content.to_string(),
        ));
        if scratchpad.len() > MAX_SCRATCHPAD {
            let drain = scratchpad.len() - MAX_SCRATCHPAD;
            scratchpad.drain(..drain);
        }
    }

    /// Take a point-in-time snapshot of the entire workspace.
    pub async fn snapshot(&self) -> WorkspaceSnapshot {
        let state = self
            .state
            .read()
            .await
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        WorkspaceSnapshot {
            tasks: self.tasks.read().await.clone(),
            artifacts: self.artifacts.read().await.clone(),
            state,
            scratchpad: self.scratchpad.read().await.clone(),
        }
    }
}

// ── Team Orchestrator ───────────────────────────────────────────────────────

/// The team orchestrator — manages the full lifecycle of a multi-agent team run.
///
/// Create one per team execution. Call [`run`](Self::run) to start execution,
/// which returns a broadcast receiver for streaming [`TeamEvent`]s.
pub struct TeamOrchestrator {
    /// Shared application state.
    app: Arc<AppState>,
    /// Communication bus for this team run.
    bus: Arc<TeamBus>,
    /// Shared workspace for this team run.
    workspace: Arc<TeamWorkspace>,
    /// Pause flag — when true, member execution stalls before each new member
    /// starts until the flag flips back to false.
    paused: Arc<AtomicBool>,
}

impl TeamOrchestrator {
    /// Create a new orchestrator for a team run.
    pub fn new(app: Arc<AppState>) -> Self {
        Self {
            app,
            bus: Arc::new(TeamBus::new()),
            workspace: Arc::new(TeamWorkspace::new()),
            paused: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Create an orchestrator whose bus and workspace shadow-write to a
    /// durable [`TeamEventSink`]. Per ADR-002 §4: every message, state put,
    /// task, and artifact lands in `team_events` so a server restart can
    /// resume the run from the last persisted event.
    pub fn new_with_sink(
        app: Arc<AppState>,
        sink: Arc<crate::team_event_sink::TeamEventSink>,
    ) -> Self {
        Self {
            app,
            bus: Arc::new(TeamBus::new().with_event_sink(Arc::clone(&sink))),
            workspace: Arc::new(TeamWorkspace::new().with_event_sink(sink)),
            paused: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Returns `true` if the orchestrator is currently paused.
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Acquire)
    }

    /// Pause the orchestrator. Members already running continue; new members
    /// wait at the gate in [`run_member`] until [`Self::resume`] is called.
    pub fn pause(&self) {
        if !self.paused.swap(true, Ordering::AcqRel) {
            self.bus.emit(TeamEvent::HitlPauseRequested {
                reason: "User requested pause".into(),
                checkpoint: "manual".into(),
            });
        }
    }

    /// Resume the orchestrator after a pause. No-op if not paused.
    pub fn resume(&self) {
        if self.paused.swap(false, Ordering::AcqRel) {
            self.bus.emit(TeamEvent::HitlResumed);
        }
    }

    /// Returns true when no SSE subscribers are actively listening to this run.
    ///
    /// Used by the team run registry to evict finished runs and prevent
    /// unbounded memory growth when many teams have been executed.
    pub fn is_finished(&self) -> bool {
        self.bus.receiver_count() == 0
    }

    /// Subscribe a new listener to this run's event stream. New subscribers
    /// only see events emitted after they subscribe (broadcast semantics).
    pub fn subscribe_events(&self) -> broadcast::Receiver<TeamEvent> {
        self.bus.subscribe_events()
    }

    /// Execute a team on a task. Returns an SSE event receiver.
    ///
    /// The orchestrator dispatches to the appropriate protocol handler based
    /// on the team's [`CoordinationProtocol`]. Execution happens in a
    /// background task; the caller streams events from the returned receiver.
    pub async fn run(
        &self,
        team: &TeamDefinition,
        task: &str,
    ) -> Result<broadcast::Receiver<TeamEvent>, String> {
        let rx = self.bus.subscribe_events();

        let members: Vec<String> = team.members.iter().map(|m| m.id.clone()).collect();
        self.bus.emit(TeamEvent::TeamStarted {
            team_id: team.id.clone(),
            task: task.to_string(),
            members,
        });

        let team = team.clone();
        let task = task.to_string();
        let app = self.app.clone();
        let bus = self.bus.clone();
        let workspace = self.workspace.clone();
        let paused = self.paused.clone();

        tokio::spawn(async move {
            let start = Instant::now();

            // Spawn a kernel process for the team execution
            let team_process_id = {
                let agent_def = nexus_agents_core::definition::AgentDefinition::default_with_name(&format!("team:{}", team.name));
                app.scheduler
                    .spawn(
                        agent_def,
                        format!("Team '{}': {}", team.name, task),
                        nexus_kernel::Priority::Normal,
                        nexus_kernel::ResourceAllocation::default(),
                        None,
                    )
                    .await
                    .ok()
            };

            // Publish team start event to kernel event bus
            app.event_bus
                .publish(nexus_kernel::SystemEvent::new(
                    format!("team.{}.start", team.id),
                    "team_started",
                    serde_json::json!({
                        "team_id": team.id,
                        "task": task,
                        "members": team.members.len(),
                    }),
                    nexus_kernel::EventSource::System { component: "team_orchestrator".into() },
                ))
                .await;
            let result = match &team.coordination {
                CoordinationProtocol::Hierarchical { lead_id } => {
                    run_hierarchical(&app, &bus, &workspace, &paused, &team, lead_id, &task).await
                }
                CoordinationProtocol::Parallel { coordinator_id } => {
                    run_parallel(&app, &bus, &workspace, &paused, &team, coordinator_id, &task)
                        .await
                }
                CoordinationProtocol::Sequential { order } => {
                    run_sequential(&app, &bus, &workspace, &paused, &team, order, &task).await
                }
                CoordinationProtocol::Consensus { quorum, max_rounds } => {
                    run_consensus(
                        &app, &bus, &workspace, &paused, &team, *quorum, *max_rounds, &task,
                    )
                    .await
                }
                CoordinationProtocol::Competitive {
                    judge_id,
                    candidates,
                } => {
                    run_competitive(
                        &app, &bus, &workspace, &paused, &team, judge_id, candidates, &task,
                    )
                    .await
                }
                CoordinationProtocol::Swarm { max_concurrent } => {
                    run_swarm(&app, &bus, &workspace, &paused, &team, *max_concurrent, &task).await
                }
            };

            let duration_ms = start.elapsed().as_millis() as u64;
            match result {
                Ok(cost) => {
                    bus.emit(TeamEvent::TeamCompleted {
                        team_id: team.id.clone(),
                        duration_ms,
                        cost_usd: cost,
                    });
                    // Mark kernel process as completed
                    if let Some(ref pid) = team_process_id {
                        app.scheduler.complete(pid, Some(format!("Team completed in {duration_ms}ms, cost ${cost:.2}"))).await.ok();
                    }
                }
                Err(reason) => {
                    bus.emit(TeamEvent::TeamFailed {
                        team_id: team.id.clone(),
                        reason: reason.clone(),
                    });
                    // Mark kernel process as failed
                    if let Some(ref pid) = team_process_id {
                        app.scheduler.fail(pid, reason).await.ok();
                    }
                }
            }
        });

        Ok(rx)
    }

    /// Inject a human message into the running team.
    ///
    /// This is used for HITL interactions where the human provides feedback
    /// or instructions to a specific team member or the whole team.
    pub async fn inject_message(&self, message: &str, target: &str) {
        let msg = AgentMessage {
            id: uuid::Uuid::new_v4().to_string(),
            from: "human".to_string(),
            to: if target == "broadcast" {
                MessageTarget::Broadcast
            } else {
                MessageTarget::Direct {
                    member_id: target.to_string(),
                }
            },
            content: MessageContent::Discussion {
                content: message.to_string(),
            },
            timestamp: chrono::Utc::now().to_rfc3339(),
            in_reply_to: None,
            priority: MessagePriority::High,
        };
        self.bus.send(msg).await;
        self.bus.emit(TeamEvent::HitlResumed);
    }

    /// Get a snapshot of the current workspace state.
    pub async fn workspace_snapshot(&self) -> WorkspaceSnapshot {
        self.workspace.snapshot().await
    }

    /// Get the message history.
    pub async fn message_history(&self) -> Vec<AgentMessage> {
        self.bus.history().await
    }
}

// ── Protocol Implementations ────────────────────────────────────────────────

/// Wait until the orchestrator is unpaused. Polls the atomic flag every
/// 200ms — cheap, and pause cycles are user-driven so latency is fine.
async fn await_unpaused(paused: &Arc<AtomicBool>) {
    while paused.load(Ordering::Acquire) {
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Run a single member through the agent runtime and return its result.
async fn run_member(
    app: &Arc<AppState>,
    bus: &Arc<TeamBus>,
    workspace: &Arc<TeamWorkspace>,
    paused: &Arc<AtomicBool>,
    team: &TeamDefinition,
    member: &TeamMember,
    task: &str,
) -> Result<AgentResult, String> {
    await_unpaused(paused).await;

    bus.emit(TeamEvent::MemberStarted {
        member_id: member.id.clone(),
        role: member.role.to_string(),
    });

    // Build the team-aware system prompt.
    let system_prompt = team_prompting::build_member_prompt(member, team, workspace).await;

    // Convert the nexus_agents_core AgentDefinition to the local type.
    let mut agent_def = convert_agent_def(&member.agent)?;
    agent_def.system_prompt = system_prompt;

    // Recall relevant memory for this team member
    let base_context = format!(
        "You are working as part of team '{}'. Your role: {}.\nTask: {}",
        team.name, member.role, task
    );
    let context_with_memory = if let Ok(mem) = app.memory_system() {
        match mem.recall_context(&team.id, Some(&member.id), &[], 10) {
            Ok(recalled) if !recalled.context_block.is_empty() => {
                format!("{}\n\n{}", base_context, recalled.context_block)
            }
            _ => base_context,
        }
    } else {
        base_context
    };

    let context = AgentContext {
        project_id: None,
        project_dir: None,
        context: context_with_memory,
        history: Vec::new(),
    };

    let mut registry = ToolRegistry::new();
    crate::agent_tools::register_default_tools(&mut registry);
    let registry = Arc::new(registry);
    let runtime = AgentRuntime::new(app.clone(), registry);

    match runtime.execute(&agent_def, task, context).await {
        Ok((mut rx, handle)) => {
            // Forward agent events as team events.
            let bus_clone = bus.clone();
            let member_id = member.id.clone();
            tokio::spawn(async move {
                use crate::agents::events::AgentEvent;
                while let Some(event) = rx.recv().await {
                    match event {
                        AgentEvent::Thinking { content, .. } => {
                            bus_clone.emit(TeamEvent::MemberThinking {
                                member_id: member_id.clone(),
                                content,
                            });
                        }
                        AgentEvent::ToolInvoked { tool, .. } => {
                            bus_clone.emit(TeamEvent::MemberToolCall {
                                member_id: member_id.clone(),
                                tool,
                            });
                        }
                        _ => {}
                    }
                }
            });

            match handle.await {
                Ok(Ok(result)) => {
                    bus.emit(TeamEvent::MemberCompleted {
                        member_id: member.id.clone(),
                        summary: truncate(&result.output, 200),
                    });
                    Ok(result)
                }
                Ok(Err(e)) => {
                    bus.emit(TeamEvent::MemberFailed {
                        member_id: member.id.clone(),
                        error: e.clone(),
                    });
                    Err(e)
                }
                Err(e) => {
                    let err = format!("Member {} panicked: {}", member.id, e);
                    bus.emit(TeamEvent::MemberFailed {
                        member_id: member.id.clone(),
                        error: err.clone(),
                    });
                    Err(err)
                }
            }
        }
        Err(e) => {
            bus.emit(TeamEvent::MemberFailed {
                member_id: member.id.clone(),
                error: e.clone(),
            });
            Err(e)
        }
    }
}

/// Find a member by ID.
fn find_member<'a>(team: &'a TeamDefinition, member_id: &str) -> Option<&'a TeamMember> {
    team.members.iter().find(|m| m.id == member_id)
}

/// Hierarchical protocol: Lead decomposes -> specialists execute -> reviewer validates.
///
/// 1. The lead receives the task + team roster and produces a plan with subtasks.
/// 2. Each subtask is assigned to a specialist who executes it.
/// 3. If a reviewer exists, they validate each specialist's output.
/// 4. The lead merges all results into a final output.
async fn run_hierarchical(
    app: &Arc<AppState>,
    bus: &Arc<TeamBus>,
    workspace: &Arc<TeamWorkspace>,
    paused: &Arc<AtomicBool>,
    team: &TeamDefinition,
    lead_id: &str,
    task: &str,
) -> Result<f32, String> {
    let lead = find_member(team, lead_id)
        .ok_or_else(|| format!("Lead member '{}' not found", lead_id))?;

    // Phase 1: Lead creates the plan.
    info!(team = %team.id, lead = %lead_id, "Hierarchical: lead planning");
    let plan_task = format!(
        "You are the team lead. Analyze this task and create a plan with subtasks for your team.\n\
         Assign each subtask to the most appropriate team member.\n\
         Task: {}\n\n\
         Respond with a structured plan listing each subtask, its assignee, and dependencies.",
        task
    );
    let plan_result = run_member(app, bus, workspace, paused, team, lead, &plan_task).await?;

    workspace
        .set_state(
            "plan",
            serde_json::Value::String(plan_result.output.clone()),
        )
        .await;

    // Phase 2: Run specialists sequentially (simplified; a full implementation
    // would parse the plan and create real workspace tasks).
    let specialists: Vec<&TeamMember> = team
        .members
        .iter()
        .filter(|m| m.id != lead_id && !matches!(m.role, TeamRole::Reviewer))
        .collect();

    let mut specialist_outputs = Vec::new();
    for specialist in &specialists {
        let member_task = format!(
            "The team lead produced this plan:\n{}\n\n\
             Complete the parts assigned to you (role: {}).\n\
             Original task: {}",
            plan_result.output, specialist.role, task
        );
        match run_member(app, bus, workspace, paused, team, specialist, &member_task).await {
            Ok(result) => specialist_outputs.push((specialist.id.clone(), result)),
            Err(e) => {
                warn!(member = %specialist.id, error = %e, "Specialist failed, continuing");
            }
        }
    }

    // Phase 3: Reviewer validates (if one exists).
    let reviewer = team
        .members
        .iter()
        .find(|m| matches!(m.role, TeamRole::Reviewer));
    if let Some(reviewer) = reviewer {
        let review_context: String = specialist_outputs
            .iter()
            .map(|(id, r)| format!("=== Output from {} ===\n{}", id, r.output))
            .collect::<Vec<_>>()
            .join("\n\n");

        let review_task = format!(
            "Review the following outputs from the team:\n\n{}\n\n\
             Original task: {}\n\
             Provide feedback and identify any issues.",
            review_context, task
        );
        let _ = run_member(app, bus, workspace, paused, team, reviewer, &review_task).await;
    }

    // Phase 4: Lead merges results.
    let merge_context: String = specialist_outputs
        .iter()
        .map(|(id, r)| format!("=== {} ===\n{}", id, r.output))
        .collect::<Vec<_>>()
        .join("\n\n");

    let merge_task = format!(
        "Merge these specialist outputs into a final, coherent result:\n\n{}\n\n\
         Original task: {}",
        merge_context, task
    );
    let _ = run_member(app, bus, workspace, paused, team, lead, &merge_task).await;

    Ok(0.0) // Cost tracking is handled by the runtime layer.
}

/// Parallel protocol: Coordinator splits task -> all run concurrently -> merge.
///
/// 1. The coordinator analyzes the task and decides how to split it.
/// 2. All non-coordinator members execute their portion concurrently.
/// 3. The coordinator merges all results.
async fn run_parallel(
    app: &Arc<AppState>,
    bus: &Arc<TeamBus>,
    workspace: &Arc<TeamWorkspace>,
    paused: &Arc<AtomicBool>,
    team: &TeamDefinition,
    coordinator_id: &str,
    task: &str,
) -> Result<f32, String> {
    let coordinator = find_member(team, coordinator_id)
        .ok_or_else(|| format!("Coordinator '{}' not found", coordinator_id))?;

    // Phase 1: Coordinator produces a split plan.
    info!(team = %team.id, "Parallel: coordinator splitting task");
    let split_task = format!(
        "Split the following task into independent subtasks, one per team member.\n\
         Task: {}\n\n\
         List each subtask clearly.",
        task
    );
    let split_result =
        run_member(app, bus, workspace, paused, team, coordinator, &split_task).await?;

    // Phase 2: All specialists run concurrently.
    let workers: Vec<&TeamMember> = team
        .members
        .iter()
        .filter(|m| m.id != coordinator_id)
        .collect();

    let mut handles = Vec::new();
    for worker in &workers {
        let app = app.clone();
        let bus = bus.clone();
        let ws = workspace.clone();
        let paused = paused.clone();
        let team_clone = team.clone();
        let member = (*worker).clone();
        let member_task = format!(
            "The coordinator split the task. Here is the plan:\n{}\n\n\
             Complete the parts relevant to your role ({}).\n\
             Original task: {}",
            split_result.output, member.role, task
        );

        let handle = tokio::spawn(async move {
            run_member(&app, &bus, &ws, &paused, &team_clone, &member, &member_task).await
        });
        handles.push((worker.id.clone(), handle));
    }

    // Collect results.
    let mut outputs = Vec::new();
    for (id, handle) in handles {
        match handle.await {
            Ok(Ok(result)) => outputs.push((id, result)),
            Ok(Err(e)) => warn!(member = %id, error = %e, "Parallel member failed"),
            Err(e) => warn!(member = %id, error = %e, "Parallel member panicked"),
        }
    }

    // Phase 3: Coordinator merges.
    let merge_context: String = outputs
        .iter()
        .map(|(id, r)| format!("=== {} ===\n{}", id, r.output))
        .collect::<Vec<_>>()
        .join("\n\n");

    let merge_task = format!(
        "Merge these parallel outputs into a unified result:\n\n{}\n\nOriginal task: {}",
        merge_context, task
    );
    let _ = run_member(app, bus, workspace, paused, team, coordinator, &merge_task).await;

    Ok(0.0)
}

/// Sequential protocol: each member runs in order, passing output as input to next.
///
/// Member N receives the original task plus the accumulated outputs from
/// members 1..N-1.
async fn run_sequential(
    app: &Arc<AppState>,
    bus: &Arc<TeamBus>,
    workspace: &Arc<TeamWorkspace>,
    paused: &Arc<AtomicBool>,
    team: &TeamDefinition,
    order: &[String],
    task: &str,
) -> Result<f32, String> {
    info!(team = %team.id, order = ?order, "Sequential: running pipeline");
    let mut accumulated_output = String::new();

    for (i, member_id) in order.iter().enumerate() {
        let member = find_member(team, member_id)
            .ok_or_else(|| format!("Sequential member '{}' not found", member_id))?;

        let step_task = if i == 0 {
            format!(
                "You are step {} of {} in a sequential pipeline.\n\
                 Task: {}\n\n\
                 Complete your portion and produce output for the next step.",
                i + 1,
                order.len(),
                task
            )
        } else {
            format!(
                "You are step {} of {} in a sequential pipeline.\n\
                 Original task: {}\n\n\
                 Previous steps produced:\n{}\n\n\
                 Continue from where they left off.",
                i + 1,
                order.len(),
                task,
                accumulated_output
            )
        };

        match run_member(app, bus, workspace, paused, team, member, &step_task).await {
            Ok(result) => {
                accumulated_output.push_str(&format!(
                    "\n=== Step {} ({}) ===\n{}\n",
                    i + 1,
                    member_id,
                    result.output
                ));
            }
            Err(e) => {
                warn!(member = %member_id, step = i + 1, error = %e, "Sequential step failed");
                return Err(format!("Sequential pipeline failed at step {}: {}", i + 1, e));
            }
        }
    }

    Ok(0.0)
}

/// Consensus protocol: all members propose -> vote -> execute winner.
///
/// 1. Each member proposes a solution approach.
/// 2. Each member votes on all proposals.
/// 3. The proposal with the most votes (meeting quorum) wins.
/// 4. The proposer executes their approach.
#[allow(clippy::too_many_arguments)]
async fn run_consensus(
    app: &Arc<AppState>,
    bus: &Arc<TeamBus>,
    workspace: &Arc<TeamWorkspace>,
    paused: &Arc<AtomicBool>,
    team: &TeamDefinition,
    quorum: f32,
    max_rounds: u32,
    task: &str,
) -> Result<f32, String> {
    info!(team = %team.id, quorum, max_rounds, "Consensus: starting deliberation");

    // Phase 1: Gather proposals from all members.
    let mut proposals: Vec<(String, String)> = Vec::new();
    for member in &team.members {
        let propose_task = format!(
            "Propose an approach to solve this task. Be specific about your plan.\nTask: {}",
            task
        );
        match run_member(app, bus, workspace, paused, team, member, &propose_task).await {
            Ok(result) => proposals.push((member.id.clone(), result.output)),
            Err(e) => warn!(member = %member.id, error = %e, "Failed to get proposal"),
        }
    }

    if proposals.is_empty() {
        return Err("No proposals received from any member".to_string());
    }

    // Phase 2: Vote (simplified — each member rates proposals).
    let proposals_text: String = proposals
        .iter()
        .enumerate()
        .map(|(i, (id, text))| format!("Proposal {} (by {}):\n{}", i + 1, id, text))
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");

    let mut vote_counts: HashMap<usize, u32> = HashMap::new();

    for member in &team.members {
        let vote_task = format!(
            "Vote on the best proposal. Respond with just the proposal number (1-{}).\n\n{}\n\nOriginal task: {}",
            proposals.len(),
            proposals_text,
            task
        );
        if let Ok(result) =
            run_member(app, bus, workspace, paused, team, member, &vote_task).await
        {
            // Parse the vote — look for a number in the output.
            if let Some(vote_num) = result
                .output
                .chars()
                .find(|c| c.is_ascii_digit())
                .and_then(|c| c.to_digit(10))
            {
                let idx = (vote_num as usize).saturating_sub(1);
                if idx < proposals.len() {
                    *vote_counts.entry(idx).or_insert(0) += 1;
                }
            }
        }
    }

    // Phase 3: Find winner (must meet quorum).
    let quorum_count = (team.members.len() as f32 * quorum).ceil() as u32;
    let winner = vote_counts
        .iter()
        .max_by_key(|(_, count)| *count)
        .filter(|(_, count)| **count >= quorum_count)
        .map(|(idx, _)| *idx);

    let winner_idx = winner.unwrap_or(0); // Default to first proposal if no quorum.
    let (winner_id, winner_proposal) = &proposals[winner_idx];

    // Phase 4: Winner executes their proposal.
    if let Some(winner_member) = find_member(team, winner_id) {
        let execute_task = format!(
            "Your proposal was selected by the team. Execute it now.\n\
             Your proposal: {}\n\nOriginal task: {}",
            winner_proposal, task
        );
        let _ =
            run_member(app, bus, workspace, paused, team, winner_member, &execute_task).await;
    }

    Ok(0.0)
}

/// Competitive protocol: candidates solve the same task, judge picks the best.
///
/// 1. All candidates receive the same task and work independently.
/// 2. The judge reviews all submissions and picks the winner.
#[allow(clippy::too_many_arguments)]
async fn run_competitive(
    app: &Arc<AppState>,
    bus: &Arc<TeamBus>,
    workspace: &Arc<TeamWorkspace>,
    paused: &Arc<AtomicBool>,
    team: &TeamDefinition,
    judge_id: &str,
    candidates: &[String],
    task: &str,
) -> Result<f32, String> {
    info!(team = %team.id, judge = %judge_id, candidates = ?candidates, "Competitive: starting");

    // Phase 1: All candidates run concurrently on the same task.
    let mut handles = Vec::new();
    for cand_id in candidates {
        if let Some(member) = find_member(team, cand_id) {
            let app = app.clone();
            let bus = bus.clone();
            let ws = workspace.clone();
            let paused = paused.clone();
            let team_clone = team.clone();
            let member = member.clone();
            let task = task.to_string();

            let handle = tokio::spawn(async move {
                run_member(&app, &bus, &ws, &paused, &team_clone, &member, &task).await
            });
            handles.push((cand_id.clone(), handle));
        }
    }

    let mut submissions = Vec::new();
    for (id, handle) in handles {
        match handle.await {
            Ok(Ok(result)) => submissions.push((id, result)),
            Ok(Err(e)) => warn!(candidate = %id, error = %e, "Candidate failed"),
            Err(e) => warn!(candidate = %id, error = %e, "Candidate panicked"),
        }
    }

    if submissions.is_empty() {
        return Err("No candidates produced results".to_string());
    }

    // Phase 2: Judge evaluates and picks winner.
    let judge = find_member(team, judge_id)
        .ok_or_else(|| format!("Judge '{}' not found", judge_id))?;

    let submissions_text: String = submissions
        .iter()
        .enumerate()
        .map(|(i, (id, r))| format!("=== Submission {} (by {}) ===\n{}", i + 1, id, r.output))
        .collect::<Vec<_>>()
        .join("\n\n");

    let judge_task = format!(
        "You are the judge. Evaluate these submissions and pick the best one.\n\
         Explain your reasoning.\n\n{}\n\nOriginal task: {}",
        submissions_text, task
    );
    let _ = run_member(app, bus, workspace, paused, team, judge, &judge_task).await;

    Ok(0.0)
}

/// Swarm protocol: self-organizing agents claim tasks from a shared board.
///
/// 1. The first agent analyzes the task and creates a board of subtasks.
/// 2. All agents concurrently pick up ready tasks, execute them, and mark done.
/// 3. Continues until all tasks are done or max_concurrent is exhausted.
async fn run_swarm(
    app: &Arc<AppState>,
    bus: &Arc<TeamBus>,
    workspace: &Arc<TeamWorkspace>,
    paused: &Arc<AtomicBool>,
    team: &TeamDefinition,
    max_concurrent: usize,
    task: &str,
) -> Result<f32, String> {
    info!(team = %team.id, max_concurrent, "Swarm: starting self-organization");

    // Phase 1: First member decomposes the task into board items.
    let first_member = team
        .members
        .first()
        .ok_or_else(|| "Swarm needs at least one member".to_string())?;

    let decompose_task = format!(
        "Decompose this task into independent subtasks that can be worked on in parallel.\n\
         List each subtask on its own line, prefixed with '- '.\n\nTask: {}",
        task
    );
    let decompose_result = run_member(
        app,
        bus,
        workspace,
        paused,
        team,
        first_member,
        &decompose_task,
    )
    .await?;

    // Parse subtasks from the output (lines starting with "- ").
    let subtask_lines: Vec<&str> = decompose_result
        .output
        .lines()
        .filter(|l| l.trim().starts_with("- "))
        .collect();

    // Create workspace tasks.
    for (i, line) in subtask_lines.iter().enumerate() {
        let title = line.trim().trim_start_matches("- ").to_string();
        let ws_task = Task {
            id: format!("swarm-task-{}", i),
            title: title.clone(),
            description: title,
            status: TaskStatus::Ready,
            assigned_to: None,
            created_by: first_member.id.clone(),
            dependencies: Vec::new(),
            subtasks: Vec::new(),
            priority: TaskPriority::Normal,
        };
        bus.emit(TeamEvent::TaskCreated {
            task_id: ws_task.id.clone(),
            title: ws_task.title.clone(),
            assigned_to: None,
        });
        workspace.create_task(ws_task).await;
    }

    // Phase 2: Agents claim and execute tasks in rounds.
    let mut iteration = 0;
    let max_iterations = 10; // Safety limit.

    loop {
        iteration += 1;
        if iteration > max_iterations {
            warn!("Swarm: max iterations reached, stopping");
            break;
        }

        let ready = workspace.ready_tasks().await;
        if ready.is_empty() {
            info!("Swarm: all tasks complete or no ready tasks");
            break;
        }

        // Assign up to max_concurrent tasks.
        let batch: Vec<Task> = ready.into_iter().take(max_concurrent).collect();
        let mut handles = Vec::new();

        for (i, ws_task) in batch.iter().enumerate() {
            let member_idx = i % team.members.len();
            let member = &team.members[member_idx];

            // Mark task as in-progress.
            let old_status = workspace
                .update_task_status(&ws_task.id, TaskStatus::InProgress)
                .await;
            if let Some(old) = old_status {
                bus.emit(TeamEvent::TaskStatusChanged {
                    task_id: ws_task.id.clone(),
                    old_status: old.to_string(),
                    new_status: "in_progress".to_string(),
                });
            }

            let app = app.clone();
            let bus_clone = bus.clone();
            let ws_clone = workspace.clone();
            let paused_clone = paused.clone();
            let team_clone = team.clone();
            let member = member.clone();
            let task_id = ws_task.id.clone();
            let member_task = format!(
                "Complete this subtask: {}\n\nContext (original task): {}",
                ws_task.description, task
            );

            let handle = tokio::spawn(async move {
                let result = run_member(
                    &app,
                    &bus_clone,
                    &ws_clone,
                    &paused_clone,
                    &team_clone,
                    &member,
                    &member_task,
                )
                .await;
                (task_id, result)
            });
            handles.push(handle);
        }

        // Wait for all in-flight tasks.
        for handle in handles {
            if let Ok((task_id, result)) = handle.await {
                let new_status = if result.is_ok() {
                    TaskStatus::Done
                } else {
                    TaskStatus::Failed
                };
                let old = workspace
                    .update_task_status(&task_id, new_status.clone())
                    .await;
                if let Some(old) = old {
                    bus.emit(TeamEvent::TaskStatusChanged {
                        task_id,
                        old_status: old.to_string(),
                        new_status: new_status.to_string(),
                    });
                }
            }
        }
    }

    Ok(0.0)
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Convert a `nexus_agents_core::AgentDefinition` to the local `crate::agents::AgentDefinition`.
///
/// Both types have identical fields and serde representations, so we round-trip
/// through JSON. This bridge exists because the two crates define the same type
/// independently.
fn convert_agent_def(
    core_def: &nexus_agents_core::definition::AgentDefinition,
) -> Result<crate::agents::AgentDefinition, String> {
    let json = serde_json::to_value(core_def).map_err(|e| e.to_string())?;
    serde_json::from_value(json).map_err(|e| e.to_string())
}

/// Truncate a string to `max_len` characters.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}...", &s[..max_len])
    } else {
        s.to_string()
    }
}

/// Extract a brief summary from message content.
fn message_summary(content: &MessageContent) -> String {
    match content {
        MessageContent::TaskAssignment { description, .. } => {
            truncate(description, 100)
        }
        MessageContent::TaskCompletion { summary, .. } => {
            truncate(summary, 100)
        }
        MessageContent::Review { artifact_id, .. } => {
            format!("Review requested for artifact {}", artifact_id)
        }
        MessageContent::ReviewVerdict { verdict, .. } => {
            format!("Review verdict: {:?}", verdict)
        }
        MessageContent::Proposal { title, .. } => {
            truncate(title, 100)
        }
        MessageContent::Vote { vote, .. } => {
            format!("Vote: {:?}", vote)
        }
        MessageContent::Escalation { issue, .. } => {
            truncate(issue, 100)
        }
        MessageContent::ContextShare { topic, .. } => {
            format!("Context: {}", truncate(topic, 80))
        }
        MessageContent::StatusUpdate { summary, .. } => {
            truncate(summary, 100)
        }
        MessageContent::Discussion { content } => {
            truncate(content, 100)
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_agents_core::workspace::{TaskPriority, TaskStatus};

    #[tokio::test]
    async fn test_team_bus_direct_message() {
        let bus = TeamBus::new();
        let mut rx = bus.register_member("alice").await;
        let _bob_rx = bus.register_member("bob").await;

        let msg = AgentMessage {
            id: "msg-1".to_string(),
            from: "bob".to_string(),
            to: MessageTarget::Direct {
                member_id: "alice".to_string(),
            },
            content: MessageContent::Discussion {
                content: "Hello Alice".to_string(),
            },
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            in_reply_to: None,
            priority: MessagePriority::Normal,
        };

        bus.send(msg.clone()).await;

        // Alice should receive the message.
        let received = rx.try_recv();
        assert!(received.is_ok());
        let received_msg = received.unwrap();
        assert_eq!(received_msg.id, "msg-1");
        assert_eq!(received_msg.from, "bob");

        // History should contain the message.
        let history = bus.history().await;
        assert_eq!(history.len(), 1);
    }

    #[tokio::test]
    async fn test_team_bus_broadcast() {
        let bus = TeamBus::new();
        let mut alice_rx = bus.register_member("alice").await;
        let mut bob_rx = bus.register_member("bob").await;
        let _charlie_rx = bus.register_member("charlie").await;

        let msg = AgentMessage {
            id: "msg-2".to_string(),
            from: "charlie".to_string(),
            to: MessageTarget::Broadcast,
            content: MessageContent::StatusUpdate {
                summary: "I am done".to_string(),
                percent_complete: Some(100),
            },
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            in_reply_to: None,
            priority: MessagePriority::Normal,
        };

        bus.send(msg).await;

        // Both alice and bob should receive it (but not charlie, the sender).
        assert!(alice_rx.try_recv().is_ok());
        assert!(bob_rx.try_recv().is_ok());
    }

    #[tokio::test]
    async fn test_team_bus_receive_filter() {
        let bus = TeamBus::new();

        // Send two messages: one to alice, one to bob.
        let msg_to_alice = AgentMessage {
            id: "m1".to_string(),
            from: "lead".to_string(),
            to: MessageTarget::Direct {
                member_id: "alice".to_string(),
            },
            content: MessageContent::Discussion {
                content: "For Alice".to_string(),
            },
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            in_reply_to: None,
            priority: MessagePriority::Normal,
        };
        let msg_to_bob = AgentMessage {
            id: "m2".to_string(),
            from: "lead".to_string(),
            to: MessageTarget::Direct {
                member_id: "bob".to_string(),
            },
            content: MessageContent::Discussion {
                content: "For Bob".to_string(),
            },
            timestamp: "2026-01-01T00:00:01Z".to_string(),
            in_reply_to: None,
            priority: MessagePriority::Normal,
        };

        bus.send(msg_to_alice).await;
        bus.send(msg_to_bob).await;

        let alice_msgs = bus.receive("alice").await;
        assert_eq!(alice_msgs.len(), 1);
        assert_eq!(alice_msgs[0].id, "m1");

        let bob_msgs = bus.receive("bob").await;
        assert_eq!(bob_msgs.len(), 1);
        assert_eq!(bob_msgs[0].id, "m2");
    }

    #[tokio::test]
    async fn test_workspace_task_board() {
        let ws = TeamWorkspace::new();

        let task = Task {
            id: "t1".to_string(),
            title: "Build login page".to_string(),
            description: "Create a login page with email/password".to_string(),
            status: TaskStatus::Ready,
            assigned_to: Some("alice".to_string()),
            created_by: "lead".to_string(),
            dependencies: Vec::new(),
            subtasks: Vec::new(),
            priority: TaskPriority::High,
        };
        ws.create_task(task).await;

        // my_tasks
        let alice_tasks = ws.my_tasks("alice").await;
        assert_eq!(alice_tasks.len(), 1);
        assert_eq!(alice_tasks[0].title, "Build login page");

        let bob_tasks = ws.my_tasks("bob").await;
        assert!(bob_tasks.is_empty());

        // ready_tasks
        let ready = ws.ready_tasks().await;
        assert_eq!(ready.len(), 1);

        // update status
        let old = ws.update_task_status("t1", TaskStatus::InProgress).await;
        assert_eq!(old, Some(TaskStatus::Ready));

        let ready_after = ws.ready_tasks().await;
        assert!(ready_after.is_empty());
    }

    #[tokio::test]
    async fn test_workspace_artifacts() {
        let ws = TeamWorkspace::new();

        let artifact = Artifact {
            id: "a1".to_string(),
            name: "login.tsx".to_string(),
            artifact_type: ArtifactType::Code,
            content: "export function Login() { ... }".to_string(),
            produced_by: "alice".to_string(),
            task_id: Some("t1".to_string()),
            version: 1,
            timestamp: "2026-01-01T00:00:00Z".to_string(),
        };
        ws.publish_artifact(artifact).await;

        let all = ws.get_artifacts().await;
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "login.tsx");

        let alice_artifacts = ws.get_artifacts_by_member("alice").await;
        assert_eq!(alice_artifacts.len(), 1);

        let bob_artifacts = ws.get_artifacts_by_member("bob").await;
        assert!(bob_artifacts.is_empty());
    }

    #[tokio::test]
    async fn test_workspace_state_and_scratchpad() {
        let ws = TeamWorkspace::new();

        // State
        ws.set_state("plan", serde_json::json!("Build a login page"))
            .await;
        let val = ws.get_state("plan").await;
        assert_eq!(val, Some(serde_json::json!("Build a login page")));

        let missing = ws.get_state("nonexistent").await;
        assert!(missing.is_none());

        // Scratchpad
        ws.add_scratchpad("alice", "notes", "Remember to add validation")
            .await;

        let snapshot = ws.snapshot().await;
        assert_eq!(snapshot.scratchpad.len(), 1);
        assert_eq!(snapshot.scratchpad[0].0, "alice");
        assert_eq!(snapshot.scratchpad[0].1, "notes");
        assert_eq!(snapshot.state.len(), 1);
    }

    #[tokio::test]
    async fn test_workspace_snapshot() {
        let ws = TeamWorkspace::new();

        ws.create_task(Task {
            id: "t1".to_string(),
            title: "Task 1".to_string(),
            description: "Desc".to_string(),
            status: TaskStatus::Ready,
            assigned_to: None,
            created_by: "lead".to_string(),
            dependencies: Vec::new(),
            subtasks: Vec::new(),
            priority: TaskPriority::Normal,
        })
        .await;

        ws.publish_artifact(Artifact {
            id: "a1".to_string(),
            name: "plan.md".to_string(),
            artifact_type: ArtifactType::Plan,
            content: "# Plan".to_string(),
            produced_by: "lead".to_string(),
            task_id: None,
            version: 1,
            timestamp: "2026-01-01T00:00:00Z".to_string(),
        })
        .await;

        ws.set_state("key", serde_json::json!(42)).await;
        ws.add_scratchpad("lead", "idea", "Use React").await;

        let snap = ws.snapshot().await;
        assert_eq!(snap.tasks.len(), 1);
        assert_eq!(snap.artifacts.len(), 1);
        assert_eq!(snap.state.len(), 1);
        assert_eq!(snap.scratchpad.len(), 1);
    }

    #[tokio::test]
    async fn test_workspace_dependency_tracking() {
        let ws = TeamWorkspace::new();

        // Task 2 depends on task 1.
        ws.create_task(Task {
            id: "t1".to_string(),
            title: "Setup DB".to_string(),
            description: "Setup database schema".to_string(),
            status: TaskStatus::Backlog,
            assigned_to: Some("alice".to_string()),
            created_by: "lead".to_string(),
            dependencies: Vec::new(),
            subtasks: Vec::new(),
            priority: TaskPriority::High,
        })
        .await;

        ws.create_task(Task {
            id: "t2".to_string(),
            title: "Build API".to_string(),
            description: "Build REST API".to_string(),
            status: TaskStatus::Backlog,
            assigned_to: Some("bob".to_string()),
            created_by: "lead".to_string(),
            dependencies: vec!["t1".to_string()],
            subtasks: Vec::new(),
            priority: TaskPriority::Normal,
        })
        .await;

        // Only t1 should be ready (t2 depends on t1 which is not done).
        let ready = ws.ready_tasks().await;
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "t1");

        // Complete t1.
        ws.update_task_status("t1", TaskStatus::Done).await;

        // Now t2 should be ready.
        let ready = ws.ready_tasks().await;
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "t2");
    }
}
