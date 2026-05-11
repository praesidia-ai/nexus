//! Durable team-event log — the persistence substrate for
//! `nexus_http::team_orchestrator`.
//!
//! Per ADR-002. Replaces the in-memory `Arc<RwLock<Vec<...>>>` state at
//! `crates/nexus-http/src/team_orchestrator.rs:165-167,319-325` with an
//! event-sourced log so a server restart resumes in-flight team runs from
//! the last persisted event instead of dropping every conversation.
//!
//! The event log is the source of truth; `team_workspace_state` is a cache
//! that is rebuilt deterministically from the events on resume.
//!
//! Schema lives in migration `007_team_events.sql`.

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{Result, StoreError};

/// Discriminator stored in `team_events.event_type`.
pub const EVENT_MESSAGE: &str = "message";
pub const EVENT_TASK: &str = "task";
pub const EVENT_ARTIFACT: &str = "artifact";
pub const EVENT_STATE: &str = "state";
pub const EVENT_PHASE: &str = "phase";
pub const EVENT_ERROR: &str = "error";

/// Run lifecycle status, mirrors `team_run_state.status`.
pub const RUN_RUNNING: &str = "running";
pub const RUN_PAUSED: &str = "paused";
pub const RUN_COMPLETED: &str = "completed";
pub const RUN_FAILED: &str = "failed";
pub const RUN_CANCELLED: &str = "cancelled";

/// One row from `team_events`.
#[derive(Debug, Clone)]
pub struct TeamEvent {
    pub run_id: String,
    pub seq: i64,
    pub occurred_at_ms: i64,
    pub event_type: String,
    pub actor: Option<String>,
    pub payload_json: String,
}

/// Lifecycle row from `team_run_state`.
#[derive(Debug, Clone)]
pub struct TeamRunState {
    pub run_id: String,
    pub team_id: String,
    pub project_id: String,
    pub tenant_id: String,
    pub status: String,
    pub started_at_ms: i64,
    pub last_event_seq: i64,
    pub last_event_at_ms: i64,
    pub pause_reason: Option<String>,
    pub error: Option<String>,
}

/// Service for appending and reading team events.
///
/// The mutex on the underlying `Connection` is the caller's responsibility
/// (per the workspace's CLAUDE.md invariant: never hold the SQLite lock
/// across an `.await`). Every method here is synchronous and uses the
/// `Connection` for a single bounded operation.
pub struct TeamEventStore<'a> {
    conn: &'a Connection,
}

impl<'a> TeamEventStore<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Register a new team run. Idempotent on `run_id`.
    pub fn register_run(
        &self,
        run_id: &str,
        team_id: &str,
        project_id: &str,
        tenant_id: &str,
        started_at_ms: i64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO team_run_state \
             (run_id, team_id, project_id, tenant_id, status, started_at_ms, \
              last_event_seq, last_event_at_ms) \
             VALUES (?1, ?2, ?3, ?4, 'running', ?5, 0, ?5)",
            params![run_id, team_id, project_id, tenant_id, started_at_ms],
        )?;
        Ok(())
    }

    /// Append one event to the log; returns the assigned monotonic `seq`.
    ///
    /// Runs in a single transaction together with the lifecycle update so
    /// `last_event_seq` is never out of sync with the latest row.
    pub fn append(
        &self,
        run_id: &str,
        event_type: &str,
        actor: Option<&str>,
        payload_json: &str,
        occurred_at_ms: i64,
    ) -> Result<i64> {
        let tx = self.conn.unchecked_transaction()?;
        let seq: i64 = tx
            .query_row(
                "SELECT COALESCE(last_event_seq, 0) + 1 FROM team_run_state WHERE run_id = ?1",
                params![run_id],
                |row| row.get(0),
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => StoreError::Msg(format!(
                    "team_run_state missing for run_id={run_id}; call register_run first"
                )),
                other => StoreError::from(other),
            })?;

        tx.execute(
            "INSERT INTO team_events \
             (run_id, seq, occurred_at_ms, event_type, actor, payload_json) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![run_id, seq, occurred_at_ms, event_type, actor, payload_json],
        )?;
        tx.execute(
            "UPDATE team_run_state \
             SET last_event_seq = ?2, last_event_at_ms = ?3 \
             WHERE run_id = ?1",
            params![run_id, seq, occurred_at_ms],
        )?;
        tx.commit()?;
        Ok(seq)
    }

    /// Append a `state` event AND update the workspace cache atomically.
    /// Returns the assigned `seq`.
    pub fn put_state(
        &self,
        run_id: &str,
        key: &str,
        value_json: &str,
        actor: Option<&str>,
        occurred_at_ms: i64,
    ) -> Result<i64> {
        let payload = format!(
            r#"{{"type":"state","key":{},"value":{}}}"#,
            serde_json_string(key),
            value_json
        );
        let tx = self.conn.unchecked_transaction()?;
        let seq: i64 = tx.query_row(
            "SELECT COALESCE(last_event_seq, 0) + 1 FROM team_run_state WHERE run_id = ?1",
            params![run_id],
            |row| row.get(0),
        )?;
        tx.execute(
            "INSERT INTO team_events \
             (run_id, seq, occurred_at_ms, event_type, actor, payload_json) \
             VALUES (?1, ?2, ?3, 'state', ?4, ?5)",
            params![run_id, seq, occurred_at_ms, actor, payload],
        )?;
        tx.execute(
            "UPDATE team_run_state \
             SET last_event_seq = ?2, last_event_at_ms = ?3 \
             WHERE run_id = ?1",
            params![run_id, seq, occurred_at_ms],
        )?;
        tx.execute(
            "INSERT INTO team_workspace_state (run_id, key, value_json, updated_seq, updated_at_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5) \
             ON CONFLICT (run_id, key) DO UPDATE SET \
                value_json = excluded.value_json, \
                updated_seq = excluded.updated_seq, \
                updated_at_ms = excluded.updated_at_ms",
            params![run_id, key, value_json, seq, occurred_at_ms],
        )?;
        tx.commit()?;
        Ok(seq)
    }

    /// Read events newer than `after_seq`, capped at `limit`.
    pub fn read_from(&self, run_id: &str, after_seq: i64, limit: usize) -> Result<Vec<TeamEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT run_id, seq, occurred_at_ms, event_type, actor, payload_json \
             FROM team_events \
             WHERE run_id = ?1 AND seq > ?2 \
             ORDER BY seq ASC \
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![run_id, after_seq, limit as i64], |row| {
            Ok(TeamEvent {
                run_id: row.get(0)?,
                seq: row.get(1)?,
                occurred_at_ms: row.get(2)?,
                event_type: row.get(3)?,
                actor: row.get(4)?,
                payload_json: row.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Last applied seq for a run, or 0 if no events yet.
    pub fn last_seq(&self, run_id: &str) -> Result<i64> {
        let v: Option<i64> = self
            .conn
            .query_row(
                "SELECT last_event_seq FROM team_run_state WHERE run_id = ?1",
                params![run_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(v.unwrap_or(0))
    }

    /// Read the materialised value for a workspace key.
    pub fn get_state(&self, run_id: &str, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT value_json FROM team_workspace_state WHERE run_id = ?1 AND key = ?2",
                params![run_id, key],
                |row| row.get::<_, String>(0),
            )
            .optional()?)
    }

    /// Update lifecycle status. Used for pause/resume/complete/cancel.
    pub fn set_status(&self, run_id: &str, status: &str, reason: Option<&str>) -> Result<()> {
        self.conn.execute(
            "UPDATE team_run_state SET status = ?2, pause_reason = ?3 WHERE run_id = ?1",
            params![run_id, status, reason],
        )?;
        Ok(())
    }

    /// Read lifecycle row.
    pub fn get_run(&self, run_id: &str) -> Result<Option<TeamRunState>> {
        Ok(self
            .conn
            .query_row(
                "SELECT run_id, team_id, project_id, tenant_id, status, started_at_ms, \
                        last_event_seq, last_event_at_ms, pause_reason, error \
                 FROM team_run_state WHERE run_id = ?1",
                params![run_id],
                |row| {
                    Ok(TeamRunState {
                        run_id: row.get(0)?,
                        team_id: row.get(1)?,
                        project_id: row.get(2)?,
                        tenant_id: row.get(3)?,
                        status: row.get(4)?,
                        started_at_ms: row.get(5)?,
                        last_event_seq: row.get(6)?,
                        last_event_at_ms: row.get(7)?,
                        pause_reason: row.get(8)?,
                        error: row.get(9)?,
                    })
                },
            )
            .optional()?)
    }

    /// On boot, mark any `running` run that hasn't recorded an event in
    /// `stale_after_ms` as `paused` with reason `server_restart`. The
    /// orchestrator can decide whether to auto-resume them based on policy.
    pub fn reconcile_orphans(&self, stale_after_ms: i64, now_ms: i64) -> Result<usize> {
        let rows = self.conn.execute(
            "UPDATE team_run_state \
             SET status = 'paused', pause_reason = 'server_restart' \
             WHERE status = 'running' AND last_event_at_ms < ?1",
            params![now_ms - stale_after_ms],
        )?;
        Ok(rows)
    }
}

/// Minimal-allocation JSON-string escaper for `key` field embedding.
/// We intentionally avoid pulling `serde_json` here for one usage; keys are
/// simple ASCII identifiers in practice.
fn serde_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_connection;
    use tempfile::tempdir;

    #[test]
    fn append_and_read_round_trip() {
        let dir = tempdir().unwrap();
        let conn = open_connection(&dir.path().join("t.db")).unwrap();
        let svc = TeamEventStore::new(&conn);

        svc.register_run("run1", "team1", "proj1", "tenantA", 100).unwrap();
        let s1 = svc.append("run1", EVENT_MESSAGE, Some("leo"), r#"{"body":"hi"}"#, 101).unwrap();
        let s2 = svc.append("run1", EVENT_TASK, Some("leo"), r#"{"task":"draft"}"#, 102).unwrap();
        assert_eq!(s1, 1);
        assert_eq!(s2, 2);

        let evs = svc.read_from("run1", 0, 100).unwrap();
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[0].event_type, EVENT_MESSAGE);
        assert_eq!(evs[1].event_type, EVENT_TASK);

        let last = svc.last_seq("run1").unwrap();
        assert_eq!(last, 2);
    }

    #[test]
    fn put_state_updates_cache_atomically() {
        let dir = tempdir().unwrap();
        let conn = open_connection(&dir.path().join("s.db")).unwrap();
        let svc = TeamEventStore::new(&conn);
        svc.register_run("run2", "team", "proj", "tenant", 0).unwrap();
        svc.put_state("run2", "draft.subject", r#""Yoga launch""#, Some("ivy"), 5).unwrap();

        let cached = svc.get_state("run2", "draft.subject").unwrap();
        assert_eq!(cached.as_deref(), Some(r#""Yoga launch""#));

        // The state event is in the log too.
        let evs = svc.read_from("run2", 0, 100).unwrap();
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].event_type, EVENT_STATE);
    }

    #[test]
    fn reconcile_orphans_marks_stale_running_as_paused() {
        let dir = tempdir().unwrap();
        let conn = open_connection(&dir.path().join("r.db")).unwrap();
        let svc = TeamEventStore::new(&conn);
        svc.register_run("orphan", "t", "p", "tn", 1).unwrap();
        svc.append("orphan", EVENT_MESSAGE, None, "{}", 1).unwrap();

        let updated = svc.reconcile_orphans(100, 1_000_000).unwrap();
        assert_eq!(updated, 1);
        let row = svc.get_run("orphan").unwrap().unwrap();
        assert_eq!(row.status, RUN_PAUSED);
        assert_eq!(row.pause_reason.as_deref(), Some("server_restart"));
    }
}
