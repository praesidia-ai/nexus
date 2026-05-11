//! API key management — create, list, revoke, and verify keys.
//!
//! Keys are generated with a `nxk_` prefix for easy identification. Only the SHA-256
//! hash is stored in the database; the raw key is returned exactly once on creation.

use rusqlite::Connection;
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::auth::Scope;

/// Metadata about an API key (the actual key material is never stored or returned
/// after creation).
#[derive(Debug, Clone, Serialize)]
pub struct ApiKeyInfo {
    /// Unique key identifier.
    pub id: String,
    /// Human-readable name for the key.
    pub name: String,
    /// Tenant this key belongs to.
    pub tenant_id: String,
    /// Permission scopes granted to this key.
    pub scopes: Vec<Scope>,
    /// ISO-8601 creation timestamp.
    pub created_at: String,
    /// ISO-8601 timestamp of last use (if any).
    pub last_used: Option<String>,
    /// First 8 characters of the key for display (e.g. `nxk_a1b2`).
    pub prefix: String,
}

/// Create a new API key for a tenant.
///
/// Returns `(raw_key, info)`. The raw key is shown to the user exactly once; only
/// the SHA-256 hash is persisted.
pub fn create_api_key(
    db: &Connection,
    tenant_id: &str,
    name: &str,
    scopes: &[Scope],
) -> Result<(String, ApiKeyInfo), String> {
    let key_id = uuid::Uuid::new_v4().to_string();
    let raw_key = generate_key();
    let key_hash_hex = hash_key(&raw_key);
    let key_prefix = if raw_key.len() >= 8 {
        raw_key[..8].to_string()
    } else {
        raw_key.clone()
    };

    let scopes_json =
        serde_json::to_string(scopes).map_err(|e| format!("Scope serialisation error: {e}"))?;

    db.execute(
        "INSERT INTO api_keys (id, tenant_id, name, key_hash, key_prefix, scopes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![key_id, tenant_id, name, key_hash_hex, key_prefix, scopes_json],
    )
    .map_err(|e| format!("Failed to insert API key: {e}"))?;

    let info = ApiKeyInfo {
        id: key_id,
        name: name.to_string(),
        tenant_id: tenant_id.to_string(),
        scopes: scopes.to_vec(),
        created_at: chrono::Utc::now().to_rfc3339(),
        last_used: None,
        prefix: key_prefix,
    };

    Ok((raw_key, info))
}

/// List all API keys for a tenant. Keys are masked — only prefix and metadata returned.
pub fn list_api_keys(db: &Connection, tenant_id: &str) -> Vec<ApiKeyInfo> {
    let mut results = Vec::new();

    let query_result = db.prepare(
        "SELECT id, name, tenant_id, scopes, created_at, last_used, key_prefix
         FROM api_keys WHERE tenant_id = ?1 ORDER BY created_at DESC",
    );

    if let Ok(mut stmt) = query_result {
        let _ = stmt
            .query_map(rusqlite::params![tenant_id], |row| {
                let scopes_json: String = row.get(3)?;
                let scopes: Vec<Scope> =
                    serde_json::from_str(&scopes_json).unwrap_or_default();
                Ok(ApiKeyInfo {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    tenant_id: row.get(2)?,
                    scopes,
                    created_at: row.get(4)?,
                    last_used: row.get(5)?,
                    prefix: row.get(6)?,
                })
            })
            .map(|rows| {
                for info in rows.flatten() {
                    results.push(info);
                }
            });
    }

    results
}

/// Revoke (delete) an API key by ID, scoped to a tenant.
///
/// Returns `Ok(())` if the key was deleted, or `Err` if it was not found or did not
/// belong to the specified tenant.
pub fn revoke_api_key(db: &Connection, key_id: &str, tenant_id: &str) -> Result<(), String> {
    let affected = db
        .execute(
            "DELETE FROM api_keys WHERE id = ?1 AND tenant_id = ?2",
            rusqlite::params![key_id, tenant_id],
        )
        .map_err(|e| format!("Failed to revoke API key: {e}"))?;

    if affected == 0 {
        Err(format!("API key {key_id} not found for tenant {tenant_id}"))
    } else {
        Ok(())
    }
}

/// Compute the SHA-256 hex digest of a key string.
pub fn hash_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hex::encode(hasher.finalize())
}

/// Generate a random API key with the `nxk_` prefix.
fn generate_key() -> String {
    let id = uuid::Uuid::new_v4();
    // nxk_ prefix + UUID without dashes = 36 chars total
    format!("nxk_{}", id.as_simple())
}

/// Internal row type for query mapping.
#[allow(dead_code)]
struct ApiKeyRow {
    id: String,
    name: String,
    tenant_id: String,
    scopes_json: String,
    created_at: String,
    last_used: Option<String>,
    key_prefix: String,
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
    fn test_create_and_list_api_key() {
        let db = setup_db();
        let scopes = vec![Scope::ProjectRead, Scope::ProjectWrite];

        let (raw_key, info) = create_api_key(&db, "tenant-1", "My Key", &scopes).unwrap();

        assert!(raw_key.starts_with("nxk_"));
        assert_eq!(info.name, "My Key");
        assert_eq!(info.tenant_id, "tenant-1");
        assert_eq!(info.scopes.len(), 2);

        let keys = list_api_keys(&db, "tenant-1");
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].name, "My Key");
    }

    #[test]
    fn test_revoke_api_key() {
        let db = setup_db();
        let (_, info) = create_api_key(&db, "tenant-1", "Temp Key", &[Scope::ProjectRead]).unwrap();

        assert!(revoke_api_key(&db, &info.id, "tenant-1").is_ok());
        assert!(list_api_keys(&db, "tenant-1").is_empty());
    }

    #[test]
    fn test_revoke_wrong_tenant() {
        let db = setup_db();
        let (_, info) = create_api_key(&db, "tenant-1", "Key", &[]).unwrap();

        // Wrong tenant should fail
        assert!(revoke_api_key(&db, &info.id, "tenant-2").is_err());
        // Key should still exist
        assert_eq!(list_api_keys(&db, "tenant-1").len(), 1);
    }

    #[test]
    fn test_hash_key_deterministic() {
        let h1 = hash_key("nxk_test123");
        let h2 = hash_key("nxk_test123");
        assert_eq!(h1, h2);

        let h3 = hash_key("nxk_different");
        assert_ne!(h1, h3);
    }

    #[test]
    fn test_api_key_verification_roundtrip() {
        let db = setup_db();
        let scopes = vec![Scope::ProjectRead];
        let (raw_key, _) = create_api_key(&db, "tenant-1", "Auth Key", &scopes).unwrap();

        let ctx = super::super::auth::verify_api_key(&raw_key, &db).unwrap();
        assert_eq!(ctx.tenant_id, "tenant-1");
        assert!(ctx.scopes.contains(&Scope::ProjectRead));
    }
}
