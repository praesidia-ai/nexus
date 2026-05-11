//! Agent Scheduler — priority-based process scheduling for the Agent OS.
//!
//! The scheduler maintains a process table and a priority-based run queue. It
//! limits concurrency to `max_concurrent` processes and emits [`ProcessEvent`]s
//! via a broadcast channel for observability.
//!
//! ## Enhanced capabilities
//!
//! - **Priority Preemption** — High-priority processes can preempt lower-priority
//!   running processes when resources are constrained.
//! - **Process Health Monitoring** — Heartbeat-based liveness tracking with
//!   configurable degraded/unresponsive thresholds.
//! - **Resource Monitoring** — Per-process resource usage tracking with budget
//!   enforcement and warnings.
//! - **Process Groups** — Group related processes for bulk operations like
//!   cancel-all.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{broadcast, Mutex, RwLock};
use tokio::time::Instant;
use tracing::{info, warn};

use crate::events::ProcessEvent;
use crate::process::{
    AgentProcess, Priority, ProcessError, ProcessId, ProcessInfo, ProcessState, ResourceAllocation,
};
use nexus_agents_core::definition::AgentDefinition;

// ── Health Monitoring ───────────────────────────────────────────────────────

/// Health status of a running process based on heartbeat liveness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    /// Process is reporting heartbeats within the expected interval.
    Healthy,
    /// Process has not reported a heartbeat within the degraded threshold.
    Degraded,
    /// Process has not reported a heartbeat within the unresponsive threshold.
    Unresponsive,
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthStatus::Healthy => write!(f, "healthy"),
            HealthStatus::Degraded => write!(f, "degraded"),
            HealthStatus::Unresponsive => write!(f, "unresponsive"),
        }
    }
}

/// Per-process health tracking data.
#[derive(Debug, Clone)]
pub struct ProcessHealth {
    /// When the last heartbeat was received.
    pub last_heartbeat: Instant,
    /// Current health status.
    pub status: HealthStatus,
}

impl ProcessHealth {
    fn new() -> Self {
        Self {
            last_heartbeat: Instant::now(),
            status: HealthStatus::Healthy,
        }
    }
}

// ── Resource Monitoring ─────────────────────────────────────────────────────

/// Resource usage counters reported by a running process.
#[derive(Debug, Clone, Default)]
pub struct ResourceUsage {
    /// Number of LLM tokens consumed.
    pub tokens_used: u64,
    /// Number of API/tool calls made.
    pub api_calls: u32,
    /// Elapsed wall-clock time in milliseconds.
    pub elapsed_ms: u64,
}

/// Aggregate resource summary across all running processes.
#[derive(Debug, Clone, Default)]
pub struct ResourceSummary {
    /// Total tokens used across all running processes.
    pub total_tokens: u64,
    /// Total API calls across all running processes.
    pub total_api_calls: u32,
    /// Total elapsed milliseconds across all running processes.
    pub total_elapsed_ms: u64,
    /// Number of running processes contributing to these totals.
    pub process_count: usize,
}

// ── Process Groups ──────────────────────────────────────────────────────────

/// A named group of related processes.
#[derive(Debug, Clone)]
pub struct ProcessGroup {
    /// Unique group identifier.
    pub id: String,
    /// Human-readable group name.
    pub name: String,
    /// Process IDs belonging to this group.
    pub members: HashSet<ProcessId>,
}

// ── Scheduler Config ────────────────────────────────────────────────────────

/// Configuration for the agent scheduler.
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Maximum number of processes that can run concurrently.
    pub max_concurrent: usize,
    /// Capacity of the process event broadcast channel.
    pub event_channel_capacity: usize,
    /// Duration without a heartbeat before a process is considered degraded.
    pub health_degraded_timeout: Duration,
    /// Duration without a heartbeat before a process is considered unresponsive
    /// (typically 3x the degraded timeout).
    pub health_unresponsive_timeout: Duration,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 8,
            event_channel_capacity: 256,
            health_degraded_timeout: Duration::from_secs(30),
            health_unresponsive_timeout: Duration::from_secs(90),
        }
    }
}

/// An entry in the priority run queue. Ordered by priority (highest first),
/// then by creation time (earliest first for FIFO within same priority).
#[derive(Debug, Clone)]
struct QueueEntry {
    pid: ProcessId,
    priority: Priority,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl PartialEq for QueueEntry {
    fn eq(&self, other: &Self) -> bool {
        self.pid == other.pid
    }
}

impl Eq for QueueEntry {}

impl PartialOrd for QueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueueEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // Higher priority first; on tie, earlier creation time first.
        self.priority
            .cmp(&other.priority)
            .then_with(|| other.created_at.cmp(&self.created_at))
    }
}

/// The agent scheduler — manages process lifecycle and scheduling.
pub struct AgentScheduler {
    /// All processes indexed by PID.
    process_table: Arc<RwLock<HashMap<ProcessId, AgentProcess>>>,
    /// Priority-based run queue for ready processes.
    run_queue: Arc<Mutex<BinaryHeap<QueueEntry>>>,
    /// Maximum number of concurrently running processes.
    max_concurrent: usize,
    /// Number of currently running processes.
    running_count: Arc<Mutex<usize>>,
    /// Broadcast sender for process events.
    event_sender: broadcast::Sender<ProcessEvent>,
    /// Per-process health tracking.
    health_table: Arc<RwLock<HashMap<ProcessId, ProcessHealth>>>,
    /// Per-process resource usage tracking.
    resource_table: Arc<RwLock<HashMap<ProcessId, ResourceUsage>>>,
    /// Process groups indexed by group ID.
    groups: Arc<RwLock<HashMap<String, ProcessGroup>>>,
    /// Scheduler configuration (for health timeouts).
    config: SchedulerConfig,
}

impl AgentScheduler {
    /// Create a new scheduler with the given configuration.
    pub fn new(config: SchedulerConfig) -> Self {
        let (event_sender, _) = broadcast::channel(config.event_channel_capacity);
        Self {
            process_table: Arc::new(RwLock::new(HashMap::new())),
            run_queue: Arc::new(Mutex::new(BinaryHeap::new())),
            max_concurrent: config.max_concurrent,
            running_count: Arc::new(Mutex::new(0)),
            event_sender,
            health_table: Arc::new(RwLock::new(HashMap::new())),
            resource_table: Arc::new(RwLock::new(HashMap::new())),
            groups: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Subscribe to process lifecycle events.
    pub fn subscribe(&self) -> broadcast::Receiver<ProcessEvent> {
        self.event_sender.subscribe()
    }

    /// Spawn a new process. Returns the PID of the new process.
    ///
    /// When a `High` or `Critical` priority process is spawned and all
    /// concurrency slots are occupied, the scheduler will attempt to preempt
    /// the lowest-priority running process to make room.
    pub async fn spawn(
        &self,
        agent: AgentDefinition,
        task: String,
        priority: Priority,
        resources: ResourceAllocation,
        parent: Option<ProcessId>,
    ) -> Result<ProcessId, ProcessError> {
        let process = AgentProcess::new(agent, task, priority, resources, parent.clone());
        let pid = process.pid.clone();
        let agent_id = process.agent.id.clone();
        let agent_name = process.agent.name.clone();

        // If there's a parent, register as child.
        if let Some(ref parent_pid) = parent {
            let mut table = self.process_table.write().await;
            if let Some(parent_proc) = table.get_mut(parent_pid) {
                parent_proc.children.push(pid.clone());
            }
            table.insert(pid.clone(), process);
        } else {
            let mut table = self.process_table.write().await;
            table.insert(pid.clone(), process);
        }

        // Enqueue for scheduling.
        {
            let mut queue = self.run_queue.lock().await;
            queue.push(QueueEntry {
                pid: pid.clone(),
                priority,
                created_at: chrono::Utc::now(),
            });
        }

        self.emit(ProcessEvent::Spawned {
            pid: pid.clone(),
            agent_id,
            agent_name,
            parent,
            priority,
        })
        .await;

        // For high-priority processes, attempt preemption if needed.
        if priority >= Priority::High {
            self.try_preempt_for(&pid, priority).await;
        }

        // Try to schedule immediately.
        self.try_schedule().await;

        Ok(pid)
    }

    /// Preempt a running process, moving it to Suspended state.
    ///
    /// The preempted process can later be resumed via [`resume`].
    pub async fn preempt(&self, process_id: &str) -> Result<(), ProcessError> {
        self.preempt_internal(process_id, None).await
    }

    /// Internal preemption with an optional "preempted by" PID for event tracking.
    async fn preempt_internal(
        &self,
        process_id: &str,
        preempted_by: Option<&str>,
    ) -> Result<(), ProcessError> {
        let (old_state, agent_id) = {
            let mut table = self.process_table.write().await;
            let process = table
                .get_mut(process_id)
                .ok_or_else(|| ProcessError::NotFound {
                    pid: process_id.to_string(),
                })?;
            let agent_id = process.agent.id.clone();
            let old = process.transition_to(ProcessState::Suspended)?;
            (old, agent_id)
        };

        if old_state == ProcessState::Running {
            let mut count = self.running_count.lock().await;
            *count = count.saturating_sub(1);
        }

        if let Some(preemptor) = preempted_by {
            self.emit(ProcessEvent::Preempted {
                pid: process_id.to_string(),
                agent_id,
                preempted_by: preemptor.to_string(),
            })
            .await;
        } else {
            self.emit(ProcessEvent::Suspended {
                pid: process_id.to_string(),
                agent_id,
            })
            .await;
        }

        Ok(())
    }

    /// Suspend a running or ready process.
    pub async fn suspend(&self, pid: &str) -> Result<(), ProcessError> {
        let (old_state, agent_id) = {
            let mut table = self.process_table.write().await;
            let process = table
                .get_mut(pid)
                .ok_or_else(|| ProcessError::NotFound { pid: pid.to_string() })?;
            let agent_id = process.agent.id.clone();
            let old = process.transition_to(ProcessState::Suspended)?;
            (old, agent_id)
        };

        if old_state == ProcessState::Running {
            let mut count = self.running_count.lock().await;
            *count = count.saturating_sub(1);
        }

        self.emit(ProcessEvent::Suspended {
            pid: pid.to_string(),
            agent_id,
        })
        .await;

        // Suspending a process may free a slot.
        self.try_schedule().await;

        Ok(())
    }

    /// Resume a suspended or sleeping process.
    pub async fn resume(&self, pid: &str) -> Result<(), ProcessError> {
        let (priority, agent_id, created_at) = {
            let mut table = self.process_table.write().await;
            let process = table
                .get_mut(pid)
                .ok_or_else(|| ProcessError::NotFound { pid: pid.to_string() })?;
            process.transition_to(ProcessState::Ready)?;
            process.wake_condition = None;
            (process.priority, process.agent.id.clone(), process.created_at)
        };

        // Re-enqueue for scheduling.
        {
            let mut queue = self.run_queue.lock().await;
            queue.push(QueueEntry {
                pid: pid.to_string(),
                priority,
                created_at,
            });
        }

        self.emit(ProcessEvent::Resumed {
            pid: pid.to_string(),
            agent_id,
        })
        .await;

        self.try_schedule().await;

        Ok(())
    }

    /// Kill a process immediately.
    pub async fn kill(&self, pid: &str, reason: String) -> Result<(), ProcessError> {
        let (old_state, agent_id) = {
            let mut table = self.process_table.write().await;
            let process = table
                .get_mut(pid)
                .ok_or_else(|| ProcessError::NotFound { pid: pid.to_string() })?;
            let agent_id = process.agent.id.clone();
            let old = process.transition_to(ProcessState::Killed)?;
            (old, agent_id)
        };

        if old_state == ProcessState::Running {
            let mut count = self.running_count.lock().await;
            *count = count.saturating_sub(1);
        }

        // Clean up health and resource tracking.
        {
            let mut health = self.health_table.write().await;
            health.remove(pid);
        }
        {
            let mut resources = self.resource_table.write().await;
            resources.remove(pid);
        }

        self.emit(ProcessEvent::Killed {
            pid: pid.to_string(),
            agent_id,
            reason,
        })
        .await;

        // Killing a process may free a slot.
        self.try_schedule().await;

        Ok(())
    }

    /// Mark a process as completed with an optional output.
    pub async fn complete(&self, pid: &str, output: Option<String>) -> Result<(), ProcessError> {
        let (agent_id, duration_secs, tokens_used, cost_used) = {
            let mut table = self.process_table.write().await;
            let process = table
                .get_mut(pid)
                .ok_or_else(|| ProcessError::NotFound { pid: pid.to_string() })?;
            process.context.output = output.clone();
            process.transition_to(ProcessState::Completed)?;
            let duration = process
                .started_at
                .map(|s| (process.updated_at - s).num_milliseconds() as f64 / 1000.0)
                .unwrap_or(0.0);
            (
                process.agent.id.clone(),
                duration,
                process.context.tokens_used,
                process.context.cost_used,
            )
        };

        {
            let mut count = self.running_count.lock().await;
            *count = count.saturating_sub(1);
        }

        // Clean up health and resource tracking.
        {
            let mut health = self.health_table.write().await;
            health.remove(pid);
        }
        {
            let mut resources = self.resource_table.write().await;
            resources.remove(pid);
        }

        self.emit(ProcessEvent::Completed {
            pid: pid.to_string(),
            agent_id,
            output,
            duration_secs,
            tokens_used,
            cost_used,
        })
        .await;

        self.try_schedule().await;

        Ok(())
    }

    /// Mark a process as failed with an error message.
    pub async fn fail(&self, pid: &str, error: String) -> Result<(), ProcessError> {
        let (agent_id, duration_secs) = {
            let mut table = self.process_table.write().await;
            let process = table
                .get_mut(pid)
                .ok_or_else(|| ProcessError::NotFound { pid: pid.to_string() })?;
            process.context.error = Some(error.clone());
            process.transition_to(ProcessState::Failed)?;
            let duration = process
                .started_at
                .map(|s| (process.updated_at - s).num_milliseconds() as f64 / 1000.0)
                .unwrap_or(0.0);
            (process.agent.id.clone(), duration)
        };

        {
            let mut count = self.running_count.lock().await;
            *count = count.saturating_sub(1);
        }

        // Clean up health and resource tracking.
        {
            let mut health = self.health_table.write().await;
            health.remove(pid);
        }
        {
            let mut resources = self.resource_table.write().await;
            resources.remove(pid);
        }

        self.emit(ProcessEvent::Failed {
            pid: pid.to_string(),
            agent_id,
            error,
            duration_secs,
        })
        .await;

        self.try_schedule().await;

        Ok(())
    }

    /// List all processes as public `ProcessInfo` views.
    pub async fn list_processes(&self) -> Vec<ProcessInfo> {
        let table = self.process_table.read().await;
        table.values().map(ProcessInfo::from).collect()
    }

    /// Get a single process by PID.
    pub async fn get_process(&self, pid: &str) -> Result<ProcessInfo, ProcessError> {
        let table = self.process_table.read().await;
        table
            .get(pid)
            .map(ProcessInfo::from)
            .ok_or_else(|| ProcessError::NotFound { pid: pid.to_string() })
    }

    /// Wait for a process to reach a terminal state. Returns the final `ProcessInfo`.
    pub async fn wait(&self, pid: &str) -> Result<ProcessInfo, ProcessError> {
        // Check if already terminal.
        {
            let table = self.process_table.read().await;
            if let Some(p) = table.get(pid) {
                if p.state.is_terminal() {
                    return Ok(ProcessInfo::from(p));
                }
            } else {
                return Err(ProcessError::NotFound { pid: pid.to_string() });
            }
        }

        // Subscribe and wait for the terminal event.
        let mut rx = self.event_sender.subscribe();
        let target_pid = pid.to_string();

        loop {
            match rx.recv().await {
                Ok(event) => {
                    let is_terminal = match &event {
                        ProcessEvent::Completed { pid, .. } => *pid == target_pid,
                        ProcessEvent::Failed { pid, .. } => *pid == target_pid,
                        ProcessEvent::Killed { pid, .. } => *pid == target_pid,
                        _ => false,
                    };
                    if is_terminal {
                        let table = self.process_table.read().await;
                        return table
                            .get(&target_pid)
                            .map(ProcessInfo::from)
                            .ok_or_else(|| ProcessError::NotFound {
                                pid: target_pid.clone(),
                            });
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("Scheduler event receiver lagged by {} events", n);
                    // Check if already terminal after lag.
                    let table = self.process_table.read().await;
                    if let Some(p) = table.get(&target_pid) {
                        if p.state.is_terminal() {
                            return Ok(ProcessInfo::from(p));
                        }
                    }
                }
                Err(broadcast::error::RecvError::Closed) => {
                    return Err(ProcessError::SchedulerError(
                        "event channel closed".to_string(),
                    ));
                }
            }
        }
    }

    /// Change the priority of a process. If the process is ready, it will be
    /// re-queued with the new priority.
    pub async fn set_priority(
        &self,
        pid: &str,
        new_priority: Priority,
    ) -> Result<(), ProcessError> {
        let (old_priority, state, created_at) = {
            let mut table = self.process_table.write().await;
            let process = table
                .get_mut(pid)
                .ok_or_else(|| ProcessError::NotFound { pid: pid.to_string() })?;
            let old = process.priority;
            process.priority = new_priority;
            process.updated_at = chrono::Utc::now();
            (old, process.state.clone(), process.created_at)
        };

        // If the process is ready, we need to rebuild the queue since BinaryHeap
        // doesn't support efficient priority updates. We drain and re-add.
        if state == ProcessState::Ready {
            let mut queue = self.run_queue.lock().await;
            let entries: Vec<QueueEntry> = std::iter::from_fn(|| queue.pop()).collect();
            for mut entry in entries {
                if entry.pid == pid {
                    entry.priority = new_priority;
                    entry.created_at = created_at;
                }
                queue.push(entry);
            }
        }

        self.emit(ProcessEvent::PriorityChanged {
            pid: pid.to_string(),
            from: old_priority,
            to: new_priority,
        })
        .await;

        Ok(())
    }

    /// The number of currently running processes.
    pub async fn running_count(&self) -> usize {
        *self.running_count.lock().await
    }

    /// The total number of processes (all states).
    pub async fn process_count(&self) -> usize {
        self.process_table.read().await.len()
    }

    // ── Health Monitoring ───────────────────────────────────────────────────

    /// Record a heartbeat for a running process.
    ///
    /// Processes should call this periodically to indicate they are alive.
    /// If no heartbeat is received within the configured timeout, the process
    /// will be marked as `Degraded` or `Unresponsive`.
    pub async fn heartbeat(&self, process_id: &str) -> Result<(), ProcessError> {
        // Verify the process exists and is running.
        {
            let table = self.process_table.read().await;
            let process = table
                .get(process_id)
                .ok_or_else(|| ProcessError::NotFound {
                    pid: process_id.to_string(),
                })?;
            if process.state != ProcessState::Running {
                return Err(ProcessError::SchedulerError(format!(
                    "cannot heartbeat process {} in state {:?}",
                    process_id, process.state
                )));
            }
        }

        let mut health = self.health_table.write().await;
        let entry = health
            .entry(process_id.to_string())
            .or_insert_with(ProcessHealth::new);
        entry.last_heartbeat = Instant::now();
        entry.status = HealthStatus::Healthy;

        Ok(())
    }

    /// Scan all running processes and evaluate their health based on heartbeat
    /// timestamps. Returns a list of `(pid, HealthStatus)` pairs.
    ///
    /// This also emits `HealthChanged` events when a process transitions
    /// between health states.
    pub async fn check_health(&self) -> Vec<(String, HealthStatus)> {
        let now = Instant::now();
        let mut results = Vec::new();

        // Collect running process PIDs and agent IDs.
        let running: Vec<(String, String)> = {
            let table = self.process_table.read().await;
            table
                .values()
                .filter(|p| p.state == ProcessState::Running)
                .map(|p| (p.pid.clone(), p.agent.id.clone()))
                .collect()
        };

        let mut health_table = self.health_table.write().await;

        for (pid, agent_id) in &running {
            let entry = health_table
                .entry(pid.clone())
                .or_insert_with(ProcessHealth::new);

            let elapsed = now.duration_since(entry.last_heartbeat);
            let old_status = entry.status;

            let new_status = if elapsed >= self.config.health_unresponsive_timeout {
                HealthStatus::Unresponsive
            } else if elapsed >= self.config.health_degraded_timeout {
                HealthStatus::Degraded
            } else {
                HealthStatus::Healthy
            };

            entry.status = new_status;
            results.push((pid.clone(), new_status));

            if old_status != new_status {
                // Emit outside the lock — collect events to emit after.
                // We'll emit inline since we only hold health_table, not event_sender.
                let _ = self.event_sender.send(ProcessEvent::HealthChanged {
                    pid: pid.clone(),
                    agent_id: agent_id.clone(),
                    status: new_status.to_string(),
                });
            }
        }

        results
    }

    // ── Resource Monitoring ─────────────────────────────────────────────────

    /// Report resource usage for a running process. The reported values are
    /// accumulated (added to existing counters).
    ///
    /// If the process exceeds its allocated resource limits, a
    /// `ResourceBudgetWarning` event is emitted.
    pub async fn report_usage(
        &self,
        process_id: &str,
        usage: ResourceUsage,
    ) -> Result<(), ProcessError> {
        let (agent_id, max_tokens) = {
            let table = self.process_table.read().await;
            let process = table
                .get(process_id)
                .ok_or_else(|| ProcessError::NotFound {
                    pid: process_id.to_string(),
                })?;
            (process.agent.id.clone(), process.resources.max_tokens)
        };

        let total_tokens = {
            let mut resource_table = self.resource_table.write().await;
            let entry = resource_table
                .entry(process_id.to_string())
                .or_default();
            entry.tokens_used += usage.tokens_used;
            entry.api_calls += usage.api_calls;
            entry.elapsed_ms += usage.elapsed_ms;
            entry.tokens_used
        };

        // Also update the process context token counter.
        {
            let mut table = self.process_table.write().await;
            if let Some(process) = table.get_mut(process_id) {
                process.context.tokens_used += usage.tokens_used;
            }
        }

        // Budget enforcement: warn if tokens exceed allocation.
        if total_tokens > max_tokens {
            self.emit(ProcessEvent::ResourceBudgetWarning {
                pid: process_id.to_string(),
                agent_id,
                resource: "tokens".to_string(),
                used: total_tokens,
                limit: max_tokens,
            })
            .await;
        }

        Ok(())
    }

    /// Get an aggregate resource summary across all running processes.
    pub async fn get_resource_summary(&self) -> ResourceSummary {
        let resource_table = self.resource_table.read().await;
        let table = self.process_table.read().await;

        let running_pids: HashSet<&String> = table
            .values()
            .filter(|p| p.state == ProcessState::Running)
            .map(|p| &p.pid)
            .collect();

        let mut summary = ResourceSummary::default();
        for (pid, usage) in resource_table.iter() {
            if running_pids.contains(pid) {
                summary.total_tokens += usage.tokens_used;
                summary.total_api_calls += usage.api_calls;
                summary.total_elapsed_ms += usage.elapsed_ms;
                summary.process_count += 1;
            }
        }

        summary
    }

    // ── Process Groups ──────────────────────────────────────────────────────

    /// Create a new process group with the given name. Returns the group ID.
    pub async fn create_group(&self, name: &str) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let group = ProcessGroup {
            id: id.clone(),
            name: name.to_string(),
            members: HashSet::new(),
        };
        let mut groups = self.groups.write().await;
        groups.insert(id.clone(), group);
        id
    }

    /// Add a process to an existing group.
    pub async fn add_to_group(
        &self,
        process_id: &str,
        group_id: &str,
    ) -> Result<(), ProcessError> {
        // Verify the process exists.
        {
            let table = self.process_table.read().await;
            if !table.contains_key(process_id) {
                return Err(ProcessError::NotFound {
                    pid: process_id.to_string(),
                });
            }
        }

        let mut groups = self.groups.write().await;
        let group = groups.get_mut(group_id).ok_or_else(|| {
            ProcessError::SchedulerError(format!("group {} not found", group_id))
        })?;
        group.members.insert(process_id.to_string());
        Ok(())
    }

    /// Cancel all processes in a group. Non-terminal processes are killed.
    /// Returns the list of process IDs that were cancelled.
    pub async fn cancel_group(&self, group_id: &str) -> Result<Vec<ProcessId>, ProcessError> {
        let (group_name, members) = {
            let groups = self.groups.read().await;
            let group = groups.get(group_id).ok_or_else(|| {
                ProcessError::SchedulerError(format!("group {} not found", group_id))
            })?;
            (group.name.clone(), group.members.clone())
        };

        let mut cancelled = Vec::new();

        for pid in &members {
            let is_non_terminal = {
                let table = self.process_table.read().await;
                table.get(pid).map(|p| !p.state.is_terminal()).unwrap_or(false)
            };

            if is_non_terminal {
                if let Ok(()) = self
                    .kill(pid, format!("group {} cancelled", group_name))
                    .await
                {
                    cancelled.push(pid.clone());
                }
            }
        }

        self.emit(ProcessEvent::GroupCancelled {
            group_id: group_id.to_string(),
            group_name,
            cancelled_pids: cancelled.clone(),
        })
        .await;

        Ok(cancelled)
    }

    /// List all process groups.
    pub async fn list_groups(&self) -> Vec<(String, String, usize)> {
        let groups = self.groups.read().await;
        groups
            .values()
            .map(|g| (g.id.clone(), g.name.clone(), g.members.len()))
            .collect()
    }

    // ── Internal methods ────────────────────────────────────────────────────

    /// If concurrency is at capacity, try to preempt the lowest-priority running
    /// process to make room for a high-priority process.
    async fn try_preempt_for(&self, new_pid: &str, new_priority: Priority) {
        let at_capacity = {
            let count = self.running_count.lock().await;
            *count >= self.max_concurrent
        };

        if !at_capacity {
            return;
        }

        // Find the lowest-priority running process that is lower than new_priority.
        let victim: Option<(String, Priority)> = {
            let table = self.process_table.read().await;
            table
                .values()
                .filter(|p| p.state == ProcessState::Running && p.pid != new_pid)
                .min_by_key(|p| p.priority)
                .filter(|p| p.priority < new_priority)
                .map(|p| (p.pid.clone(), p.priority))
        };

        if let Some((victim_pid, victim_priority)) = victim {
            info!(
                new_pid = new_pid,
                victim_pid = %victim_pid,
                new_priority = ?new_priority,
                victim_priority = ?victim_priority,
                "Preempting lower-priority process"
            );
            // Ignore errors — the victim may have already completed.
            let _ = self.preempt_internal(&victim_pid, Some(new_pid)).await;
        }
    }

    /// Try to schedule the next process from the run queue.
    async fn try_schedule(&self) {
        loop {
            let can_run = {
                let count = self.running_count.lock().await;
                *count < self.max_concurrent
            };

            if !can_run {
                return;
            }

            let entry = {
                let mut queue = self.run_queue.lock().await;
                queue.pop()
            };

            let entry = match entry {
                Some(e) => e,
                None => return,
            };

            // Verify the process is still in a schedulable state.
            let should_start = {
                let table = self.process_table.read().await;
                table
                    .get(&entry.pid)
                    .map(|p| p.state.can_run())
                    .unwrap_or(false)
            };

            if should_start {
                self.start_process(&entry.pid).await;
            }
            // If not schedulable (e.g., already killed), skip and try next.
        }
    }

    /// Transition a process to Running and increment the running count.
    async fn start_process(&self, pid: &str) {
        let agent_id = {
            let mut table = self.process_table.write().await;
            if let Some(process) = table.get_mut(pid) {
                match process.transition_to(ProcessState::Running) {
                    Ok(_) => Some(process.agent.id.clone()),
                    Err(e) => {
                        warn!("Failed to start process {}: {}", pid, e);
                        None
                    }
                }
            } else {
                None
            }
        };

        if let Some(agent_id) = agent_id {
            {
                let mut count = self.running_count.lock().await;
                *count += 1;
            }

            // Initialize health tracking for the newly started process.
            {
                let mut health = self.health_table.write().await;
                health.insert(pid.to_string(), ProcessHealth::new());
            }

            info!(pid = pid, agent_id = %agent_id, "Process started");

            self.emit(ProcessEvent::Started {
                pid: pid.to_string(),
                agent_id,
            })
            .await;
        }
    }

    /// Emit a process event to all subscribers.
    async fn emit(&self, event: ProcessEvent) {
        // Ignore send errors — means no active receivers.
        let _ = self.event_sender.send(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_agents_core::definition::{AgentDefinition, ExecutionMode};

    fn test_agent(id: &str, name: &str) -> AgentDefinition {
        AgentDefinition {
            id: id.to_string(),
            name: name.to_string(),
            system_prompt: "Test agent".to_string(),
            skills: vec![],
            tools: vec![],
            model_preference: None,
            execution_mode: ExecutionMode::Interactive,
            max_iterations: 10,
            timeout_secs: 60,
        }
    }

    #[tokio::test]
    async fn test_spawn_and_list() {
        let scheduler = AgentScheduler::new(SchedulerConfig::default());
        let agent = test_agent("test-1", "TestAgent");

        let pid = scheduler
            .spawn(
                agent,
                "do something".to_string(),
                Priority::Normal,
                ResourceAllocation::default(),
                None,
            )
            .await
            .unwrap();

        let processes = scheduler.list_processes().await;
        assert_eq!(processes.len(), 1);
        assert_eq!(processes[0].pid, pid);
        // Should have been auto-scheduled to Running.
        assert_eq!(processes[0].state, ProcessState::Running);
    }

    #[tokio::test]
    async fn test_spawn_respects_max_concurrent() {
        let config = SchedulerConfig {
            max_concurrent: 1,
            event_channel_capacity: 64,
            ..Default::default()
        };
        let scheduler = AgentScheduler::new(config);

        let pid1 = scheduler
            .spawn(
                test_agent("a1", "Agent1"),
                "task1".to_string(),
                Priority::Normal,
                ResourceAllocation::default(),
                None,
            )
            .await
            .unwrap();

        let pid2 = scheduler
            .spawn(
                test_agent("a2", "Agent2"),
                "task2".to_string(),
                Priority::Normal,
                ResourceAllocation::default(),
                None,
            )
            .await
            .unwrap();

        let p1 = scheduler.get_process(&pid1).await.unwrap();
        let p2 = scheduler.get_process(&pid2).await.unwrap();

        assert_eq!(p1.state, ProcessState::Running);
        assert_eq!(p2.state, ProcessState::Ready);
    }

    #[tokio::test]
    async fn test_kill_process() {
        let scheduler = AgentScheduler::new(SchedulerConfig::default());
        let pid = scheduler
            .spawn(
                test_agent("a1", "Agent1"),
                "task".to_string(),
                Priority::Normal,
                ResourceAllocation::default(),
                None,
            )
            .await
            .unwrap();

        scheduler.kill(&pid, "test kill".to_string()).await.unwrap();

        let info = scheduler.get_process(&pid).await.unwrap();
        assert_eq!(info.state, ProcessState::Killed);
    }

    #[tokio::test]
    async fn test_complete_frees_slot() {
        let config = SchedulerConfig {
            max_concurrent: 1,
            event_channel_capacity: 64,
            ..Default::default()
        };
        let scheduler = AgentScheduler::new(config);

        let pid1 = scheduler
            .spawn(
                test_agent("a1", "Agent1"),
                "task1".to_string(),
                Priority::Normal,
                ResourceAllocation::default(),
                None,
            )
            .await
            .unwrap();

        let pid2 = scheduler
            .spawn(
                test_agent("a2", "Agent2"),
                "task2".to_string(),
                Priority::Normal,
                ResourceAllocation::default(),
                None,
            )
            .await
            .unwrap();

        // pid2 should be Ready since max_concurrent=1.
        assert_eq!(
            scheduler.get_process(&pid2).await.unwrap().state,
            ProcessState::Ready
        );

        // Complete pid1 — pid2 should be scheduled.
        scheduler
            .complete(&pid1, Some("done".to_string()))
            .await
            .unwrap();

        assert_eq!(
            scheduler.get_process(&pid1).await.unwrap().state,
            ProcessState::Completed
        );
        assert_eq!(
            scheduler.get_process(&pid2).await.unwrap().state,
            ProcessState::Running
        );
    }

    #[tokio::test]
    async fn test_suspend_and_resume() {
        let scheduler = AgentScheduler::new(SchedulerConfig::default());
        let pid = scheduler
            .spawn(
                test_agent("a1", "Agent1"),
                "task".to_string(),
                Priority::Normal,
                ResourceAllocation::default(),
                None,
            )
            .await
            .unwrap();

        scheduler.suspend(&pid).await.unwrap();
        assert_eq!(
            scheduler.get_process(&pid).await.unwrap().state,
            ProcessState::Suspended
        );

        scheduler.resume(&pid).await.unwrap();
        // After resume, it goes to Ready then gets auto-scheduled to Running.
        assert_eq!(
            scheduler.get_process(&pid).await.unwrap().state,
            ProcessState::Running
        );
    }

    #[tokio::test]
    async fn test_priority_scheduling() {
        let config = SchedulerConfig {
            max_concurrent: 1,
            event_channel_capacity: 64,
            ..Default::default()
        };
        let scheduler = AgentScheduler::new(config);

        // Fill the slot with a High process so it won't be preempted.
        let pid_blocking = scheduler
            .spawn(
                test_agent("blocker", "Blocker"),
                "blocking".to_string(),
                Priority::High,
                ResourceAllocation::default(),
                None,
            )
            .await
            .unwrap();

        // Spawn low and normal priority (neither can preempt the High blocker).
        let _pid_low = scheduler
            .spawn(
                test_agent("low", "Low"),
                "low task".to_string(),
                Priority::Low,
                ResourceAllocation::default(),
                None,
            )
            .await
            .unwrap();

        let pid_normal = scheduler
            .spawn(
                test_agent("normal", "NormalAgent"),
                "normal task".to_string(),
                Priority::Normal,
                ResourceAllocation::default(),
                None,
            )
            .await
            .unwrap();

        // Complete the blocker — normal priority should run next (higher than low).
        scheduler
            .complete(&pid_blocking, None)
            .await
            .unwrap();

        let normal_info = scheduler.get_process(&pid_normal).await.unwrap();
        assert_eq!(normal_info.state, ProcessState::Running);
    }

    #[tokio::test]
    async fn test_set_priority() {
        let scheduler = AgentScheduler::new(SchedulerConfig::default());
        let mut rx = scheduler.subscribe();

        let pid = scheduler
            .spawn(
                test_agent("a1", "Agent1"),
                "task".to_string(),
                Priority::Normal,
                ResourceAllocation::default(),
                None,
            )
            .await
            .unwrap();

        scheduler.set_priority(&pid, Priority::Critical).await.unwrap();

        // Drain events to find the PriorityChanged event.
        let mut found = false;
        while let Ok(event) = rx.try_recv() {
            if let ProcessEvent::PriorityChanged { pid: ep, from, to, .. } = event {
                if ep == pid {
                    assert_eq!(from, Priority::Normal);
                    assert_eq!(to, Priority::Critical);
                    found = true;
                }
            }
        }
        assert!(found, "should have received PriorityChanged event");
    }

    #[tokio::test]
    async fn test_wait_for_completion() {
        let scheduler = Arc::new(AgentScheduler::new(SchedulerConfig::default()));
        let pid = scheduler
            .spawn(
                test_agent("a1", "Agent1"),
                "task".to_string(),
                Priority::Normal,
                ResourceAllocation::default(),
                None,
            )
            .await
            .unwrap();

        let scheduler_clone = scheduler.clone();
        let pid_clone = pid.clone();

        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            scheduler_clone
                .complete(&pid_clone, Some("result".to_string()))
                .await
                .unwrap();
        });

        let info = scheduler.wait(&pid).await.unwrap();
        assert_eq!(info.state, ProcessState::Completed);
    }

    #[tokio::test]
    async fn test_parent_child_relationship() {
        let scheduler = AgentScheduler::new(SchedulerConfig::default());

        let parent_pid = scheduler
            .spawn(
                test_agent("parent", "Parent"),
                "parent task".to_string(),
                Priority::Normal,
                ResourceAllocation::default(),
                None,
            )
            .await
            .unwrap();

        let child_pid = scheduler
            .spawn(
                test_agent("child", "Child"),
                "child task".to_string(),
                Priority::Normal,
                ResourceAllocation::default(),
                Some(parent_pid.clone()),
            )
            .await
            .unwrap();

        let parent_info = scheduler.get_process(&parent_pid).await.unwrap();
        assert_eq!(parent_info.children, vec![child_pid.clone()]);

        let child_info = scheduler.get_process(&child_pid).await.unwrap();
        assert_eq!(child_info.parent, Some(parent_pid));
    }

    #[tokio::test]
    async fn test_not_found_error() {
        let scheduler = AgentScheduler::new(SchedulerConfig::default());
        let result = scheduler.get_process("nonexistent").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ProcessError::NotFound { .. }));
    }

    // ── New tests for enhanced features ─────────────────────────────────────

    #[tokio::test]
    async fn test_preemption_high_priority_preempts_normal() {
        // With max_concurrent=1, spawning a High process should preempt
        // the running Normal process.
        let config = SchedulerConfig {
            max_concurrent: 1,
            event_channel_capacity: 64,
            ..Default::default()
        };
        let scheduler = AgentScheduler::new(config);
        let mut rx = scheduler.subscribe();

        // Spawn a normal-priority process (takes the only slot).
        let pid_normal = scheduler
            .spawn(
                test_agent("normal", "Normal"),
                "normal task".to_string(),
                Priority::Normal,
                ResourceAllocation::default(),
                None,
            )
            .await
            .unwrap();

        assert_eq!(
            scheduler.get_process(&pid_normal).await.unwrap().state,
            ProcessState::Running
        );

        // Spawn a high-priority process — should preempt the normal one.
        let pid_high = scheduler
            .spawn(
                test_agent("high", "High"),
                "high task".to_string(),
                Priority::High,
                ResourceAllocation::default(),
                None,
            )
            .await
            .unwrap();

        // The normal process should now be suspended.
        assert_eq!(
            scheduler.get_process(&pid_normal).await.unwrap().state,
            ProcessState::Suspended
        );
        // The high process should be running.
        assert_eq!(
            scheduler.get_process(&pid_high).await.unwrap().state,
            ProcessState::Running
        );

        // Verify we got a Preempted event.
        let mut found_preempted = false;
        while let Ok(event) = rx.try_recv() {
            if let ProcessEvent::Preempted {
                pid,
                preempted_by,
                ..
            } = event
            {
                if pid == pid_normal && preempted_by == pid_high {
                    found_preempted = true;
                }
            }
        }
        assert!(found_preempted, "should have received Preempted event");
    }

    #[tokio::test]
    async fn test_health_monitoring_heartbeat_and_check() {
        let config = SchedulerConfig {
            max_concurrent: 8,
            event_channel_capacity: 64,
            health_degraded_timeout: Duration::from_millis(50),
            health_unresponsive_timeout: Duration::from_millis(150),
        };
        let scheduler = AgentScheduler::new(config);

        let pid = scheduler
            .spawn(
                test_agent("a1", "Agent1"),
                "task".to_string(),
                Priority::Normal,
                ResourceAllocation::default(),
                None,
            )
            .await
            .unwrap();

        // Immediately after spawn, process should be healthy.
        scheduler.heartbeat(&pid).await.unwrap();
        let health = scheduler.check_health().await;
        assert_eq!(health.len(), 1);
        assert_eq!(health[0].0, pid);
        assert_eq!(health[0].1, HealthStatus::Healthy);

        // Wait past the degraded timeout, then check.
        tokio::time::sleep(Duration::from_millis(70)).await;
        let health = scheduler.check_health().await;
        assert_eq!(health[0].1, HealthStatus::Degraded);

        // Send a heartbeat — should go back to healthy.
        scheduler.heartbeat(&pid).await.unwrap();
        let health = scheduler.check_health().await;
        assert_eq!(health[0].1, HealthStatus::Healthy);

        // Wait past the unresponsive timeout.
        tokio::time::sleep(Duration::from_millis(160)).await;
        let health = scheduler.check_health().await;
        assert_eq!(health[0].1, HealthStatus::Unresponsive);
    }

    #[tokio::test]
    async fn test_resource_usage_tracking_and_budget_warning() {
        let scheduler = AgentScheduler::new(SchedulerConfig::default());
        let mut rx = scheduler.subscribe();

        let pid = scheduler
            .spawn(
                test_agent("a1", "Agent1"),
                "task".to_string(),
                Priority::Normal,
                ResourceAllocation {
                    max_tokens: 1000,
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();

        // Report usage within budget.
        scheduler
            .report_usage(
                &pid,
                ResourceUsage {
                    tokens_used: 500,
                    api_calls: 5,
                    elapsed_ms: 1000,
                },
            )
            .await
            .unwrap();

        let summary = scheduler.get_resource_summary().await;
        assert_eq!(summary.total_tokens, 500);
        assert_eq!(summary.total_api_calls, 5);
        assert_eq!(summary.process_count, 1);

        // Report usage exceeding budget — should trigger warning.
        scheduler
            .report_usage(
                &pid,
                ResourceUsage {
                    tokens_used: 600,
                    api_calls: 3,
                    elapsed_ms: 500,
                },
            )
            .await
            .unwrap();

        let summary = scheduler.get_resource_summary().await;
        assert_eq!(summary.total_tokens, 1100);
        assert_eq!(summary.total_api_calls, 8);

        // Check for ResourceBudgetWarning event.
        let mut found_warning = false;
        while let Ok(event) = rx.try_recv() {
            if let ProcessEvent::ResourceBudgetWarning {
                pid: ep,
                used,
                limit,
                ..
            } = event
            {
                if ep == pid {
                    assert_eq!(used, 1100);
                    assert_eq!(limit, 1000);
                    found_warning = true;
                }
            }
        }
        assert!(found_warning, "should have received ResourceBudgetWarning event");
    }

    #[tokio::test]
    async fn test_process_groups_create_add_cancel() {
        let scheduler = AgentScheduler::new(SchedulerConfig::default());

        // Create a group.
        let group_id = scheduler.create_group("team-alpha").await;

        // Spawn two processes and add to group.
        let pid1 = scheduler
            .spawn(
                test_agent("a1", "Agent1"),
                "task1".to_string(),
                Priority::Normal,
                ResourceAllocation::default(),
                None,
            )
            .await
            .unwrap();

        let pid2 = scheduler
            .spawn(
                test_agent("a2", "Agent2"),
                "task2".to_string(),
                Priority::Normal,
                ResourceAllocation::default(),
                None,
            )
            .await
            .unwrap();

        scheduler.add_to_group(&pid1, &group_id).await.unwrap();
        scheduler.add_to_group(&pid2, &group_id).await.unwrap();

        // Verify the group has 2 members.
        let groups = scheduler.list_groups().await;
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].1, "team-alpha");
        assert_eq!(groups[0].2, 2);

        // Cancel the group — both processes should be killed.
        let cancelled = scheduler.cancel_group(&group_id).await.unwrap();
        assert_eq!(cancelled.len(), 2);

        assert_eq!(
            scheduler.get_process(&pid1).await.unwrap().state,
            ProcessState::Killed
        );
        assert_eq!(
            scheduler.get_process(&pid2).await.unwrap().state,
            ProcessState::Killed
        );
    }

    #[tokio::test]
    async fn test_preempt_explicit() {
        let scheduler = AgentScheduler::new(SchedulerConfig::default());

        let pid = scheduler
            .spawn(
                test_agent("a1", "Agent1"),
                "task".to_string(),
                Priority::Normal,
                ResourceAllocation::default(),
                None,
            )
            .await
            .unwrap();

        assert_eq!(
            scheduler.get_process(&pid).await.unwrap().state,
            ProcessState::Running
        );

        // Explicitly preempt.
        scheduler.preempt(&pid).await.unwrap();

        assert_eq!(
            scheduler.get_process(&pid).await.unwrap().state,
            ProcessState::Suspended
        );

        // Should be resumable.
        scheduler.resume(&pid).await.unwrap();
        assert_eq!(
            scheduler.get_process(&pid).await.unwrap().state,
            ProcessState::Running
        );
    }
}
