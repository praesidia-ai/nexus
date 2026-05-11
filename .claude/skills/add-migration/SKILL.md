---
name: add-migration
description: Add a new SQLite migration to nexus-store. Use when creating new tables, adding columns, or changing the schema.
---

# Adding a SQLite Migration to nexus-store

## Current migration state

Baseline lives in [`001_baseline.sql`](crates/nexus-store/migrations/001_baseline.sql). Incremental migrations: `002_taste_tables`, `003_tenant_isolation`, `004_agent_tv_replay`, `005_marketplace_reputation`, `006_team_runs_scope`. Always check the actual highest number first:

```
ls crates/nexus-store/migrations/ | sort | tail -5
```

## Step 1 — Create the SQL file

File: `crates/nexus-store/migrations/0NN_<descriptive_name>.sql`

Rules:
- Use `CREATE TABLE IF NOT EXISTS` (never bare `CREATE TABLE`)
- Use `CREATE INDEX IF NOT EXISTS` (never bare `CREATE INDEX`)
- Every table needs `id TEXT PRIMARY KEY` (UUID as text) or `id INTEGER PRIMARY KEY AUTOINCREMENT`
- Add `created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))` on all new tables
- Add `updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))` on mutable tables
- For soft delete: add `deleted_at INTEGER` (nullable)
- Enable foreign keys in migration batch: they are already set globally via `PRAGMA foreign_keys = ON;` in `run_migrations`

```sql
-- 002_my_feature.sql

CREATE TABLE IF NOT EXISTS my_table (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    data TEXT,                                      -- JSON blob when schema is flexible
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_my_table_project_id ON my_table(project_id);
CREATE INDEX IF NOT EXISTS idx_my_table_created_at ON my_table(created_at);
```

## Step 2 — Register in db.rs

Open `crates/nexus-store/src/db.rs` and:

1. Add the constant near the existing `MIGRATION_*` constants:
```rust
const MIGRATION_MY_FEATURE: &str =
    include_str!("../migrations/00N_my_feature.sql");
```

2. Add the guard block at the end of `run_migrations`, after the last existing apply:
```rust
if current < N {
    apply(N, MIGRATION_MY_FEATURE)?;
}
```

The `apply` closure (already defined in `run_migrations`) wraps each migration in a `BEGIN IMMEDIATE` / `COMMIT` transaction with rollback on failure — never write your own `execute_batch + INSERT` pair, always go through `apply`. The version number in the guard must match the file prefix.

## Step 3 — Add store functions

Add CRUD functions in a new or existing file in `crates/nexus-store/src/`.

```rust
// crates/nexus-store/src/my_feature.rs

use rusqlite::{Connection, params};
use crate::error::Result;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MyRecord {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub data: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

pub fn create_my_record(conn: &Connection, record: &MyRecord) -> Result<()> {
    conn.execute(
        "INSERT INTO my_table (id, project_id, name, data, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            record.id,
            record.project_id,
            record.name,
            record.data,
            record.created_at,
            record.updated_at,
        ],
    )?;
    Ok(())
}

pub fn list_my_records(conn: &Connection, project_id: &str) -> Result<Vec<MyRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, project_id, name, data, created_at, updated_at
         FROM my_table WHERE project_id = ?1 ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map(params![project_id], |row| {
        Ok(MyRecord {
            id: row.get(0)?,
            project_id: row.get(1)?,
            name: row.get(2)?,
            data: row.get(3)?,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
}
```

## Step 4 — Export from nexus-store lib.rs

Add the module and re-export public types in `crates/nexus-store/src/lib.rs`:

```rust
pub mod my_feature;
pub use my_feature::{create_my_record, list_my_records, MyRecord};
```

## Invariants

- Baseline runs first (`current < 1`), then incremental migrations in ascending version order. Never renumber or reorder applied versions for existing databases.
- `run_migrations` is idempotent — re-running it is safe because of the `schema_migrations` version guard.
- Never use `ALTER TABLE DROP COLUMN` in SQLite (not supported in older SQLite versions). Use a new column or recreate the table via a new migration.
- For additive schema changes (add column): `ALTER TABLE my_table ADD COLUMN new_col TEXT;`
- For breaking changes: create a new table, migrate data, drop old table — all in one migration batch.

## Verify

```
cargo build -p nexus-store
cargo test -p nexus-store
```

The migration will be applied automatically on the next server start via `AppState::init`.
