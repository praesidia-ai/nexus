-- Migration 002 — Taste engine persistence
--
-- Promotes the taste_reports / taste_profiles tables from the ad-hoc
-- handler-side `CREATE TABLE IF NOT EXISTS` calls into a versioned migration
-- so the canonical schema is auditable from one place.
--
-- Both statements use `IF NOT EXISTS` to remain a no-op on databases that
-- were initialised against the prior, handler-driven schema.

CREATE TABLE IF NOT EXISTS taste_reports (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id TEXT NOT NULL,
    profile TEXT NOT NULL,
    overall_score REAL NOT NULL,
    report_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_taste_project ON taste_reports(project_id);
CREATE INDEX IF NOT EXISTS idx_taste_reports_created ON taste_reports(created_at);

CREATE TABLE IF NOT EXISTS taste_profiles (
    project_id TEXT PRIMARY KEY,
    profile_json TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
