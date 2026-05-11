-- Migration 006: add tenant and project scope to team_runs.
--
-- The baseline team_runs table recorded team-run execution but carried no
-- ownership columns — any authenticated caller could list or replay a run
-- regardless of which project or tenant it came from, and billing could not
-- be attributed. This migration backfills both columns with safe defaults
-- so pre-existing rows remain readable; new inserts are required to set
-- tenant_id explicitly (enforced at the application layer).
--
-- Wrapped in BEGIN IMMEDIATE/COMMIT by run_migrations; re-entry safe.

-- project_id may legitimately be NULL for "global" team runs not yet
-- migrated to a specific project; tenant_id must exist.
ALTER TABLE team_runs ADD COLUMN tenant_id TEXT NOT NULL DEFAULT 'default';
ALTER TABLE team_runs ADD COLUMN project_id TEXT;

-- Index pairings: tenant-first so list queries scoped to one tenant are
-- O(matching rows); project index supports per-project history views.
CREATE INDEX IF NOT EXISTS idx_team_runs_tenant     ON team_runs(tenant_id);
CREATE INDEX IF NOT EXISTS idx_team_runs_project    ON team_runs(project_id);
CREATE INDEX IF NOT EXISTS idx_team_runs_tenant_started
    ON team_runs(tenant_id, started_at DESC);
