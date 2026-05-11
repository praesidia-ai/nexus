//! Agent TV persistence sink — every live build event that flows
//! through `build_event_bus` is also appended to the
//! `agent_tv_events` table, so any run becomes a replayable,
//! embeddable artifact (NEXUS_MASTER_PLAN §4).
//!
//! # Usage
//!
//! ```rust,ignore
//! let run_id = agent_tv_sink::start_run(
//!     &app, project_id, "oneshot", Some(prompt.clone())
//! ).await?;
//!
//! agent_tv_sink::record_event(
//!     &app,
//!     &run_id,
//!     "agent_action",
//!     Some("nova"),
//!     None, // mini_kind
//!     &json!({"agent": "nova", "action": "planning"}),
//! ).await;
//!
//! // …later…
//! agent_tv_sink::complete_run(&app, &run_id, "completed",
//!     tokens_used, cost_usd, duration_ms).await?;
//! ```
//!
//! Failures inside the sink are logged at WARN but never propagated —
//! persistence is best-effort and must never break the live stream.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use serde_json::Value;
use tokio::sync::Mutex;
use tracing::warn;
use uuid::Uuid;

use crate::state::AppState;

/// Start a new replayable run. Returns the generated `run_id` which
/// the caller should use as the SSE stream identifier so replays and
/// the live stream stay joinable.
pub async fn start_run(
    app: &Arc<AppState>,
    project_id: &str,
    run_kind: &str,
    prompt: Option<String>,
) -> Result<String, rusqlite::Error> {
    let run_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let db = app.db.lock().await;
    db.execute(
        "INSERT INTO agent_tv_runs
            (id, project_id, run_kind, prompt, status, started_at)
         VALUES (?1, ?2, ?3, ?4, 'running', ?5)",
        rusqlite::params![&run_id, project_id, run_kind, prompt.as_deref(), now],
    )?;
    Ok(run_id)
}

/// Monotonic seq-counter state per `run_id`. Kept in-process so two
/// concurrent writers emitting into the same run see a stable order
/// even when SQLite's per-row timestamps collide. Implemented as a
/// `HashMap<String, i64>` under a single async-`Mutex` because runs
/// are rarely concurrent (usually ≤ 4 active) and this avoids pulling
/// in `dashmap` as a new workspace dep.
fn counters() -> &'static Mutex<HashMap<String, i64>> {
    static COUNTERS: OnceLock<Mutex<HashMap<String, i64>>> = OnceLock::new();
    COUNTERS.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn next_seq_for(run_id: &str) -> i64 {
    let mut m = counters().lock().await;
    let slot = m.entry(run_id.to_string()).or_insert(0);
    let n = *slot;
    *slot += 1;
    n
}

async fn drop_counter_for(run_id: &str) {
    let mut m = counters().lock().await;
    m.remove(run_id);
}

/// Append one event. Any SQL failure is logged at WARN and swallowed.
pub async fn record_event(
    app: &Arc<AppState>,
    run_id: &str,
    event_type: &str,
    conductor: Option<&str>,
    mini_kind: Option<&str>,
    payload: &Value,
) {
    let seq = next_seq_for(run_id).await;
    let now = chrono::Utc::now().to_rfc3339();
    let payload_str = serde_json::to_string(payload).unwrap_or_else(|_| "{}".to_string());

    let db = app.db.lock().await;
    if let Err(e) = db.execute(
        "INSERT INTO agent_tv_events
            (run_id, seq, event_type, conductor, mini_kind, payload, ts)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![run_id, seq, event_type, conductor, mini_kind, &payload_str, &now],
    ) {
        warn!(run_id = %run_id, error = %e, "failed to persist agent-tv event");
    }
}

/// Close out a run. `status` typically `"completed"` / `"failed"` /
/// `"cancelled"`. Totals are denormalised onto `agent_tv_runs` so the
/// gallery never has to aggregate `agent_tv_events`.
///
/// Automatically issues a Trust Certificate v1 (see `trust.rs`): the
/// run's ordered events are canonicalised into a Merkle log, the root
/// is Ed25519-signed with this deployment's persistent key, and
/// `merkle_root`+`signature` are stamped onto the `agent_tv_runs`
/// row. Certificate issuance is best-effort — a failure logs at WARN
/// and does not abort the complete-run write. Callers that want the
/// full cert back can GET `/trust/cert/:runId`.
pub async fn complete_run(
    app: &Arc<AppState>,
    run_id: &str,
    status: &str,
    total_tokens: i64,
    total_cost_usd: f64,
    duration_ms: i64,
) -> Result<(), rusqlite::Error> {
    let now = chrono::Utc::now().to_rfc3339();
    let total_events = next_seq_for(run_id).await;

    // Build + sign the Trust Certificate *before* taking the DB lock
    // so the UPDATE carries merkle_root + signature in one shot. A
    // later GET /trust/cert/:id call will re-derive the same root.
    let (merkle_root, signature) = match issue_certificate(app, run_id).await {
        Ok(Some((r, s))) => (Some(r), Some(s)),
        Ok(None) => (None, None),
        Err(e) => {
            warn!(run_id = %run_id, error = %e, "trust cert issuance failed; continuing");
            (None, None)
        }
    };

    let db = app.db.lock().await;
    db.execute(
        "UPDATE agent_tv_runs
         SET status = ?1,
             completed_at = ?2,
             total_events = ?3,
             total_tokens = ?4,
             total_cost_usd = ?5,
             duration_ms = ?6,
             merkle_root = COALESCE(?8, merkle_root),
             signature = COALESCE(?9, signature)
         WHERE id = ?7",
        rusqlite::params![
            status,
            &now,
            total_events,
            total_tokens,
            total_cost_usd,
            duration_ms,
            run_id,
            merkle_root.as_deref(),
            signature.as_deref()
        ],
    )?;
    // Drop the per-run seq counter — long-running servers would
    // otherwise accumulate one per run forever.
    drop(db);
    drop_counter_for(run_id).await;
    Ok(())
}

/// Pull every persisted event for `run_id`, canonicalise it, and
/// return the `(merkle_root, signature)` pair for stamping onto the
/// `agent_tv_runs` row. Returns `Ok(None)` if the run has no events
/// yet (a degenerate case — still safe because the caller leaves the
/// existing cert unchanged via `COALESCE`).
async fn issue_certificate(
    app: &Arc<AppState>,
    run_id: &str,
) -> anyhow::Result<Option<(String, String)>> {
    let mut builder = crate::trust::TrustLogBuilder::new(run_id);
    let mut had_any = false;
    {
        let db = app.db.lock().await;
        let mut stmt = db.prepare(
            "SELECT event_type, payload, ts
             FROM agent_tv_events
             WHERE run_id = ?1 ORDER BY seq ASC",
        )?;
        let rows = stmt.query_map([run_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        for r in rows.flatten() {
            had_any = true;
            let payload: serde_json::Value =
                serde_json::from_str(&r.1).unwrap_or(serde_json::Value::Null);
            builder.append(r.0, r.2, &payload);
        }
    }
    if !had_any {
        return Ok(None);
    }
    let signer = crate::trust::TrustSigner::load_or_generate(&app.data_dir)?;
    let cert = builder.finalize(&signer);
    Ok(Some((cert.merkle_root, cert.signature)))
}

/// Flip visibility on an existing run. Used by the "Publish to
/// gallery" UI affordance — the replay stays the same, only the
/// gallery-listing bit changes.
pub async fn set_visibility(
    app: &Arc<AppState>,
    run_id: &str,
    visibility: &str,
) -> Result<(), rusqlite::Error> {
    debug_assert!(matches!(visibility, "private" | "unlisted" | "public"));
    let db = app.db.lock().await;
    db.execute(
        "UPDATE agent_tv_runs SET visibility = ?1 WHERE id = ?2",
        rusqlite::params![visibility, run_id],
    )?;
    Ok(())
}
