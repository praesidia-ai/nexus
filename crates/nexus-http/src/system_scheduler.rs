//! System Scheduler — priority-aware task scheduling replacing polling loops.
//!
//! Replaces the 30-second polling in `background_executor.rs` with a proper
//! scheduler that supports:
//! - Priority levels (1=highest, 10=lowest)
//! - Conflict groups (tasks in same group don't run concurrently)
//! - Dynamic interval adjustment
//! - Persistent state (survives restarts)
//!
//! Built-in tasks:
//! - `evolution_cycle` — run agent evolution analysis
//! - `skill_extraction` — extract skill DNA from recent executions
//! - `agent_mutation` — check and promote agent versions
//! - `quality_audit` — run taste scoring on active projects
//! - `metric_cleanup` — prune old metrics data

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::state::AppState;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub id: String,
    pub name: String,
    pub task_type: String,
    pub config: serde_json::Value,
    pub interval_secs: i64,
    pub priority: i64,
    pub conflict_group: Option<String>,
    pub status: TaskStatus,
    pub last_run: Option<String>,
    pub last_result: Option<String>,
    pub last_error: Option<String>,
    pub run_count: i64,
    pub fail_count: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Active,
    Paused,
    Disabled,
}

/// Handler function for a task type.
#[async_trait::async_trait]
pub trait TaskHandler: Send + Sync {
    async fn execute(&self, app: &Arc<AppState>, config: &serde_json::Value) -> Result<String, String>;
}

// ---------------------------------------------------------------------------
// Scheduler
// ---------------------------------------------------------------------------

pub struct SystemScheduler {
    app: Arc<AppState>,
    handlers: HashMap<String, Box<dyn TaskHandler>>,
    running_groups: Arc<Mutex<HashSet<String>>>,
}

impl SystemScheduler {
    pub fn new(app: Arc<AppState>) -> Self {
        Self {
            app,
            handlers: HashMap::new(),
            running_groups: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Register a handler for a task type.
    pub fn register(&mut self, task_type: &str, handler: Box<dyn TaskHandler>) {
        self.handlers.insert(task_type.to_string(), handler);
    }

    /// Seed default scheduled tasks (idempotent).
    pub async fn seed_defaults(&self) {
        let defaults = vec![
            ("evolution_cycle", "Evolution Cycle", 600, 3, Some("evolution")),
            ("skill_extraction", "Skill DNA Extraction", 900, 4, Some("evolution")),
            ("agent_mutation", "Agent Version Check", 1800, 5, Some("evolution")),
            ("quality_audit", "Quality Audit", 3600, 6, None),
            ("metric_cleanup", "Metric Cleanup", 86400, 10, None),
        ];

        let db = self.app.db.lock().await;
        for (task_type, name, interval, priority, conflict_group) in defaults {
            let id = format!("builtin_{}", task_type);
            let _ = db.execute(
                "INSERT OR IGNORE INTO scheduled_tasks (id, name, task_type, interval_secs, priority, conflict_group, status)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active')",
                rusqlite::params![id, name, task_type, interval, priority, conflict_group],
            );
        }
    }

    /// Run one scheduling tick — check for due tasks and execute them.
    pub async fn tick(&self) {
        let tasks = self.get_due_tasks().await;
        if tasks.is_empty() {
            return;
        }

        for task in tasks {
            // Check conflict group — insert then drop the lock BEFORE running
            // the handler so we do not hold `running_groups` across `.await`.
            if let Some(ref group) = task.conflict_group {
                let mut running = self.running_groups.lock().await;
                if running.contains(group) {
                    continue; // Skip — another task in this group is running
                }
                running.insert(group.clone());
                drop(running);
            }

            let handler = match self.handlers.get(&task.task_type) {
                Some(h) => h,
                None => {
                    warn!(task_type = %task.task_type, "No handler registered for task type");
                    // Release the group we just reserved, otherwise it is stuck.
                    if let Some(group) = task.conflict_group.as_ref() {
                        self.running_groups.lock().await.remove(group);
                    }
                    continue;
                }
            };

            let app = self.app.clone();
            let task_id = task.id.clone();
            let task_type = task.task_type.clone();
            let conflict_group = task.conflict_group.clone();
            let running_groups = self.running_groups.clone();

            // Execute the handler. A panic inside the future would prevent the
            // conflict-group release below from running, wedging the scheduler.
            // We intentionally keep the await on a single line here; the
            // `_release` guard ensures cleanup regardless of early return.
            struct GroupGuard {
                group: Option<String>,
                running: Arc<tokio::sync::Mutex<std::collections::HashSet<String>>>,
            }
            impl Drop for GroupGuard {
                fn drop(&mut self) {
                    if let Some(group) = self.group.take() {
                        // Spawn a short task because Drop isn't async — the
                        // Mutex is tokio so we use try_lock then fall back.
                        let running = self.running.clone();
                        tokio::spawn(async move {
                            running.lock().await.remove(&group);
                        });
                    }
                }
            }
            let _release = GroupGuard {
                group: conflict_group.clone(),
                running: running_groups.clone(),
            };

            info!(task = %task.name, task_type = %task_type, "Running scheduled task");

            let result = handler.execute(&app, &task.config).await;

            // Explicit release for the happy path (deterministic ordering).
            drop(_release);

            // Record result
            self.record_result(&task_id, &result).await;
        }
    }

    /// Get tasks that are due to run.
    async fn get_due_tasks(&self) -> Vec<ScheduledTask> {
        let db = self.app.db.lock().await;
        let mut stmt = match db.prepare(
            "SELECT id, name, task_type, config, interval_secs, priority, conflict_group,
                    status, last_run, last_result, last_error, run_count, fail_count
             FROM scheduled_tasks
             WHERE status = 'active'
               AND (last_run IS NULL
                    OR datetime(last_run, '+' || interval_secs || ' seconds') <= datetime('now'))
             ORDER BY priority ASC, last_run ASC"
        ) {
            Ok(s) => s,
            Err(e) => {
                warn!("Failed to query scheduled tasks: {}", e);
                return Vec::new();
            }
        };

        let rows = stmt.query_map([], |row| {
            Ok(ScheduledTask {
                id: row.get(0)?,
                name: row.get(1)?,
                task_type: row.get(2)?,
                config: serde_json::from_str(&row.get::<_, String>(3).unwrap_or_default())
                    .unwrap_or(serde_json::Value::Null),
                interval_secs: row.get(4)?,
                priority: row.get(5)?,
                conflict_group: row.get(6)?,
                status: TaskStatus::Active,
                last_run: row.get(8)?,
                last_result: row.get(9)?,
                last_error: row.get(10)?,
                run_count: row.get(11)?,
                fail_count: row.get(12)?,
            })
        }).ok();

        rows.map(|iter| iter.filter_map(|r| r.ok()).collect()).unwrap_or_default()
    }

    /// Record the result of a task execution.
    async fn record_result(&self, task_id: &str, result: &Result<String, String>) {
        let db = self.app.db.lock().await;
        match result {
            Ok(_) => {
                let _ = db.execute(
                    "UPDATE scheduled_tasks SET
                        last_run = datetime('now'),
                        last_result = 'success',
                        last_error = NULL,
                        run_count = run_count + 1,
                        updated_at = datetime('now')
                     WHERE id = ?1",
                    rusqlite::params![task_id],
                );
            }
            Err(e) => {
                let _ = db.execute(
                    "UPDATE scheduled_tasks SET
                        last_run = datetime('now'),
                        last_result = 'failure',
                        last_error = ?2,
                        run_count = run_count + 1,
                        fail_count = fail_count + 1,
                        updated_at = datetime('now')
                     WHERE id = ?1",
                    rusqlite::params![task_id, e],
                );
            }
        }
    }
}

/// Start the unified scheduler as a background tokio task.
///
/// This replaces both the old `background_executor` polling loop and the
/// separate `system_scheduler` loop — one tick loop runs everything.
pub fn start_scheduler(app: Arc<AppState>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // Wait for server init
        tokio::time::sleep(Duration::from_secs(5)).await;

        let scheduler = SystemScheduler::new(app.clone());
        scheduler.seed_defaults().await;

        info!("Unified scheduler started (15s tick)");

        let mut interval = tokio::time::interval(Duration::from_secs(15));
        loop {
            interval.tick().await;

            // Tick scheduled_tasks table
            scheduler.tick().await;

            // Also tick background_agents table (merged from background_executor)
            if let Err(e) = crate::background_executor::tick(&app).await {
                warn!(error = %e, "Background agent tick failed");
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Built-in task handlers
// ---------------------------------------------------------------------------

pub struct EvolutionCycleHandler;

#[async_trait::async_trait]
impl TaskHandler for EvolutionCycleHandler {
    async fn execute(&self, app: &Arc<AppState>, _config: &serde_json::Value) -> Result<String, String> {
        // Trigger evolution cycle for all agent roles
        let roles = ["architect", "coder", "reviewer", "tester", "debugger"];
        let mut promotions = 0;

        for role in &roles {
            if let Some(_id) = crate::agent_versioning::check_promotion(app, role).await {
                promotions += 1;
            }
        }

        // Promote ready skills
        crate::skill_dna::promote_if_ready(app).await;

        Ok(format!("Evolution cycle complete. {} agent promotions.", promotions))
    }
}

pub struct MetricCleanupHandler;

#[async_trait::async_trait]
impl TaskHandler for MetricCleanupHandler {
    async fn execute(&self, app: &Arc<AppState>, _config: &serde_json::Value) -> Result<String, String> {
        // Clean up old data (keep 30 days)
        let db = app.db.lock().await;

        let deleted = db.execute(
            "DELETE FROM quality_gate_log WHERE created_at < datetime('now', '-30 days')",
            [],
        ).unwrap_or(0);

        Ok(format!("Cleaned up {} old records", deleted))
    }
}
