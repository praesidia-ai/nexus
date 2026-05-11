//! Project and conversation CRUD for the Nexus store schema.

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    /// Nexus conversation phase: 1=Exploration, 2=Structuring, 3=Materialization, 4=Publishing
    pub phase: i64,
    /// Per-project LLM provider override (e.g. "openai", "anthropic").
    pub llm_provider: Option<String>,
    /// Per-project LLM model override (e.g. "gpt-4o", "claude-sonnet-4-6").
    pub llm_model: Option<String>,
    /// Tenant that owns this project. Defaults to "default" for single-tenant deployments.
    pub tenant_id: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub project_id: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusMessage {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    /// Display content (nexus_state stripped by the AI layer before storage).
    pub content: String,
    /// Original AI response including the `<nexus_state>…</nexus_state>` block.
    pub raw_content: Option<String>,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

pub struct ProjectService<'c> {
    pub(crate) conn: &'c Connection,
}

impl<'c> ProjectService<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    // -----------------------------------------------------------------------
    // Projects
    // -----------------------------------------------------------------------

    pub fn create_project(
        &self,
        name: &str,
        description: Option<&str>,
        tenant_id: &str,
    ) -> Result<Project> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO projects (id, name, description, phase, tenant_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, 1, ?4, ?5, ?5)",
            params![id, name, description, tenant_id, now],
        )?;
        Ok(Project {
            id,
            name: name.to_string(),
            description: description.map(String::from),
            phase: 1,
            llm_provider: None,
            llm_model: None,
            tenant_id: tenant_id.to_string(),
            created_at: now.clone(),
            updated_at: now,
        })
    }

    pub fn list_projects(&self) -> Result<Vec<Project>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, description, phase, llm_provider, llm_model, tenant_id, created_at, updated_at
             FROM projects ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Project {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                phase: row.get(3)?,
                llm_provider: row.get(4)?,
                llm_model: row.get(5)?,
                tenant_id: row.get::<_, Option<String>>(6)?.unwrap_or_else(|| "default".to_string()),
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// List projects belonging to a specific tenant.
    pub fn list_projects_for_tenant(&self, tenant_id: &str) -> Result<Vec<Project>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, description, phase, llm_provider, llm_model, tenant_id, created_at, updated_at
             FROM projects WHERE tenant_id = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![tenant_id], |row| {
            Ok(Project {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                phase: row.get(3)?,
                llm_provider: row.get(4)?,
                llm_model: row.get(5)?,
                tenant_id: row.get::<_, Option<String>>(6)?.unwrap_or_else(|| "default".to_string()),
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get_project(&self, id: &str) -> Result<Option<Project>> {
        self.conn
            .query_row(
                "SELECT id, name, description, phase, llm_provider, llm_model, tenant_id, created_at, updated_at
                 FROM projects WHERE id = ?1",
                params![id],
                |row| {
                    Ok(Project {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        description: row.get(2)?,
                        phase: row.get(3)?,
                        llm_provider: row.get(4)?,
                        llm_model: row.get(5)?,
                        tenant_id: row.get::<_, Option<String>>(6)?.unwrap_or_else(|| "default".to_string()),
                        created_at: row.get(7)?,
                        updated_at: row.get(8)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn update_project_phase(&self, id: &str, phase: i64) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE projects SET phase = ?1, updated_at = ?2 WHERE id = ?3",
            params![phase, now, id],
        )?;
        Ok(())
    }

    /// Set the per-project LLM provider and model. Pass None to clear (fall back to global).
    pub fn update_project_model(
        &self,
        id: &str,
        provider: Option<&str>,
        model: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE projects SET llm_provider = ?1, llm_model = ?2, updated_at = ?3 WHERE id = ?4",
            params![provider, model, now, id],
        )?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Conversations
    // -----------------------------------------------------------------------

    pub fn create_conversation(&self, project_id: &str) -> Result<Conversation> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO conversations (id, project_id, created_at) VALUES (?1, ?2, ?3)",
            params![id, project_id, now],
        )?;
        Ok(Conversation {
            id,
            project_id: project_id.to_string(),
            created_at: now,
        })
    }

    pub fn list_conversations(&self, project_id: &str) -> Result<Vec<Conversation>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, created_at FROM conversations
             WHERE project_id = ?1 ORDER BY created_at",
        )?;
        let rows = stmt.query_map(params![project_id], |row| {
            Ok(Conversation {
                id: row.get(0)?,
                project_id: row.get(1)?,
                created_at: row.get(2)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    // -----------------------------------------------------------------------
    // Messages
    // -----------------------------------------------------------------------

    pub fn append_nexus_message(
        &self,
        conversation_id: &str,
        role: &str,
        content: &str,
        raw_content: Option<&str>,
    ) -> Result<NexusMessage> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO nexus_messages (id, conversation_id, role, content, raw_content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, conversation_id, role, content, raw_content, now],
        )?;
        Ok(NexusMessage {
            id,
            conversation_id: conversation_id.to_string(),
            role: role.to_string(),
            content: content.to_string(),
            raw_content: raw_content.map(String::from),
            created_at: now,
        })
    }

    pub fn list_messages(&self, conversation_id: &str) -> Result<Vec<NexusMessage>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, conversation_id, role, content, raw_content, created_at
             FROM nexus_messages WHERE conversation_id = ?1 ORDER BY created_at",
        )?;
        let rows = stmt.query_map(params![conversation_id], |row| {
            Ok(NexusMessage {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                raw_content: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    // -----------------------------------------------------------------------
    // Settings
    // -----------------------------------------------------------------------

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let stored: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?;
        match stored {
            Some(s) if is_sensitive_setting(key) => Ok(Some(
                crate::crypto::decrypt(&s)
                    .map_err(|e| crate::error::StoreError::Msg(format!("decrypt setting: {e}")))?,
            )),
            Some(s) => Ok(Some(s)),
            None => Ok(None),
        }
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        // Encrypt at-rest for sensitive keys (LLM API keys, provider
        // credentials, etc). Non-sensitive settings stay plaintext so
        // operators can still inspect them via `sqlite3`.
        let stored_value: String = if is_sensitive_setting(key) {
            crate::crypto::encrypt(value)
                .map_err(|e| crate::error::StoreError::Msg(format!("encrypt setting: {e}")))?
        } else {
            value.to_string()
        };
        self.conn.execute(
            "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![key, stored_value, now],
        )?;
        Ok(())
    }

    pub fn all_settings(&self) -> Result<std::collections::HashMap<String, String>> {
        let mut stmt = self.conn.prepare("SELECT key, value FROM settings")?;
        let rows = stmt.query_map([], |row| {
            let key: String = row.get(0)?;
            let raw: String = row.get(1)?;
            let plain = if is_sensitive_setting(&key) {
                crate::crypto::decrypt(&raw).unwrap_or(raw)
            } else {
                raw
            };
            Ok((key, plain))
        })?;
        rows.collect::<std::result::Result<std::collections::HashMap<_, _>, _>>()
            .map_err(Into::into)
    }
}

/// Setting keys that hold secrets and must be encrypted at rest.
fn is_sensitive_setting(key: &str) -> bool {
    // Any LLM provider API key, OAuth tokens, webhook secrets, etc.
    key.ends_with(".api_key")
        || key.ends_with(".secret")
        || key.ends_with(".token")
        || key.ends_with("_secret")
        || key.ends_with("_token")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_connection;
    use std::path::Path;

    fn fresh_db() -> Connection {
        open_connection(Path::new(":memory:")).expect("open in-memory db")
    }

    #[test]
    fn create_project_persists_and_lists_per_tenant() {
        let conn = fresh_db();
        let svc = ProjectService::new(&conn);

        let p_a = svc.create_project("alpha", Some("desc-a"), "tenant-a").unwrap();
        let p_b = svc.create_project("beta", None, "tenant-b").unwrap();

        let list_a = svc.list_projects_for_tenant("tenant-a").unwrap();
        let list_b = svc.list_projects_for_tenant("tenant-b").unwrap();

        assert_eq!(list_a.len(), 1, "tenant-a should see exactly its project");
        assert_eq!(list_b.len(), 1, "tenant-b should see exactly its project");
        assert_eq!(list_a[0].id, p_a.id);
        assert_eq!(list_b[0].id, p_b.id);
        assert_eq!(list_a[0].tenant_id, "tenant-a");
        assert_eq!(list_b[0].tenant_id, "tenant-b");
    }

    #[test]
    fn list_projects_does_not_leak_across_tenants() {
        let conn = fresh_db();
        let svc = ProjectService::new(&conn);

        for i in 0..5 {
            svc.create_project(&format!("a{i}"), None, "tenant-a").unwrap();
        }
        for i in 0..3 {
            svc.create_project(&format!("b{i}"), None, "tenant-b").unwrap();
        }

        let list_a = svc.list_projects_for_tenant("tenant-a").unwrap();
        assert_eq!(list_a.len(), 5);
        assert!(list_a.iter().all(|p| p.tenant_id == "tenant-a"));

        let list_other = svc.list_projects_for_tenant("nonexistent").unwrap();
        assert!(list_other.is_empty());
    }

    #[test]
    fn create_conversation_links_to_project() {
        let conn = fresh_db();
        let svc = ProjectService::new(&conn);

        let p = svc.create_project("proj", None, "tenant-a").unwrap();
        let conv = svc.create_conversation(&p.id).unwrap();
        assert_eq!(conv.project_id, p.id);
    }
}
