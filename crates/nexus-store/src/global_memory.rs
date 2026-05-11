//! Global memory — cross-project learning and user preferences.

use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub category: String,
    pub key: String,
    pub value: String,
    pub source: Option<String>,
    pub confidence: f64,
    pub times_applied: i32,
    pub created_at: String,
    pub updated_at: String,
}

pub struct GlobalMemoryService<'c> { conn: &'c Connection }

impl<'c> GlobalMemoryService<'c> {
    pub fn new(conn: &'c Connection) -> Self { Self { conn } }

    pub fn remember(&self, category: &str, key: &str, value: &str, source: Option<&str>) -> Result<MemoryEntry> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        // Upsert: if key exists, update value and bump confidence
        self.conn.execute(
            "INSERT INTO global_memory (id, category, key, value, source, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT(key) DO UPDATE SET value=?4, confidence=MIN(confidence+0.1, 1.0), updated_at=?6, times_applied=times_applied+1",
            params![id, category, key, value, source, now],
        )?;
        Ok(MemoryEntry { id, category: category.into(), key: key.into(), value: value.into(),
            source: source.map(String::from), confidence: 1.0, times_applied: 0, created_at: now.clone(), updated_at: now })
    }

    pub fn recall(&self, key: &str) -> Result<Option<MemoryEntry>> {
        let r = self.conn.query_row(
            "SELECT id, category, key, value, source, confidence, times_applied, created_at, updated_at FROM global_memory WHERE key=?1",
            params![key], |r| Ok(MemoryEntry {
                id: r.get(0)?, category: r.get(1)?, key: r.get(2)?, value: r.get(3)?,
                source: r.get(4)?, confidence: r.get(5)?, times_applied: r.get(6)?,
                created_at: r.get(7)?, updated_at: r.get(8)?,
            })
        );
        match r { Ok(e) => Ok(Some(e)), Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None), Err(e) => Err(e.into()) }
    }

    pub fn recall_by_category(&self, category: &str) -> Result<Vec<MemoryEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, category, key, value, source, confidence, times_applied, created_at, updated_at
             FROM global_memory WHERE category=?1 ORDER BY confidence DESC, times_applied DESC"
        )?;
        let rows = stmt.query_map(params![category], |r| Ok(MemoryEntry {
            id: r.get(0)?, category: r.get(1)?, key: r.get(2)?, value: r.get(3)?,
            source: r.get(4)?, confidence: r.get(5)?, times_applied: r.get(6)?,
            created_at: r.get(7)?, updated_at: r.get(8)?,
        }))?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn list_all(&self) -> Result<Vec<MemoryEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, category, key, value, source, confidence, times_applied, created_at, updated_at
             FROM global_memory ORDER BY updated_at DESC"
        )?;
        let rows = stmt.query_map([], |r| Ok(MemoryEntry {
            id: r.get(0)?, category: r.get(1)?, key: r.get(2)?, value: r.get(3)?,
            source: r.get(4)?, confidence: r.get(5)?, times_applied: r.get(6)?,
            created_at: r.get(7)?, updated_at: r.get(8)?,
        }))?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn forget(&self, key: &str) -> Result<()> {
        self.conn.execute("DELETE FROM global_memory WHERE key=?1", params![key])?;
        Ok(())
    }

    /// Get all memories as LLM context string
    pub fn to_context(&self) -> String {
        let all = self.list_all().unwrap_or_default();
        if all.is_empty() { return String::new(); }
        let mut ctx = String::from("\n## USER PREFERENCES & PATTERNS (learned from past sessions)\n");
        for m in all.iter().take(20) {
            ctx.push_str(&format!("- [{}] {}: {}\n", m.category, m.key, m.value));
        }
        ctx
    }
}
