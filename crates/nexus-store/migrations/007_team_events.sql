-- Migration 007: durable team-event log.
--
-- Per ADR-002. Replaces the in-memory `Arc<RwLock<Vec<...>>>` state in
-- `crates/nexus-http/src/team_orchestrator.rs:165-167,319-325` with an
-- event-sourced log so a server restart resumes in-flight team runs from
-- the last persisted event instead of dropping every conversation.
--
-- Audit references: NEXUS_FEATURE_AUDIT.md §1.6 #13, #15.

-- Append-only event log. Source of truth.
CREATE TABLE IF NOT EXISTS team_events (
    run_id          TEXT NOT NULL,
    seq             INTEGER NOT NULL,            -- monotonic per run_id, 1-based
    occurred_at_ms  INTEGER NOT NULL,            -- unix ms
    event_type      TEXT NOT NULL,               -- 'message' | 'task' | 'artifact' | 'state' | 'phase' | 'error'
    actor           TEXT,                        -- agent id or 'system'
    payload_json    TEXT NOT NULL,               -- typed by event_type
    PRIMARY KEY (run_id, seq)
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_team_events_occurred
    ON team_events(run_id, occurred_at_ms);

-- Materialised current state, regenerated on resume from the event log.
-- Optional cache; events are authoritative.
CREATE TABLE IF NOT EXISTS team_workspace_state (
    run_id          TEXT NOT NULL,
    key             TEXT NOT NULL,
    value_json      TEXT NOT NULL,
    updated_seq     INTEGER NOT NULL,            -- which event wrote this
    updated_at_ms   INTEGER NOT NULL,
    PRIMARY KEY (run_id, key)
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_team_workspace_state_run
    ON team_workspace_state(run_id, updated_seq DESC);

-- Run-level metadata. Note: the existing `team_runs` table from migration 006
-- uses a different shape (legacy run-id keyed); we extend rather than collide
-- by introducing `team_run_state` for the durable lifecycle metadata.
CREATE TABLE IF NOT EXISTS team_run_state (
    run_id          TEXT PRIMARY KEY,
    team_id         TEXT NOT NULL,
    project_id      TEXT NOT NULL,
    tenant_id       TEXT NOT NULL,
    status          TEXT NOT NULL,               -- 'running' | 'paused' | 'completed' | 'failed' | 'cancelled'
    started_at_ms   INTEGER NOT NULL,
    last_event_seq  INTEGER NOT NULL DEFAULT 0,
    last_event_at_ms INTEGER NOT NULL,
    pause_reason    TEXT,
    error           TEXT
);

CREATE INDEX IF NOT EXISTS idx_team_run_state_tenant_status
    ON team_run_state(tenant_id, status);
CREATE INDEX IF NOT EXISTS idx_team_run_state_project
    ON team_run_state(project_id);
CREATE INDEX IF NOT EXISTS idx_team_run_state_status_heartbeat
    ON team_run_state(status, last_event_at_ms);
