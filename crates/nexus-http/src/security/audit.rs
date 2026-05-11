//! Audit logging — append-only log of security-sensitive operations.
//!
//! Every mutation or privileged action should be recorded here for compliance
//! and forensic analysis.

use rusqlite::Connection;
use serde::Serialize;

/// A single audit log entry.
#[derive(Debug, Clone, Serialize)]
pub struct AuditEntry {
    /// Unique entry identifier.
    pub id: String,
    /// ISO-8601 timestamp.
    pub timestamp: String,
    /// Tenant the action was performed in.
    pub tenant_id: String,
    /// User who performed the action.
    pub user_id: String,
    /// Action performed (e.g. `"project.create"`, `"api_key.revoke"`).
    pub action: String,
    /// Resource type (e.g. `"project"`, `"api_key"`, `"agent"`).
    pub resource: String,
    /// Identifier of the affected resource.
    pub resource_id: String,
    /// Arbitrary JSON details about the action.
    pub details: serde_json::Value,
    /// IP address of the request originator.
    pub ip_address: String,
}

impl AuditEntry {
    /// Create a new audit entry with auto-generated ID and current timestamp.
    pub fn new(
        tenant_id: &str,
        user_id: &str,
        action: &str,
        resource: &str,
        resource_id: &str,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            tenant_id: tenant_id.to_string(),
            user_id: user_id.to_string(),
            action: action.to_string(),
            resource: resource.to_string(),
            resource_id: resource_id.to_string(),
            details: serde_json::Value::Object(serde_json::Map::new()),
            ip_address: String::new(),
        }
    }

    /// Attach additional JSON details to the entry.
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = details;
        self
    }

    /// Attach the originating IP address.
    pub fn with_ip(mut self, ip: &str) -> Self {
        self.ip_address = ip.to_string();
        self
    }
}

/// Record an audit entry in the database.
pub fn record_audit(db: &Connection, entry: &AuditEntry) -> Result<(), String> {
    let details_str = serde_json::to_string(&entry.details).unwrap_or_else(|_| "{}".to_string());

    db.execute(
        "INSERT INTO security_audit_log (id, timestamp, tenant_id, user_id, action, resource, resource_id, details, ip_address)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            entry.id,
            entry.timestamp,
            entry.tenant_id,
            entry.user_id,
            entry.action,
            entry.resource,
            entry.resource_id,
            details_str,
            entry.ip_address,
        ],
    )
    .map_err(|e| format!("Failed to record audit entry: {e}"))?;

    Ok(())
}

/// List audit entries for a tenant, ordered by timestamp descending.
///
/// Supports pagination via `limit` and `offset`.
pub fn list_audit(
    db: &Connection,
    tenant_id: &str,
    limit: usize,
    offset: usize,
) -> Vec<AuditEntry> {
    let mut results = Vec::new();

    let query_result = db.prepare(
        "SELECT id, timestamp, tenant_id, user_id, action, resource, resource_id, details, ip_address
         FROM security_audit_log
         WHERE tenant_id = ?1
         ORDER BY timestamp DESC
         LIMIT ?2 OFFSET ?3",
    );

    if let Ok(mut stmt) = query_result {
        let _ = stmt
            .query_map(
                rusqlite::params![tenant_id, limit as i64, offset as i64],
                |row| {
                    let details_str: String = row.get(7)?;
                    let details: serde_json::Value =
                        serde_json::from_str(&details_str).unwrap_or(serde_json::json!({}));
                    Ok(AuditEntry {
                        id: row.get(0)?,
                        timestamp: row.get(1)?,
                        tenant_id: row.get(2)?,
                        user_id: row.get(3)?,
                        action: row.get(4)?,
                        resource: row.get(5)?,
                        resource_id: row.get(6)?,
                        details,
                        ip_address: row.get(8)?,
                    })
                },
            )
            .map(|rows| {
                for entry in rows.flatten() {
                    results.push(entry);
                }
            });
    }

    results
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::init_security_tables;

    fn setup_db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        init_security_tables(&db).unwrap();
        db
    }

    #[test]
    fn test_record_and_list_audit() {
        let db = setup_db();

        let entry = AuditEntry::new("tenant-1", "user-1", "project.create", "project", "p1")
            .with_details(serde_json::json!({"name": "My Project"}))
            .with_ip("127.0.0.1");

        record_audit(&db, &entry).unwrap();

        let entries = list_audit(&db, "tenant-1", 10, 0);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].action, "project.create");
        assert_eq!(entries[0].resource_id, "p1");
        assert_eq!(entries[0].ip_address, "127.0.0.1");
    }

    #[test]
    fn test_audit_tenant_isolation() {
        let db = setup_db();

        let e1 = AuditEntry::new("tenant-1", "u1", "action.a", "res", "r1");
        let e2 = AuditEntry::new("tenant-2", "u2", "action.b", "res", "r2");

        record_audit(&db, &e1).unwrap();
        record_audit(&db, &e2).unwrap();

        assert_eq!(list_audit(&db, "tenant-1", 10, 0).len(), 1);
        assert_eq!(list_audit(&db, "tenant-2", 10, 0).len(), 1);
        assert_eq!(list_audit(&db, "tenant-3", 10, 0).len(), 0);
    }

    #[test]
    fn test_audit_pagination() {
        let db = setup_db();

        for i in 0..5 {
            let entry = AuditEntry::new("t1", "u1", &format!("action.{i}"), "res", &format!("r{i}"));
            record_audit(&db, &entry).unwrap();
        }

        let page1 = list_audit(&db, "t1", 2, 0);
        assert_eq!(page1.len(), 2);

        let page2 = list_audit(&db, "t1", 2, 2);
        assert_eq!(page2.len(), 2);

        let page3 = list_audit(&db, "t1", 2, 4);
        assert_eq!(page3.len(), 1);
    }
}
