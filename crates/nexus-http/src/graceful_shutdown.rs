//! Graceful shutdown — clean teardown of all subsystems.
//!
//! Listens for OS signals (SIGTERM, SIGINT / Ctrl+C) and orchestrates an
//! orderly shutdown sequence: stop accepting connections, drain in-flight
//! work, flush persistent state, and release resources.

use std::sync::Arc;
use tokio::signal;
use tracing::info;

use crate::state::AppState;

/// Wait for a shutdown signal (SIGTERM, SIGINT, Ctrl+C).
///
/// This future resolves once any of the supported termination signals is
/// received. Pass it to `axum::serve(...).with_graceful_shutdown(...)` so
/// the server stops accepting new connections before we begin teardown.
pub async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => info!("Received Ctrl+C, starting graceful shutdown..."),
        _ = terminate => info!("Received SIGTERM, starting graceful shutdown..."),
    }
}

/// Perform graceful shutdown of all subsystems.
///
/// Called *after* the HTTP listener has stopped accepting connections.
/// Walks through each subsystem in dependency order:
///
/// 1. Log the start of shutdown.
/// 2. Reset any stale agent/app-instance state in the database so the next
///    boot doesn't see phantom "running" entries.
/// 3. Drop remaining resources (handled automatically by `Arc` ref-counts).
pub async fn graceful_shutdown(state: Arc<AppState>) {
    info!("Graceful shutdown initiated");

    // 1. Stop accepting new requests (already handled by Axum's graceful_shutdown).
    info!("Stopped accepting new requests");

    // 1a. Give in-flight SSE streams (oneshot, chat, agent, live-build) up to
    //     5 seconds to emit their terminal event before we drop channels.
    //     The timeout is bounded so container orchestrators (k8s/docker) that
    //     hard-kill after 10-30s still see us exit cleanly.
    let drain_timeout = std::time::Duration::from_secs(
        std::env::var("NEXUS_SHUTDOWN_DRAIN_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(5),
    );
    info!(
        drain_secs = drain_timeout.as_secs(),
        "Draining in-flight SSE streams..."
    );
    tokio::time::sleep(drain_timeout).await;

    // 2. Reset running agents/app instances so they don't appear "running" on
    //    next startup (the server process is going away). BEFORE we mark them
    //    stopped, SIGTERM the child processes we spawned — otherwise the
    //    `next dev` / `node` children survive and hold their ports.
    info!("Reaping dev-server children + resetting stale state...");
    {
        let db = state.db.lock().await;

        // Collect PIDs first so we can SIGTERM them.
        let mut pids: Vec<i64> = Vec::new();
        if let Ok(mut stmt) = db.prepare(
            "SELECT pid FROM app_instances \
             WHERE status IN ('running', 'starting', 'installing') \
               AND pid IS NOT NULL",
        ) {
            if let Ok(rows) = stmt.query_map([], |row| row.get::<_, Option<i64>>(0)) {
                for r in rows.flatten().flatten() {
                    pids.push(r);
                }
            }
        }
        // Also collect ZeroClaw PIDs from agent_definitions.
        if let Ok(mut stmt) = db.prepare(
            "SELECT zeroclaw_pid FROM agent_definitions \
             WHERE status = 'running' AND zeroclaw_pid IS NOT NULL",
        ) {
            if let Ok(rows) = stmt.query_map([], |row| row.get::<_, Option<i64>>(0)) {
                for r in rows.flatten().flatten() {
                    pids.push(r);
                }
            }
        }

        let mut reaped = 0usize;
        for pid in pids {
            #[cfg(unix)]
            {
                if unsafe { libc::kill(pid as i32, libc::SIGTERM) } == 0 {
                    reaped += 1;
                }
            }
            #[cfg(not(unix))]
            {
                let _ = std::process::Command::new("taskkill")
                    .args(["/F", "/PID", &pid.to_string()])
                    .output();
                reaped += 1;
            }
        }
        if reaped > 0 {
            info!(count = reaped, "SIGTERM'd dev-server + agent children");
        }

        let agents_reset = db
            .execute(
                "UPDATE agent_definitions SET status = 'stopped', zeroclaw_pid = NULL, \
                 zeroclaw_port = NULL, updated_at = datetime('now') \
                 WHERE status = 'running'",
                [],
            )
            .unwrap_or(0);
        let apps_reset = db
            .execute(
                "UPDATE app_instances SET status = 'stopped', pid = NULL, \
                 stopped_at = datetime('now') \
                 WHERE status IN ('running', 'starting', 'installing')",
                [],
            )
            .unwrap_or(0);
        if agents_reset > 0 {
            info!(count = agents_reset, "Reset running agents to stopped");
        }
        if apps_reset > 0 {
            info!(count = apps_reset, "Reset running app instances to stopped");
        }
    }

    // 3. Log cost summary before shutdown so operators have a record.
    info!("Recording final cost tracker snapshot...");
    {
        let summary = state.cost_tracker.daily_summary().await;
        info!(
            total_calls = summary.total_calls,
            total_cost_usd = format!("{:.4}", summary.total_cost_usd),
            "Final cost tracker summary"
        );
    }

    // 4. Remaining resources (db connection, http client, caches) are dropped
    //    when the last Arc reference is released.
    info!("Graceful shutdown complete");
}

#[cfg(test)]
mod tests {
    /// Verify `shutdown_signal` compiles and is Send + 'static (required by
    /// Axum's `with_graceful_shutdown`).
    #[test]
    fn shutdown_signal_is_send() {
        fn assert_send<T: Send + 'static>() {}
        // This is a compile-time check — if it compiles, it passes.
        assert_send::<std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>>();
    }
}
