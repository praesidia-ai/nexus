//! Tenant isolation — project access validation and path sanitisation.
//!
//! Every data access must be scoped to the authenticated tenant. This module provides
//! helpers to validate that a project belongs to a tenant and to construct safe,
//! tenant-scoped filesystem paths.

use std::path::{Path, PathBuf};

use rusqlite::Connection;

/// Validate that a project belongs to the given tenant.
///
/// Returns `Ok(())` if the project exists and is owned by the tenant,
/// or `Err` with a descriptive message otherwise.
///
/// Requires `tenant_id` on `projects` (baseline schema). All projects created
/// after that column exists carry a non-NULL tenant; legacy rows are
/// backfilled with `'default'`, so single-tenant deployments (where the JWT
/// `tid` is always `'default'`) continue to work without any change.
pub fn validate_project_access(
    db: &Connection,
    project_id: &str,
    tenant_id: &str,
) -> Result<(), String> {
    let mut stmt = db
        .prepare("SELECT tenant_id FROM projects WHERE id = ?1")
        .map_err(|e| format!("DB error: {e}"))?;

    let project_tenant: Option<String> = stmt
        .query_row(rusqlite::params![project_id], |row| {
            row.get::<_, Option<String>>(0)
        })
        .map_err(|_| format!("Project {project_id} not found"))?;

    match project_tenant {
        Some(ref tid) if tid == tenant_id => Ok(()),
        Some(_) => Err(format!(
            "Tenant {tenant_id} does not have access to project {project_id}"
        )),
        // NULL — treat as 'default' tenant (pre-migration row).
        None if tenant_id == "default" => Ok(()),
        None => Err(format!(
            "Tenant {tenant_id} does not have access to project {project_id}"
        )),
    }
}

/// Sanitise and validate a path to prevent directory traversal attacks.
///
/// Returns the canonicalised path if it is safely within the tenant's project directory.
/// Rejects paths containing `..`, null bytes, or that resolve outside the expected root.
pub fn validate_path(
    data_dir: &Path,
    tenant_id: &str,
    project_id: &str,
    path: &str,
) -> Result<PathBuf, String> {
    // Reject null bytes
    if path.contains('\0') {
        return Err("Path contains null bytes".to_string());
    }

    // Reject path traversal patterns
    if path.contains("..") {
        return Err("Path traversal detected: '..' not allowed".to_string());
    }

    // Reject absolute paths
    if path.starts_with('/') || path.starts_with('\\') {
        return Err("Absolute paths not allowed".to_string());
    }

    // Reject Windows-style drive letters
    if path.len() >= 2 && path.as_bytes()[1] == b':' {
        return Err("Windows drive paths not allowed".to_string());
    }

    let project_root = tenant_project_dir(data_dir, tenant_id, project_id);
    let full_path = project_root.join(path);

    // Normalise the path components to catch sneaky traversals
    // (e.g. "foo/./../../bar"). We walk each component and reject `..`.
    for component in full_path.components() {
        if let std::path::Component::ParentDir = component {
            return Err("Path traversal detected after normalisation".to_string());
        }
    }

    Ok(full_path)
}

/// Get the tenant-scoped project directory.
///
/// Layout: `<data_dir>/tenants/<tenant_id>/projects/<project_id>`
pub fn tenant_project_dir(
    data_dir: &Path,
    tenant_id: &str,
    project_id: &str,
) -> PathBuf {
    data_dir
        .join("tenants")
        .join(tenant_id)
        .join("projects")
        .join(project_id)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_tenant_project_dir() {
        let dir = tenant_project_dir(Path::new("/data"), "t1", "p1");
        assert_eq!(dir, PathBuf::from("/data/tenants/t1/projects/p1"));
    }

    #[test]
    fn test_validate_path_normal() {
        let result = validate_path(Path::new("/data"), "t1", "p1", "src/main.rs");
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            PathBuf::from("/data/tenants/t1/projects/p1/src/main.rs")
        );
    }

    #[test]
    fn test_validate_path_traversal_dotdot() {
        let result = validate_path(Path::new("/data"), "t1", "p1", "../../../etc/passwd");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("traversal"));
    }

    #[test]
    fn test_validate_path_traversal_hidden() {
        let result = validate_path(Path::new("/data"), "t1", "p1", "foo/../../bar");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_path_null_bytes() {
        let result = validate_path(Path::new("/data"), "t1", "p1", "foo\0bar");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("null"));
    }

    #[test]
    fn test_validate_path_absolute() {
        let result = validate_path(Path::new("/data"), "t1", "p1", "/etc/passwd");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Absolute"));
    }

    #[test]
    fn test_validate_path_windows_drive() {
        let result = validate_path(Path::new("/data"), "t1", "p1", "C:\\windows\\system32");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_project_access_null_tenant_default_user() {
        // NULL tenant_id in projects rows is treated as 'default'.
        // The 'default' tenant user should still be able to access them.
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE projects (id TEXT PRIMARY KEY, name TEXT, tenant_id TEXT);
             INSERT INTO projects VALUES ('p1', 'Test', NULL);",
        )
        .unwrap();

        assert!(validate_project_access(&db, "p1", "default").is_ok());
        assert!(validate_project_access(&db, "p1", "other-tenant").is_err());
    }

    #[test]
    fn test_validate_project_access_with_tenant() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE projects (id TEXT PRIMARY KEY, name TEXT, tenant_id TEXT);
             INSERT INTO projects VALUES ('p1', 'Test', 'tenant-a');",
        )
        .unwrap();

        // Correct tenant
        assert!(validate_project_access(&db, "p1", "tenant-a").is_ok());
        // Wrong tenant
        assert!(validate_project_access(&db, "p1", "tenant-b").is_err());
        // Missing project
        assert!(validate_project_access(&db, "p999", "tenant-a").is_err());
    }
}
