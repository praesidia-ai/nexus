-- Agent TV persistent replay (see docs/NEXUS_MASTER_PLAN.md §4).
--
-- Every SSE run a conductor spawns is persisted here so that:
--   GET /tv/:runId            → replay endpoint (structured event stream)
--   GET /tv                   → public gallery of runs
--   GET /tv/:runId/embed      → oEmbed + iframe for social sharing
--
-- `agent_tv_runs` holds one row per run. `agent_tv_events` is the
-- append-only event log — one row per SSE event emitted by the
-- `build_event_bus` multiplexer.
--
-- Storage note: events are ordered by (run_id, seq) not by timestamp,
-- so replay scrubbing is deterministic even under clock skew.

CREATE TABLE IF NOT EXISTS agent_tv_runs (
    id              TEXT PRIMARY KEY NOT NULL,
    project_id      TEXT NOT NULL,
    tenant_id       TEXT,
    -- "oneshot" | "wave" | "swarm" | "agent" | "mutation" | …
    run_kind        TEXT NOT NULL,
    prompt          TEXT,
    status          TEXT NOT NULL DEFAULT 'running',
    -- Denormalised totals (updated on run completion). Avoid aggregate
    -- scans of agent_tv_events for the gallery view.
    total_events    INTEGER NOT NULL DEFAULT 0,
    total_tokens    INTEGER NOT NULL DEFAULT 0,
    total_cost_usd  REAL NOT NULL DEFAULT 0.0,
    duration_ms     INTEGER,
    -- Visibility: "private" (default) | "unlisted" | "public" (gallery)
    visibility      TEXT NOT NULL DEFAULT 'private',
    started_at      TEXT NOT NULL,
    completed_at    TEXT,
    -- Optional Trust Certificate references (§7).
    merkle_root     TEXT,
    signature       TEXT,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_agent_tv_runs_project
    ON agent_tv_runs(project_id, started_at DESC);

CREATE INDEX IF NOT EXISTS idx_agent_tv_runs_visibility
    ON agent_tv_runs(visibility, started_at DESC)
    WHERE visibility != 'private';

CREATE TABLE IF NOT EXISTS agent_tv_events (
    run_id      TEXT NOT NULL,
    -- Monotonic sequence within a run. First event is 0.
    seq         INTEGER NOT NULL,
    -- SSE event type: "agent_action" | "file_created" | "progress" |
    -- "taste_score" | "complete" | "error" | "lag" | …
    event_type  TEXT NOT NULL,
    -- Which conductor emitted this event (Nova / Atlas / Kai / …).
    conductor   TEXT,
    -- Which mini-agent (if any) emitted this event — dotted wire name
    -- from MiniKind::as_wire_str (e.g. "fs.reader", "test.writer").
    mini_kind   TEXT,
    -- JSON payload — shape depends on event_type.
    payload     TEXT NOT NULL,
    ts          TEXT NOT NULL,
    PRIMARY KEY (run_id, seq),
    FOREIGN KEY (run_id) REFERENCES agent_tv_runs(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_agent_tv_events_run
    ON agent_tv_events(run_id, seq);
