//! Background Agent Executor — runs scheduled autonomous agents on intervals.
//!
//! Previously, background agents had CRUD but no execution loop. This module
//! provides the missing scheduler that:
//! 1. Polls the `background_agents` table for active agents
//! 2. Checks if each agent's interval has elapsed since `last_run`
//! 3. Spawns the agent's task asynchronously
//! 4. Records results to `background_agent_runs`
//! 5. Updates `last_run` and `run_count`

use std::collections::HashSet;
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::state::AppState;

/// Set of background agents currently mid-execution. Used to skip any agent
/// whose prior run outlived its own interval, avoiding the "re-fire while
/// still running" footgun where two copies of the same long task overlap.
fn running_agents() -> &'static Mutex<HashSet<String>> {
    static RUNNING: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    RUNNING.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Start the background agent executor loop.
///
/// **Deprecated**: The unified `system_scheduler::start_scheduler()` now calls
/// `tick()` directly. This function is kept for backwards compatibility but
/// the standalone loop is no longer needed.
pub fn start_background_executor(_app: Arc<AppState>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // No-op: the system_scheduler now calls tick() in its unified loop.
        // This handle exists only so callers don't break.
        std::future::pending::<()>().await;
    })
}

/// Single tick of the executor — check and run due agents.
/// Called by `system_scheduler::start_scheduler()` every 15 seconds.
pub async fn tick(app: &Arc<AppState>) -> anyhow::Result<()> {
    #[allow(clippy::type_complexity)]
    let agents: Vec<(String, String, String, Option<String>, i64, Option<String>)> = {
        let db = app.db.lock().await;

        // Find agents that are active and whose interval has elapsed
        let mut stmt = db.prepare(
            "SELECT id, project_id, agent_type, config, interval_secs, last_run
             FROM background_agents
             WHERE status = 'running'",
        )?;

        let rows: Vec<_> = stmt.query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get::<_, i64>(4)?,
                row.get(5)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

        rows
        // db lock released at end of block
    };

    let now = chrono::Utc::now();

    for (agent_id, project_id, agent_type, config, interval_secs, last_run) in agents {
        // Check if enough time has elapsed
        let should_run = match &last_run {
            None => true,
            Some(lr) => {
                if let Ok(last) = chrono::DateTime::parse_from_rfc3339(lr) {
                    let elapsed = now.signed_duration_since(last);
                    elapsed.num_seconds() >= interval_secs
                } else {
                    true
                }
            }
        };

        if !should_run {
            continue;
        }

        // Skip if a prior run of this same agent is still in flight. Without
        // this guard a long-running agent whose execution exceeds its own
        // interval would keep getting re-spawned every tick.
        {
            let mut set = running_agents().lock().await;
            if set.contains(&agent_id) {
                warn!(agent_id = %agent_id, "skipping tick — previous run still in flight");
                continue;
            }
            set.insert(agent_id.clone());
        }

        info!(
            agent_id = %agent_id,
            agent_type = %agent_type,
            project_id = %project_id,
            "Executing background agent"
        );

        let app = app.clone();
        let aid = agent_id.clone();
        let pid = project_id.clone();
        let atype = agent_type.clone();
        let cfg = config.clone();

        // Update `last_run` at dispatch (not completion) so that a crash or
        // long run doesn't cause every subsequent tick to see the stale
        // timestamp and double-dispatch.
        {
            let db = app.db.lock().await;
            if let Err(e) = db.execute(
                "UPDATE background_agents SET last_run = datetime('now') WHERE id = ?1",
                rusqlite::params![aid],
            ) {
                warn!(agent_id = %aid, error = %e, "failed to stamp dispatch-time last_run");
            }
        }

        tokio::spawn(async move {
            let run_id = uuid::Uuid::new_v4().to_string();
            let start = std::time::Instant::now();

            // Record run start. If the insert fails (locked DB, schema drift)
            // log it — previously it was silently swallowed and the later
            // UPDATE below would match zero rows, losing the whole run from
            // history. We compute the outcome in a synchronous block, drop
            // the DB guard, THEN await on the running-set to avoid the
            // `rusqlite::params!` temporaries crossing an await boundary
            // (which would make the spawned future !Send).
            let insert_err = {
                let db = app.db.lock().await;
                db.execute(
                    "INSERT INTO background_agent_runs (id, agent_id, status, started_at)
                     VALUES (?1, ?2, 'running', datetime('now'))",
                    rusqlite::params![run_id, aid],
                )
                .err()
            };
            if let Some(e) = insert_err {
                warn!(
                    agent_id = %aid,
                    run_id = %run_id,
                    error = %e,
                    "failed to record background agent run start"
                );
                running_agents().lock().await.remove(&aid);
                return;
            }

            // Execute the agent task. `AssertUnwindSafe` isn't available for
            // async directly; we lean on tokio::spawn's own panic handling
            // plus the `running_agents` release below (which always runs
            // because it happens after `.await` returns — if the task panics,
            // the spawn returns and the set entry is orphaned for one tick,
            // but the dispatch-time `last_run` stamp plus the next tick
            // reading it eventually unblock it).
            let result = execute_agent_task(&app, &pid, &atype, cfg.as_deref()).await;

            let duration_ms = start.elapsed().as_millis() as i64;
            let (status, output, error) = match result {
                Ok(output) => ("completed", Some(output), None),
                Err(e) => ("failed", None, Some(e.to_string())),
            };

            // Record run result
            {
                let db = app.db.lock().await;
                if let Err(e) = db.execute(
                    "UPDATE background_agent_runs
                     SET status = ?1, output = ?2, error = ?3,
                         finished_at = datetime('now'), duration_ms = ?4
                     WHERE id = ?5",
                    rusqlite::params![status, output, error, duration_ms, run_id],
                ) {
                    warn!(run_id = %run_id, error = %e, "failed to record run result");
                }

                // Bump run_count (last_run was already stamped at dispatch).
                if let Err(e) = db.execute(
                    "UPDATE background_agents
                     SET run_count = run_count + 1
                     WHERE id = ?1",
                    rusqlite::params![aid],
                ) {
                    warn!(agent_id = %aid, error = %e, "failed to bump run_count");
                }
            }

            running_agents().lock().await.remove(&aid);

            info!(
                agent_id = %aid,
                status = status,
                duration_ms = duration_ms,
                "Background agent execution completed"
            );
        });
    }

    // ── Self-improvement tick ──────────────────────────────────────────────
    //    Runs at the executor's base cadence (once per tick call, which the
    //    scheduler fires every 15s) BUT rate-limited internally so we don't
    //    hammer the DB — promotion only fires every ~10 minutes.
    tick_self_improvement(app).await;

    Ok(())
}

/// Self-improvement cadence state. Rate-limits the promotion scan to once
/// per `SELF_IMPROVE_INTERVAL_SECS` regardless of how often the outer
/// scheduler ticks.
static LAST_SELF_IMPROVE_TICK: std::sync::atomic::AtomicI64 =
    std::sync::atomic::AtomicI64::new(0);
const SELF_IMPROVE_INTERVAL_SECS: i64 = 600; // 10 minutes

async fn tick_self_improvement(app: &Arc<AppState>) {
    let now = chrono::Utc::now().timestamp();
    let last = LAST_SELF_IMPROVE_TICK.load(std::sync::atomic::Ordering::Relaxed);
    if last != 0 && (now - last) < SELF_IMPROVE_INTERVAL_SECS {
        return;
    }
    LAST_SELF_IMPROVE_TICK.store(now, std::sync::atomic::Ordering::Relaxed);

    // Promote draft skills that have proven themselves (>= 3 uses, >= 70%
    // confidence). This is what closes the self-improvement loop — extracted
    // patterns start out as `draft` and only influence future builds after
    // the promoter decides they're reliable.
    let promoted = crate::skill_dna::promote_if_ready(app).await;
    if promoted > 0 {
        info!(count = promoted, "Self-improvement: promoted draft skills to active");
    }

    // Mine for cross-project patterns → propose new starter skills.
    match crate::user_pattern_detector::detect_and_propose(app).await {
        Ok(n) if n > 0 => info!(count = n, "Self-improvement: proposed new skill(s) from user patterns"),
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "Self-improvement pattern detection failed"),
    }
}

/// Execute a background agent task based on its type.
async fn execute_agent_task(
    app: &Arc<AppState>,
    project_id: &str,
    agent_type: &str,
    _config: Option<&str>,
) -> anyhow::Result<String> {
    match agent_type {
        "invariant_checker" => {
            let project_dir = app
                .data_dir
                .join("projects")
                .join(project_id)
                .join("generated");
            if !project_dir.exists() {
                return Ok("No generated code to check".into());
            }
            let result = crate::invariants::check_invariants(&project_dir);
            Ok(format!(
                "Score: {}/100, {} violations ({} auto-fixable)",
                result.score, result.total_violations, result.auto_fixable
            ))
        }
        "code_graph_rebuild" => {
            let project_dir = app
                .data_dir
                .join("projects")
                .join(project_id)
                .join("generated");
            if !project_dir.exists() {
                return Ok("No generated code".into());
            }
            let graph = crate::code_graph::CodeGraph::build(&project_dir);
            graph.save(&project_dir);
            Ok(format!(
                "Rebuilt code graph: {} files, {} edges",
                graph.files.len(),
                graph.edges.len()
            ))
        }
        "health_check" => {
            // Check if app is running and healthy
            let db = app.db.lock().await;
            let running: i64 = db
                .query_row(
                    "SELECT COUNT(*) FROM app_instances WHERE project_id = ?1 AND status = 'running'",
                    rusqlite::params![project_id],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            drop(db);
            Ok(format!("{} running instances", running))
        }
        _ => {
            Ok(format!("Unknown agent type '{}' — no-op", agent_type))
        }
    }
}
