//! Coding tasks — parallel feature development with smart agents.
//!
//! Each task follows the cycle:
//! 1. **Understand** — Agent reads existing code context
//! 2. **Plan** — Agent produces a structured plan (files to create/modify)
//! 3. **Implement** — Agent generates code
//! 4. **Validate** — Agent reviews its own output for bugs/security
//! 5. **Complete** — Output is ready for the user

use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;

// ---------------------------------------------------------------------------
// CodingTask
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodingTask {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub description: String,
    pub agent: String,
    pub status: String,
    pub status_detail: Option<String>,
    pub priority: i32,
    pub plan_json: Option<serde_json::Value>,
    pub files_json: Option<serde_json::Value>,
    pub output: Option<String>,
    pub review_json: Option<serde_json::Value>,
    pub error: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewCodingTask {
    pub title: String,
    pub description: String,
    pub agent: Option<String>,
    pub priority: Option<i32>,
}

// ---------------------------------------------------------------------------
// CodingTaskService
// ---------------------------------------------------------------------------

pub struct CodingTaskService<'c> {
    conn: &'c Connection,
}

impl<'c> CodingTaskService<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    pub fn create_task(
        &self,
        project_id: &str,
        input: &NewCodingTask,
    ) -> Result<CodingTask> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let agent = input.agent.as_deref().unwrap_or("nova");
        let priority = input.priority.unwrap_or(3);

        self.conn.execute(
            "INSERT INTO coding_tasks (id, project_id, title, description, agent, status, priority, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6, ?7, ?7)",
            params![id, project_id, input.title, input.description, agent, priority, now],
        )?;

        Ok(CodingTask {
            id, project_id: project_id.into(),
            title: input.title.clone(), description: input.description.clone(),
            agent: agent.into(), status: "pending".into(), status_detail: None,
            priority, plan_json: None, files_json: None, output: None,
            review_json: None, error: None, started_at: None, completed_at: None,
            created_at: now.clone(), updated_at: now,
        })
    }

    pub fn update_status(&self, task_id: &str, status: &str, detail: Option<&str>) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let started = if status != "pending" { Some(now.clone()) } else { None };
        let completed = if status == "completed" || status == "failed" { Some(now.clone()) } else { None };

        self.conn.execute(
            "UPDATE coding_tasks SET status = ?1, status_detail = ?2, updated_at = ?3,
             started_at = COALESCE(started_at, ?4), completed_at = COALESCE(?5, completed_at)
             WHERE id = ?6",
            params![status, detail, now, started, completed, task_id],
        )?;
        Ok(())
    }

    pub fn set_plan(&self, task_id: &str, plan: &serde_json::Value) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE coding_tasks SET plan_json = ?1, updated_at = ?2 WHERE id = ?3",
            params![plan.to_string(), now, task_id],
        )?;
        Ok(())
    }

    pub fn set_files(&self, task_id: &str, files: &serde_json::Value) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE coding_tasks SET files_json = ?1, updated_at = ?2 WHERE id = ?3",
            params![files.to_string(), now, task_id],
        )?;
        Ok(())
    }

    pub fn set_output(&self, task_id: &str, output: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE coding_tasks SET output = ?1, updated_at = ?2 WHERE id = ?3",
            params![output, now, task_id],
        )?;
        Ok(())
    }

    pub fn set_review(&self, task_id: &str, review: &serde_json::Value) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE coding_tasks SET review_json = ?1, updated_at = ?2 WHERE id = ?3",
            params![review.to_string(), now, task_id],
        )?;
        Ok(())
    }

    pub fn set_error(&self, task_id: &str, error: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE coding_tasks SET status = 'failed', error = ?1, completed_at = ?2, updated_at = ?2 WHERE id = ?3",
            params![error, now, task_id],
        )?;
        Ok(())
    }

    pub fn get_task(&self, task_id: &str) -> Result<Option<CodingTask>> {
        let result = self.conn.query_row(
            "SELECT id, project_id, title, description, agent, status, status_detail, priority,
                    plan_json, files_json, output, review_json, error, started_at, completed_at,
                    created_at, updated_at
             FROM coding_tasks WHERE id = ?1",
            params![task_id],
            Self::row_to_task,
        );
        match result {
            Ok(t) => Ok(Some(t)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn list_tasks(&self, project_id: &str) -> Result<Vec<CodingTask>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, title, description, agent, status, status_detail, priority,
                    plan_json, files_json, output, review_json, error, started_at, completed_at,
                    created_at, updated_at
             FROM coding_tasks WHERE project_id = ?1 ORDER BY priority ASC, created_at DESC",
        )?;
        let rows = stmt.query_map(params![project_id], Self::row_to_task)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn list_active_tasks(&self, project_id: &str) -> Result<Vec<CodingTask>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, title, description, agent, status, status_detail, priority,
                    plan_json, files_json, output, review_json, error, started_at, completed_at,
                    created_at, updated_at
             FROM coding_tasks WHERE project_id = ?1 AND status NOT IN ('completed', 'failed')
             ORDER BY priority ASC, created_at ASC",
        )?;
        let rows = stmt.query_map(params![project_id], Self::row_to_task)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn delete_task(&self, task_id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM coding_tasks WHERE id = ?1", params![task_id])?;
        Ok(())
    }

    fn row_to_task(row: &rusqlite::Row) -> rusqlite::Result<CodingTask> {
        let plan_str: Option<String> = row.get(8)?;
        let files_str: Option<String> = row.get(9)?;
        let review_str: Option<String> = row.get(11)?;
        Ok(CodingTask {
            id: row.get(0)?,
            project_id: row.get(1)?,
            title: row.get(2)?,
            description: row.get(3)?,
            agent: row.get(4)?,
            status: row.get(5)?,
            status_detail: row.get(6)?,
            priority: row.get(7)?,
            plan_json: plan_str.and_then(|s| serde_json::from_str(&s).ok()),
            files_json: files_str.and_then(|s| serde_json::from_str(&s).ok()),
            output: row.get(10)?,
            review_json: review_str.and_then(|s| serde_json::from_str(&s).ok()),
            error: row.get(12)?,
            started_at: row.get(13)?,
            completed_at: row.get(14)?,
            created_at: row.get(15)?,
            updated_at: row.get(16)?,
        })
    }
}
