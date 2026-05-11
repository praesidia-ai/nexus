//! App-specific tools for business agents.
//!
//! These tools operate on generated app projects stored under
//! `~/.nexus/projects/{project_id}/`. They allow agents to query the app's
//! SQLite database, call the app's dev server, and edit content files in the
//! output directory.

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::agents::tools::registry::{Tool, ToolInput, ToolOutput};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn ok(result: Value) -> ToolOutput {
    ToolOutput {
        result,
        success: true,
        error: None,
    }
}

fn err(msg: impl Into<String>) -> ToolOutput {
    ToolOutput {
        result: json!({}),
        success: false,
        error: Some(msg.into()),
    }
}

/// Resolve `~/.nexus/projects/{project_id}` to an absolute path.
///
/// Respects `NEXUS_DATA_DIR` if set; otherwise falls back to `$HOME/.nexus`.
fn project_dir(project_id: &str) -> Result<std::path::PathBuf, ToolOutput> {
    // Reject path traversal attempts
    if project_id.contains("..") || project_id.contains('/') || project_id.contains('\\') {
        return Err(err(
            "Invalid project_id: must not contain path separators or '..'",
        ));
    }

    let data_dir = std::env::var("NEXUS_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            std::path::PathBuf::from(home).join(".nexus")
        });

    Ok(data_dir.join("projects").join(project_id))
}

// ---------------------------------------------------------------------------
// AppDatabaseQueryTool
// ---------------------------------------------------------------------------

/// Query the generated app's SQLite database at
/// `~/.nexus/projects/{project_id}/data.db`.
pub struct AppDatabaseQueryTool;

#[async_trait]
impl Tool for AppDatabaseQueryTool {
    fn name(&self) -> &str {
        "app_database_query"
    }

    fn description(&self) -> &str {
        "Query the generated app's SQLite database. Executes a SQL query against \
         ~/.nexus/projects/{project_id}/data.db and returns results as JSON. \
         SELECT queries return rows; write queries return affected row count."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "project_id": {
                    "type": "string",
                    "description": "The project identifier (directory name under ~/.nexus/projects/)"
                },
                "query": {
                    "type": "string",
                    "description": "SQL query to execute against the app's data.db"
                }
            },
            "required": ["project_id", "query"]
        })
    }

    fn category(&self) -> &str {
        "app_specific"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let project_id = match input.parameters.get("project_id").and_then(|v| v.as_str()) {
            Some(p) => p.to_string(),
            None => return err("Missing required parameter: project_id"),
        };
        let query = match input.parameters.get("query").and_then(|v| v.as_str()) {
            Some(q) => q.to_string(),
            None => return err("Missing required parameter: query"),
        };

        // Block dangerous queries (check before file access)
        let query_upper = query.trim().to_uppercase();
        if query_upper.starts_with("DROP") || query_upper.starts_with("TRUNCATE") {
            return err(
                "DROP and TRUNCATE queries are not allowed for safety. Use a migration instead.",
            );
        }

        let db_path = match project_dir(&project_id) {
            Ok(dir) => dir.join("data.db"),
            Err(e) => return e,
        };

        let db_path_str = db_path.to_string_lossy().to_string();

        if !db_path.exists() {
            return err(format!(
                "Database not found at {}. Has the app been initialized?",
                db_path_str
            ));
        }

        let result =
            tokio::task::spawn_blocking(move || execute_app_sql(&db_path_str, &query)).await;

        match result {
            Ok(output) => output,
            Err(e) => err(format!("Task panicked: {e}")),
        }
    }
}

fn execute_app_sql(db_path: &str, query: &str) -> ToolOutput {
    let conn = match rusqlite::Connection::open(db_path) {
        Ok(c) => c,
        Err(e) => return err(format!("Failed to open database: {e}")),
    };

    let query_upper = query.trim().to_uppercase();
    let is_select = query_upper.starts_with("SELECT")
        || query_upper.starts_with("PRAGMA")
        || query_upper.starts_with("EXPLAIN");

    if is_select {
        let mut stmt = match conn.prepare(query) {
            Ok(s) => s,
            Err(e) => return err(format!("SQL prepare error: {e}")),
        };

        let column_names: Vec<String> = stmt
            .column_names()
            .iter()
            .map(|c| c.to_string())
            .collect();

        let rows_result = stmt.query_map([], |row| {
            let mut obj = serde_json::Map::new();
            for (i, col_name) in column_names.iter().enumerate() {
                let val: Value = match row.get_ref(i) {
                    Ok(rusqlite::types::ValueRef::Null) => Value::Null,
                    Ok(rusqlite::types::ValueRef::Integer(n)) => json!(n),
                    Ok(rusqlite::types::ValueRef::Real(f)) => json!(f),
                    Ok(rusqlite::types::ValueRef::Text(t)) => {
                        Value::String(String::from_utf8_lossy(t).to_string())
                    }
                    Ok(rusqlite::types::ValueRef::Blob(b)) => {
                        Value::String(format!("<blob: {} bytes>", b.len()))
                    }
                    Err(_) => Value::Null,
                };
                obj.insert(col_name.clone(), val);
            }
            Ok(Value::Object(obj))
        });

        match rows_result {
            Ok(mapped_rows) => {
                let max_rows = 200;
                let mut rows = Vec::new();
                for row in mapped_rows {
                    if rows.len() >= max_rows {
                        break;
                    }
                    match row {
                        Ok(v) => rows.push(v),
                        Err(e) => return err(format!("Row read error: {e}")),
                    }
                }
                let count = rows.len();
                ok(json!({
                    "columns": column_names,
                    "rows": rows,
                    "count": count,
                    "truncated": count >= max_rows
                }))
            }
            Err(e) => err(format!("Query execution error: {e}")),
        }
    } else {
        match conn.execute(query, []) {
            Ok(affected) => ok(json!({
                "affected_rows": affected,
                "query_type": "write"
            })),
            Err(e) => err(format!("SQL execution error: {e}")),
        }
    }
}

// ---------------------------------------------------------------------------
// AppApiCallTool
// ---------------------------------------------------------------------------

/// Make HTTP requests to the generated app's dev server at `localhost:{port}`.
pub struct AppApiCallTool;

#[async_trait]
impl Tool for AppApiCallTool {
    fn name(&self) -> &str {
        "app_api_call"
    }

    fn description(&self) -> &str {
        "Make an HTTP request to the generated app's dev server running at localhost:{port}. \
         Supports GET, POST, PUT, and DELETE methods. Returns the response status, headers, \
         and body."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "project_id": {
                    "type": "string",
                    "description": "The project identifier (for context/logging)"
                },
                "method": {
                    "type": "string",
                    "enum": ["GET", "POST", "PUT", "DELETE"],
                    "description": "HTTP method"
                },
                "path": {
                    "type": "string",
                    "description": "Request path (e.g. /api/users). Will be appended to http://localhost:{port}"
                },
                "port": {
                    "type": "integer",
                    "description": "Port the dev server is running on (default: 3000)"
                },
                "body": {
                    "description": "Request body as JSON (optional, used with POST/PUT)"
                },
                "headers": {
                    "type": "object",
                    "description": "Additional HTTP headers (optional)"
                }
            },
            "required": ["project_id", "method", "path"]
        })
    }

    fn category(&self) -> &str {
        "app_specific"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let _project_id = match input.parameters.get("project_id").and_then(|v| v.as_str()) {
            Some(p) => p.to_string(),
            None => return err("Missing required parameter: project_id"),
        };
        let method = match input.parameters.get("method").and_then(|v| v.as_str()) {
            Some(m) => m.to_uppercase(),
            None => return err("Missing required parameter: method"),
        };
        let path = match input.parameters.get("path").and_then(|v| v.as_str()) {
            Some(p) => p.to_string(),
            None => return err("Missing required parameter: path"),
        };
        let port = input
            .parameters
            .get("port")
            .and_then(|v| v.as_u64())
            .unwrap_or(3000);
        let body = input.parameters.get("body").cloned();
        let headers = input
            .parameters
            .get("headers")
            .and_then(|v| v.as_object())
            .cloned();

        // Validate method
        if !matches!(method.as_str(), "GET" | "POST" | "PUT" | "DELETE") {
            return err(format!(
                "Unsupported HTTP method: {}. Must be GET, POST, PUT, or DELETE.",
                method
            ));
        }

        // Ensure path starts with /
        let normalized_path = if path.starts_with('/') {
            path
        } else {
            format!("/{}", path)
        };

        let url = format!("http://localhost:{}{}", port, normalized_path);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build();

        let client = match client {
            Ok(c) => c,
            Err(e) => return err(format!("Failed to create HTTP client: {e}")),
        };

        // The match arms are kept in sync with the validation above; use an
        // explicit error here instead of `unreachable!()` so a future code
        // change cannot turn attacker-controlled input into a worker panic.
        let mut request = match method.as_str() {
            "GET" => client.get(&url),
            "POST" => client.post(&url),
            "PUT" => client.put(&url),
            "DELETE" => client.delete(&url),
            other => {
                return err(format!(
                    "Internal error: method '{other}' passed validation but has no handler"
                ));
            }
        };

        // Add custom headers
        if let Some(hdrs) = headers {
            for (key, value) in hdrs {
                if let Some(val_str) = value.as_str() {
                    request = request.header(&key, val_str);
                }
            }
        }

        // Add JSON body for POST/PUT
        if let Some(body_val) = body {
            if matches!(method.as_str(), "POST" | "PUT") {
                request = request
                    .header("Content-Type", "application/json")
                    .json(&body_val);
            }
        }

        match request.send().await {
            Ok(response) => {
                let status = response.status().as_u16();
                let resp_headers: serde_json::Map<String, Value> = response
                    .headers()
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.as_str().to_string(),
                            Value::String(v.to_str().unwrap_or("<binary>").to_string()),
                        )
                    })
                    .collect();

                let body_text = response.text().await.unwrap_or_default();

                // Try to parse body as JSON
                let body_json: Value = serde_json::from_str(&body_text)
                    .unwrap_or(Value::String(body_text));

                ok(json!({
                    "status": status,
                    "headers": resp_headers,
                    "body": body_json,
                    "url": url
                }))
            }
            Err(e) => {
                if e.is_connect() {
                    err(format!(
                        "Connection refused at {}. Is the dev server running on port {}?",
                        url, port
                    ))
                } else if e.is_timeout() {
                    err(format!("Request to {} timed out after 30 seconds", url))
                } else {
                    err(format!("HTTP request failed: {e}"))
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// AppContentEditTool
// ---------------------------------------------------------------------------

/// Edit content files in the generated app's output directory
/// `~/.nexus/projects/{project_id}/output/`.
pub struct AppContentEditTool;

#[async_trait]
impl Tool for AppContentEditTool {
    fn name(&self) -> &str {
        "app_content_edit"
    }

    fn description(&self) -> &str {
        "Edit content files in the generated app's output directory. Performs a \
         search-and-replace operation on a file at \
         ~/.nexus/projects/{project_id}/output/{file_path}."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "project_id": {
                    "type": "string",
                    "description": "The project identifier (directory name under ~/.nexus/projects/)"
                },
                "file_path": {
                    "type": "string",
                    "description": "Relative path within the output/ directory (e.g. 'src/App.tsx')"
                },
                "search": {
                    "type": "string",
                    "description": "Exact string to search for in the file"
                },
                "replace": {
                    "type": "string",
                    "description": "Replacement string"
                }
            },
            "required": ["project_id", "file_path", "search", "replace"]
        })
    }

    fn category(&self) -> &str {
        "app_specific"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let project_id = match input.parameters.get("project_id").and_then(|v| v.as_str()) {
            Some(p) => p.to_string(),
            None => return err("Missing required parameter: project_id"),
        };
        let file_path = match input.parameters.get("file_path").and_then(|v| v.as_str()) {
            Some(p) => p.to_string(),
            None => return err("Missing required parameter: file_path"),
        };
        let search = match input.parameters.get("search").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => return err("Missing required parameter: search"),
        };
        let replace = match input.parameters.get("replace").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => return err("Missing required parameter: replace"),
        };

        let base_dir = match project_dir(&project_id) {
            Ok(dir) => dir.join("output"),
            Err(e) => return e,
        };

        // Reject path traversal in file_path
        if file_path.contains("..") {
            return err("Invalid file_path: must not contain '..'");
        }

        let full_path = base_dir.join(&file_path);

        // Ensure the resolved path is still within the output directory
        let canonical_base = match base_dir.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                return err(format!(
                    "Output directory does not exist: {}",
                    base_dir.display()
                ))
            }
        };

        // The file must exist to edit it
        if !full_path.exists() {
            return err(format!(
                "File not found: {}",
                full_path.display()
            ));
        }

        let canonical_file = match full_path.canonicalize() {
            Ok(p) => p,
            Err(e) => return err(format!("Cannot resolve file path: {e}")),
        };

        if !canonical_file.starts_with(&canonical_base) {
            return err("file_path resolves outside the project output directory");
        }

        let canonical_file_clone = canonical_file.clone();
        let result = tokio::task::spawn_blocking(move || {
            edit_content_file(&canonical_file_clone, &search, &replace)
        })
        .await;

        match result {
            Ok(output) => output,
            Err(e) => err(format!("Task panicked: {e}")),
        }
    }
}

fn edit_content_file(
    path: &std::path::Path,
    search: &str,
    replace: &str,
) -> ToolOutput {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => return err(format!("Cannot read '{}': {e}", path.display())),
    };

    let count = content.matches(search).count();
    if count == 0 {
        return err(format!(
            "Search string not found in {}. Read the file first to verify exact content.",
            path.display()
        ));
    }

    // Replace first occurrence only (like a precise edit)
    let new_content = content.replacen(search, replace, 1);

    match std::fs::write(path, &new_content) {
        Ok(()) => ok(json!({
            "path": path.display().to_string(),
            "occurrences_found": count,
            "replacements_made": 1,
            "old_lines": search.lines().count(),
            "new_lines": replace.lines().count()
        })),
        Err(e) => err(format!("Cannot write '{}': {e}", path.display())),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_database_query_tool_metadata() {
        let tool = AppDatabaseQueryTool;
        assert_eq!(tool.name(), "app_database_query");
        assert_eq!(tool.category(), "app_specific");
        let schema = tool.schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("project_id")));
        assert!(required.contains(&json!("query")));
    }

    #[test]
    fn app_api_call_tool_metadata() {
        let tool = AppApiCallTool;
        assert_eq!(tool.name(), "app_api_call");
        assert_eq!(tool.category(), "app_specific");
        let schema = tool.schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("project_id")));
        assert!(required.contains(&json!("method")));
        assert!(required.contains(&json!("path")));
    }

    #[test]
    fn app_content_edit_tool_metadata() {
        let tool = AppContentEditTool;
        assert_eq!(tool.name(), "app_content_edit");
        assert_eq!(tool.category(), "app_specific");
        let schema = tool.schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("project_id")));
        assert!(required.contains(&json!("file_path")));
        assert!(required.contains(&json!("search")));
        assert!(required.contains(&json!("replace")));
    }

    #[tokio::test]
    async fn app_database_query_rejects_traversal() {
        let tool = AppDatabaseQueryTool;
        let out = tool
            .execute(ToolInput {
                parameters: json!({
                    "project_id": "../../../etc",
                    "query": "SELECT 1"
                }),
            })
            .await;
        assert!(!out.success);
        assert!(out.error.unwrap().contains("Invalid project_id"));
    }

    #[tokio::test]
    async fn app_database_query_rejects_drop() {
        let tool = AppDatabaseQueryTool;
        let out = tool
            .execute(ToolInput {
                parameters: json!({
                    "project_id": "test-project",
                    "query": "DROP TABLE users"
                }),
            })
            .await;
        assert!(!out.success);
        assert!(out.error.unwrap().contains("not allowed"));
    }

    #[tokio::test]
    async fn app_content_edit_rejects_traversal() {
        let tool = AppContentEditTool;
        let out = tool
            .execute(ToolInput {
                parameters: json!({
                    "project_id": "test-project",
                    "file_path": "../../etc/passwd",
                    "search": "root",
                    "replace": "hacked"
                }),
            })
            .await;
        assert!(!out.success);
        assert!(out.error.unwrap().contains(".."));
    }

    #[tokio::test]
    async fn app_api_call_rejects_invalid_method() {
        let tool = AppApiCallTool;
        let out = tool
            .execute(ToolInput {
                parameters: json!({
                    "project_id": "test-project",
                    "method": "PATCH",
                    "path": "/api/test"
                }),
            })
            .await;
        assert!(!out.success);
        assert!(out.error.unwrap().contains("Unsupported HTTP method"));
    }
}
