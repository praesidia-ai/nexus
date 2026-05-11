# ADR-002 — Durable team-event schema

- **Status:** accepted (2026-04-26)
- **Owners:** Nova (backend), Sage (data)
- **Unblocks roadmap:** #4 durable team orchestrator
- **Closes audit weaknesses:** §1.6 #13 (in-memory `Arc<RwLock<Vec<T>>>` at `team_orchestrator.rs:165-167,319-325`), #15 (silent 1000-key cap at `:357`)

## Context

`team_orchestrator.rs` is the runtime for multi-agent business teams (the wedge from `NEXUS_TOP1_RESEARCH §2.4` — Nova/Atlas/Kai/Luna/Orion/Sage/Ivy/Rex/Leo/Mia operating the deployed product). Today every team's messages, tasks, artifacts and shared workspace state live in `Arc<RwLock<Vec<…>>>` and `Arc<RwLock<HashMap<…>>>` on the heap of one `nexus-server` process. A graceful or ungraceful restart wipes every in-flight team. The wedge demo's claim ("your business has 10 agents on staff, sleep well") is currently false: the staff dies when the server reboots.

Endpoints `POST /teams/runs/:run_id/resume` and `/pause` are wired in the router but no-op against in-memory state.

We need an event-sourced log that survives restart, keyed by `run_id`, with a hot in-memory index for read latency. We do **not** need cross-instance distribution yet (Temporal-class durability is a separate, later decision).

## Decision

### 1. New SQLite migration `007_team_events.sql`

```sql
-- Append-only event log. The source of truth.
CREATE TABLE team_events (
    run_id          TEXT NOT NULL,
    seq             INTEGER NOT NULL,            -- monotonic per run_id, 1-based
    occurred_at     INTEGER NOT NULL,            -- unix ms
    event_type      TEXT NOT NULL,               -- 'message' | 'task' | 'artifact' | 'state' | 'phase' | 'error'
    actor           TEXT,                        -- agent id or 'system'
    payload_json    TEXT NOT NULL,               -- typed by event_type
    PRIMARY KEY (run_id, seq)
) WITHOUT ROWID;
CREATE INDEX idx_team_events_occurred ON team_events(run_id, occurred_at);

-- Materialised current state, regenerated on resume from the event log.
-- Optional cache; events are authoritative.
CREATE TABLE team_workspace_state (
    run_id          TEXT NOT NULL,
    key             TEXT NOT NULL,
    value_json      TEXT NOT NULL,
    updated_seq     INTEGER NOT NULL,            -- which event wrote this
    PRIMARY KEY (run_id, key)
) WITHOUT ROWID;

-- Run-level metadata (status, started_at, paused_at, last_seq).
CREATE TABLE team_runs (
    run_id          TEXT PRIMARY KEY,
    team_id         TEXT NOT NULL,
    project_id      TEXT NOT NULL,
    tenant_id       TEXT NOT NULL,
    status          TEXT NOT NULL,               -- 'running' | 'paused' | 'completed' | 'failed' | 'cancelled'
    started_at      INTEGER NOT NULL,
    last_event_seq  INTEGER NOT NULL DEFAULT 0,
    last_event_at   INTEGER NOT NULL,
    pause_reason    TEXT,
    error           TEXT
);
CREATE INDEX idx_team_runs_tenant_status ON team_runs(tenant_id, status);
CREATE INDEX idx_team_runs_project ON team_runs(project_id);
```

### 2. Event taxonomy (`event_type` + payload schema)

```rust
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TeamEventPayload {
    Message    { from: String, to: TeamRecipient, body: String, content_refs: Vec<String> },
    Task       { task_id: String, op: TaskOp, assignee: Option<String>, parent: Option<String> },
    Artifact   { artifact_id: String, kind: String, ref_uri: String, sha256: String },
    State      { key: String, value: serde_json::Value, prev_seq: Option<i64> },
    Phase      { phase: TeamPhase, prev_phase: Option<TeamPhase> },
    Error      { code: String, message: String, agent: Option<String>, recoverable: bool },
}
```

Every mutation that exists today as `Vec::push` / `HashMap::insert` becomes an `append_event(run_id, payload)` call. **There is no other write path.**

### 3. Append API

```rust
pub trait TeamEventStore: Send + Sync {
    async fn append(&self, run_id: &str, payload: TeamEventPayload) -> Result<i64 /* seq */>;
    async fn read_from(&self, run_id: &str, after_seq: i64, limit: usize) -> Result<Vec<TeamEvent>>;
    async fn last_seq(&self, run_id: &str) -> Result<i64>;
    async fn get_state(&self, run_id: &str, key: &str) -> Result<Option<serde_json::Value>>;
    async fn put_state(&self, run_id: &str, key: &str, value: serde_json::Value, seq: i64) -> Result<()>;
}
```

- `append` writes the event in a single transaction and increments `team_runs.last_event_seq`. Lock guard is dropped before any `.await` past the transaction (CLAUDE.md invariant).
- Writes are batched in a 50ms window per `run_id` to reduce SQLite contention. Hard ceiling: 100 events / batch.
- `state` events also update `team_workspace_state` in the same tx so read-back is O(1).

### 4. Hot index (in-memory, derived)

Each running team holds an `Arc<RwLock<TeamRunIndex>>` populated on `start_run` / `resume_run` from the event log. The index is a **derivation**, not the source of truth:

- `messages: Vec<AgentMessage>` (cap 1000, then ring-buffer; older events served from SQLite on demand).
- `subscribers: HashMap<member_id, mpsc::Sender>` — same as today.
- `event_tx: broadcast::Sender<TeamEvent>` — same as today, capacity 4096.

Crash → in-memory index is lost; on next `resume_run(run_id)` it's rebuilt by replaying `team_events` (≤1000 most recent + state snapshot).

### 5. Deletion of in-memory primary state

The following fields in `crates/nexus-http/src/team_orchestrator.rs` are deleted:
- `TeamBus.messages: Arc<RwLock<Vec<AgentMessage>>>` (line 165)
- `TeamWorkspace.state: Arc<RwLock<HashMap<String, serde_json::Value>>>` (lines 319-325 area)
- The silent 1000-key cap at `:357`.

Replaced in place by `TeamEventStore` calls. **No `_legacy_messages` field stays around.**

### 6. Resume semantics

`POST /teams/runs/:run_id/resume`:
1. Load `team_runs` row; if `status != 'paused'` return 409.
2. Replay event log to rebuild hot index.
3. Re-broadcast a `Phase::Resumed` event (seq+1) so SSE consumers reattach cleanly.
4. Mark `status = 'running'`.

Crash recovery on server boot: scan `team_runs WHERE status = 'running' AND last_event_at < (now - 60s)` → mark as `paused` with `pause_reason = 'server_restart'`. Operators / users explicitly resume them.

### 7. Tenant isolation

Every query filters by `tenant_id` via `validate_project_access` (existing helper in `security/tenant.rs:20`). New `team_runs.tenant_id` column is enforced NOT NULL in the migration.

### 8. Backpressure on overflow

The 1000-key cap (today silent) is replaced with: every `state` event over 64 KB payload OR every `state` write that would put `team_workspace_state` row count > 100k for one run is **rejected** with `ApiError::TeamStateOverflow`. Metric: `nexus_team_state_overflow_total{run_id}`. The agent receives a structured tool error and can compact.

## Consequences

**Positive**
- Wedge demo's "the team is still running tomorrow" claim becomes literally true.
- `pause` / `resume` endpoints stop being theatrical.
- One source of truth → easier to debug ("what did Leo tell Ivy at 14:32?" is a single SQL query).
- `team_events` is the substrate the audit chain (`audit_trail_handler.rs`) can sign over for cryptographic non-repudiation later.

**Negative**
- Every message round-trip now includes a SQLite write. Mitigated by 50ms batch window and `WITHOUT ROWID` PK.
- Migration touches a hot table; runs on first boot after upgrade.
- Increases test surface — Mia owns a `tests/team_durability.rs` integration test (kill -9 mid-run, assert resume).

**Neutral**
- Sets the precedent that all in-memory `Arc<RwLock<Vec<…>>>` in `nexus-http/src/` storing business data are tech debt. Subsequent ADRs will pull more of them onto durable schemas.

## Alternatives considered

- **Use the existing `agent_tv_events` table.** Rejected: `agent_tv` is presentational; coupling the operational substrate to it would constrain both. Separate table, loose join via `run_id`.
- **Embed Temporal / Inngest now.** Rejected for v1: blocks #4 on a multi-month integration. Revisit after #10 SSO ships and we have multi-tenant-cluster customers.
- **Redis stream for events, SQLite only for snapshots.** Rejected: adds a runtime dependency that blocks the §2.2 #10 single-binary pattern. SQLite is fast enough for the per-tenant event rate we model (peak ~200 events/sec/run).
- **Keep `Vec<AgentMessage>` as primary, snapshot to SQLite every N events.** Rejected: snapshot windows lose data on crash; event-sourcing was the requirement.

## Acceptance test

```rust
// tests/team_durability.rs
#[tokio::test]
async fn team_run_survives_restart() {
    let app = TestApp::start().await;
    let run_id = app.start_team_run("yoga-saas-team").await;
    app.send_message(&run_id, "Leo", "Ivy", "draft instagram post").await;
    app.kill_server().await;        // SIGKILL
    let app = TestApp::restart().await;
    app.resume_run(&run_id).await;
    let history = app.message_history(&run_id).await;
    assert!(history.iter().any(|m| m.body.contains("instagram")));
    assert_eq!(app.run_status(&run_id).await, "running");
}
```
