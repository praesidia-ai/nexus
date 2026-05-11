//! Codegen manifest — tracks generated files + schema versions for incremental updates.

use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::error::Result;

/// A record of a generated file and its content hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileManifestEntry {
    pub path: String,
    pub content_hash: String,
    pub generated_at: String,
}

/// Schema version for a table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaVersion {
    pub table_name: String,
    pub version: i32,
    pub fields_json: String,
    pub migration_sql: Option<String>,
    pub created_at: String,
}

pub struct CodegenManifest<'c> {
    conn: &'c Connection,
}

impl<'c> CodegenManifest<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Get the content hash for a previously generated file.
    pub fn get_file_hash(&self, project_id: &str, path: &str) -> Result<Option<String>> {
        let result = self.conn.query_row(
            "SELECT content_hash FROM codegen_manifest WHERE project_id = ?1 AND path = ?2",
            params![project_id, path],
            |row| row.get(0),
        );
        match result {
            Ok(hash) => Ok(Some(hash)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Store/update the content hash for a generated file.
    pub fn set_file_hash(&self, project_id: &str, path: &str, hash: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO codegen_manifest (project_id, path, content_hash, generated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(project_id, path) DO UPDATE SET content_hash = ?3, generated_at = ?4",
            params![project_id, path, hash, now],
        )?;
        Ok(())
    }

    /// Check if a file needs regeneration by comparing content hash.
    pub fn needs_regeneration(&self, project_id: &str, path: &str, new_content: &str) -> Result<bool> {
        let new_hash = simple_hash(new_content);
        match self.get_file_hash(project_id, path)? {
            Some(old_hash) => Ok(old_hash != new_hash),
            None => Ok(true), // never generated before
        }
    }

    /// Get the latest schema version for a table.
    pub fn get_schema_version(&self, project_id: &str, table_name: &str) -> Result<Option<SchemaVersion>> {
        let result = self.conn.query_row(
            "SELECT table_name, version, fields_json, migration_sql, created_at
             FROM schema_versions WHERE project_id = ?1 AND table_name = ?2
             ORDER BY version DESC LIMIT 1",
            params![project_id, table_name],
            |row| Ok(SchemaVersion {
                table_name: row.get(0)?,
                version: row.get(1)?,
                fields_json: row.get(2)?,
                migration_sql: row.get(3)?,
                created_at: row.get(4)?,
            }),
        );
        match result {
            Ok(sv) => Ok(Some(sv)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Record a new schema version and generate migration SQL.
    pub fn record_schema_version(
        &self,
        project_id: &str,
        table_name: &str,
        fields_json: &str,
        migration_sql: Option<&str>,
    ) -> Result<SchemaVersion> {
        let current_version = self.get_schema_version(project_id, table_name)?
            .map(|sv| sv.version)
            .unwrap_or(0);
        let new_version = current_version + 1;
        let now = Utc::now().to_rfc3339();

        self.conn.execute(
            "INSERT INTO schema_versions (project_id, table_name, version, fields_json, migration_sql, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![project_id, table_name, new_version, fields_json, migration_sql, now],
        )?;

        Ok(SchemaVersion {
            table_name: table_name.to_string(),
            version: new_version,
            fields_json: fields_json.to_string(),
            migration_sql: migration_sql.map(String::from),
            created_at: now,
        })
    }

    /// Generate ALTER TABLE migration SQL by comparing old and new field sets.
    /// Returns None if no changes detected.
    pub fn generate_migration(
        &self,
        table_name: &str,
        old_fields_json: &str,
        new_fields_json: &str,
    ) -> Option<String> {
        let old_fields: Vec<serde_json::Value> = serde_json::from_str(old_fields_json).unwrap_or_default();
        let new_fields: Vec<serde_json::Value> = serde_json::from_str(new_fields_json).unwrap_or_default();

        let old_names: std::collections::HashSet<String> = old_fields.iter()
            .filter_map(|f| f.get("name").and_then(|n| n.as_str()).map(String::from))
            .collect();
        let new_names: std::collections::HashSet<String> = new_fields.iter()
            .filter_map(|f| f.get("name").and_then(|n| n.as_str()).map(String::from))
            .collect();

        let mut stmts = Vec::new();

        // Added fields
        for field in &new_fields {
            let name = field.get("name").and_then(|n| n.as_str()).unwrap_or("");
            if !old_names.contains(name) {
                let ftype = field.get("type").and_then(|t| t.as_str()).unwrap_or("TEXT");
                let not_null = field.get("not_null").and_then(|n| n.as_bool()).unwrap_or(false);
                let mut stmt = format!("ALTER TABLE {} ADD COLUMN {} {}", table_name, name, ftype);
                if not_null {
                    stmt.push_str(" NOT NULL DEFAULT ''");
                }
                stmts.push(format!("{};", stmt));
            }
        }

        // Note: SQLite doesn't support DROP COLUMN before 3.35.0
        // For removed columns, we just add a comment
        for name in &old_names {
            if !new_names.contains(name) {
                stmts.push(format!("-- Column '{}' removed from {} (manual migration needed)", name, table_name));
            }
        }

        if stmts.is_empty() {
            None
        } else {
            Some(stmts.join("\n"))
        }
    }
}

/// Simple content hash (FNV-1a 64-bit for speed, not crypto).
pub fn simple_hash(content: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in content.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", hash)
}
