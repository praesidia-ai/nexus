-- Migration 003: tenant-scope user_preferences so one tenant cannot overwrite
-- another's preference rows. The baseline schema keyed user_preferences on
-- (category, key) alone which is a singleton across tenants.
--
-- The migration runner wraps this file in BEGIN IMMEDIATE ... COMMIT/ROLLBACK,
-- so any failure rolls the whole thing back. In addition, every statement
-- here is written to be safe to re-run if an operator ever has to manually
-- recover from a half-applied state: we drop any leftover *_new table first
-- and we guard the rename with IF EXISTS.

-- Clean up any stray rebuild table from a previously aborted attempt.
DROP TABLE IF EXISTS user_preferences_new;

-- Add tenant_id column (default 'default' backfills legacy rows).
-- SQLite has no `ADD COLUMN IF NOT EXISTS`; if the column already exists the
-- surrounding BEGIN IMMEDIATE ... ROLLBACK in run_migrations undoes the
-- partial work and the operator can drop the column and retry.
ALTER TABLE user_preferences ADD COLUMN tenant_id TEXT NOT NULL DEFAULT 'default';

-- Rebuild the table with the new composite uniqueness constraint. SQLite
-- cannot drop or alter an existing UNIQUE constraint in place, so we copy
-- rows into a fresh table and swap them.
CREATE TABLE user_preferences_new (
    id              TEXT PRIMARY KEY,
    tenant_id       TEXT NOT NULL DEFAULT 'default',
    category        TEXT NOT NULL,
    key             TEXT NOT NULL,
    value           TEXT NOT NULL,
    confidence      REAL NOT NULL DEFAULT 0.5,
    times_observed  INTEGER NOT NULL DEFAULT 1,
    source          TEXT NOT NULL DEFAULT 'inferred',
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(tenant_id, category, key)
);

INSERT INTO user_preferences_new
    (id, tenant_id, category, key, value, confidence, times_observed, source, created_at, updated_at)
SELECT
    id, tenant_id, category, key, value, confidence, times_observed, source, created_at, updated_at
FROM user_preferences;

DROP TABLE IF EXISTS user_preferences;
ALTER TABLE user_preferences_new RENAME TO user_preferences;

CREATE INDEX IF NOT EXISTS idx_user_preferences_tenant ON user_preferences(tenant_id);
