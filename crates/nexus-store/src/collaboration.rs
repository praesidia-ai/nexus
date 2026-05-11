//! Collaboration -- multi-user + multi-agent session management.

use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollabSession {
    pub id: String,
    pub project_id: String,
    pub created_by: String,
    pub status: String,
    pub created_at: String,
    pub ended_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    pub id: String,
    pub session_id: String,
    pub name: String,
    pub role: String,
    pub participant_type: String,
    pub status: String,
    pub joined_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Activity {
    pub id: String,
    pub session_id: String,
    pub participant_id: String,
    pub activity_type: String,
    pub target: Option<String>,
    pub detail: Option<String>,
    pub created_at: String,
}

pub struct CollabService<'c> {
    conn: &'c Connection,
}

impl<'c> CollabService<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    pub fn create_session(&self, project_id: &str, created_by: &str) -> Result<CollabSession> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO collab_sessions (id, project_id, created_by, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![id, project_id, created_by, now],
        )?;
        Ok(CollabSession {
            id,
            project_id: project_id.into(),
            created_by: created_by.into(),
            status: "active".into(),
            created_at: now,
            ended_at: None,
        })
    }

    pub fn join_session(
        &self,
        session_id: &str,
        name: &str,
        role: &str,
        ptype: &str,
    ) -> Result<Participant> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO collab_participants (id, session_id, name, role, participant_type, joined_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, session_id, name, role, ptype, now],
        )?;
        Ok(Participant {
            id,
            session_id: session_id.into(),
            name: name.into(),
            role: role.into(),
            participant_type: ptype.into(),
            status: "active".into(),
            joined_at: now,
        })
    }

    pub fn list_participants(&self, session_id: &str) -> Result<Vec<Participant>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, name, role, participant_type, status, joined_at FROM collab_participants WHERE session_id=?1",
        )?;
        let rows = stmt.query_map(params![session_id], |r| {
            Ok(Participant {
                id: r.get(0)?,
                session_id: r.get(1)?,
                name: r.get(2)?,
                role: r.get(3)?,
                participant_type: r.get(4)?,
                status: r.get(5)?,
                joined_at: r.get(6)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn post_activity(
        &self,
        session_id: &str,
        participant_id: &str,
        activity_type: &str,
        target: Option<&str>,
        detail: Option<&str>,
    ) -> Result<Activity> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO collab_activity (id, session_id, participant_id, activity_type, target, detail) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, session_id, participant_id, activity_type, target, detail],
        )?;
        Ok(Activity {
            id,
            session_id: session_id.into(),
            participant_id: participant_id.into(),
            activity_type: activity_type.into(),
            target: target.map(String::from),
            detail: detail.map(String::from),
            created_at: now,
        })
    }

    pub fn recent_activity(&self, session_id: &str, limit: i64) -> Result<Vec<Activity>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, participant_id, activity_type, target, detail, created_at
             FROM collab_activity WHERE session_id=?1 ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![session_id, limit], |r| {
            Ok(Activity {
                id: r.get(0)?,
                session_id: r.get(1)?,
                participant_id: r.get(2)?,
                activity_type: r.get(3)?,
                target: r.get(4)?,
                detail: r.get(5)?,
                created_at: r.get(6)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn end_session(&self, session_id: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE collab_sessions SET status='ended', ended_at=?1 WHERE id=?2",
            params![now, session_id],
        )?;
        Ok(())
    }
}
