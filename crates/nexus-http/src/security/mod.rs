//! Security module — authentication, authorization, tenant isolation, rate limiting,
//! API key management, and audit logging.
//!
//! # Architecture
//!
//! - [`auth`] — JWT and API key verification, Axum middleware, `AuthContext` extraction.
//! - [`tenant`] — Tenant-scoped path validation and project access checks.
//! - [`rate_limit`] — Per-tenant token-bucket rate limiter (separate from the global IP limiter).
//! - [`api_keys`] — API key CRUD with SHA-256 hashing.
//! - [`audit`] — Append-only audit log for security-sensitive operations.

pub mod api_keys;
pub mod audit;
pub mod auth;
pub mod project_access;
pub mod rate_limit;
pub mod tenant;
pub mod url_guard;

use rusqlite::Connection;
use tracing::info;

/// Initialise the security-related database tables.
///
/// This is idempotent — safe to call on every startup.
pub fn init_security_tables(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS api_keys (
            id          TEXT PRIMARY KEY,
            tenant_id   TEXT NOT NULL,
            name        TEXT NOT NULL,
            key_hash    TEXT NOT NULL,
            key_prefix  TEXT NOT NULL,
            scopes      TEXT NOT NULL,
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            last_used   TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_api_keys_tenant ON api_keys(tenant_id);
        CREATE INDEX IF NOT EXISTS idx_api_keys_hash   ON api_keys(key_hash);

        CREATE TABLE IF NOT EXISTS security_audit_log (
            id          TEXT PRIMARY KEY,
            timestamp   TEXT NOT NULL DEFAULT (datetime('now')),
            tenant_id   TEXT NOT NULL,
            user_id     TEXT NOT NULL,
            action      TEXT NOT NULL,
            resource    TEXT NOT NULL,
            resource_id TEXT NOT NULL,
            details     TEXT NOT NULL DEFAULT '{}',
            ip_address  TEXT NOT NULL DEFAULT ''
        );

        CREATE INDEX IF NOT EXISTS idx_sec_audit_tenant    ON security_audit_log(tenant_id);
        CREATE INDEX IF NOT EXISTS idx_sec_audit_timestamp ON security_audit_log(timestamp);
        ",
    )
    .map_err(|e| format!("Failed to create security tables: {e}"))?;

    info!("Security tables initialised");
    Ok(())
}
