//! Database tools — SQL queries and schema inspection.
//!
//! Uses `rusqlite` for SQLite database operations. All queries are
//! executed in a `spawn_blocking` context to avoid blocking the async
//! runtime.

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

// ---------------------------------------------------------------------------
// SqlQueryTool
// ---------------------------------------------------------------------------

/// Run SQL queries against a SQLite database file.
pub struct SqlQueryTool;

#[async_trait]
impl Tool for SqlQueryTool {
    fn name(&self) -> &str {
        "sql_query"
    }

    fn description(&self) -> &str {
        "Execute a SQL query against a SQLite database file. Returns rows as JSON. Use for SELECT queries; write queries return affected row count."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "database_path": { "type": "string", "description": "Path to the SQLite database file" },
                "query": { "type": "string", "description": "SQL query to execute" },
                "params": {
                    "type": "array",
                    "items": {},
                    "description": "Positional query parameters (optional)"
                },
                "max_rows": { "type": "integer", "description": "Maximum rows to return (default: 100)" }
            },
            "required": ["database_path", "query"]
        })
    }

    fn category(&self) -> &str {
        "data_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let db_path = match input.parameters.get("database_path").and_then(|v| v.as_str()) {
            Some(p) => p.to_string(),
            None => return err("Missing required parameter: database_path"),
        };
        let query = match input.parameters.get("query").and_then(|v| v.as_str()) {
            Some(q) => q.to_string(),
            None => return err("Missing required parameter: query"),
        };
        let max_rows = input
            .parameters
            .get("max_rows")
            .and_then(|v| v.as_u64())
            .unwrap_or(100) as usize;

        let params: Vec<Value> = input
            .parameters
            .get("params")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        // Reject dangerous operations on certain keywords for safety
        let query_upper = query.trim().to_uppercase();
        if query_upper.starts_with("DROP") || query_upper.starts_with("TRUNCATE") {
            return err("DROP and TRUNCATE queries are not allowed for safety. Use a migration instead.");
        }

        let result = tokio::task::spawn_blocking(move || {
            execute_sql(&db_path, &query, &params, max_rows)
        })
        .await;

        match result {
            Ok(output) => output,
            Err(e) => err(format!("Task panicked: {e}")),
        }
    }
}

fn execute_sql(db_path: &str, query: &str, params: &[Value], max_rows: usize) -> ToolOutput {
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

        let rusqlite_params: Vec<Box<dyn rusqlite::types::ToSql>> = params
            .iter()
            .map(|v| -> Box<dyn rusqlite::types::ToSql> {
                match v {
                    Value::String(s) => Box::new(s.clone()),
                    Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            Box::new(i)
                        } else if let Some(f) = n.as_f64() {
                            Box::new(f)
                        } else {
                            Box::new(n.to_string())
                        }
                    }
                    Value::Bool(b) => Box::new(*b),
                    Value::Null => Box::new(rusqlite::types::Null),
                    _ => Box::new(v.to_string()),
                }
            })
            .collect();

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            rusqlite_params.iter().map(|p| p.as_ref()).collect();

        let rows_result = stmt.query_map(param_refs.as_slice(), |row| {
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
        // Write query (INSERT, UPDATE, DELETE, CREATE, ALTER)
        let rusqlite_params: Vec<Box<dyn rusqlite::types::ToSql>> = params
            .iter()
            .map(|v| -> Box<dyn rusqlite::types::ToSql> {
                match v {
                    Value::String(s) => Box::new(s.clone()),
                    Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            Box::new(i)
                        } else if let Some(f) = n.as_f64() {
                            Box::new(f)
                        } else {
                            Box::new(n.to_string())
                        }
                    }
                    Value::Bool(b) => Box::new(*b),
                    Value::Null => Box::new(rusqlite::types::Null),
                    _ => Box::new(v.to_string()),
                }
            })
            .collect();

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            rusqlite_params.iter().map(|p| p.as_ref()).collect();

        match conn.execute(query, param_refs.as_slice()) {
            Ok(affected) => ok(json!({
                "affected_rows": affected,
                "query_type": "write"
            })),
            Err(e) => err(format!("SQL execution error: {e}")),
        }
    }
}

// ---------------------------------------------------------------------------
// DbSchemaTool
// ---------------------------------------------------------------------------

/// Describe the schema of a SQLite database.
pub struct DbSchemaTool;

#[async_trait]
impl Tool for DbSchemaTool {
    fn name(&self) -> &str {
        "db_schema"
    }

    fn description(&self) -> &str {
        "Describe the schema of a SQLite database — tables, columns, types, and indexes."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "database_path": { "type": "string", "description": "Path to the SQLite database file" },
                "table_name": { "type": "string", "description": "Specific table to inspect (optional, lists all tables if omitted)" }
            },
            "required": ["database_path"]
        })
    }

    fn category(&self) -> &str {
        "data_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let db_path = match input.parameters.get("database_path").and_then(|v| v.as_str()) {
            Some(p) => p.to_string(),
            None => return err("Missing required parameter: database_path"),
        };
        let table_name = input
            .parameters
            .get("table_name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let result = tokio::task::spawn_blocking(move || {
            describe_schema(&db_path, table_name.as_deref())
        })
        .await;

        match result {
            Ok(output) => output,
            Err(e) => err(format!("Task panicked: {e}")),
        }
    }
}

fn describe_schema(db_path: &str, table_name: Option<&str>) -> ToolOutput {
    let conn = match rusqlite::Connection::open(db_path) {
        Ok(c) => c,
        Err(e) => return err(format!("Failed to open database: {e}")),
    };

    // Get list of tables
    let tables: Vec<String> = if let Some(name) = table_name {
        vec![name.to_string()]
    } else {
        let mut stmt = match conn.prepare(
            "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name",
        ) {
            Ok(s) => s,
            Err(e) => return err(format!("Failed to query tables: {e}")),
        };
        let rows = match stmt.query_map([], |row| row.get::<_, String>(0)) {
            Ok(rows) => rows.filter_map(|r| r.ok()).collect::<Vec<String>>(),
            Err(e) => return err(format!("Failed to list tables: {e}")),
        };
        rows
    };

    let mut table_schemas = Vec::new();

    for table in &tables {
        // Get column info
        let pragma = format!("PRAGMA table_info(\"{}\")", table.replace('"', "\"\""));
        let columns: Vec<Value> = match conn.prepare(&pragma) {
            Ok(mut stmt) => {
                match stmt.query_map([], |row| {
                    let cid: i32 = row.get(0)?;
                    let name: String = row.get(1)?;
                    let col_type: String = row.get(2)?;
                    let notnull: bool = row.get(3)?;
                    let pk: bool = row.get(5)?;
                    Ok(json!({
                        "cid": cid,
                        "name": name,
                        "type": col_type,
                        "not_null": notnull,
                        "primary_key": pk
                    }))
                }) {
                    Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
                    Err(_) => Vec::new(),
                }
            }
            Err(_) => Vec::new(),
        };

        // Get indexes
        let idx_pragma = format!("PRAGMA index_list(\"{}\")", table.replace('"', "\"\""));
        let indexes: Vec<Value> = match conn.prepare(&idx_pragma) {
            Ok(mut stmt) => {
                match stmt.query_map([], |row| {
                    let name: String = row.get(1)?;
                    let unique: bool = row.get(2)?;
                    Ok(json!({ "name": name, "unique": unique }))
                }) {
                    Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
                    Err(_) => Vec::new(),
                }
            }
            Err(_) => Vec::new(),
        };

        // Get row count
        let count_query = format!("SELECT COUNT(*) FROM \"{}\"", table.replace('"', "\"\""));
        let row_count: i64 = conn
            .query_row(&count_query, [], |row| row.get(0))
            .unwrap_or(0);

        table_schemas.push(json!({
            "table": table,
            "columns": columns,
            "indexes": indexes,
            "row_count": row_count
        }));
    }

    ok(json!({
        "database": db_path,
        "tables": table_schemas,
        "table_count": table_schemas.len()
    }))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sql_query_tool_metadata() {
        let tool = SqlQueryTool;
        assert_eq!(tool.name(), "sql_query");
        assert_eq!(tool.category(), "data_ops");
        let schema = tool.schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("database_path")));
        assert!(required.contains(&json!("query")));
    }

    #[test]
    fn db_schema_tool_metadata() {
        let tool = DbSchemaTool;
        assert_eq!(tool.name(), "db_schema");
        assert_eq!(tool.category(), "data_ops");
    }

    #[tokio::test]
    async fn sql_query_blocks_drop() {
        let tool = SqlQueryTool;
        let out = tool
            .execute(ToolInput {
                parameters: json!({
                    "database_path": "/tmp/test.db",
                    "query": "DROP TABLE users"
                }),
            })
            .await;
        assert!(!out.success);
        assert!(out.error.unwrap().contains("not allowed"));
    }
}
