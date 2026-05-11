use std::path::Path;

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::open_connection;
use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRow {
    pub id: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRow {
    pub id: String,
    pub summary: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRow {
    pub id: String,
    pub kind: String,
    pub path: Option<String>,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub session_id: String,
    pub title: Option<String>,
    pub messages: Vec<MessageRow>,
    pub decisions: Vec<DecisionRow>,
    pub artifacts: Vec<ArtifactRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterializeResult {
    pub agent_yaml_path: String,
    pub policy_yaml_path: String,
    pub artifact_ids: Vec<String>,
}

pub struct NexusSessionService {
    conn: Connection,
}

impl NexusSessionService {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = open_connection(path)?;
        Ok(Self { conn })
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::open(Path::new(":memory:"))
    }

    pub fn ensure_session(&self, session_id: Option<&str>, title: Option<&str>) -> Result<String> {
        let sid = session_id
            .map(String::from)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT OR IGNORE INTO sessions (id, created_at, title) VALUES (?1, ?2, ?3)",
            params![&sid, &now, title],
        )?;
        Ok(sid)
    }

    pub fn append_message(&self, session_id: &str, role: &str, content: &str) -> Result<String> {
        self.ensure_session(Some(session_id), None)?;
        let mid = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO messages (id, session_id, role, content, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![&mid, session_id, role, content, &now],
        )?;
        Ok(mid)
    }

    pub fn record_decision(&self, session_id: &str, summary: &str) -> Result<String> {
        self.ensure_session(Some(session_id), None)?;
        let did = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO decisions (id, session_id, summary, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![&did, session_id, summary, &now],
        )?;
        Ok(did)
    }

    pub fn propose_artifact(
        &self,
        session_id: &str,
        kind: &str,
        path: Option<&str>,
        content_preview: Option<&str>,
    ) -> Result<String> {
        self.ensure_session(Some(session_id), None)?;
        let aid = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO artifacts (id, session_id, kind, path, content, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5, 'proposed', ?6)",
            params![
                &aid,
                session_id,
                kind,
                path,
                content_preview,
                &now
            ],
        )?;
        Ok(aid)
    }

    pub fn commit_artifact_path(&self, artifact_id: &str, path: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE artifacts SET path = ?1, status = 'committed' WHERE id = ?2",
            params![path, artifact_id],
        )?;
        Ok(())
    }

    pub fn list_artifacts(&self, session_id: &str) -> Result<Vec<ArtifactRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, path, status, created_at FROM artifacts WHERE session_id = ?1 ORDER BY created_at",
        )?;
        let rows = stmt.query_map(params![session_id], |row| {
            Ok(ArtifactRow {
                id: row.get(0)?,
                kind: row.get(1)?,
                path: row.get(2)?,
                status: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get_session_state(&self, session_id: &str) -> Result<SessionState> {
        let title: Option<String> = self
            .conn
            .query_row(
                "SELECT title FROM sessions WHERE id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .optional()?;

        let mut mstmt = self.conn.prepare(
            "SELECT id, role, content, created_at FROM messages WHERE session_id = ?1 ORDER BY created_at",
        )?;
        let messages: Vec<MessageRow> = mstmt
            .query_map(params![session_id], |row| {
                Ok(MessageRow {
                    id: row.get(0)?,
                    role: row.get(1)?,
                    content: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let mut dstmt = self.conn.prepare(
            "SELECT id, summary, created_at FROM decisions WHERE session_id = ?1 ORDER BY created_at",
        )?;
        let decisions: Vec<DecisionRow> = dstmt
            .query_map(params![session_id], |row| {
                Ok(DecisionRow {
                    id: row.get(0)?,
                    summary: row.get(1)?,
                    created_at: row.get(2)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let artifacts = self.list_artifacts(session_id)?;

        Ok(SessionState {
            session_id: session_id.to_string(),
            title,
            messages,
            decisions,
            artifacts,
        })
    }

    /// Writes `neuro.agent.yaml` and `praesidia.policy.yaml` under `project_dir`.
    /// `business_summary` drives description fields; if empty, uses the last user message line.
    pub fn materialize_agent_artifacts(
        &self,
        session_id: &str,
        project_dir: &Path,
        agent_name: &str,
        business_summary: Option<&str>,
    ) -> Result<MaterializeResult> {
        self.ensure_session(Some(session_id), None)?;
        std::fs::create_dir_all(project_dir)?;

        let summary = business_summary
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                let state = self.get_session_state(session_id).ok()?;
                state
                    .messages
                    .iter()
                    .rev()
                    .find(|m| m.role == "user")
                    .map(|m| m.content.lines().next().unwrap_or(&m.content).to_string())
            })
            .unwrap_or_else(|| "Nexus project".to_string());

        let safe_name = agent_name
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .collect::<String>();
        let name = if safe_name.is_empty() {
            "nexus-agent".to_string()
        } else {
            safe_name
        };

        let agent_yaml = format!(
            r#"name: {name}
version: 1.0.0
description: |
  {desc}
author: Nexus
license: MIT
capabilities:
  - chat
model:
  provider: openai
  name: gpt-4.1
  temperature: 0.7
  max_tokens: 2048
memory:
  type: conversation-buffer
  max_messages: 50
tools: []
endpoint: /a2a
"#,
            name = name,
            desc = summary.replace('\n', "\n  ")
        );

        let policy_yaml = r#"schema_version: 1
enforcement: permissive
rules:
  ingress: []
  egress: []
  tools: []
"#
        .to_string();

        let agent_path = project_dir.join("neuro.agent.yaml");
        let policy_path = project_dir.join("praesidia.policy.yaml");

        std::fs::write(&agent_path, &agent_yaml)?;
        std::fs::write(&policy_path, &policy_yaml)?;

        let a1 = self.propose_artifact(
            session_id,
            "agent_yaml",
            Some(agent_path.to_string_lossy().as_ref()),
            Some(&agent_yaml[..agent_yaml.len().min(500)]),
        )?;
        self.commit_artifact_path(&a1, agent_path.to_string_lossy().as_ref())?;

        let a2 = self.propose_artifact(
            session_id,
            "policy_yaml",
            Some(policy_path.to_string_lossy().as_ref()),
            Some(&policy_yaml[..policy_yaml.len().min(500)]),
        )?;
        self.commit_artifact_path(&a2, policy_path.to_string_lossy().as_ref())?;

        Ok(MaterializeResult {
            agent_yaml_path: agent_path.to_string_lossy().into_owned(),
            policy_yaml_path: policy_path.to_string_lossy().into_owned(),
            artifact_ids: vec![a1, a2],
        })
    }

    /// Copy committed artifact files into `dest_dir` (flat: basename only).
    pub fn export_project(&self, session_id: &str, dest_dir: &Path) -> Result<Vec<String>> {
        std::fs::create_dir_all(dest_dir)?;
        let arts = self.list_artifacts(session_id)?;
        let mut out = Vec::new();
        for a in arts {
            if a.status != "committed" {
                continue;
            }
            let Some(ref p) = a.path else { continue };
            let src = Path::new(p);
            if !src.is_file() {
                continue;
            }
            let name = src
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file");
            let dst = dest_dir.join(name);
            std::fs::copy(src, &dst)?;
            out.push(dst.to_string_lossy().into_owned());
        }
        Ok(out)
    }
}
