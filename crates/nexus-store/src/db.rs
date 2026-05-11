use rusqlite::Connection;

use crate::error::Result;

const MIGRATION_BASELINE: &str = include_str!("../migrations/001_baseline.sql");
const MIGRATION_TASTE: &str = include_str!("../migrations/002_taste_tables.sql");
const MIGRATION_TENANT_ISOLATION: &str =
    include_str!("../migrations/003_tenant_isolation.sql");
const MIGRATION_AGENT_TV_REPLAY: &str =
    include_str!("../migrations/004_agent_tv_replay.sql");
const MIGRATION_MARKETPLACE_REPUTATION: &str =
    include_str!("../migrations/005_marketplace_reputation.sql");
const MIGRATION_TEAM_RUNS_SCOPE: &str =
    include_str!("../migrations/006_team_runs_scope.sql");
const MIGRATION_TEAM_EVENTS: &str =
    include_str!("../migrations/007_team_events.sql");
const MIGRATION_PLUGINS_WASI: &str =
    include_str!("../migrations/008_plugins_wasi.sql");
const MIGRATION_COST_RECORDS: &str =
    include_str!("../migrations/009_cost_records.sql");

pub fn run_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY NOT NULL);",
    )?;

    // Read current schema version once; we apply each migration in-order.
    // Each migration runs inside its own transaction so a failure mid-DDL
    // does NOT leave the DB in a half-migrated state (previously we could
    // write partial DDL + skip the version-row insert and re-run would then
    // fail with `duplicate column`/`table exists`).
    let current: i64 = conn.query_row(
        "SELECT COALESCE((SELECT MAX(version) FROM schema_migrations), 0)",
        [],
        |row| row.get(0),
    )?;

    let apply = |version: i64, sql: &str| -> Result<()> {
        conn.execute_batch("BEGIN IMMEDIATE")?;
        // On any failure below we roll back; otherwise commit atomically.
        let run = (|| -> Result<()> {
            conn.execute_batch(sql)?;
            conn.execute(
                "INSERT OR IGNORE INTO schema_migrations (version) VALUES (?1)",
                [version],
            )?;
            Ok(())
        })();
        match run {
            Ok(()) => {
                conn.execute_batch("COMMIT")?;
                Ok(())
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    };

    if current < 1 {
        apply(1, MIGRATION_BASELINE)?;
    }
    if current < 2 {
        apply(2, MIGRATION_TASTE)?;
    }
    if current < 3 {
        apply(3, MIGRATION_TENANT_ISOLATION)?;
    }
    if current < 4 {
        apply(4, MIGRATION_AGENT_TV_REPLAY)?;
    }
    if current < 5 {
        apply(5, MIGRATION_MARKETPLACE_REPUTATION)?;
    }
    if current < 6 {
        apply(6, MIGRATION_TEAM_RUNS_SCOPE)?;
    }
    if current < 7 {
        apply(7, MIGRATION_TEAM_EVENTS)?;
    }
    if current < 8 {
        apply(8, MIGRATION_PLUGINS_WASI)?;
    }
    if current < 9 {
        apply(9, MIGRATION_COST_RECORDS)?;
    }
    Ok(())
}

pub fn open_connection(path: &std::path::Path) -> Result<Connection> {
    let conn = if path.to_str() == Some(":memory:") {
        Connection::open_in_memory()?
    } else {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                crate::error::StoreError::Msg(format!("create_dir_all: {}", e))
            })?;
        }
        Connection::open(path)?
    };
    // WAL mode allows concurrent readers + one writer, substantially improving
    // throughput under multi-request workloads. busy_timeout prevents immediate
    // SQLITE_BUSY errors under write contention; synchronous=NORMAL is safe with WAL.
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA busy_timeout=5000;
         PRAGMA cache_size=-32000;",
    )?;
    run_migrations(&conn)?;
    Ok(conn)
}
