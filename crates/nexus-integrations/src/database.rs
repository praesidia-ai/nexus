//! Database integration — health checks and schema introspection.
//!
//! Supports **Supabase** via its REST API and provides informative error messages
//! for PostgreSQL, MySQL, and SQLite suggesting MCP server usage for direct access.

use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::IntegrationError;

// ─── Public data types ──────────────────────────────────────────────────────

/// Supported database backends.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DatabaseType {
    /// Standard PostgreSQL connection.
    #[serde(rename = "postgresql")]
    PostgreSQL,
    /// Standard MySQL connection.
    #[serde(rename = "mysql")]
    MySQL,
    /// Local SQLite file.
    #[serde(rename = "sqlite")]
    SQLite,
    /// Supabase project accessed via the PostgREST API.
    Supabase {
        /// The project URL, e.g. `https://xyz.supabase.co`.
        project_url: String,
        /// The anonymous (public) API key.
        anon_key: String,
    },
}

/// Summary information about a database table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableInfo {
    /// Table name.
    pub name: String,
    /// Approximate row count (if available).
    pub row_count: Option<u64>,
}

/// Full schema description for a single table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSchema {
    /// Table name.
    pub name: String,
    /// Ordered list of columns.
    pub columns: Vec<ColumnInfo>,
}

/// Metadata about a single column.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnInfo {
    /// Column name.
    pub name: String,
    /// SQL data type (e.g. `text`, `integer`, `timestamp with time zone`).
    pub data_type: String,
    /// Whether the column allows NULL values.
    pub nullable: bool,
    /// Whether the column is (part of) the primary key.
    pub primary_key: bool,
}

// ─── Supabase internal response types ───────────────────────────────────────

/// Row returned by Supabase's `information_schema.tables` query via PostgREST.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct SupabaseTableRow {
    table_name: String,
}

/// Row returned by Supabase's `information_schema.columns` query via PostgREST.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct SupabaseColumnRow {
    column_name: String,
    data_type: String,
    is_nullable: String,
    #[serde(default)]
    ordinal_position: Option<u64>,
}

/// Row from a count query against a specific table.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct SupabaseCountRow {
    count: Option<u64>,
}

// ─── Integration ────────────────────────────────────────────────────────────

/// Client for database health checks and schema introspection.
///
/// Currently only Supabase has full REST-based support. For PostgreSQL, MySQL,
/// and SQLite the integration returns descriptive errors recommending MCP servers.
pub struct DatabaseIntegration {
    db_type: DatabaseType,
    /// Connection string (used for non-Supabase types as a reference; Supabase
    /// uses the embedded `project_url` and `anon_key` instead).
    connection_info: String,
    client: reqwest::Client,
}

impl DatabaseIntegration {
    /// Create a new database integration.
    ///
    /// For Supabase, `connection` can be left empty or set to any descriptive
    /// string — the actual credentials come from [`DatabaseType::Supabase`].
    pub fn new(db_type: DatabaseType, connection: &str) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("failed to build reqwest client");

        Self {
            db_type,
            connection_info: connection.to_owned(),
            client,
        }
    }

    /// Check whether the database is reachable.
    ///
    /// For Supabase this hits the `/rest/v1/` endpoint. For other database types
    /// this returns a config error with guidance on how to connect directly.
    pub async fn health_check(&self) -> Result<bool, IntegrationError> {
        match &self.db_type {
            DatabaseType::Supabase {
                project_url,
                anon_key,
            } => {
                debug!(project_url = %project_url, "Supabase health check");
                let url = format!("{project_url}/rest/v1/");
                let resp = self
                    .client
                    .get(&url)
                    .header("apikey", anon_key.as_str())
                    .header("Authorization", format!("Bearer {anon_key}"))
                    .send()
                    .await
                    .map_err(IntegrationError::from_reqwest)?;

                // Supabase returns 200 with an empty array for the root endpoint
                // when the key is valid, or 401/403 otherwise.
                Ok(resp.status().is_success())
            }
            other => Err(IntegrationError::Config(format!(
                "Direct connection to {db_type} is not supported via REST. \
                 Use an MCP server (e.g. `@modelcontextprotocol/server-postgres`) for \
                 direct database access. Connection info: {conn}",
                db_type = db_type_label(other),
                conn = self.connection_info,
            ))),
        }
    }

    /// List tables in the database.
    ///
    /// For Supabase this queries `information_schema.tables` via PostgREST's
    /// RPC mechanism or a direct table read (requires the schema to be exposed).
    pub async fn list_tables(&self) -> Result<Vec<TableInfo>, IntegrationError> {
        match &self.db_type {
            DatabaseType::Supabase {
                project_url,
                anon_key,
            } => {
                debug!("Listing Supabase tables");

                // Use the Supabase RPC endpoint to query information_schema.
                // This requires an SQL function `list_tables()` or direct
                // access to `information_schema.tables` via the API.
                // We try the OpenAPI spec endpoint first which lists all exposed tables.
                let url = format!("{project_url}/rest/v1/");
                let resp = self
                    .client
                    .get(&url)
                    .header("apikey", anon_key.as_str())
                    .header("Authorization", format!("Bearer {anon_key}"))
                    .send()
                    .await
                    .map_err(IntegrationError::from_reqwest)?;

                if !resp.status().is_success() {
                    return Err(IntegrationError::from_response(resp).await);
                }

                // The root endpoint returns the OpenAPI spec as JSON. We parse
                // table names from the `definitions` or `paths` keys.
                let spec: serde_json::Value = resp
                    .json()
                    .await
                    .map_err(IntegrationError::from_reqwest)?;

                let mut tables = Vec::new();

                // PostgREST OpenAPI spec has `definitions` with one key per table.
                if let Some(definitions) = spec.get("definitions").and_then(|d| d.as_object()) {
                    for table_name in definitions.keys() {
                        tables.push(TableInfo {
                            name: table_name.clone(),
                            row_count: None,
                        });
                    }
                } else if let Some(paths) = spec.get("paths").and_then(|p| p.as_object()) {
                    // Fallback: extract table names from path keys like `/table_name`.
                    for path in paths.keys() {
                        let name = path.trim_start_matches('/');
                        if !name.is_empty() && !name.contains('/') {
                            tables.push(TableInfo {
                                name: name.to_owned(),
                                row_count: None,
                            });
                        }
                    }
                }

                tables.sort_by(|a, b| a.name.cmp(&b.name));
                Ok(tables)
            }
            other => Err(IntegrationError::Config(format!(
                "Direct table listing for {db_type} is not supported via REST. \
                 Use an MCP server for direct database access.",
                db_type = db_type_label(other),
            ))),
        }
    }

    /// Describe a table's schema (columns, types, nullability, primary keys).
    ///
    /// For Supabase this reads from the OpenAPI spec's `definitions` section.
    pub async fn describe_table(&self, table: &str) -> Result<TableSchema, IntegrationError> {
        match &self.db_type {
            DatabaseType::Supabase {
                project_url,
                anon_key,
            } => {
                debug!(table, "Describing Supabase table");

                let url = format!("{project_url}/rest/v1/");
                let resp = self
                    .client
                    .get(&url)
                    .header("apikey", anon_key.as_str())
                    .header("Authorization", format!("Bearer {anon_key}"))
                    .send()
                    .await
                    .map_err(IntegrationError::from_reqwest)?;

                if !resp.status().is_success() {
                    return Err(IntegrationError::from_response(resp).await);
                }

                let spec: serde_json::Value = resp
                    .json()
                    .await
                    .map_err(IntegrationError::from_reqwest)?;

                let definition = spec
                    .get("definitions")
                    .and_then(|d| d.get(table))
                    .ok_or_else(|| {
                        IntegrationError::NotFound(format!("Table '{table}' not found in schema"))
                    })?;

                let properties = definition
                    .get("properties")
                    .and_then(|p| p.as_object())
                    .unwrap_or(&serde_json::Map::new())
                    .clone();

                let required: Vec<String> = definition
                    .get("required")
                    .and_then(|r| serde_json::from_value(r.clone()).ok())
                    .unwrap_or_default();

                let mut columns = Vec::new();
                for (col_name, col_spec) in &properties {
                    let data_type = col_spec
                        .get("format")
                        .or_else(|| col_spec.get("type"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_owned();

                    let description = col_spec
                        .get("description")
                        .and_then(|d| d.as_str())
                        .unwrap_or("");

                    let primary_key = description.contains("Primary Key")
                        || description.contains("primary key");

                    columns.push(ColumnInfo {
                        name: col_name.clone(),
                        data_type,
                        nullable: !required.contains(col_name),
                        primary_key,
                    });
                }

                columns.sort_by(|a, b| a.name.cmp(&b.name));

                Ok(TableSchema {
                    name: table.to_owned(),
                    columns,
                })
            }
            other => Err(IntegrationError::Config(format!(
                "Direct table description for {db_type} is not supported via REST. \
                 Use an MCP server for direct database access.",
                db_type = db_type_label(other),
            ))),
        }
    }
}

/// Human-readable label for a database type.
fn db_type_label(db_type: &DatabaseType) -> &'static str {
    match db_type {
        DatabaseType::PostgreSQL => "PostgreSQL",
        DatabaseType::MySQL => "MySQL",
        DatabaseType::SQLite => "SQLite",
        DatabaseType::Supabase { .. } => "Supabase",
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_type_serializes_with_tag() {
        let pg = DatabaseType::PostgreSQL;
        let json = serde_json::to_value(&pg).unwrap();
        assert_eq!(json["type"], "postgresql");

        let sb = DatabaseType::Supabase {
            project_url: "https://xyz.supabase.co".into(),
            anon_key: "key123".into(),
        };
        let json = serde_json::to_value(&sb).unwrap();
        assert_eq!(json["type"], "supabase");
        assert_eq!(json["project_url"], "https://xyz.supabase.co");
    }

    #[test]
    fn table_info_serializes() {
        let info = TableInfo {
            name: "users".into(),
            row_count: Some(42),
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"users\""));
        assert!(json.contains("42"));
    }

    #[test]
    fn column_info_round_trips() {
        let col = ColumnInfo {
            name: "email".into(),
            data_type: "text".into(),
            nullable: false,
            primary_key: false,
        };
        let json = serde_json::to_value(&col).unwrap();
        let col2: ColumnInfo = serde_json::from_value(json).unwrap();
        assert_eq!(col2.name, "email");
        assert!(!col2.nullable);
    }

    #[test]
    fn non_supabase_health_check_returns_config_error() {
        let db = DatabaseIntegration::new(
            DatabaseType::PostgreSQL,
            "postgresql://localhost/test",
        );
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = rt.block_on(db.health_check());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntegrationError::Config(_)));
    }
}
