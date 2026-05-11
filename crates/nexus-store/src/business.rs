//! Business teams and events — CRUD for business_team_instances and business_events.

use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessTeamInstance {
    pub id: String,
    pub project_id: String,
    pub template_name: String,
    pub display_name: String,
    pub autonomy_level: String,
    pub control_mode: String,
    pub status: String,
    pub config_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessEvent {
    pub id: String,
    pub project_id: String,
    pub team_id: Option<String>,
    pub event_type: String,
    pub payload_json: String,
    pub severity: String,
    pub created_at: String,
}

pub struct BusinessService<'c> {
    conn: &'c Connection,
}

impl<'c> BusinessService<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    pub fn create_team(
        &self,
        project_id: &str,
        template_name: &str,
        display_name: &str,
        autonomy_level: &str,
        control_mode: &str,
        config_json: Option<&str>,
    ) -> Result<BusinessTeamInstance> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO business_team_instances (id, project_id, template_name, display_name, autonomy_level, control_mode, config_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![id, project_id, template_name, display_name, autonomy_level, control_mode, config_json, now, now],
        )?;
        Ok(BusinessTeamInstance {
            id,
            project_id: project_id.into(),
            template_name: template_name.into(),
            display_name: display_name.into(),
            autonomy_level: autonomy_level.into(),
            control_mode: control_mode.into(),
            status: "active".into(),
            config_json: config_json.map(String::from),
            created_at: now.clone(),
            updated_at: now,
        })
    }

    pub fn list_teams(&self, project_id: &str) -> Result<Vec<BusinessTeamInstance>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, template_name, display_name, autonomy_level, control_mode, status, config_json, created_at, updated_at
             FROM business_team_instances WHERE project_id=?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![project_id], |r| {
            Ok(BusinessTeamInstance {
                id: r.get(0)?,
                project_id: r.get(1)?,
                template_name: r.get(2)?,
                display_name: r.get(3)?,
                autonomy_level: r.get(4)?,
                control_mode: r.get(5)?,
                status: r.get(6)?,
                config_json: r.get(7)?,
                created_at: r.get(8)?,
                updated_at: r.get(9)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn get_team(&self, team_id: &str) -> Result<Option<BusinessTeamInstance>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, template_name, display_name, autonomy_level, control_mode, status, config_json, created_at, updated_at
             FROM business_team_instances WHERE id=?1",
        )?;
        let mut rows = stmt.query_map(params![team_id], |r| {
            Ok(BusinessTeamInstance {
                id: r.get(0)?,
                project_id: r.get(1)?,
                template_name: r.get(2)?,
                display_name: r.get(3)?,
                autonomy_level: r.get(4)?,
                control_mode: r.get(5)?,
                status: r.get(6)?,
                config_json: r.get(7)?,
                created_at: r.get(8)?,
                updated_at: r.get(9)?,
            })
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn update_team_status(&self, team_id: &str, status: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE business_team_instances SET status=?1, updated_at=?2 WHERE id=?3",
            params![status, now, team_id],
        )?;
        Ok(())
    }

    pub fn update_team_control_mode(&self, team_id: &str, control_mode: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE business_team_instances SET control_mode=?1, updated_at=?2 WHERE id=?3",
            params![control_mode, now, team_id],
        )?;
        Ok(())
    }

    pub fn record_event(
        &self,
        project_id: &str,
        team_id: Option<&str>,
        event_type: &str,
        payload_json: &str,
        severity: &str,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO business_events (id, project_id, team_id, event_type, payload_json, severity, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, project_id, team_id, event_type, payload_json, severity, now],
        )?;
        Ok(id)
    }

    pub fn list_events(&self, project_id: &str, limit: usize) -> Result<Vec<BusinessEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, team_id, event_type, payload_json, severity, created_at
             FROM business_events WHERE project_id=?1 ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![project_id, limit as i64], |r| {
            Ok(BusinessEvent {
                id: r.get(0)?,
                project_id: r.get(1)?,
                team_id: r.get(2)?,
                event_type: r.get(3)?,
                payload_json: r.get(4)?,
                severity: r.get(5)?,
                created_at: r.get(6)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn list_events_by_team(&self, team_id: &str, limit: usize) -> Result<Vec<BusinessEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, team_id, event_type, payload_json, severity, created_at
             FROM business_events WHERE team_id=?1 ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![team_id, limit as i64], |r| {
            Ok(BusinessEvent {
                id: r.get(0)?,
                project_id: r.get(1)?,
                team_id: r.get(2)?,
                event_type: r.get(3)?,
                payload_json: r.get(4)?,
                severity: r.get(5)?,
                created_at: r.get(6)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }
}
