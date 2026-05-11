//! Portal and vault persistence services.

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;

// ---------------------------------------------------------------------------
// Portal
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Portal {
    pub id: String,
    pub project_id: String,
    pub slug: String,
    pub project_name: String,
    pub agent_name: String,
    pub published_at: Option<String>,
    pub created_at: String,
}

pub struct PortalService<'c> {
    pub(crate) conn: &'c Connection,
}

impl<'c> PortalService<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Create or update a portal record for a project.
    /// If one already exists for the project, updates slug/names.
    pub fn upsert_portal(
        &self,
        project_id: &str,
        slug: &str,
        project_name: &str,
        agent_name: &str,
    ) -> Result<Portal> {
        // Check existing
        let existing: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM portals WHERE project_id = ?1",
                params![project_id],
                |row| row.get(0),
            )
            .optional()?;

        let now = Utc::now().to_rfc3339();

        if let Some(id) = existing {
            self.conn.execute(
                "UPDATE portals SET slug = ?1, project_name = ?2, agent_name = ?3
                 WHERE id = ?4",
                params![slug, project_name, agent_name, id],
            )?;
            self.get_portal(&id)?
                .ok_or_else(|| crate::error::StoreError::Msg(
                    format!("portal {id} vanished between update and read")
                ))
        } else {
            let id = Uuid::new_v4().to_string();
            self.conn.execute(
                "INSERT INTO portals (id, project_id, slug, project_name, agent_name, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, project_id, slug, project_name, agent_name, now],
            )?;
            Ok(Portal {
                id,
                project_id: project_id.to_string(),
                slug: slug.to_string(),
                project_name: project_name.to_string(),
                agent_name: agent_name.to_string(),
                published_at: None,
                created_at: now,
            })
        }
    }

    pub fn publish_portal(&self, portal_id: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE portals SET published_at = ?1 WHERE id = ?2",
            params![now, portal_id],
        )?;
        Ok(())
    }

    pub fn get_portal(&self, id: &str) -> Result<Option<Portal>> {
        self.conn
            .query_row(
                "SELECT id, project_id, slug, project_name, agent_name, published_at, created_at
                 FROM portals WHERE id = ?1",
                params![id],
                |row| {
                    Ok(Portal {
                        id: row.get(0)?,
                        project_id: row.get(1)?,
                        slug: row.get(2)?,
                        project_name: row.get(3)?,
                        agent_name: row.get(4)?,
                        published_at: row.get(5)?,
                        created_at: row.get(6)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn get_portal_by_slug(&self, slug: &str) -> Result<Option<Portal>> {
        self.conn
            .query_row(
                "SELECT id, project_id, slug, project_name, agent_name, published_at, created_at
                 FROM portals WHERE slug = ?1",
                params![slug],
                |row| {
                    Ok(Portal {
                        id: row.get(0)?,
                        project_id: row.get(1)?,
                        slug: row.get(2)?,
                        project_name: row.get(3)?,
                        agent_name: row.get(4)?,
                        published_at: row.get(5)?,
                        created_at: row.get(6)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn get_portal_for_project(&self, project_id: &str) -> Result<Option<Portal>> {
        self.conn
            .query_row(
                "SELECT id, project_id, slug, project_name, agent_name, published_at, created_at
                 FROM portals WHERE project_id = ?1",
                params![project_id],
                |row| {
                    Ok(Portal {
                        id: row.get(0)?,
                        project_id: row.get(1)?,
                        slug: row.get(2)?,
                        project_name: row.get(3)?,
                        agent_name: row.get(4)?,
                        published_at: row.get(5)?,
                        created_at: row.get(6)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }
}

// ---------------------------------------------------------------------------
// Vault
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultEntry {
    pub id: String,
    pub project_id: String,
    pub key: String,
    pub value: String,
    pub created_at: String,
}

pub struct VaultService<'c> {
    pub(crate) conn: &'c Connection,
}

impl<'c> VaultService<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    pub fn set(&self, project_id: &str, key: &str, value: &str) -> Result<VaultEntry> {
        let now = Utc::now().to_rfc3339();

        // Encrypt at-rest. Vault values were previously stored plaintext;
        // the `enc:v1:` prefix distinguishes new ciphertexts from legacy
        // rows so we can upgrade in-place on next write without a data
        // migration.
        let stored = crate::crypto::encrypt(value)
            .map_err(|e| crate::error::StoreError::Msg(format!("encrypt: {e}")))?;

        // Check existing
        let existing: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM vault_entries WHERE project_id = ?1 AND key = ?2",
                params![project_id, key],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(id) = existing {
            self.conn.execute(
                "UPDATE vault_entries SET value = ?1 WHERE id = ?2",
                params![stored, id],
            )?;
            Ok(VaultEntry {
                id,
                project_id: project_id.to_string(),
                key: key.to_string(),
                value: value.to_string(),
                created_at: now,
            })
        } else {
            let id = Uuid::new_v4().to_string();
            self.conn.execute(
                "INSERT INTO vault_entries (id, project_id, key, value, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, project_id, key, stored, now],
            )?;
            Ok(VaultEntry {
                id,
                project_id: project_id.to_string(),
                key: key.to_string(),
                value: value.to_string(),
                created_at: now,
            })
        }
    }

    pub fn get(&self, project_id: &str, key: &str) -> Result<Option<String>> {
        let stored: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM vault_entries WHERE project_id = ?1 AND key = ?2",
                params![project_id, key],
                |row| row.get(0),
            )
            .optional()?;
        match stored {
            Some(s) => Ok(Some(crate::crypto::decrypt(&s).map_err(|e| {
                crate::error::StoreError::Msg(format!("decrypt: {e}"))
            })?)),
            None => Ok(None),
        }
    }

    pub fn list(&self, project_id: &str) -> Result<Vec<VaultEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, key, value, created_at
             FROM vault_entries WHERE project_id = ?1 ORDER BY key",
        )?;
        let rows = stmt.query_map(params![project_id], |row| {
            let raw_value: String = row.get(3)?;
            // Transparently decrypt so callers see plaintext. Legacy rows
            // without the `enc:v1:` prefix pass through unchanged.
            let plain = crate::crypto::decrypt(&raw_value).unwrap_or(raw_value);
            Ok(VaultEntry {
                id: row.get(0)?,
                project_id: row.get(1)?,
                key: row.get(2)?,
                value: plain,
                created_at: row.get(4)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn delete(&self, project_id: &str, key: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM vault_entries WHERE project_id = ?1 AND key = ?2",
            params![project_id, key],
        )?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// WorkflowRun
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRun {
    pub id: String,
    pub project_id: Option<String>,
    pub pipeline: String,
    pub status: String,
    pub input_json: Option<String>,
    pub output_json: Option<String>,
    pub error: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

pub struct WorkflowRunService<'c> {
    pub(crate) conn: &'c Connection,
}

impl<'c> WorkflowRunService<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    pub fn create_run(
        &self,
        project_id: Option<&str>,
        pipeline: &str,
        input_json: Option<&str>,
    ) -> Result<WorkflowRun> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO workflow_runs (id, project_id, pipeline, status, input_json, started_at)
             VALUES (?1, ?2, ?3, 'running', ?4, ?5)",
            params![id, project_id, pipeline, input_json, now],
        )?;
        Ok(WorkflowRun {
            id,
            project_id: project_id.map(String::from),
            pipeline: pipeline.to_string(),
            status: "running".to_string(),
            input_json: input_json.map(String::from),
            output_json: None,
            error: None,
            started_at: now,
            finished_at: None,
        })
    }

    pub fn complete_run(
        &self,
        run_id: &str,
        output_json: &str,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE workflow_runs SET status = 'completed', output_json = ?1, finished_at = ?2
             WHERE id = ?3",
            params![output_json, now, run_id],
        )?;
        Ok(())
    }

    pub fn fail_run(&self, run_id: &str, error: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE workflow_runs SET status = 'failed', error = ?1, finished_at = ?2
             WHERE id = ?3",
            params![error, now, run_id],
        )?;
        Ok(())
    }

    pub fn get_run(&self, run_id: &str) -> Result<Option<WorkflowRun>> {
        self.conn
            .query_row(
                "SELECT id, project_id, pipeline, status, input_json, output_json, error,
                        started_at, finished_at
                 FROM workflow_runs WHERE id = ?1",
                params![run_id],
                |row| {
                    Ok(WorkflowRun {
                        id: row.get(0)?,
                        project_id: row.get(1)?,
                        pipeline: row.get(2)?,
                        status: row.get(3)?,
                        input_json: row.get(4)?,
                        output_json: row.get(5)?,
                        error: row.get(6)?,
                        started_at: row.get(7)?,
                        finished_at: row.get(8)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn list_runs(&self, project_id: &str) -> Result<Vec<WorkflowRun>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, pipeline, status, input_json, output_json, error,
                    started_at, finished_at
             FROM workflow_runs WHERE project_id = ?1 ORDER BY started_at DESC",
        )?;
        let rows = stmt.query_map(params![project_id], |row| {
            Ok(WorkflowRun {
                id: row.get(0)?,
                project_id: row.get(1)?,
                pipeline: row.get(2)?,
                status: row.get(3)?,
                input_json: row.get(4)?,
                output_json: row.get(5)?,
                error: row.get(6)?,
                started_at: row.get(7)?,
                finished_at: row.get(8)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }
}
