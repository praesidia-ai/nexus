//! Background agent management — long-running autonomous agents.

use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundAgent {
    pub id: String,
    pub project_id: String,
    pub agent_type: String,
    pub config: serde_json::Value,
    pub status: String,
    pub last_run: Option<String>,
    pub run_count: i32,
    pub interval_secs: i32,
    pub created_at: String,
}

pub struct BackgroundAgentService<'c> { conn: &'c Connection }

impl<'c> BackgroundAgentService<'c> {
    pub fn new(conn: &'c Connection) -> Self { Self { conn } }

    pub fn create(&self, project_id: &str, agent_type: &str, config: &serde_json::Value, interval_secs: i32) -> Result<BackgroundAgent> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let config_str = serde_json::to_string(config).unwrap_or_else(|_| "{}".to_string());
        self.conn.execute(
            "INSERT INTO background_agents (id, project_id, agent_type, config, status, interval_secs, created_at)
             VALUES (?1, ?2, ?3, ?4, 'stopped', ?5, ?6)",
            params![id, project_id, agent_type, config_str, interval_secs, now],
        )?;
        Ok(BackgroundAgent { id, project_id: project_id.into(), agent_type: agent_type.into(),
            config: config.clone(), status: "stopped".into(), last_run: None, run_count: 0, interval_secs, created_at: now })
    }

    pub fn list(&self, project_id: &str) -> Result<Vec<BackgroundAgent>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, agent_type, config, status, last_run, run_count, interval_secs, created_at
             FROM background_agents WHERE project_id=?1 ORDER BY created_at DESC"
        )?;
        let rows = stmt.query_map(params![project_id], |r| {
            let config_str: String = r.get(3)?;
            Ok(BackgroundAgent {
                id: r.get(0)?, project_id: r.get(1)?, agent_type: r.get(2)?,
                config: serde_json::from_str(&config_str).unwrap_or_default(),
                status: r.get(4)?, last_run: r.get(5)?, run_count: r.get(6)?,
                interval_secs: r.get(7)?, created_at: r.get(8)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn update_status(&self, id: &str, status: &str) -> Result<()> {
        self.conn.execute("UPDATE background_agents SET status=?1 WHERE id=?2", params![status, id])?;
        Ok(())
    }

    pub fn record_run(&self, id: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute("UPDATE background_agents SET last_run=?1, run_count=run_count+1 WHERE id=?2", params![now, id])?;
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM background_agents WHERE id=?1", params![id])?;
        Ok(())
    }
}
