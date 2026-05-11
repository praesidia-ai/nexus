//! Knowledge graph CRUD for Nexus projects.
//!
//! Knowledge items are the live sidebar view: entities, relationships, rules,
//! agents, integrations, portals, and databases that emerge from the
//! AI-guided conversation. They are populated from `nexus_state.new_items`.

use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum KnowledgeItemType {
    Entity,
    Relationship,
    Rule,
    Agent,
    Integration,
    Portal,
    Database,
}

impl std::fmt::Display for KnowledgeItemType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            KnowledgeItemType::Entity => "entity",
            KnowledgeItemType::Relationship => "relationship",
            KnowledgeItemType::Rule => "rule",
            KnowledgeItemType::Agent => "agent",
            KnowledgeItemType::Integration => "integration",
            KnowledgeItemType::Portal => "portal",
            KnowledgeItemType::Database => "database",
        };
        f.write_str(s)
    }
}

impl std::str::FromStr for KnowledgeItemType {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "entity" => Ok(KnowledgeItemType::Entity),
            "relationship" => Ok(KnowledgeItemType::Relationship),
            "rule" => Ok(KnowledgeItemType::Rule),
            "agent" => Ok(KnowledgeItemType::Agent),
            "integration" => Ok(KnowledgeItemType::Integration),
            "portal" => Ok(KnowledgeItemType::Portal),
            "database" => Ok(KnowledgeItemType::Database),
            other => Err(format!("unknown knowledge item type: {}", other)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeItem {
    pub id: String,
    pub project_id: String,
    pub item_type: String,
    pub name: String,
    pub description: Option<String>,
    /// Single emoji icon chosen by the AI.
    pub icon: Option<String>,
    /// Arbitrary JSON metadata blob.
    pub metadata: Option<serde_json::Value>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewKnowledgeItem {
    pub item_type: String,
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

pub struct KnowledgeService<'c> {
    pub(crate) conn: &'c Connection,
}

impl<'c> KnowledgeService<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    pub fn add_item(&self, project_id: &str, item: &NewKnowledgeItem) -> Result<KnowledgeItem> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let metadata_str = item
            .metadata
            .as_ref()
            .map(|m| serde_json::to_string(m).unwrap_or_default());

        self.conn.execute(
            "INSERT INTO knowledge_items (id, project_id, item_type, name, description, icon, metadata, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                project_id,
                item.item_type,
                item.name,
                item.description,
                item.icon,
                metadata_str,
                now
            ],
        )?;

        Ok(KnowledgeItem {
            id,
            project_id: project_id.to_string(),
            item_type: item.item_type.clone(),
            name: item.name.clone(),
            description: item.description.clone(),
            icon: item.icon.clone(),
            metadata: item.metadata.clone(),
            created_at: now,
        })
    }

    /// Bulk-insert items — used when processing `nexus_state.new_items`.
    pub fn add_items(
        &self,
        project_id: &str,
        items: &[NewKnowledgeItem],
    ) -> Result<Vec<KnowledgeItem>> {
        items.iter().map(|i| self.add_item(project_id, i)).collect()
    }

    pub fn list_items(&self, project_id: &str) -> Result<Vec<KnowledgeItem>> {
        // Back-compat: default to a single page of 1000 items. Callers that
        // need fuller listings should use `list_items_paged` explicitly.
        self.list_items_paged(project_id, 1000, 0)
    }

    /// Paginated list. `limit` is capped at 1000 to keep the response small
    /// and prevent DOM explosions in the UI; `offset` drives scroll pages.
    pub fn list_items_paged(
        &self,
        project_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<KnowledgeItem>> {
        let limit = limit.clamp(1, 1000) as i64;
        let offset = offset as i64;
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, item_type, name, description, icon, metadata, created_at
             FROM knowledge_items WHERE project_id = ?1 ORDER BY created_at
             LIMIT ?2 OFFSET ?3",
        )?;
        let rows = stmt.query_map(params![project_id, limit, offset], |row| {
            let metadata_str: Option<String> = row.get(6)?;
            let metadata = metadata_str
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok());
            Ok(KnowledgeItem {
                id: row.get(0)?,
                project_id: row.get(1)?,
                item_type: row.get(2)?,
                name: row.get(3)?,
                description: row.get(4)?,
                icon: row.get(5)?,
                metadata,
                created_at: row.get(7)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn list_by_type(
        &self,
        project_id: &str,
        item_type: &str,
    ) -> Result<Vec<KnowledgeItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, item_type, name, description, icon, metadata, created_at
             FROM knowledge_items WHERE project_id = ?1 AND item_type = ?2 ORDER BY created_at",
        )?;
        let rows = stmt.query_map(params![project_id, item_type], |row| {
            let metadata_str: Option<String> = row.get(6)?;
            let metadata = metadata_str
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok());
            Ok(KnowledgeItem {
                id: row.get(0)?,
                project_id: row.get(1)?,
                item_type: row.get(2)?,
                name: row.get(3)?,
                description: row.get(4)?,
                icon: row.get(5)?,
                metadata,
                created_at: row.get(7)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn delete_item(&self, item_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM knowledge_items WHERE id = ?1",
            params![item_id],
        )?;
        Ok(())
    }

    pub fn clear_project(&self, project_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM knowledge_items WHERE project_id = ?1",
            params![project_id],
        )?;
        Ok(())
    }
}
