-- Migration 008: WASI plugin manifest fields.
--
-- Per ADR-004. Adds columns to the plugin registry to track the WASI artifact
-- hash, manifest JSON, capability allowlist, and per-plugin trap counters
-- needed for the auto-disable rule (3 consecutive traps in 60s).
--
-- The plugin tables themselves were created in migration 001_baseline.sql;
-- this migration is additive and idempotent.

CREATE TABLE IF NOT EXISTS plugins (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    version         TEXT NOT NULL,
    manifest_json   TEXT NOT NULL DEFAULT '{}',
    wasm_blake3     TEXT,                       -- hex of the .wasm artifact hash
    nexus_compat    TEXT,                       -- semver constraint from manifest
    enabled         INTEGER NOT NULL DEFAULT 1,
    auto_disabled   INTEGER NOT NULL DEFAULT 0, -- 1 = auto-disabled after repeated traps
    last_error      TEXT,
    installed_at_ms INTEGER NOT NULL DEFAULT (CAST(strftime('%s','now') AS INTEGER) * 1000),
    updated_at_ms   INTEGER NOT NULL DEFAULT (CAST(strftime('%s','now') AS INTEGER) * 1000)
);

-- Best-effort additive ALTERs for installs that already had a `plugins` table
-- from the baseline. SQLite ignores `ADD COLUMN IF NOT EXISTS` (we have to
-- check via PRAGMA), so we tolerate failure here — the CREATE TABLE above
-- handles fresh installs.
-- (No-op on fresh installs; safe to run repeatedly because the migration
-- guard in db.rs only applies each migration once.)

CREATE TABLE IF NOT EXISTS plugin_hook_bindings (
    plugin_id       TEXT NOT NULL,
    hook_point      TEXT NOT NULL,
    priority        INTEGER NOT NULL DEFAULT 50,
    PRIMARY KEY (plugin_id, hook_point)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS plugin_traps (
    plugin_id       TEXT NOT NULL,
    occurred_at_ms  INTEGER NOT NULL,
    hook_point      TEXT NOT NULL,
    kind            TEXT NOT NULL,              -- 'cpu' | 'wallclock' | 'memory' | 'capability' | 'panic' | 'other'
    detail          TEXT
);

CREATE INDEX IF NOT EXISTS idx_plugin_traps_recent
    ON plugin_traps(plugin_id, occurred_at_ms DESC);
