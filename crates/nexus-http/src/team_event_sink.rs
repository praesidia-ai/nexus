//! Bridge between the in-memory team orchestrator (`TeamBus` /
//! `TeamWorkspace`) and the durable [`nexus_store::team_events`] log.
//!
//! Per ADR-002 §4: the event log is the source of truth, the in-memory
//! collections are a hot read index. This module is the *write* side of
//! that contract — every `TeamBus::send` and `TeamWorkspace::set_state`
//! call shadow-writes into SQLite via a [`TeamEventSink`].
//!
//! All writes happen behind the workspace SQLite mutex
//! (`Arc<tokio::sync::Mutex<rusqlite::Connection>>`). Per CLAUDE.md
//! invariant #1 we only hold the lock for the bounded transaction below
//! and drop it before any further `.await`.

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::Mutex;

use nexus_store::team_events::{
    TeamEventStore, EVENT_ARTIFACT, EVENT_ERROR, EVENT_MESSAGE, EVENT_PHASE, EVENT_TASK,
    RUN_PAUSED, RUN_RUNNING,
};

/// A handle to the SQLite-backed durable event log scoped to one team run.
///
/// Cheap to clone — internally a pair of `Arc`s. Cloning lets the bus and
/// the workspace share one logical sink without locking each other out.
#[derive(Clone)]
pub struct TeamEventSink {
    db: Arc<Mutex<rusqlite::Connection>>,
    run_id: String,
}

impl TeamEventSink {
    /// Construct a new sink. Caller has already invoked
    /// [`register_run`](nexus_store::team_events::TeamEventStore::register_run)
    /// for `run_id` (typically at orchestrator startup).
    pub fn new(db: Arc<Mutex<rusqlite::Connection>>, run_id: impl Into<String>) -> Self {
        Self {
            db,
            run_id: run_id.into(),
        }
    }

    /// Run id this sink is scoped to.
    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    /// Register a new run with the lifecycle table. Idempotent.
    pub async fn register_run(
        &self,
        team_id: &str,
        project_id: &str,
        tenant_id: &str,
        started_at_ms: i64,
    ) -> Result<(), String> {
        let conn = self.db.lock().await;
        TeamEventStore::new(&conn)
            .register_run(&self.run_id, team_id, project_id, tenant_id, started_at_ms)
            .map_err(|e| format!("register_run: {e}"))
    }

    /// Persist a `message` event. Errors are logged and swallowed —
    /// the in-memory bus has already accepted the message and the user
    /// can still see it; durability is best-effort, never blocking.
    pub async fn append_message(&self, actor: Option<&str>, payload: &Value) {
        self.append_event(EVENT_MESSAGE, actor, payload).await;
    }

    /// Persist a `task` event.
    pub async fn append_task(&self, actor: Option<&str>, payload: &Value) {
        self.append_event(EVENT_TASK, actor, payload).await;
    }

    /// Persist an `artifact` event.
    pub async fn append_artifact(&self, actor: Option<&str>, payload: &Value) {
        self.append_event(EVENT_ARTIFACT, actor, payload).await;
    }

    /// Persist a `phase` event (run-level lifecycle: started, paused, etc.).
    pub async fn append_phase(&self, actor: Option<&str>, payload: &Value) {
        self.append_event(EVENT_PHASE, actor, payload).await;
    }

    /// Persist an `error` event.
    pub async fn append_error(&self, actor: Option<&str>, payload: &Value) {
        self.append_event(EVENT_ERROR, actor, payload).await;
    }

    async fn append_event(&self, event_type: &str, actor: Option<&str>, payload: &Value) {
        let payload_str = payload.to_string();
        let now_ms = unix_ms();
        let result = {
            let conn = self.db.lock().await;
            TeamEventStore::new(&conn).append(
                &self.run_id,
                event_type,
                actor,
                &payload_str,
                now_ms,
            )
        };
        if let Err(e) = result {
            tracing::warn!(
                run_id = %self.run_id,
                event_type,
                error = %e,
                "team event log append failed (in-memory state retained)"
            );
        }
    }

    /// Persist a `state` event AND update the workspace state cache atomically.
    /// Mirror of [`TeamWorkspace::set_state`].
    pub async fn put_state(&self, actor: Option<&str>, key: &str, value: &Value) {
        let value_str = value.to_string();
        let now_ms = unix_ms();
        let result = {
            let conn = self.db.lock().await;
            TeamEventStore::new(&conn).put_state(
                &self.run_id,
                key,
                &value_str,
                actor,
                now_ms,
            )
        };
        if let Err(e) = result {
            tracing::warn!(
                run_id = %self.run_id,
                key,
                error = %e,
                "team workspace state persistence failed (in-memory state retained)"
            );
        }
    }

    /// Update lifecycle status (running/paused/completed/failed/cancelled).
    pub async fn set_status(&self, status: &str, reason: Option<&str>) {
        let result = {
            let conn = self.db.lock().await;
            TeamEventStore::new(&conn).set_status(&self.run_id, status, reason)
        };
        if let Err(e) = result {
            tracing::warn!(
                run_id = %self.run_id,
                status,
                error = %e,
                "team run state status update failed"
            );
        }
    }

    /// Convenience: mark this run paused due to server restart.
    /// Used by the boot-time reconciler.
    pub async fn mark_paused_on_restart(&self) {
        self.set_status(RUN_PAUSED, Some("server_restart")).await;
    }

    /// Convenience: mark this run running (e.g. on resume).
    pub async fn mark_running(&self) {
        self.set_status(RUN_RUNNING, None).await;
    }
}

fn unix_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Boot-time reconciler: any `team_run_state` row marked `running` whose
/// last event is older than `stale_after_ms` is rewritten to `paused` with
/// `pause_reason = 'server_restart'`. Per ADR-002 §6 — operators / users
/// then explicitly resume via `POST /teams/runs/:run_id/resume`.
///
/// Returns the number of rows reconciled. Failures are logged and swallowed
/// so a SQLite hiccup at boot does not block the server starting.
pub async fn reconcile_orphans_on_boot(
    db: &Arc<Mutex<rusqlite::Connection>>,
    stale_after_ms: i64,
) {
    let now_ms = unix_ms();
    let result = {
        let conn = db.lock().await;
        nexus_store::team_events::TeamEventStore::new(&conn)
            .reconcile_orphans(stale_after_ms, now_ms)
    };
    match result {
        Ok(n) if n > 0 => tracing::info!(reconciled = n, "team runs marked paused on restart"),
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "team-event reconcile_orphans failed at boot"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    async fn open_db() -> Arc<Mutex<rusqlite::Connection>> {
        let dir = tempdir().unwrap();
        let conn = nexus_store::open_connection(&dir.path().join("t.db")).unwrap();
        // Leak the tempdir for the test's lifetime by Box::leak — simpler than
        // threading it through.
        let _ = Box::leak(Box::new(dir));
        Arc::new(Mutex::new(conn))
    }

    #[tokio::test]
    async fn sink_persists_messages() {
        let db = open_db().await;
        let sink = TeamEventSink::new(db.clone(), "run-x");
        sink.register_run("team-1", "proj-1", "tenant-A", 0).await.unwrap();

        sink.append_message(Some("leo"), &json!({"body": "hi"})).await;
        sink.append_message(Some("ivy"), &json!({"body": "hello"})).await;

        let conn = db.lock().await;
        let store = nexus_store::team_events::TeamEventStore::new(&conn);
        let evs = store.read_from("run-x", 0, 100).unwrap();
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[0].event_type, EVENT_MESSAGE);
    }

    #[tokio::test]
    async fn sink_state_round_trip() {
        let db = open_db().await;
        let sink = TeamEventSink::new(db.clone(), "run-y");
        sink.register_run("t", "p", "tn", 0).await.unwrap();

        sink.put_state(Some("ivy"), "draft.subject", &json!("Yoga launch")).await;

        let conn = db.lock().await;
        let store = nexus_store::team_events::TeamEventStore::new(&conn);
        let cached = store.get_state("run-y", "draft.subject").unwrap();
        assert_eq!(cached.as_deref(), Some(r#""Yoga launch""#));
    }

    /// End-to-end integration: insert a stale `running` row, run the boot
    /// reconciler, assert the row flipped to `paused` with the expected
    /// `pause_reason`. Mirrors the real `AppState::init` startup path so a
    /// regression in the reconciler is caught here, not at 3am in prod.
    #[tokio::test]
    async fn reconcile_orphans_on_boot_flips_stale_running_rows() {
        let db = open_db().await;

        // Insert a stale run: last event one hour ago.
        let now_ms = {
            use std::time::{SystemTime, UNIX_EPOCH};
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64
        };
        let stale_ms = now_ms - 3_600_000;
        {
            let conn = db.lock().await;
            let store = nexus_store::team_events::TeamEventStore::new(&conn);
            store
                .register_run("orphan-1", "team-x", "proj-x", "tenant-A", stale_ms)
                .unwrap();
            // last_event_at_ms is set by register_run to started_at_ms.
        }

        // Sanity: still marked running before reconcile.
        {
            let conn = db.lock().await;
            let store = nexus_store::team_events::TeamEventStore::new(&conn);
            let row = store.get_run("orphan-1").unwrap().unwrap();
            assert_eq!(row.status, "running");
        }

        // Boot reconciler: anything stale > 60_000ms gets paused.
        reconcile_orphans_on_boot(&db, 60_000).await;

        // Now paused with the right reason.
        let conn = db.lock().await;
        let store = nexus_store::team_events::TeamEventStore::new(&conn);
        let row = store.get_run("orphan-1").unwrap().unwrap();
        assert_eq!(row.status, "paused");
        assert_eq!(row.pause_reason.as_deref(), Some("server_restart"));
    }

    /// Negative case: a recently-active run should NOT be touched.
    #[tokio::test]
    async fn reconcile_orphans_on_boot_leaves_recent_runs_alone() {
        let db = open_db().await;
        let now_ms = {
            use std::time::{SystemTime, UNIX_EPOCH};
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64
        };
        {
            let conn = db.lock().await;
            let store = nexus_store::team_events::TeamEventStore::new(&conn);
            store
                .register_run("fresh-1", "team-x", "proj-x", "tenant-A", now_ms)
                .unwrap();
        }
        reconcile_orphans_on_boot(&db, 60_000).await;
        let conn = db.lock().await;
        let store = nexus_store::team_events::TeamEventStore::new(&conn);
        let row = store.get_run("fresh-1").unwrap().unwrap();
        assert_eq!(row.status, "running");
        assert!(row.pause_reason.is_none());
    }
}
