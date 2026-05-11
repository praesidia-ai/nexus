//! SQLite persistence layer for workflow instances and step states.
//!
//! All workflow state is stored in two tables:
//! - `workflow_instances` — one row per running/completed workflow
//! - `workflow_step_states` — one row per step per instance

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};

use crate::engine::{StepState, StepStatus, WorkflowInstance, WorkflowStatus};
use crate::error::WorkflowError;

/// SQLite-backed persistence for workflow instances.
pub struct WorkflowStore {
    /// Path to the SQLite database file.
    db_path: PathBuf,
}

impl WorkflowStore {
    /// Create a new store pointing at `data_dir/workflows.db`.
    pub fn new(data_dir: &Path) -> Self {
        Self {
            db_path: data_dir.join("workflows.db"),
        }
    }

    /// Open a connection to the database.
    fn conn(&self) -> Result<Connection, WorkflowError> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
        Ok(conn)
    }

    /// Create tables if they do not exist yet. Idempotent.
    pub fn init_tables(&self) -> Result<(), WorkflowError> {
        let conn = self.conn()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS workflow_instances (
                id              TEXT PRIMARY KEY,
                definition_id   TEXT NOT NULL,
                definition_json TEXT NOT NULL,
                status          TEXT NOT NULL DEFAULT 'running',
                context         TEXT NOT NULL DEFAULT '{}',
                started_at      TEXT NOT NULL,
                updated_at      TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS workflow_step_states (
                instance_id     TEXT NOT NULL,
                step_id         TEXT NOT NULL,
                status          TEXT NOT NULL DEFAULT 'pending',
                started_at      TEXT,
                completed_at    TEXT,
                result          TEXT,
                error           TEXT,
                retries         INTEGER NOT NULL DEFAULT 0,
                execution_count INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (instance_id, step_id),
                FOREIGN KEY (instance_id) REFERENCES workflow_instances(id)
            );

            CREATE INDEX IF NOT EXISTS idx_step_states_instance
                ON workflow_step_states(instance_id);
            CREATE INDEX IF NOT EXISTS idx_instances_status
                ON workflow_instances(status);",
        )?;
        Ok(())
    }

    /// Persist a new workflow instance (insert).
    ///
    /// The instance row and all step-state rows are written inside a single
    /// transaction so a crash mid-save can never leave a workflow with
    /// missing steps (which would make `load_instance` return an instance
    /// `advance()` could never complete).
    pub fn save_instance(&self, instance: &WorkflowInstance) -> Result<(), WorkflowError> {
        let mut conn = self.conn()?;
        let def_json = serde_json::to_string(&instance.definition)?;
        let ctx_json = serde_json::to_string(&instance.context)?;

        let tx = conn.transaction()?;

        tx.execute(
            "INSERT INTO workflow_instances (id, definition_id, definition_json, status, context, started_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
                status = excluded.status,
                context = excluded.context,
                updated_at = excluded.updated_at",
            params![
                instance.id,
                instance.definition_id,
                def_json,
                instance.status.as_str(),
                ctx_json,
                instance.started_at,
                instance.updated_at,
            ],
        )?;

        // Upsert step states inside the same tx so partial state is impossible.
        {
            let mut stmt = tx.prepare(
                "INSERT INTO workflow_step_states (instance_id, step_id, status, started_at, completed_at, result, error, retries, execution_count)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(instance_id, step_id) DO UPDATE SET
                    status = excluded.status,
                    started_at = excluded.started_at,
                    completed_at = excluded.completed_at,
                    result = excluded.result,
                    error = excluded.error,
                    retries = excluded.retries,
                    execution_count = excluded.execution_count",
            )?;

            for (step_id, state) in &instance.step_states {
                let result_json = state
                    .result
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?;
                stmt.execute(params![
                    instance.id,
                    step_id,
                    state.status.as_str(),
                    state.started_at,
                    state.completed_at,
                    result_json,
                    state.error,
                    state.retries,
                    state.execution_count,
                ])?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    /// Load a single workflow instance by ID.
    pub fn load_instance(&self, id: &str) -> Result<WorkflowInstance, WorkflowError> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT id, definition_id, definition_json, status, context, started_at, updated_at
             FROM workflow_instances WHERE id = ?1",
        )?;

        let instance = stmt
            .query_row(params![id], |row| {
                let id: String = row.get(0)?;
                let definition_id: String = row.get(1)?;
                let def_json: String = row.get(2)?;
                let status_str: String = row.get(3)?;
                let ctx_json: String = row.get(4)?;
                let started_at: String = row.get(5)?;
                let updated_at: String = row.get(6)?;
                Ok((id, definition_id, def_json, status_str, ctx_json, started_at, updated_at))
            })
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    WorkflowError::NotFound(format!("workflow instance {id} not found"))
                }
                other => WorkflowError::Storage(other.to_string()),
            })?;

        let (inst_id, definition_id, def_json, status_str, ctx_json, started_at, updated_at) = instance;

        let definition = serde_json::from_str(&def_json)
            .map_err(|e| WorkflowError::Storage(format!("corrupt definition json: {e}")))?;
        let context = serde_json::from_str(&ctx_json)
            .map_err(|e| WorkflowError::Storage(format!("corrupt context json: {e}")))?;
        let status = WorkflowStatus::parse(&status_str);

        // Load step states
        let mut step_stmt = conn.prepare(
            "SELECT step_id, status, started_at, completed_at, result, error, retries, execution_count
             FROM workflow_step_states WHERE instance_id = ?1",
        )?;

        let step_states: std::collections::HashMap<String, StepState> = step_stmt
            .query_map(params![inst_id], |row| {
                let step_id: String = row.get(0)?;
                let status_str: String = row.get(1)?;
                let started_at: Option<String> = row.get(2)?;
                let completed_at: Option<String> = row.get(3)?;
                let result_json: Option<String> = row.get(4)?;
                let error: Option<String> = row.get(5)?;
                let retries: u32 = row.get(6)?;
                let execution_count: u32 = row.get(7)?;
                Ok((step_id, status_str, started_at, completed_at, result_json, error, retries, execution_count))
            })?
            .filter_map(|r| r.ok())
            .map(|(step_id, status_str, started_at, completed_at, result_json, error, retries, execution_count)| {
                let result = result_json
                    .and_then(|j| serde_json::from_str(&j).ok());
                let state = StepState {
                    status: StepStatus::parse(&status_str),
                    started_at,
                    completed_at,
                    result,
                    error,
                    retries,
                    execution_count,
                };
                (step_id, state)
            })
            .collect();

        Ok(WorkflowInstance {
            id: inst_id,
            definition_id,
            definition,
            status,
            step_states,
            context,
            started_at,
            updated_at,
        })
    }

    /// List all workflow instances, ordered by most recent first.
    pub fn list_instances(&self) -> Result<Vec<WorkflowInstance>, WorkflowError> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            "SELECT id FROM workflow_instances ORDER BY updated_at DESC",
        )?;

        let ids: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        // Re-use load_instance for each (opens its own connection but that's fine
        // for correctness; optimise later if needed).
        let mut instances = Vec::with_capacity(ids.len());
        for id in ids {
            instances.push(self.load_instance(&id)?);
        }
        Ok(instances)
    }

    /// Update a single step's state within an instance.
    pub fn update_step(
        &self,
        instance_id: &str,
        step_id: &str,
        state: &StepState,
    ) -> Result<(), WorkflowError> {
        let conn = self.conn()?;
        let result_json = state
            .result
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;

        let rows = conn.execute(
            "UPDATE workflow_step_states
             SET status = ?1, started_at = ?2, completed_at = ?3, result = ?4, error = ?5, retries = ?6, execution_count = ?7
             WHERE instance_id = ?8 AND step_id = ?9",
            params![
                state.status.as_str(),
                state.started_at,
                state.completed_at,
                result_json,
                state.error,
                state.retries,
                state.execution_count,
                instance_id,
                step_id,
            ],
        )?;

        if rows == 0 {
            return Err(WorkflowError::NotFound(format!(
                "step {step_id} in instance {instance_id} not found"
            )));
        }
        Ok(())
    }

    /// Update just the instance-level status and updated_at.
    pub fn update_instance_status(
        &self,
        instance_id: &str,
        status: &WorkflowStatus,
        context: &serde_json::Value,
    ) -> Result<(), WorkflowError> {
        let conn = self.conn()?;
        let ctx_json = serde_json::to_string(context)?;
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "UPDATE workflow_instances SET status = ?1, context = ?2, updated_at = ?3 WHERE id = ?4",
            params![status.as_str(), ctx_json, now, instance_id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definition::*;
    use tempfile::TempDir;

    fn sample_definition() -> crate::definition::WorkflowDefinition {
        WorkflowDefinition {
            id: "test-def".to_string(),
            name: "Test Workflow".to_string(),
            steps: vec![WorkflowStep {
                id: "step1".to_string(),
                name: "Step One".to_string(),
                action: StepAction::Command {
                    cmd: "echo hello".to_string(),
                    cwd: None,
                },
                depends_on: vec![],
                condition: None,
                retry: RetryPolicy::default(),
                timeout_secs: 60,
                timeout_ms: None,
                parallel: false,
                parallel_group: None,
                max_iterations: None,
            }],
            on_error: ErrorPolicy::FailFast,
            version: 1,
            template: false,
        }
    }

    #[test]
    fn test_save_and_load_instance() {
        let tmp = TempDir::new().unwrap();
        let store = WorkflowStore::new(tmp.path());
        store.init_tables().unwrap();

        let now = chrono::Utc::now().to_rfc3339();
        let mut step_states = std::collections::HashMap::new();
        step_states.insert(
            "step1".to_string(),
            StepState {
                status: StepStatus::Pending,
                started_at: None,
                completed_at: None,
                result: None,
                error: None,
                retries: 0,
                execution_count: 0,
            },
        );

        let instance = WorkflowInstance {
            id: "inst-1".to_string(),
            definition_id: "test-def".to_string(),
            definition: sample_definition(),
            status: WorkflowStatus::Running,
            step_states,
            context: serde_json::json!({"key": "value"}),
            started_at: now.clone(),
            updated_at: now,
        };

        store.save_instance(&instance).unwrap();

        let loaded = store.load_instance("inst-1").unwrap();
        assert_eq!(loaded.id, "inst-1");
        assert_eq!(loaded.definition_id, "test-def");
        assert_eq!(loaded.status, WorkflowStatus::Running);
        assert_eq!(loaded.step_states.len(), 1);
        assert_eq!(
            loaded.step_states["step1"].status,
            StepStatus::Pending
        );
    }

    #[test]
    fn test_update_step() {
        let tmp = TempDir::new().unwrap();
        let store = WorkflowStore::new(tmp.path());
        store.init_tables().unwrap();

        let now = chrono::Utc::now().to_rfc3339();
        let mut step_states = std::collections::HashMap::new();
        step_states.insert(
            "s1".to_string(),
            StepState {
                status: StepStatus::Pending,
                started_at: None,
                completed_at: None,
                result: None,
                error: None,
                retries: 0,
                execution_count: 0,
            },
        );

        let def = sample_definition();
        let instance = WorkflowInstance {
            id: "inst-2".to_string(),
            definition_id: def.id.clone(),
            definition: def,
            status: WorkflowStatus::Running,
            step_states,
            context: serde_json::json!({}),
            started_at: now.clone(),
            updated_at: now.clone(),
        };
        store.save_instance(&instance).unwrap();

        let updated_state = StepState {
            status: StepStatus::Completed,
            started_at: Some(now.clone()),
            completed_at: Some(now),
            result: Some(serde_json::json!({"output": "done"})),
            error: None,
            retries: 0,
            execution_count: 1,
        };
        store.update_step("inst-2", "s1", &updated_state).unwrap();

        let loaded = store.load_instance("inst-2").unwrap();
        assert_eq!(loaded.step_states["s1"].status, StepStatus::Completed);
        assert!(loaded.step_states["s1"].result.is_some());
    }

    #[test]
    fn test_list_instances() {
        let tmp = TempDir::new().unwrap();
        let store = WorkflowStore::new(tmp.path());
        store.init_tables().unwrap();

        let now = chrono::Utc::now().to_rfc3339();
        let def = sample_definition();

        for i in 0..3 {
            let instance = WorkflowInstance {
                id: format!("inst-list-{i}"),
                definition_id: def.id.clone(),
                definition: def.clone(),
                status: WorkflowStatus::Running,
                step_states: std::collections::HashMap::new(),
                context: serde_json::json!({}),
                started_at: now.clone(),
                updated_at: now.clone(),
            };
            store.save_instance(&instance).unwrap();
        }

        let all = store.list_instances().unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_not_found() {
        let tmp = TempDir::new().unwrap();
        let store = WorkflowStore::new(tmp.path());
        store.init_tables().unwrap();

        let result = store.load_instance("nonexistent");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), WorkflowError::NotFound(_)));
    }
}
