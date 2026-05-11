//! Tables and records handlers.
//! Tables are materialized SQLite tables in the project's data.db.
//! Records are rows in those tables.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use nexus_store::SchemaBuilder;

use crate::{
    error::{ApiError, ApiResult},
    security::auth::AuthContext,
    security::tenant::validate_project_access,
    state::AppState,
};

// ---------------------------------------------------------------------------
// GET /projects/:id/tables
// ---------------------------------------------------------------------------

pub async fn list_tables(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(project_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;
    let sb = SchemaBuilder::new(&db);
    let tables = sb.list_tables(&project_id)?;
    Ok(Json(serde_json::to_value(tables)?))
}

// ---------------------------------------------------------------------------
// GET /projects/:id/tables/:table_name/records
// ---------------------------------------------------------------------------

pub async fn list_records(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path((project_id, table_name)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    {
        let db = app.db.lock().await;
        validate_project_access(&db, &project_id, &auth.tenant_id)
            .map_err(ApiError::Forbidden)?;
    }
    let data_db = app.project_data_db(&project_id);
    if !data_db.exists() {
        return Ok(Json(serde_json::json!({"records": [], "columns": []})));
    }

    let conn = rusqlite::Connection::open(&data_db)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let safe_name = sanitize_identifier(&table_name);

    // Get column names
    let mut col_stmt = conn
        .prepare(&format!("PRAGMA table_info({})", safe_name))
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let columns: Vec<String> = col_stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    if columns.is_empty() {
        return Err(ApiError::NotFound(format!("table {} not found", table_name)));
    }

    // Fetch rows
    let mut stmt = conn
        .prepare(&format!("SELECT * FROM {} ORDER BY rowid DESC LIMIT 1000", safe_name))
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let col_count = columns.len();
    let rows: Vec<Vec<serde_json::Value>> = stmt
        .query_map([], |row| {
            let mut values = Vec::new();
            for i in 0..col_count {
                let v: rusqlite::types::Value = row.get(i)?;
                values.push(value_to_json(v));
            }
            Ok(values)
        })
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(Json(serde_json::json!({
        "columns": columns,
        "records": rows
    })))
}

// ---------------------------------------------------------------------------
// POST /projects/:id/tables/:table_name/records
// ---------------------------------------------------------------------------

pub async fn insert_record(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path((project_id, table_name)): Path<(String, String)>,
    Json(body): Json<serde_json::Map<String, serde_json::Value>>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    {
        let db = app.db.lock().await;
        validate_project_access(&db, &project_id, &auth.tenant_id)
            .map_err(ApiError::Forbidden)?;
    }
    let data_db = app.project_data_db(&project_id);
    if !data_db.exists() {
        return Err(ApiError::NotFound(format!(
            "No data.db for project {}",
            project_id
        )));
    }

    let conn = rusqlite::Connection::open(&data_db)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let safe_table = sanitize_identifier(&table_name);

    let cols: Vec<String> = body.keys().map(|k| sanitize_identifier(k)).collect();
    let placeholders: Vec<String> = (1..=cols.len()).map(|i| format!("?{}", i)).collect();

    let sql = format!(
        "INSERT INTO {} ({}) VALUES ({})",
        safe_table,
        cols.join(", "),
        placeholders.join(", ")
    );

    let values: Vec<rusqlite::types::Value> = body
        .values()
        .map(json_to_sql_value)
        .collect();

    conn.execute(&sql, rusqlite::params_from_iter(values.iter()))
        .map_err(map_sqlite_err)?;

    let rowid = conn.last_insert_rowid();

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({"rowid": rowid, "ok": true})),
    ))
}

// ---------------------------------------------------------------------------
// DELETE /projects/:id/tables/:table_name/records/:rowid
// ---------------------------------------------------------------------------

pub async fn delete_record(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path((project_id, table_name, rowid)): Path<(String, String, i64)>,
) -> ApiResult<StatusCode> {
    {
        let db = app.db.lock().await;
        validate_project_access(&db, &project_id, &auth.tenant_id)
            .map_err(ApiError::Forbidden)?;
    }
    let data_db = app.project_data_db(&project_id);
    if !data_db.exists() {
        return Err(ApiError::NotFound("No data.db".to_string()));
    }

    let conn = rusqlite::Connection::open(&data_db)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let safe_table = sanitize_identifier(&table_name);
    conn.execute(
        &format!("DELETE FROM {} WHERE rowid = ?1", safe_table),
        rusqlite::params![rowid],
    )
    .map_err(map_sqlite_err)?;

    Ok(StatusCode::NO_CONTENT)
}

/// Map SQLite errors raised by user-supplied INSERT/DELETE payloads onto the
/// correct HTTP class. Without this every NOT NULL / FK / type / unique
/// violation became a generic `500 Internal Server Error` and the user had no
/// idea their input was the problem.
fn map_sqlite_err(e: rusqlite::Error) -> ApiError {
    use rusqlite::ErrorCode::*;
    if let rusqlite::Error::SqliteFailure(ref err, ref msg) = e {
        let detail = msg.clone().unwrap_or_else(|| e.to_string());
        match err.code {
            ConstraintViolation => return ApiError::BadRequest(format!("constraint violation: {detail}")),
            TypeMismatch => return ApiError::BadRequest(format!("type mismatch: {detail}")),
            DatabaseCorrupt | DiskFull => return ApiError::Internal(detail),
            _ => {}
        }
    }
    if matches!(e, rusqlite::Error::QueryReturnedNoRows) {
        return ApiError::NotFound("not found".into());
    }
    ApiError::Internal(e.to_string())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn sanitize_identifier(s: &str) -> String {
    let clean: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect();
    if clean.starts_with(|c: char| c.is_ascii_digit()) {
        format!("_{}", clean)
    } else {
        clean
    }
}

fn value_to_json(v: rusqlite::types::Value) -> serde_json::Value {
    match v {
        rusqlite::types::Value::Null => serde_json::Value::Null,
        rusqlite::types::Value::Integer(i) => serde_json::Value::Number(i.into()),
        rusqlite::types::Value::Real(f) => {
            serde_json::Number::from_f64(f)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null)
        }
        rusqlite::types::Value::Text(s) => serde_json::Value::String(s),
        rusqlite::types::Value::Blob(b) => {
            serde_json::Value::String(format!("<blob {} bytes>", b.len()))
        }
    }
}

fn json_to_sql_value(v: &serde_json::Value) -> rusqlite::types::Value {
    match v {
        serde_json::Value::Null => rusqlite::types::Value::Null,
        serde_json::Value::Bool(b) => rusqlite::types::Value::Integer(*b as i64),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                rusqlite::types::Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                rusqlite::types::Value::Real(f)
            } else {
                rusqlite::types::Value::Text(n.to_string())
            }
        }
        serde_json::Value::String(s) => rusqlite::types::Value::Text(s.clone()),
        other => rusqlite::types::Value::Text(other.to_string()),
    }
}
