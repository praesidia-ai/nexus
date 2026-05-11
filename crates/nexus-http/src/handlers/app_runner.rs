//! App runner HTTP handlers — start, stop, and monitor generated apps.
//! Supports both bare-metal (npm run dev) and Docker sandbox mode.
//! Includes SSE log streaming, build step tracking, health checks,
//! auto-restart with exponential backoff, and environment variable management.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use tracing::{error, info, warn};

use nexus_store::{
    app_runner::{
        check_docker_available, docker_build, docker_logs, docker_run, docker_stop, npm_install,
        spawn_dev_server, stop_process,
    },
    AppRunnerService, ProjectService,
};

use crate::{
    error::{ApiError, ApiResult},
    security::{auth::Scope, project_access::ProjectAccess},
    state::AppState,
};

// ---------------------------------------------------------------------------
// Health check helper
// ---------------------------------------------------------------------------

async fn check_app_health(port: u16) -> bool {
    let url = format!("http://127.0.0.1:{}", port);
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "check_app_health: failed to build client");
            return false;
        }
    };
    match client.get(&url).send().await {
        Ok(resp) => resp.status().is_success() || resp.status().as_u16() == 404,
        Err(_) => false,
    }
}

// ---------------------------------------------------------------------------
// Watchdog — monitors process health and auto-restarts on crash
// ---------------------------------------------------------------------------

fn spawn_watchdog(app: Arc<AppState>, project_id: String, instance_id: String, port: u16) {
    tokio::spawn(async move {
        // Wait for the app to be running first
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;

        let mut consecutive_failures = 0u32;
        let max_restarts = 5;

        loop {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;

            // Check if instance is still supposed to be running
            let (status, auto_restart, restart_count) = {
                let db = app.db.lock().await;
                let svc = AppRunnerService::new(&db);
                match svc.get_running_instance(&project_id).ok().flatten() {
                    Some(inst) if inst.id == instance_id => {
                        (inst.status, inst.auto_restart, inst.restart_count)
                    }
                    _ => break, // Instance no longer active
                }
            };

            if status != "running" {
                break;
            }

            // Health check
            let healthy = check_app_health(port).await;

            {
                let db = app.db.lock().await;
                let svc = AppRunnerService::new(&db);
                let _ = svc.update_health_status(
                    &instance_id,
                    if healthy { "healthy" } else { "unhealthy" },
                );
            }

            if healthy {
                consecutive_failures = 0;
                continue;
            }

            consecutive_failures += 1;

            if consecutive_failures >= 3 && auto_restart && restart_count < max_restarts {
                warn!(
                    instance_id = %instance_id,
                    failures = consecutive_failures,
                    "App unhealthy, attempting auto-restart"
                );

                // Exponential backoff: 10s, 20s, 40s, 80s, 160s
                let backoff =
                    std::time::Duration::from_secs(10 * (1u64 << restart_count.min(4) as u64));
                tokio::time::sleep(backoff).await;

                let db = app.db.lock().await;
                let svc = AppRunnerService::new(&db);
                let new_count = svc.increment_restart_count(&instance_id).unwrap_or(0);
                let _ = svc.update_status(
                    &instance_id,
                    "crashed",
                    None,
                    Some("Process became unresponsive"),
                );
                let _ = svc.append_logs(
                    &instance_id,
                    &format!(
                        "[nexus] Auto-restart #{} triggered (backoff: {}s)",
                        new_count,
                        backoff.as_secs()
                    ),
                );
                break;
            }

            if consecutive_failures >= 3 && (!auto_restart || restart_count >= max_restarts) {
                let error_log = {
                    let db = app.db.lock().await;
                    let svc = AppRunnerService::new(&db);
                    let _ = svc.update_status(
                        &instance_id,
                        "crashed",
                        None,
                        Some("Process unresponsive, auto-restart exhausted"),
                    );
                    let _ = svc.append_logs(
                        &instance_id,
                        "[nexus] App crashed and auto-restart limit reached. Triggering self-healing...",
                    );
                    svc.get_running_instance(&project_id)
                        .ok()
                        .flatten()
                        .map(|i| i.logs)
                        .unwrap_or_default()
                };

                let heal_app = Arc::clone(&app);
                let heal_project = project_id.clone();
                let heal_instance = instance_id.clone();
                tokio::spawn(async move {
                    crate::self_healing::attempt_heal(
                        heal_app,
                        heal_project,
                        heal_instance,
                        error_log,
                    )
                    .await;
                });
                break;
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Write .env.local from DB env vars
// ---------------------------------------------------------------------------

fn write_env_file(
    _app: &AppState,
    project_id: &str,
    output_dir: &std::path::Path,
    db: &rusqlite::Connection,
) -> String {
    let svc = AppRunnerService::new(db);
    let env_vars = svc.list_env_vars(project_id).unwrap_or_default();
    if env_vars.is_empty() {
        return String::new();
    }

    let mut content = String::new();
    for var in &env_vars {
        content.push_str(&format!("{}={}\n", var.key, var.value));
    }

    let env_path = output_dir.join(".env.local");
    if let Err(e) = std::fs::write(&env_path, &content) {
        return format!("[nexus] Warning: failed to write .env.local: {}\n", e);
    }
    // Tighten permissions to owner-only — .env.local contains secrets and
    // must not be world- or group-readable.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(&env_path, std::fs::Permissions::from_mode(0o600))
        {
            return format!(
                "[nexus] Warning: wrote .env.local but failed to chmod 600: {}\n",
                e
            );
        }
    }
    format!(
        "[nexus] Wrote {} environment variable(s) to .env.local (mode 0600)\n",
        env_vars.len()
    )
}

// ---------------------------------------------------------------------------
// POST /projects/:id/app/start
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct StartAppReq {
    /// When true, run the app inside a Docker container with restricted capabilities.
    #[serde(default)]
    pub sandbox: Option<bool>,
}

pub async fn start_app(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    body: Option<Json<StartAppReq>>,
) -> ApiResult<Json<serde_json::Value>> {
    access
        .require_scope(&Scope::RuntimeManage)
        .map_err(|_| ApiError::Forbidden("runtime:manage scope required to start apps".into()))?;
    let project_id = access.project_id.clone();
    let output_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");

    // Check that generated code exists.
    if !output_dir.exists() {
        return Err(ApiError::BadRequest(
            "No generated code found. Run codegen first.".to_string(),
        ));
    }

    // Determine sandbox mode: explicit request > settings default > false
    let sandbox_requested = body.as_ref().and_then(|b| b.sandbox);
    let sandbox = if let Some(explicit) = sandbox_requested {
        explicit
    } else {
        // Check settings for default
        let db = app.db.lock().await;
        let svc = ProjectService::new(&db);
        svc.get_setting("sandbox.enabled")
            .unwrap_or(None)
            .map(|v| v == "true")
            .unwrap_or(false)
    };

    // If sandbox requested, verify Docker is available early
    if sandbox {
        if let Err(e) = check_docker_available() {
            return Err(ApiError::BadRequest(format!(
                "Sandbox mode requires Docker: {}",
                e
            )));
        }
    }

    // Check if already running.
    let (instance_id, port) = {
        let db = app.db.lock().await;
        let svc = AppRunnerService::new(&db);

        if let Some(running) = svc.get_running_instance(&project_id)? {
            return Err(ApiError::BadRequest(format!(
                "App is already {} (instance {})",
                running.status, running.id
            )));
        }

        let port = svc.next_available_port()?;
        let instance = svc.create_instance(
            &project_id,
            port,
            output_dir.to_str().unwrap_or_default(),
            sandbox,
        )?;
        (instance.id, instance.port)
    };

    // Spawn background task for install + start.
    let bg_app = Arc::clone(&app);
    let bg_instance_id = instance_id.clone();
    let bg_output_dir = output_dir.clone();
    let bg_port = port;
    let bg_project_id = project_id.clone();

    if sandbox {
        // ── Docker sandbox path ──────────────────────────────────────
        tokio::spawn(async move {
            let image_tag = format!("nexus-app-{}", bg_project_id);
            let container_name = format!(
                "nexus-app-{}-{}",
                bg_project_id,
                bg_instance_id.get(..8).unwrap_or(&bg_instance_id)
            );

            // Create build steps
            let (env_step_id, build_step_id, run_step_id, health_step_id) = {
                let db = bg_app.db.lock().await;
                let svc = AppRunnerService::new(&db);
                let s1 = svc
                    .create_build_step(&bg_instance_id, "env_setup", "Setting up environment")
                    .ok()
                    .map(|s| s.id)
                    .unwrap_or_default();
                let s2 = svc
                    .create_build_step(&bg_instance_id, "docker_build", "Building Docker image")
                    .ok()
                    .map(|s| s.id)
                    .unwrap_or_default();
                let s3 = svc
                    .create_build_step(&bg_instance_id, "docker_run", "Starting container")
                    .ok()
                    .map(|s| s.id)
                    .unwrap_or_default();
                let s4 = svc
                    .create_build_step(&bg_instance_id, "health_check", "Checking health")
                    .ok()
                    .map(|s| s.id)
                    .unwrap_or_default();
                (s1, s2, s3, s4)
            };

            // Step 0: env setup
            {
                let db = bg_app.db.lock().await;
                let svc = AppRunnerService::new(&db);
                let _ = svc.update_build_step(&env_step_id, "running", None, None);
                let env_log = write_env_file(&bg_app, &bg_project_id, &bg_output_dir, &db);
                let _ = svc.append_logs(&bg_instance_id, &env_log);
                let _ = svc.complete_build_step(&env_step_id, true, Some(&env_log), None);
            }

            // Step 1: docker build (returns ProjectType)
            {
                let db = bg_app.db.lock().await;
                let svc = AppRunnerService::new(&db);
                let _ = svc.update_build_step(&build_step_id, "running", None, None);
                let _ = svc.append_logs(&bg_instance_id, "[nexus] Building Docker image...");
            }

            let project_type = match tokio::task::spawn_blocking({
                let dir = bg_output_dir.clone();
                let tag = image_tag.clone();
                move || docker_build(&dir, &tag)
            })
            .await
            {
                Ok(Ok(pt)) => {
                    info!(instance_id = %bg_instance_id, project_type = ?pt, "Docker image built");
                    let db = bg_app.db.lock().await;
                    let svc = AppRunnerService::new(&db);
                    let _ = svc.complete_build_step(
                        &build_step_id,
                        true,
                        Some("Docker image built successfully"),
                        None,
                    );
                    pt
                }
                Ok(Err(e)) => {
                    error!(instance_id = %bg_instance_id, error = %e, "Docker build failed");
                    let db = bg_app.db.lock().await;
                    let svc = AppRunnerService::new(&db);
                    let _ = svc.complete_build_step(&build_step_id, false, None, Some(&e));
                    let _ = svc.update_status(&bg_instance_id, "failed", None, Some(&e));
                    return;
                }
                Err(e) => {
                    error!(instance_id = %bg_instance_id, error = %e, "Docker build task panicked");
                    let db = bg_app.db.lock().await;
                    let svc = AppRunnerService::new(&db);
                    let _ =
                        svc.complete_build_step(&build_step_id, false, None, Some(&e.to_string()));
                    let _ =
                        svc.update_status(&bg_instance_id, "failed", None, Some(&e.to_string()));
                    return;
                }
            };

            // Update status to "starting"
            {
                let db = bg_app.db.lock().await;
                let svc = AppRunnerService::new(&db);
                let _ = svc.update_status(&bg_instance_id, "starting", None, None);
                let _ = svc.update_build_step(&run_step_id, "running", None, None);
            }

            // Step 2: docker run
            match tokio::task::spawn_blocking({
                let tag = image_tag.clone();
                let name = container_name.clone();
                move || docker_run(&tag, bg_port, &name, project_type)
            })
            .await
            {
                Ok(Ok(container_id)) => {
                    info!(
                        instance_id = %bg_instance_id,
                        container_id = %container_id,
                        port = bg_port,
                        "Sandbox container started"
                    );
                    let db = bg_app.db.lock().await;
                    let svc = AppRunnerService::new(&db);
                    let _ = svc.set_container_id(&bg_instance_id, &container_id);
                    let _ = svc.complete_build_step(
                        &run_step_id,
                        true,
                        Some("Container started"),
                        None,
                    );
                }
                Ok(Err(e)) => {
                    error!(instance_id = %bg_instance_id, error = %e, "Docker run failed");
                    let db = bg_app.db.lock().await;
                    let svc = AppRunnerService::new(&db);
                    let _ = svc.complete_build_step(&run_step_id, false, None, Some(&e));
                    let _ = svc.update_status(&bg_instance_id, "failed", None, Some(&e));
                    return;
                }
                Err(e) => {
                    error!(instance_id = %bg_instance_id, error = %e, "Docker run task panicked");
                    let db = bg_app.db.lock().await;
                    let svc = AppRunnerService::new(&db);
                    let _ =
                        svc.complete_build_step(&run_step_id, false, None, Some(&e.to_string()));
                    let _ =
                        svc.update_status(&bg_instance_id, "failed", None, Some(&e.to_string()));
                    return;
                }
            }

            // Step 3: health check
            {
                let db = bg_app.db.lock().await;
                let svc = AppRunnerService::new(&db);
                let _ = svc.update_build_step(&health_step_id, "running", None, None);
                let _ = svc.append_logs(
                    &bg_instance_id,
                    "[nexus] Waiting for app to become healthy...",
                );
            }

            let mut healthy = false;
            for attempt in 1..=30 {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                if check_app_health(bg_port).await {
                    healthy = true;
                    let db = bg_app.db.lock().await;
                    let svc = AppRunnerService::new(&db);
                    let _ = svc.append_logs(
                        &bg_instance_id,
                        &format!("[nexus] Health check passed on attempt {}", attempt),
                    );
                    break;
                }
            }

            {
                let db = bg_app.db.lock().await;
                let svc = AppRunnerService::new(&db);
                if healthy {
                    let _ = svc.complete_build_step(
                        &health_step_id,
                        true,
                        Some("App is healthy"),
                        None,
                    );
                    let _ = svc.update_health_status(&bg_instance_id, "healthy");
                    let _ = svc.update_status(&bg_instance_id, "running", None, None);
                    let _ = svc.append_logs(
                        &bg_instance_id,
                        &format!("[nexus] App running at http://localhost:{}", bg_port),
                    );
                } else {
                    let _ = svc.complete_build_step(
                        &health_step_id,
                        false,
                        None,
                        Some("Health check timed out after 30 attempts"),
                    );
                    let _ = svc.update_health_status(&bg_instance_id, "unhealthy");
                    // Still mark as running — the container is up, just not responding yet
                    let _ = svc.update_status(&bg_instance_id, "running", None, None);
                    let _ = svc.append_logs(
                        &bg_instance_id,
                        "[nexus] Warning: health check timed out, app may still be starting.",
                    );
                }
            }

            // Spawn watchdog
            spawn_watchdog(Arc::clone(&bg_app), bg_project_id, bg_instance_id, bg_port);
        });
    } else {
        // ── Bare-metal path ───────────────────────────────────────────
        tokio::spawn(async move {
            // Create build steps
            let (env_step_id, install_step_id, start_step_id, health_step_id) = {
                let db = bg_app.db.lock().await;
                let svc = AppRunnerService::new(&db);
                let s1 = svc
                    .create_build_step(&bg_instance_id, "env_setup", "Setting up environment")
                    .ok()
                    .map(|s| s.id)
                    .unwrap_or_default();
                let s2 = svc
                    .create_build_step(&bg_instance_id, "install", "Installing dependencies")
                    .ok()
                    .map(|s| s.id)
                    .unwrap_or_default();
                let s3 = svc
                    .create_build_step(&bg_instance_id, "start", "Starting dev server")
                    .ok()
                    .map(|s| s.id)
                    .unwrap_or_default();
                let s4 = svc
                    .create_build_step(&bg_instance_id, "health_check", "Checking health")
                    .ok()
                    .map(|s| s.id)
                    .unwrap_or_default();
                (s1, s2, s3, s4)
            };

            // Step 0: env setup
            {
                let db = bg_app.db.lock().await;
                let svc = AppRunnerService::new(&db);
                let _ = svc.update_build_step(&env_step_id, "running", None, None);
                let env_log = write_env_file(&bg_app, &bg_project_id, &bg_output_dir, &db);
                let _ = svc.append_logs(&bg_instance_id, &env_log);
                let _ = svc.complete_build_step(&env_step_id, true, Some(&env_log), None);
            }

            // Step 1: npm install
            {
                let db = bg_app.db.lock().await;
                let svc = AppRunnerService::new(&db);
                let _ = svc.update_build_step(&install_step_id, "running", None, None);
                let _ = svc.append_logs(&bg_instance_id, "[nexus] Running npm install...");
            }

            match tokio::task::spawn_blocking({
                let dir = bg_output_dir.clone();
                move || npm_install(&dir)
            })
            .await
            {
                Ok(Ok(output)) => {
                    info!(instance_id = %bg_instance_id, "npm install completed");
                    let db = bg_app.db.lock().await;
                    let svc = AppRunnerService::new(&db);
                    let _ = svc.append_logs(&bg_instance_id, &output);
                    let _ = svc.append_logs(
                        &bg_instance_id,
                        "[nexus] npm install completed successfully.",
                    );
                    let _ = svc.complete_build_step(&install_step_id, true, Some(&output), None);
                }
                Ok(Err(e)) => {
                    error!(instance_id = %bg_instance_id, error = %e, "npm install failed");
                    let db = bg_app.db.lock().await;
                    let svc = AppRunnerService::new(&db);
                    let _ = svc.append_logs(
                        &bg_instance_id,
                        &format!("[nexus] npm install FAILED:\n{}", e),
                    );
                    let _ = svc.complete_build_step(&install_step_id, false, None, Some(&e));
                    let _ = svc.update_status(&bg_instance_id, "failed", None, Some(&e));
                    return;
                }
                Err(e) => {
                    error!(instance_id = %bg_instance_id, error = %e, "npm install task panicked");
                    let db = bg_app.db.lock().await;
                    let svc = AppRunnerService::new(&db);
                    let _ = svc.append_logs(
                        &bg_instance_id,
                        &format!("[nexus] npm install task panicked: {}", e),
                    );
                    let _ = svc.complete_build_step(
                        &install_step_id,
                        false,
                        None,
                        Some(&e.to_string()),
                    );
                    let _ =
                        svc.update_status(&bg_instance_id, "failed", None, Some(&e.to_string()));
                    return;
                }
            }

            // Step 2: start dev server
            {
                let db = bg_app.db.lock().await;
                let svc = AppRunnerService::new(&db);
                let _ = svc.update_status(&bg_instance_id, "starting", None, None);
                let _ = svc.update_build_step(&start_step_id, "running", None, None);
                let _ = svc.append_logs(
                    &bg_instance_id,
                    &format!("[nexus] Starting dev server on port {}...", bg_port),
                );
            }

            let pid = match tokio::task::spawn_blocking({
                let dir = bg_output_dir.clone();
                move || spawn_dev_server(&dir, bg_port)
            })
            .await
            {
                Ok(Ok(pid)) => {
                    info!(instance_id = %bg_instance_id, pid = pid, port = bg_port, "Dev server started");
                    let db = bg_app.db.lock().await;
                    let svc = AppRunnerService::new(&db);
                    let _ = svc.append_logs(
                        &bg_instance_id,
                        &format!(
                            "[nexus] Dev server started (PID {}, port {}).",
                            pid, bg_port
                        ),
                    );
                    let _ = svc.complete_build_step(
                        &start_step_id,
                        true,
                        Some(&format!("PID: {}", pid)),
                        None,
                    );
                    pid
                }
                Ok(Err(e)) => {
                    error!(instance_id = %bg_instance_id, error = %e, "Failed to spawn dev server");
                    let db = bg_app.db.lock().await;
                    let svc = AppRunnerService::new(&db);
                    let _ = svc.append_logs(
                        &bg_instance_id,
                        &format!("[nexus] Dev server FAILED: {}", e),
                    );
                    let _ = svc.complete_build_step(&start_step_id, false, None, Some(&e));
                    let _ = svc.update_status(&bg_instance_id, "failed", None, Some(&e));
                    return;
                }
                Err(e) => {
                    error!(instance_id = %bg_instance_id, error = %e, "Dev server task panicked");
                    let db = bg_app.db.lock().await;
                    let svc = AppRunnerService::new(&db);
                    let _ = svc.append_logs(
                        &bg_instance_id,
                        &format!("[nexus] Dev server task panicked: {}", e),
                    );
                    let _ =
                        svc.complete_build_step(&start_step_id, false, None, Some(&e.to_string()));
                    let _ =
                        svc.update_status(&bg_instance_id, "failed", None, Some(&e.to_string()));
                    return;
                }
            };

            // Step 3: health check
            {
                let db = bg_app.db.lock().await;
                let svc = AppRunnerService::new(&db);
                let _ = svc.update_build_step(&health_step_id, "running", None, None);
                let _ = svc.append_logs(
                    &bg_instance_id,
                    "[nexus] Waiting for app to become healthy...",
                );
            }

            let mut healthy = false;
            for attempt in 1..=30 {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                if check_app_health(bg_port).await {
                    healthy = true;
                    let db = bg_app.db.lock().await;
                    let svc = AppRunnerService::new(&db);
                    let _ = svc.append_logs(
                        &bg_instance_id,
                        &format!("[nexus] Health check passed on attempt {}", attempt),
                    );
                    break;
                }
            }

            {
                let db = bg_app.db.lock().await;
                let svc = AppRunnerService::new(&db);
                if healthy {
                    let _ = svc.complete_build_step(
                        &health_step_id,
                        true,
                        Some("App is healthy"),
                        None,
                    );
                    let _ = svc.update_health_status(&bg_instance_id, "healthy");
                    let _ = svc.update_status(&bg_instance_id, "running", Some(pid), None);
                    let _ = svc.append_logs(
                        &bg_instance_id,
                        &format!("[nexus] App running at http://localhost:{}", bg_port),
                    );
                } else {
                    let _ = svc.complete_build_step(
                        &health_step_id,
                        false,
                        None,
                        Some("Health check timed out after 30 attempts"),
                    );
                    let _ = svc.update_health_status(&bg_instance_id, "unhealthy");
                    // Still mark as running — process is up, just not responding yet
                    let _ = svc.update_status(&bg_instance_id, "running", Some(pid), None);
                    let _ = svc.append_logs(
                        &bg_instance_id,
                        "[nexus] Warning: health check timed out, app may still be starting.",
                    );
                }
            }

            // Spawn watchdog
            spawn_watchdog(Arc::clone(&bg_app), bg_project_id, bg_instance_id, bg_port);
        });
    }

    Ok(Json(json!({
        "id": instance_id,
        "port": port,
        "status": "installing",
        "sandbox": sandbox,
    })))
}

// ---------------------------------------------------------------------------
// POST /projects/:id/app/stop
// ---------------------------------------------------------------------------

pub async fn stop_app(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    access
        .require_scope(&Scope::RuntimeManage)
        .map_err(|_| ApiError::Forbidden("runtime:manage scope required to stop apps".into()))?;
    let project_id = access.project_id.clone();
    let db = app.db.lock().await;
    let svc = AppRunnerService::new(&db);

    let instance = svc
        .get_running_instance(&project_id)?
        .ok_or_else(|| ApiError::NotFound("No running app found".to_string()))?;

    if instance.sandbox {
        // Stop Docker container
        if let Some(ref cid) = instance.container_id {
            if let Err(e) = docker_stop(cid) {
                error!(container_id = %cid, error = %e, "Failed to stop container");
            }
        }
    } else {
        // Stop bare-metal process
        if let Some(pid) = instance.pid {
            if let Err(e) = stop_process(pid) {
                error!(pid = pid, error = %e, "Failed to stop process");
            }
        }
    }

    svc.update_status(&instance.id, "stopped", None, None)?;

    Ok(Json(json!({
        "id": instance.id,
        "status": "stopped",
    })))
}

// ---------------------------------------------------------------------------
// POST /projects/:id/app/restart
// Stops the running instance, then starts a new one on the same port.
// ---------------------------------------------------------------------------

pub async fn restart_app(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    access
        .require_scope(&Scope::RuntimeManage)
        .map_err(|_| ApiError::Forbidden("runtime:manage scope required to restart apps".into()))?;
    let project_id = access.project_id.clone();
    let output_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");

    if !output_dir.exists() {
        return Err(ApiError::BadRequest(
            "No generated code found. Run codegen first.".to_string(),
        ));
    }

    // Stop the current instance if running
    let previous_sandbox = {
        let db = app.db.lock().await;
        let svc = AppRunnerService::new(&db);

        if let Some(instance) = svc.get_running_instance(&project_id)? {
            let was_sandbox = instance.sandbox;

            if instance.sandbox {
                if let Some(ref cid) = instance.container_id {
                    if let Err(e) = docker_stop(cid) {
                        error!(container_id = %cid, error = %e, "Failed to stop container during restart");
                    }
                }
            } else if let Some(pid) = instance.pid {
                if let Err(e) = stop_process(pid) {
                    error!(pid = pid, error = %e, "Failed to stop process during restart");
                }
            }

            svc.update_status(&instance.id, "stopped", None, None)?;
            info!(project_id = %project_id, old_instance = %instance.id, "Stopped previous instance for restart");
            Some(was_sandbox)
        } else {
            None
        }
    };

    // Determine sandbox mode: reuse previous setting, or fall back to settings default
    let sandbox = if let Some(was_sandbox) = previous_sandbox {
        was_sandbox
    } else {
        let db = app.db.lock().await;
        let svc = ProjectService::new(&db);
        svc.get_setting("sandbox.enabled")
            .unwrap_or(None)
            .map(|v| v == "true")
            .unwrap_or(false)
    };

    if sandbox {
        if let Err(e) = check_docker_available() {
            return Err(ApiError::BadRequest(format!(
                "Sandbox mode requires Docker: {}",
                e
            )));
        }
    }

    // Create a new instance
    let (instance_id, port) = {
        let db = app.db.lock().await;
        let svc = AppRunnerService::new(&db);
        let port = svc.next_available_port()?;
        let instance = svc.create_instance(
            &project_id,
            port,
            output_dir.to_str().unwrap_or_default(),
            sandbox,
        )?;
        (instance.id, instance.port)
    };

    // Spawn background task (same logic as start_app)
    let bg_app = Arc::clone(&app);
    let bg_instance_id = instance_id.clone();
    let bg_output_dir = output_dir.clone();
    let bg_port = port;
    let bg_project_id = project_id.clone();

    if sandbox {
        tokio::spawn(async move {
            let image_tag = format!("nexus-app-{}", bg_project_id);
            let container_name = format!(
                "nexus-app-{}-{}",
                bg_project_id,
                bg_instance_id.get(..8).unwrap_or(&bg_instance_id)
            );

            // Create build steps
            let (env_step_id, build_step_id, run_step_id, health_step_id) = {
                let db = bg_app.db.lock().await;
                let svc = AppRunnerService::new(&db);
                let s1 = svc
                    .create_build_step(&bg_instance_id, "env_setup", "Setting up environment")
                    .ok()
                    .map(|s| s.id)
                    .unwrap_or_default();
                let s2 = svc
                    .create_build_step(&bg_instance_id, "docker_build", "Building Docker image")
                    .ok()
                    .map(|s| s.id)
                    .unwrap_or_default();
                let s3 = svc
                    .create_build_step(&bg_instance_id, "docker_run", "Starting container")
                    .ok()
                    .map(|s| s.id)
                    .unwrap_or_default();
                let s4 = svc
                    .create_build_step(&bg_instance_id, "health_check", "Checking health")
                    .ok()
                    .map(|s| s.id)
                    .unwrap_or_default();
                (s1, s2, s3, s4)
            };

            // Step 0: env setup
            {
                let db = bg_app.db.lock().await;
                let svc = AppRunnerService::new(&db);
                let _ = svc.update_build_step(&env_step_id, "running", None, None);
                let env_log = write_env_file(&bg_app, &bg_project_id, &bg_output_dir, &db);
                let _ = svc.append_logs(&bg_instance_id, &env_log);
                let _ = svc.complete_build_step(&env_step_id, true, Some(&env_log), None);
            }

            // Step 1: docker build
            {
                let db = bg_app.db.lock().await;
                let svc = AppRunnerService::new(&db);
                let _ = svc.update_build_step(&build_step_id, "running", None, None);
            }

            let project_type = match tokio::task::spawn_blocking({
                let dir = bg_output_dir.clone();
                let tag = image_tag.clone();
                move || docker_build(&dir, &tag)
            })
            .await
            {
                Ok(Ok(pt)) => {
                    info!(instance_id = %bg_instance_id, project_type = ?pt, "Docker image rebuilt for restart");
                    let db = bg_app.db.lock().await;
                    let svc = AppRunnerService::new(&db);
                    let _ =
                        svc.complete_build_step(&build_step_id, true, Some("Image rebuilt"), None);
                    pt
                }
                Ok(Err(e)) => {
                    error!(instance_id = %bg_instance_id, error = %e, "Docker build failed on restart");
                    let db = bg_app.db.lock().await;
                    let svc = AppRunnerService::new(&db);
                    let _ = svc.complete_build_step(&build_step_id, false, None, Some(&e));
                    let _ = svc.update_status(&bg_instance_id, "failed", None, Some(&e));
                    return;
                }
                Err(e) => {
                    let db = bg_app.db.lock().await;
                    let svc = AppRunnerService::new(&db);
                    let _ =
                        svc.complete_build_step(&build_step_id, false, None, Some(&e.to_string()));
                    let _ =
                        svc.update_status(&bg_instance_id, "failed", None, Some(&e.to_string()));
                    return;
                }
            };

            {
                let db = bg_app.db.lock().await;
                let svc = AppRunnerService::new(&db);
                let _ = svc.update_status(&bg_instance_id, "starting", None, None);
                let _ = svc.update_build_step(&run_step_id, "running", None, None);
            }

            match tokio::task::spawn_blocking({
                let tag = image_tag;
                let name = container_name;
                move || docker_run(&tag, bg_port, &name, project_type)
            })
            .await
            {
                Ok(Ok(container_id)) => {
                    info!(instance_id = %bg_instance_id, container_id = %container_id, "Sandbox container restarted");
                    let db = bg_app.db.lock().await;
                    let svc = AppRunnerService::new(&db);
                    let _ = svc.set_container_id(&bg_instance_id, &container_id);
                    let _ = svc.complete_build_step(
                        &run_step_id,
                        true,
                        Some("Container started"),
                        None,
                    );
                }
                Ok(Err(e)) => {
                    let db = bg_app.db.lock().await;
                    let svc = AppRunnerService::new(&db);
                    let _ = svc.complete_build_step(&run_step_id, false, None, Some(&e));
                    let _ = svc.update_status(&bg_instance_id, "failed", None, Some(&e));
                    return;
                }
                Err(e) => {
                    let db = bg_app.db.lock().await;
                    let svc = AppRunnerService::new(&db);
                    let _ =
                        svc.complete_build_step(&run_step_id, false, None, Some(&e.to_string()));
                    let _ =
                        svc.update_status(&bg_instance_id, "failed", None, Some(&e.to_string()));
                    return;
                }
            }

            // Health check
            {
                let db = bg_app.db.lock().await;
                let svc = AppRunnerService::new(&db);
                let _ = svc.update_build_step(&health_step_id, "running", None, None);
            }

            let mut healthy = false;
            for attempt in 1..=30 {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                if check_app_health(bg_port).await {
                    healthy = true;
                    let db = bg_app.db.lock().await;
                    let svc = AppRunnerService::new(&db);
                    let _ = svc.append_logs(
                        &bg_instance_id,
                        &format!("[nexus] Health check passed on attempt {}", attempt),
                    );
                    break;
                }
            }

            {
                let db = bg_app.db.lock().await;
                let svc = AppRunnerService::new(&db);
                if healthy {
                    let _ = svc.complete_build_step(
                        &health_step_id,
                        true,
                        Some("App is healthy"),
                        None,
                    );
                    let _ = svc.update_health_status(&bg_instance_id, "healthy");
                    let _ = svc.update_status(&bg_instance_id, "running", None, None);
                } else {
                    let _ = svc.complete_build_step(
                        &health_step_id,
                        false,
                        None,
                        Some("Health check timed out"),
                    );
                    let _ = svc.update_health_status(&bg_instance_id, "unhealthy");
                    let _ = svc.update_status(&bg_instance_id, "running", None, None);
                }
            }

            spawn_watchdog(Arc::clone(&bg_app), bg_project_id, bg_instance_id, bg_port);
        });
    } else {
        tokio::spawn(async move {
            // Create build steps
            let (env_step_id, install_step_id, start_step_id, health_step_id) = {
                let db = bg_app.db.lock().await;
                let svc = AppRunnerService::new(&db);
                let s1 = svc
                    .create_build_step(&bg_instance_id, "env_setup", "Setting up environment")
                    .ok()
                    .map(|s| s.id)
                    .unwrap_or_default();
                let s2 = svc
                    .create_build_step(&bg_instance_id, "install", "Installing dependencies")
                    .ok()
                    .map(|s| s.id)
                    .unwrap_or_default();
                let s3 = svc
                    .create_build_step(&bg_instance_id, "start", "Starting dev server")
                    .ok()
                    .map(|s| s.id)
                    .unwrap_or_default();
                let s4 = svc
                    .create_build_step(&bg_instance_id, "health_check", "Checking health")
                    .ok()
                    .map(|s| s.id)
                    .unwrap_or_default();
                (s1, s2, s3, s4)
            };

            // Step 0: env setup
            {
                let db = bg_app.db.lock().await;
                let svc = AppRunnerService::new(&db);
                let _ = svc.update_build_step(&env_step_id, "running", None, None);
                let env_log = write_env_file(&bg_app, &bg_project_id, &bg_output_dir, &db);
                let _ = svc.append_logs(&bg_instance_id, &env_log);
                let _ = svc.complete_build_step(&env_step_id, true, Some(&env_log), None);
            }

            // Step 1: npm install
            {
                let db = bg_app.db.lock().await;
                let svc = AppRunnerService::new(&db);
                let _ = svc.update_build_step(&install_step_id, "running", None, None);
                let _ = svc.append_logs(
                    &bg_instance_id,
                    "[nexus] Restarting \u{2014} running npm install...",
                );
            }

            match tokio::task::spawn_blocking({
                let dir = bg_output_dir.clone();
                move || npm_install(&dir)
            })
            .await
            {
                Ok(Ok(output)) => {
                    info!(instance_id = %bg_instance_id, "npm install completed for restart");
                    let db = bg_app.db.lock().await;
                    let svc = AppRunnerService::new(&db);
                    let _ = svc.append_logs(&bg_instance_id, &output);
                    let _ = svc.append_logs(&bg_instance_id, "[nexus] npm install completed.");
                    let _ = svc.complete_build_step(&install_step_id, true, Some(&output), None);
                }
                Ok(Err(e)) => {
                    error!(instance_id = %bg_instance_id, error = %e, "npm install failed on restart");
                    let db = bg_app.db.lock().await;
                    let svc = AppRunnerService::new(&db);
                    let _ = svc.append_logs(
                        &bg_instance_id,
                        &format!("[nexus] npm install FAILED:\n{}", e),
                    );
                    let _ = svc.complete_build_step(&install_step_id, false, None, Some(&e));
                    let _ = svc.update_status(&bg_instance_id, "failed", None, Some(&e));
                    return;
                }
                Err(e) => {
                    let db = bg_app.db.lock().await;
                    let svc = AppRunnerService::new(&db);
                    let _ = svc.append_logs(
                        &bg_instance_id,
                        &format!("[nexus] npm install panicked: {}", e),
                    );
                    let _ = svc.complete_build_step(
                        &install_step_id,
                        false,
                        None,
                        Some(&e.to_string()),
                    );
                    let _ =
                        svc.update_status(&bg_instance_id, "failed", None, Some(&e.to_string()));
                    return;
                }
            }

            // Step 2: start dev server
            {
                let db = bg_app.db.lock().await;
                let svc = AppRunnerService::new(&db);
                let _ = svc.update_status(&bg_instance_id, "starting", None, None);
                let _ = svc.update_build_step(&start_step_id, "running", None, None);
                let _ = svc.append_logs(
                    &bg_instance_id,
                    &format!("[nexus] Starting dev server on port {}...", bg_port),
                );
            }

            let pid = match tokio::task::spawn_blocking({
                let dir = bg_output_dir;
                move || spawn_dev_server(&dir, bg_port)
            })
            .await
            {
                Ok(Ok(pid)) => {
                    info!(instance_id = %bg_instance_id, pid = pid, port = bg_port, "Dev server restarted");
                    let db = bg_app.db.lock().await;
                    let svc = AppRunnerService::new(&db);
                    let _ = svc.append_logs(
                        &bg_instance_id,
                        &format!(
                            "[nexus] Dev server started (PID {}, port {}).",
                            pid, bg_port
                        ),
                    );
                    let _ = svc.complete_build_step(
                        &start_step_id,
                        true,
                        Some(&format!("PID: {}", pid)),
                        None,
                    );
                    pid
                }
                Ok(Err(e)) => {
                    let db = bg_app.db.lock().await;
                    let svc = AppRunnerService::new(&db);
                    let _ = svc.append_logs(
                        &bg_instance_id,
                        &format!("[nexus] Dev server FAILED: {}", e),
                    );
                    let _ = svc.complete_build_step(&start_step_id, false, None, Some(&e));
                    let _ = svc.update_status(&bg_instance_id, "failed", None, Some(&e));
                    return;
                }
                Err(e) => {
                    let db = bg_app.db.lock().await;
                    let svc = AppRunnerService::new(&db);
                    let _ = svc.append_logs(
                        &bg_instance_id,
                        &format!("[nexus] Dev server panicked: {}", e),
                    );
                    let _ =
                        svc.complete_build_step(&start_step_id, false, None, Some(&e.to_string()));
                    let _ =
                        svc.update_status(&bg_instance_id, "failed", None, Some(&e.to_string()));
                    return;
                }
            };

            // Step 3: health check
            {
                let db = bg_app.db.lock().await;
                let svc = AppRunnerService::new(&db);
                let _ = svc.update_build_step(&health_step_id, "running", None, None);
                let _ = svc.append_logs(
                    &bg_instance_id,
                    "[nexus] Waiting for app to become healthy...",
                );
            }

            let mut healthy = false;
            for attempt in 1..=30 {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                if check_app_health(bg_port).await {
                    healthy = true;
                    let db = bg_app.db.lock().await;
                    let svc = AppRunnerService::new(&db);
                    let _ = svc.append_logs(
                        &bg_instance_id,
                        &format!("[nexus] Health check passed on attempt {}", attempt),
                    );
                    break;
                }
            }

            {
                let db = bg_app.db.lock().await;
                let svc = AppRunnerService::new(&db);
                if healthy {
                    let _ = svc.complete_build_step(
                        &health_step_id,
                        true,
                        Some("App is healthy"),
                        None,
                    );
                    let _ = svc.update_health_status(&bg_instance_id, "healthy");
                    let _ = svc.update_status(&bg_instance_id, "running", Some(pid), None);
                    let _ = svc.append_logs(
                        &bg_instance_id,
                        &format!("[nexus] App running at http://localhost:{}", bg_port),
                    );
                } else {
                    let _ = svc.complete_build_step(
                        &health_step_id,
                        false,
                        None,
                        Some("Health check timed out"),
                    );
                    let _ = svc.update_health_status(&bg_instance_id, "unhealthy");
                    let _ = svc.update_status(&bg_instance_id, "running", Some(pid), None);
                    let _ = svc.append_logs(
                        &bg_instance_id,
                        "[nexus] Warning: health check timed out, app may still be starting.",
                    );
                }
            }

            spawn_watchdog(Arc::clone(&bg_app), bg_project_id, bg_instance_id, bg_port);
        });
    }

    info!(project_id = %project_id, "App restart initiated");

    Ok(Json(json!({
        "id": instance_id,
        "port": port,
        "status": "installing",
        "sandbox": sandbox,
        "restarted": true,
    })))
}

// ---------------------------------------------------------------------------
// GET /projects/:id/app/status
// ---------------------------------------------------------------------------

pub async fn app_status(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    let project_id = access.project_id.clone();
    let db = app.db.lock().await;
    let svc = AppRunnerService::new(&db);

    let instance = svc.get_running_instance(&project_id)?;

    match instance {
        Some(inst) => {
            let url = if inst.status == "running" {
                Some(format!("http://localhost:{}", inst.port))
            } else {
                None
            };
            let steps = svc.list_build_steps(&inst.id).unwrap_or_default();
            Ok(Json(json!({
                "instance": inst,
                "url": url,
                "build_steps": steps,
            })))
        }
        None => {
            // Fall back to latest instance of any status.
            let all = svc.list_instances(&project_id)?;
            match all.into_iter().next() {
                Some(latest) => {
                    let steps = svc.list_build_steps(&latest.id).unwrap_or_default();
                    Ok(Json(json!({
                        "instance": latest,
                        "url": serde_json::Value::Null,
                        "build_steps": steps,
                    })))
                }
                None => Ok(Json(json!({
                    "instance": serde_json::Value::Null,
                    "url": serde_json::Value::Null,
                    "build_steps": [],
                }))),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// GET /projects/:id/app/instances
// ---------------------------------------------------------------------------

pub async fn list_instances(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    let project_id = access.project_id.clone();
    let db = app.db.lock().await;
    let svc = AppRunnerService::new(&db);
    let instances = svc.list_instances(&project_id)?;
    Ok(Json(serde_json::to_value(instances)?))
}

// ---------------------------------------------------------------------------
// GET /projects/:id/app/logs — get build/install logs for the current instance
// Works for both bare-metal (from DB) and sandbox (from Docker + DB).
// ---------------------------------------------------------------------------

pub async fn app_logs(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    let project_id = access.project_id.clone();
    let db = app.db.lock().await;
    let svc = AppRunnerService::new(&db);

    // Try running instance first, fall back to latest of any status
    let instance = if let Some(inst) = svc.get_running_instance(&project_id)? {
        inst
    } else {
        let all = svc.list_instances(&project_id)?;
        all.into_iter()
            .next()
            .ok_or_else(|| ApiError::NotFound("No app instances found".to_string()))?
    };

    // For sandbox instances, append Docker container logs
    let mut logs = instance.logs.clone();
    if instance.sandbox {
        if let Some(ref cid) = instance.container_id {
            if let Ok(docker_output) = docker_logs(cid, 200) {
                if !docker_output.is_empty() {
                    logs.push_str("\n--- Docker container logs ---\n");
                    logs.push_str(&docker_output);
                }
            }
        }
    }

    Ok(Json(json!({
        "instance_id": instance.id,
        "status": instance.status,
        "error": instance.error,
        "logs": logs,
    })))
}

// ---------------------------------------------------------------------------
// GET /projects/:id/app/logs/stream — SSE endpoint for real-time log streaming
// ---------------------------------------------------------------------------

pub async fn stream_logs(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> Sse<impl futures_util::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let project_id = access.project_id.clone();
    let stream = async_stream::stream! {
        let mut last_len = 0usize;
        let mut last_status = String::new();
        // Hard-cap the polling loop. Without this, an instance stuck in
        // `installing` / `starting` (npm install hung, postinstall script
        // panicked before reaching `failed`) leaves the loop hammering the
        // global SQLite mutex twice per 500 ms forever, starving every other
        // DB-using handler. 10 minutes is well past the longest realistic
        // healthy startup; clients can reconnect afterwards.
        let started = std::time::Instant::now();
        const MAX_STREAM_DURATION: std::time::Duration = std::time::Duration::from_secs(600);

        loop {
            if started.elapsed() > MAX_STREAM_DURATION {
                yield Ok(Event::default()
                    .event("timeout")
                    .json_data(serde_json::json!({
                        "message": "stream_logs reached max duration; reconnect to continue",
                        "elapsed_secs": started.elapsed().as_secs(),
                    }))
                    .unwrap_or_else(|_| Event::default().data("{}")));
                break;
            }
            let (instance, steps) = {
                let db = app.db.lock().await;
                let svc = AppRunnerService::new(&db);
                let inst = svc
                    .get_running_instance(&project_id)
                    .ok()
                    .flatten()
                    .or_else(|| svc.list_instances(&project_id).ok()?.into_iter().next());
                let steps = inst
                    .as_ref()
                    .and_then(|i| svc.list_build_steps(&i.id).ok())
                    .unwrap_or_default();
                (inst, steps)
            };

            if let Some(ref inst) = instance {
                // Send new log lines
                let current_logs = &inst.logs;
                if current_logs.len() > last_len {
                    let new_content = &current_logs[last_len..];
                    last_len = current_logs.len();
                    yield Ok(Event::default()
                        .event("logs")
                        .json_data(serde_json::json!({"content": new_content}))
                        .unwrap_or_else(|_| Event::default().data("{}")));
                }

                // Send status changes
                if inst.status != last_status {
                    last_status.clone_from(&inst.status);
                    yield Ok(Event::default()
                        .event("status")
                        .json_data(serde_json::json!({
                            "status": inst.status,
                            "port": inst.port,
                            "pid": inst.pid,
                            "error": inst.error,
                            "health_status": inst.health_status,
                            "restart_count": inst.restart_count,
                        }))
                        .unwrap_or_else(|_| Event::default().data("{}")));
                }

                // Send build steps
                if !steps.is_empty() {
                    yield Ok(Event::default()
                        .event("build_steps")
                        .json_data(serde_json::json!({"steps": steps}))
                        .unwrap_or_else(|_| Event::default().data("{}")));
                }

                // If terminal state, send done and break
                if matches!(inst.status.as_str(), "running" | "stopped" | "failed" | "crashed") {
                    yield Ok(Event::default()
                        .event("done")
                        .json_data(serde_json::json!({"status": inst.status}))
                        .unwrap_or_else(|_| Event::default().data("{}")));
                    break;
                }
            } else {
                yield Ok(Event::default()
                    .event("waiting")
                    .json_data(serde_json::json!({"message": "No instance found"}))
                    .unwrap_or_else(|_| Event::default().data("{}")));
            }

            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("ping"),
    )
}

// ---------------------------------------------------------------------------
// GET /projects/:id/app/build-steps
// ---------------------------------------------------------------------------

pub async fn get_build_steps(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    let project_id = access.project_id.clone();
    let db = app.db.lock().await;
    let svc = AppRunnerService::new(&db);

    let instance = svc
        .get_running_instance(&project_id)?
        .or_else(|| svc.list_instances(&project_id).ok()?.into_iter().next());

    match instance {
        Some(inst) => {
            let steps = svc.list_build_steps(&inst.id)?;
            Ok(Json(json!({
                "instance_id": inst.id,
                "steps": steps,
            })))
        }
        None => Ok(Json(json!({
            "instance_id": serde_json::Value::Null,
            "steps": [],
        }))),
    }
}

// ---------------------------------------------------------------------------
// POST /projects/:id/app/auto-restart — toggle auto-restart
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ToggleAutoRestartReq {
    pub enabled: bool,
}

pub async fn toggle_auto_restart(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(body): Json<ToggleAutoRestartReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let project_id = access.project_id.clone();
    let db = app.db.lock().await;
    let svc = AppRunnerService::new(&db);

    let instance = svc
        .get_running_instance(&project_id)?
        .ok_or_else(|| ApiError::NotFound("No running app found".to_string()))?;

    svc.set_auto_restart(&instance.id, body.enabled)?;

    Ok(Json(json!({
        "instance_id": instance.id,
        "auto_restart": body.enabled,
    })))
}

// ---------------------------------------------------------------------------
// Environment variables
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SetEnvVarReq {
    pub key: String,
    pub value: String,
}

/// GET /projects/:id/app/env
pub async fn list_env_vars(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    access
        .require_scope(&Scope::ProjectRead)
        .map_err(|_| ApiError::Forbidden("project:read scope required".into()))?;
    let project_id = access.project_id.clone();
    let db = app.db.lock().await;
    let svc = AppRunnerService::new(&db);
    let vars = svc.list_env_vars(&project_id)?;
    Ok(Json(json!({ "env_vars": vars })))
}

/// POST /projects/:id/app/env
pub async fn set_env_var(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(body): Json<SetEnvVarReq>,
) -> ApiResult<Json<serde_json::Value>> {
    access
        .require_scope(&Scope::ProjectWrite)
        .map_err(|_| ApiError::Forbidden("project:write scope required".into()))?;
    let project_id = access.project_id.clone();
    if body.key.is_empty() {
        return Err(ApiError::BadRequest("Key cannot be empty".into()));
    }
    let db = app.db.lock().await;
    let svc = AppRunnerService::new(&db);
    let var = svc.set_env_var(&project_id, &body.key, &body.value)?;
    info!(project_id = %project_id, key = %body.key, "Environment variable set");
    Ok(Json(serde_json::to_value(var)?))
}

/// DELETE /projects/:id/app/env/:key
pub async fn delete_env_var(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Path((_project_id, key)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    access
        .require_scope(&Scope::ProjectWrite)
        .map_err(|_| ApiError::Forbidden("project:write scope required".into()))?;
    let project_id = access.project_id.clone();
    let db = app.db.lock().await;
    let svc = AppRunnerService::new(&db);
    svc.delete_env_var(&project_id, &key)?;
    info!(project_id = %project_id, key = %key, "Environment variable deleted");
    Ok(Json(json!({ "status": "deleted", "key": key })))
}

// ---------------------------------------------------------------------------
// GET /projects/:id/app/files — list generated files as a tree
// ---------------------------------------------------------------------------

pub async fn list_files(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    access
        .require_scope(&Scope::ProjectRead)
        .map_err(|_| ApiError::Forbidden("project:read scope required".into()))?;
    let project_id = access.project_id.clone();
    let output_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");
    if !output_dir.exists() {
        return Ok(Json(
            json!({ "files": [], "root": output_dir.display().to_string() }),
        ));
    }

    let mut files = Vec::new();
    collect_files(&output_dir, &output_dir, &mut files);
    files.sort_by(|a, b| {
        let pa = a.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let pb = b.get("path").and_then(|v| v.as_str()).unwrap_or("");
        pa.cmp(pb)
    });

    Ok(Json(
        json!({ "files": files, "root": output_dir.display().to_string() }),
    ))
}

// ---------------------------------------------------------------------------
// GET /projects/:id/app/files/*path — read a single file's content
// ---------------------------------------------------------------------------

pub async fn read_file(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Path((_pid, file_path)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    access
        .require_scope(&Scope::ProjectRead)
        .map_err(|_| ApiError::Forbidden("project:read scope required".into()))?;
    let project_id = access.project_id.clone();
    let output_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");
    let decoded = file_path
        .trim_start_matches('/')
        .replace("%2F", "/")
        .replace("%5C", "/");
    let full_path = output_dir.join(&decoded);

    let canonical = full_path
        .canonicalize()
        .map_err(|_| ApiError::NotFound(format!("File not found: {}", decoded)))?;
    let canonical_root = output_dir
        .canonicalize()
        .map_err(|_| ApiError::NotFound("Generated directory not found".into()))?;
    if !canonical.starts_with(&canonical_root) {
        return Err(ApiError::BadRequest("Path traversal not allowed".into()));
    }

    let content = std::fs::read_to_string(&canonical)
        .map_err(|e| ApiError::NotFound(format!("Cannot read file: {}", e)))?;

    let extension = canonical
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_string();

    Ok(Json(json!({
        "path": decoded,
        "content": content,
        "extension": extension,
        "size": content.len()
    })))
}

// ---------------------------------------------------------------------------
// PUT /projects/:id/app/files/*path — write/update a file's content
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct WriteFileReq {
    pub content: String,
}

pub async fn write_file(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Path((_pid, file_path)): Path<(String, String)>,
    Json(body): Json<WriteFileReq>,
) -> ApiResult<Json<serde_json::Value>> {
    access
        .require_scope(&Scope::ProjectWrite)
        .map_err(|_| ApiError::Forbidden("project:write scope required".into()))?;
    // Bound file size explicitly — the global 10 MiB body cap catches truly
    // pathological payloads but is too generous for a source-file editor.
    if body.content.len() > crate::input_limits::MAX_FILE_WRITE_BYTES {
        return Err(ApiError::BadRequest(format!(
            "file content exceeds {} bytes (got {})",
            crate::input_limits::MAX_FILE_WRITE_BYTES,
            body.content.len()
        )));
    }
    let project_id = access.project_id.clone();
    let output_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");
    if !output_dir.exists() {
        return Err(ApiError::BadRequest("No generated code found".into()));
    }

    let decoded = file_path
        .trim_start_matches('/')
        .replace("%2F", "/")
        .replace("%5C", "/");
    let full_path = output_dir.join(&decoded);

    let parent = full_path
        .parent()
        .ok_or_else(|| ApiError::BadRequest("Invalid file path".into()))?;

    // Validate path against the canonical project root BEFORE creating any
    // directories, to prevent a traversal payload (e.g. "../../evil/") from
    // materialising attacker-controlled directories on disk.
    let canonical_root = output_dir
        .canonicalize()
        .map_err(|_| ApiError::NotFound("Generated directory not found".into()))?;

    // Walk up from `parent` until we find an existing ancestor; canonicalize
    // that, then append the remaining components and verify containment.
    let mut existing = parent;
    let mut tail = std::path::PathBuf::new();
    while !existing.exists() {
        if let Some(name) = existing.file_name() {
            tail = std::path::Path::new(name).join(&tail);
        }
        match existing.parent() {
            Some(p) => existing = p,
            None => break,
        }
    }
    let canonical_existing = existing
        .canonicalize()
        .map_err(|_| ApiError::BadRequest("Invalid path".into()))?;
    let resolved_parent = canonical_existing.join(&tail);
    if !resolved_parent.starts_with(&canonical_root) {
        return Err(ApiError::BadRequest("Path traversal not allowed".into()));
    }

    // Safe to create parent dirs now.
    if !parent.exists() {
        std::fs::create_dir_all(parent)
            .map_err(|e| ApiError::Internal(format!("Failed to create directories: {}", e)))?;
    }

    // Re-verify after mkdir (defence in depth in case of symlink races).
    let canonical_parent = parent
        .canonicalize()
        .map_err(|_| ApiError::NotFound("Parent directory not found".into()))?;
    if !canonical_parent.starts_with(&canonical_root) {
        return Err(ApiError::BadRequest("Path traversal not allowed".into()));
    }

    std::fs::write(&full_path, &body.content)
        .map_err(|e| ApiError::Internal(format!("Failed to write file: {}", e)))?;

    info!(project_id = %project_id, path = %decoded, "File written");

    Ok(Json(json!({
        "path": decoded,
        "size": body.content.len(),
        "status": "saved"
    })))
}

// ---------------------------------------------------------------------------
// DELETE /projects/:id/app/files/*path — delete a file
// ---------------------------------------------------------------------------

pub async fn delete_file(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Path((_pid, file_path)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    access
        .require_scope(&Scope::ProjectWrite)
        .map_err(|_| ApiError::Forbidden("project:write scope required".into()))?;
    let project_id = access.project_id.clone();
    let output_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");
    let decoded = file_path
        .trim_start_matches('/')
        .replace("%2F", "/")
        .replace("%5C", "/");
    let full_path = output_dir.join(&decoded);

    let canonical = full_path
        .canonicalize()
        .map_err(|_| ApiError::NotFound(format!("File not found: {}", decoded)))?;
    let canonical_root = output_dir
        .canonicalize()
        .map_err(|_| ApiError::NotFound("Generated directory not found".into()))?;
    if !canonical.starts_with(&canonical_root) {
        return Err(ApiError::BadRequest("Path traversal not allowed".into()));
    }

    if canonical.is_dir() {
        std::fs::remove_dir_all(&canonical)
            .map_err(|e| ApiError::Internal(format!("Failed to delete directory: {}", e)))?;
    } else {
        std::fs::remove_file(&canonical)
            .map_err(|e| ApiError::Internal(format!("Failed to delete file: {}", e)))?;
    }

    info!(project_id = %project_id, path = %decoded, "File deleted");

    Ok(Json(json!({ "path": decoded, "status": "deleted" })))
}

// ---------------------------------------------------------------------------
// GET /projects/:id/app/download — download project as ZIP
// ---------------------------------------------------------------------------

pub async fn download_zip(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> Result<axum::response::Response, ApiError> {
    access
        .require_scope(&Scope::ProjectRead)
        .map_err(|_| ApiError::Forbidden("project:read scope required".into()))?;
    let project_id = access.project_id.clone();
    use axum::response::IntoResponse;
    use std::io::Write;

    let output_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");
    if !output_dir.exists() {
        return Err(ApiError::BadRequest("No generated code found".into()));
    }

    // Build ZIP in memory
    let buf = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(buf);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    fn add_dir_to_zip(
        zip: &mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>,
        root: &std::path::Path,
        current: &std::path::Path,
        options: zip::write::SimpleFileOptions,
    ) -> std::io::Result<()> {
        for entry in std::fs::read_dir(current)?.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name == "node_modules" || name == ".next" {
                continue;
            }
            // Refuse to follow symlinks. A user-writable project dir could
            // contain a symlink to /etc/passwd or to a sibling tenant's
            // generated/ directory; following it lets the ZIP exfiltrate
            // arbitrary host files.
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                tracing::warn!(path = %path.display(), "skipping symlink in zip download");
                continue;
            }
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .display()
                .to_string();
            if file_type.is_dir() {
                zip.add_directory(format!("{}/", relative), options)?;
                add_dir_to_zip(zip, root, &path, options)?;
            } else if file_type.is_file() {
                zip.start_file(&relative, options)?;
                let content = std::fs::read(&path)?;
                zip.write_all(&content)?;
            }
        }
        Ok(())
    }

    add_dir_to_zip(&mut zip, &output_dir, &output_dir, options)
        .map_err(|e| ApiError::Internal(format!("Failed to create ZIP: {}", e)))?;

    let result = zip
        .finish()
        .map_err(|e| ApiError::Internal(format!("Failed to finalize ZIP: {}", e)))?;
    let bytes = result.into_inner();

    info!(project_id = %project_id, size = bytes.len(), "ZIP download generated");

    Ok((
        axum::http::StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, "application/zip"),
            (
                axum::http::header::CONTENT_DISPOSITION,
                "attachment; filename=\"project.zip\"",
            ),
        ],
        bytes,
    )
        .into_response())
}

// ---------------------------------------------------------------------------
// POST /projects/:id/app/github — push to GitHub
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct GithubPushReq {
    /// e.g. "https://github.com/user/repo.git" or "git@github.com:user/repo.git"
    pub repo_url: String,
    /// Branch to push to (default: main)
    pub branch: Option<String>,
    /// Commit message
    pub message: Option<String>,
    /// GitHub personal access token (for HTTPS auth)
    pub token: Option<String>,
}

fn is_valid_git_ref(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'))
        && !name.starts_with('/')
        && !name.starts_with('.')
        && !name.contains("..")
}

fn is_valid_repo_url(url: &str) -> bool {
    if url.is_empty() || url.len() > 2048 {
        return false;
    }
    if let Some(rest) = url.strip_prefix("https://") {
        return !rest.contains(char::is_whitespace);
    }
    if let Some(rest) = url.strip_prefix("git@") {
        return !rest.contains(char::is_whitespace) && rest.contains(':');
    }
    false
}

/// Build a `Command` with a tightly-scoped environment.
///
/// The Nexus process holds master secrets (`NEXUS_JWT_SECRET`,
/// `NEXUS_ENCRYPTION_KEY`) and provider keys (`OPENAI_API_KEY`,
/// `ANTHROPIC_API_KEY`, etc.). Subprocesses we invoke during deploy
/// (`vercel`, `railway`, `docker`, `npx ...`, `npm install` postinstall
/// scripts, `git`, `ssh`) must NOT inherit those — `vercel` is documented
/// to upload the build environment to its server, and arbitrary postinstall
/// scripts can `printenv` whatever they like.
///
/// We start from a clean env and re-add only PATH + a small allowlist
/// needed by deploy tools (HOME, USER, LANG, SSH_AUTH_SOCK for git/ssh,
/// DOCKER_HOST for docker, plus any explicit `extra_envs` the caller
/// passed in).
pub(crate) fn build_deploy_command(program: &str) -> std::process::Command {
    let mut cmd = std::process::Command::new(program);
    cmd.env_clear();
    const PASSTHROUGH: &[&str] = &[
        "PATH",
        "HOME",
        "USER",
        "LOGNAME",
        "LANG",
        "LC_ALL",
        "TERM",
        "TMPDIR",
        "SSH_AUTH_SOCK",
        "DOCKER_HOST",
        "DOCKER_CONFIG",
        "XDG_RUNTIME_DIR",
    ];
    for k in PASSTHROUGH {
        if let Ok(v) = std::env::var(k) {
            cmd.env(k, v);
        }
    }
    cmd
}

fn run_git(
    dir: &std::path::Path,
    args: &[&str],
    envs: &[(&str, &str)],
) -> std::io::Result<std::process::Output> {
    let mut cmd = build_deploy_command("git");
    cmd.current_dir(dir).args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output()
}

pub async fn push_to_github(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(body): Json<GithubPushReq>,
) -> ApiResult<Json<serde_json::Value>> {
    access
        .require_scope(&Scope::DeployManage)
        .map_err(|_| ApiError::Forbidden("deploy:manage scope required".into()))?;
    let project_id = access.project_id.clone();
    let output_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");
    if !output_dir.exists() {
        return Err(ApiError::BadRequest("No generated code found".into()));
    }

    let branch = body.branch.as_deref().unwrap_or("main").to_string();
    if !is_valid_git_ref(&branch) {
        return Err(ApiError::BadRequest("Invalid branch name".into()));
    }
    if !is_valid_repo_url(&body.repo_url) {
        return Err(ApiError::BadRequest(
            "Invalid repo URL (expected https:// or git@...)".into(),
        ));
    }
    let message = body
        .message
        .as_deref()
        .unwrap_or("Deploy from Nexus")
        .to_string();
    if message.len() > 10_000 || message.contains('\0') {
        return Err(ApiError::BadRequest("Invalid commit message".into()));
    }

    // Use GIT_ASKPASS so the token never appears in argv or the remote URL.
    // We keep the origin URL clean (no embedded credentials) and inject the
    // token only via the credential helper for HTTPS pushes.
    let token = body.token.clone();
    let repo_url = body.repo_url.clone();
    let askpass_path: Option<std::path::PathBuf> = if token.is_some()
        && repo_url.starts_with("https://")
    {
        use std::io::Write as _;
        let askpass_dir = app.data_dir.join("tmp").join("askpass");
        std::fs::create_dir_all(&askpass_dir)
            .map_err(|e| ApiError::Internal(format!("askpass dir: {}", e)))?;
        let path = askpass_dir.join(format!("askpass-{}-{}.sh", project_id, std::process::id()));
        let mut f = std::fs::File::create(&path)
            .map_err(|e| ApiError::Internal(format!("askpass create: {}", e)))?;
        writeln!(
            f,
            "#!/bin/sh\ncase \"$1\" in\n  Username*) printf 'x-access-token';;\n  *) printf '%s' \"$GIT_TOKEN\";;\nesac\n"
        )
        .map_err(|e| ApiError::Internal(format!("askpass write: {}", e)))?;
        drop(f);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mut perms = std::fs::metadata(&path)
                .map_err(|e| ApiError::Internal(format!("askpass meta: {}", e)))?
                .permissions();
            perms.set_mode(0o700);
            std::fs::set_permissions(&path, perms)
                .map_err(|e| ApiError::Internal(format!("askpass chmod: {}", e)))?;
        }
        Some(path)
    } else {
        None
    };

    let dir_for_task = output_dir.clone();
    let branch_for_task = branch.clone();
    let message_for_task = message.clone();
    let repo_url_for_task = repo_url.clone();
    let token_for_task = token.clone();
    let askpass_path_for_task = askpass_path.clone();

    let result = tokio::task::spawn_blocking(move || -> std::io::Result<(bool, String, String)> {
        let askpass_env: Vec<(&'static str, String)> =
            match (askpass_path_for_task.as_ref(), token_for_task.as_ref()) {
                (Some(p), Some(t)) => vec![
                    ("GIT_ASKPASS", p.display().to_string()),
                    ("GIT_TERMINAL_PROMPT", "0".to_string()),
                    ("GIT_TOKEN", t.clone()),
                ],
                _ => vec![("GIT_TERMINAL_PROMPT", "0".to_string())],
            };
        let envs: Vec<(&str, &str)> = askpass_env.iter().map(|(k, v)| (*k, v.as_str())).collect();

        let steps: Vec<Vec<&str>> = vec![
            vec!["init"],
            vec!["checkout", "-B", &branch_for_task],
            vec!["add", "-A"],
            vec!["commit", "-m", &message_for_task, "--allow-empty"],
        ];
        let mut stdout = String::new();
        let mut stderr = String::new();
        for args in &steps {
            let out = run_git(&dir_for_task, args, &envs)?;
            stdout.push_str(&String::from_utf8_lossy(&out.stdout));
            stderr.push_str(&String::from_utf8_lossy(&out.stderr));
            if !out.status.success() {
                return Ok((false, stdout, stderr));
            }
        }
        // remote remove origin — ignore failure (may not exist)
        let _ = run_git(&dir_for_task, &["remote", "remove", "origin"], &envs);
        let add_remote = run_git(
            &dir_for_task,
            &["remote", "add", "origin", &repo_url_for_task],
            &envs,
        )?;
        stderr.push_str(&String::from_utf8_lossy(&add_remote.stderr));
        if !add_remote.status.success() {
            return Ok((false, stdout, stderr));
        }
        let push = run_git(
            &dir_for_task,
            &["push", "-u", "origin", &branch_for_task, "--force"],
            &envs,
        )?;
        stdout.push_str(&String::from_utf8_lossy(&push.stdout));
        stderr.push_str(&String::from_utf8_lossy(&push.stderr));
        Ok((push.status.success(), stdout, stderr))
    })
    .await
    .map_err(|e| ApiError::Internal(format!("Task panicked: {}", e)))?
    .map_err(|e| ApiError::Internal(format!("Git command failed: {}", e)))?;

    if let Some(p) = askpass_path.as_ref() {
        let _ = std::fs::remove_file(p);
    }

    let (success, stdout, stderr) = result;

    let scrub = |s: &str| -> String {
        let mut out = s.to_string();
        if let Some(t) = token.as_deref() {
            if !t.is_empty() {
                out = out.replace(t, "***");
            }
        }
        out
    };

    if !success {
        let safe_stderr = scrub(&stderr);
        error!(project_id = %project_id, stderr = %safe_stderr, "GitHub push failed");
        return Ok(Json(json!({
            "status": "error",
            "error": safe_stderr,
        })));
    }

    info!(project_id = %project_id, repo = %body.repo_url, branch = %branch, "Pushed to GitHub");

    Ok(Json(json!({
        "status": "ok",
        "repo_url": body.repo_url,
        "branch": branch,
        "output": scrub(&stdout),
    })))
}

// ---------------------------------------------------------------------------
// POST /projects/:id/app/deploy — deploy to a remote server via rsync+ssh
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct DeployServerReq {
    /// e.g. "user@host:/var/www/myapp"
    pub target: String,
    /// Optional SSH key path (default: uses ssh-agent or ~/.ssh/id_rsa)
    pub ssh_key: Option<String>,
    /// Optional: run a command on the server after deploy (e.g. "cd /var/www/myapp && npm install && pm2 restart app")
    pub post_deploy_cmd: Option<String>,
}

fn is_valid_deploy_target(target: &str) -> bool {
    // Accept strictly user@host:/path or host:/path.
    // No shell metacharacters; host part is DNS-safe.
    if target.is_empty() || target.len() > 1024 {
        return false;
    }
    if target.contains(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                ';' | '&' | '|' | '`' | '$' | '<' | '>' | '(' | ')' | '*' | '?' | '\\' | '"' | '\''
            )
    }) {
        return false;
    }
    let (hostpart, path) = match target.split_once(':') {
        Some((h, p)) => (h, p),
        None => return false,
    };
    if path.is_empty() {
        return false;
    }
    let host = hostpart
        .rsplit_once('@')
        .map(|(_, h)| h)
        .unwrap_or(hostpart);
    !host.is_empty()
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

fn is_valid_ssh_key_path(path: &str) -> bool {
    if path.is_empty() || path.len() > 1024 {
        return false;
    }
    !path.contains(|c: char| {
        c.is_control()
            || matches!(
                c,
                ';' | '&' | '|' | '`' | '$' | '<' | '>' | '(' | ')' | '*' | '?' | '"' | '\''
            )
    })
}

pub async fn deploy_to_server(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(body): Json<DeployServerReq>,
) -> ApiResult<Json<serde_json::Value>> {
    access
        .require_scope(&crate::security::auth::Scope::DeployManage)
        .map_err(|_| ApiError::Forbidden("deploy:manage scope required".into()))?;
    let project_id = access.project_id.clone();
    let output_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");
    if !output_dir.exists() {
        return Err(ApiError::BadRequest("No generated code found".into()));
    }

    if !is_valid_deploy_target(&body.target) {
        return Err(ApiError::BadRequest(
            "Invalid target (expected user@host:/path)".into(),
        ));
    }
    if let Some(key) = body.ssh_key.as_deref() {
        if !is_valid_ssh_key_path(key) {
            return Err(ApiError::BadRequest("Invalid ssh_key path".into()));
        }
    }

    let src_arg = {
        let mut s = output_dir.display().to_string();
        if !s.ends_with('/') {
            s.push('/');
        }
        s
    };
    let target_for_task = body.target.clone();
    let ssh_key_for_task = body.ssh_key.clone();

    // Forensic audit: every deploy attempt is recorded into the signed
    // Merkle chain BEFORE any external process runs. The target string
    // (user@host:/path) is logged; ssh_key path is recorded as boolean
    // presence only so the audit trail does not point at a private key file.
    app.audit_log
        .append(
            "system",
            "app_deploy_attempt",
            serde_json::json!({
                "project_id": project_id,
                "target": body.target,
                "ssh_key_supplied": body.ssh_key.is_some(),
                "post_deploy_cmd": body.post_deploy_cmd.is_some(),
                "actor": access.user_id.clone(),
                "tenant": access.tenant_id.clone(),
            }),
            &app.audit_keypair,
        )
        .await;

    let result = tokio::task::spawn_blocking(move || -> std::io::Result<std::process::Output> {
        let mut cmd = build_deploy_command("rsync");
        cmd.args([
            "-avz",
            "--exclude",
            "node_modules",
            "--exclude",
            ".next",
            "--exclude",
            ".git",
        ]);
        if let Some(key) = ssh_key_for_task.as_deref() {
            cmd.arg("-e").arg(format!("ssh -i {}", key));
        }
        cmd.arg(&src_arg).arg(&target_for_task).output()
    })
    .await
    .map_err(|e| ApiError::Internal(format!("Task panicked: {}", e)))?
    .map_err(|e| ApiError::Internal(format!("rsync failed: {}", e)))?;

    let stdout = String::from_utf8_lossy(&result.stdout).to_string();
    let stderr = String::from_utf8_lossy(&result.stderr).to_string();

    if !result.status.success() {
        error!(project_id = %project_id, stderr = %stderr, "Deploy rsync failed");
        return Ok(Json(json!({
            "status": "error",
            "step": "rsync",
            "error": stderr,
        })));
    }

    // Run post-deploy command if provided. The remote command is sent to ssh
    // as a single argv element; ssh joins them on the remote side, but we
    // never invoke a local shell, so no local injection is possible. The
    // remote shell will still parse the command string — callers with
    // deploy:manage scope are trusted to the extent that their local shell
    // user is, which is the standard rsync/ssh contract.
    if let Some(ref cmd_str) = body.post_deploy_cmd {
        if cmd_str.len() > 8192 || cmd_str.contains('\0') {
            return Err(ApiError::BadRequest("Invalid post_deploy_cmd".into()));
        }
        let ssh_host = body
            .target
            .split(':')
            .next()
            .unwrap_or(&body.target)
            .to_string();
        let ssh_key_for_post = body.ssh_key.clone();
        let cmd_for_post = cmd_str.clone();

        let post_result =
            tokio::task::spawn_blocking(move || -> std::io::Result<std::process::Output> {
                let mut cmd = build_deploy_command("ssh");
                cmd.arg("-o").arg("BatchMode=yes");
                cmd.arg("-o").arg("StrictHostKeyChecking=accept-new");
                if let Some(key) = ssh_key_for_post.as_deref() {
                    cmd.arg("-i").arg(key);
                }
                cmd.arg(&ssh_host).arg(&cmd_for_post).output()
            })
            .await
            .map_err(|e| ApiError::Internal(format!("Task panicked: {}", e)))?
            .map_err(|e| ApiError::Internal(format!("SSH command failed: {}", e)))?;

        let post_stdout = String::from_utf8_lossy(&post_result.stdout).to_string();
        let post_stderr = String::from_utf8_lossy(&post_result.stderr).to_string();

        if !post_result.status.success() {
            return Ok(Json(json!({
                "status": "partial",
                "rsync": "ok",
                "post_deploy_error": post_stderr,
            })));
        }

        info!(project_id = %project_id, target = %body.target, "Deployed with post-deploy command");

        return Ok(Json(json!({
            "status": "ok",
            "target": body.target,
            "rsync_output": stdout,
            "post_deploy_output": post_stdout,
        })));
    }

    info!(project_id = %project_id, target = %body.target, "Deployed via rsync");

    Ok(Json(json!({
        "status": "ok",
        "target": body.target,
        "rsync_output": stdout,
    })))
}

fn collect_files(
    root: &std::path::Path,
    current: &std::path::Path,
    files: &mut Vec<serde_json::Value>,
) {
    let Ok(entries) = std::fs::read_dir(current) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // Skip hidden files/dirs and node_modules
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name == "node_modules" || name == ".next" {
            continue;
        }
        // Refuse to follow symlinks — see add_dir_to_zip rationale.
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            tracing::warn!(path = %path.display(), "skipping symlink in file listing");
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .display()
            .to_string();
        if file_type.is_dir() {
            files.push(json!({ "path": relative, "type": "dir", "name": name }));
            collect_files(root, &path, files);
        } else if file_type.is_file() {
            let size = path.metadata().map(|m| m.len()).unwrap_or(0);
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_string();
            files.push(json!({ "path": relative, "type": "file", "name": name, "size": size, "extension": ext }));
        }
    }
}

// ---------------------------------------------------------------------------
// Auto-start helper (called from oneshot pipeline after generation completes)
// ---------------------------------------------------------------------------

/// Automatically start the generated app's dev server after code generation.
/// This is the consumer-friendly "zero-click preview" experience.
pub async fn auto_start_app(app: &Arc<AppState>, project_id: &str) -> anyhow::Result<()> {
    let output_dir = app
        .data_dir
        .join("projects")
        .join(project_id)
        .join("generated");

    if !output_dir.join("package.json").exists() {
        return Ok(()); // No package.json, nothing to start
    }

    // Create instance record
    let (instance_id, port) = {
        let db = app.db.lock().await;
        let svc = AppRunnerService::new(&db);

        // Skip if already running
        if svc
            .get_running_instance(project_id)
            .ok()
            .flatten()
            .is_some()
        {
            return Ok(());
        }

        let port = svc.next_available_port()?;
        let instance = svc.create_instance(
            project_id,
            port,
            output_dir.to_str().unwrap_or_default(),
            false, // no sandbox for auto-start
        )?;
        (instance.id, instance.port)
    };

    // Run npm install + dev server in background
    let app_clone = Arc::clone(app);
    let project_id_owned = project_id.to_string();
    tokio::spawn(async move {
        tracing::info!(project_id = %project_id_owned, port, "Auto-starting app preview");

        // npm install
        {
            let db = app_clone.db.lock().await;
            let svc = AppRunnerService::new(&db);
            svc.create_build_step(&instance_id, "npm_install", "Installing packages...")
                .ok();
        }

        let install_result = npm_install(&output_dir);
        if let Err(e) = install_result {
            tracing::warn!(project_id = %project_id_owned, error = %e, "npm install failed during auto-start");
            let db = app_clone.db.lock().await;
            let svc = AppRunnerService::new(&db);
            svc.update_status(&instance_id, "failed", None, None).ok();
            return;
        }

        // Start dev server
        {
            let db = app_clone.db.lock().await;
            let svc = AppRunnerService::new(&db);
            svc.create_build_step(&instance_id, "dev_server", "Starting your app...")
                .ok();
        }

        match spawn_dev_server(&output_dir, port) {
            Ok(pid) => {
                let db = app_clone.db.lock().await;
                let svc = AppRunnerService::new(&db);
                svc.update_status(&instance_id, "running", Some(pid), None)
                    .ok();
                tracing::info!(project_id = %project_id_owned, port, pid, "App preview auto-started");
            }
            Err(e) => {
                tracing::warn!(project_id = %project_id_owned, error = %e, "Failed to auto-start dev server");
                let db = app_clone.db.lock().await;
                let svc = AppRunnerService::new(&db);
                svc.update_status(&instance_id, "failed", None, None).ok();
            }
        }
    });

    Ok(())
}

// ── Generic deploy endpoint ─────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct DeployProjectReq {
    pub target: String,
    #[serde(flatten)]
    pub opts: serde_json::Value,
}

/// POST /projects/:id/deploy — unified deploy dispatcher
pub async fn deploy_project(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(body): Json<DeployProjectReq>,
) -> ApiResult<Json<serde_json::Value>> {
    access
        .require_scope(&crate::security::auth::Scope::DeployManage)
        .map_err(|_| ApiError::Forbidden("deploy:manage scope required".into()))?;
    let project_id = access.project_id.clone();
    let output_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");
    if !output_dir.exists() {
        return Err(ApiError::BadRequest(
            "No generated code found. Build the project first.".into(),
        ));
    }

    // Strict input validators — anything user-controlled that lands on argv
    // must first pass one of these regexes. This defeats shell-metachar
    // injection even if a future refactor accidentally reintroduces `sh -c`.
    fn is_valid_docker_tag(s: &str) -> bool {
        // name[:tag] where name is lowercase alnum + `._-/` and tag is alnum + `._-`
        static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        let re = RE.get_or_init(|| {
            regex::Regex::new(
                r"^[a-z0-9]([a-z0-9._/-]{0,126}[a-z0-9])?(:[A-Za-z0-9_][A-Za-z0-9._-]{0,127})?$",
            )
            .unwrap()
        });
        re.is_match(s) && !s.is_empty() && s.len() <= 256
    }
    fn is_valid_ssh_host(s: &str) -> bool {
        // Accept `[user@]host` where user/host are restrictive. No whitespace,
        // no shell metachars, no leading dashes (which would look like flags).
        static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        let re = RE.get_or_init(|| {
            regex::Regex::new(r"^(?:[A-Za-z0-9_][A-Za-z0-9._-]{0,63}@)?[A-Za-z0-9]([A-Za-z0-9.-]{0,253}[A-Za-z0-9])?$").unwrap()
        });
        re.is_match(s) && !s.starts_with('-')
    }
    fn is_valid_remote_path(s: &str) -> bool {
        // Absolute POSIX path, no shell metachars, no embedded `..` segments.
        if s.is_empty() || s.len() > 1024 || !s.starts_with('/') {
            return false;
        }
        if s.contains("..") {
            return false;
        }
        s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '.'))
    }

    match body.target.as_str() {
        "docker" => {
            let tag = body
                .opts
                .get("image_tag")
                .and_then(|v| v.as_str())
                .unwrap_or("nexus-app:latest");
            if !is_valid_docker_tag(tag) {
                return Ok(Json(json!({
                    "status": "error",
                    "logs": "Invalid image_tag: must match docker tag format [a-z0-9._/-]+[:tag]"
                })));
            }
            let dockerfile = output_dir.join("Dockerfile");
            if !dockerfile.exists() {
                return Ok(Json(json!({
                    "status": "error",
                    "logs": "No Dockerfile found in generated code. Add a Dockerfile first."
                })));
            }
            let tag_owned = tag.to_string();
            let cwd = output_dir.clone();
            let result = tokio::task::spawn_blocking(move || {
                build_deploy_command("docker")
                    .args(["build", "-t", tag_owned.as_str(), "."])
                    .current_dir(&cwd)
                    .output()
            })
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .map_err(|e| ApiError::Internal(e.to_string()))?;

            let logs = format!(
                "{}\n{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
            let status = if result.status.success() {
                "success"
            } else {
                "error"
            };
            Ok(Json(json!({ "status": status, "logs": logs })))
        }
        "ssh" => {
            let host = body.opts.get("host").and_then(|v| v.as_str()).unwrap_or("");
            let path = body
                .opts
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("/var/www/app");
            if host.is_empty() {
                return Ok(Json(json!({
                    "status": "error",
                    "logs": "SSH host is required"
                })));
            }
            if !is_valid_ssh_host(host) {
                return Ok(Json(json!({
                    "status": "error",
                    "logs": "Invalid host: must be [user@]hostname with no shell metacharacters"
                })));
            }
            if !is_valid_remote_path(path) {
                return Ok(Json(json!({
                    "status": "error",
                    "logs": "Invalid path: must be an absolute POSIX path with no shell metacharacters or .. segments"
                })));
            }
            // rsync's SRC argument needs a trailing slash to copy contents.
            let mut src = output_dir.clone().into_os_string();
            src.push("/");
            let target = format!("{}:{}", host, path);
            let result = tokio::task::spawn_blocking(move || {
                build_deploy_command("rsync")
                    .arg("-avz")
                    .arg("--exclude")
                    .arg("node_modules")
                    .arg("--exclude")
                    .arg(".next")
                    .arg("--exclude")
                    .arg(".git")
                    .arg("--")
                    .arg(&src)
                    .arg(&target)
                    .output()
            })
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .map_err(|e| ApiError::Internal(e.to_string()))?;

            let logs = format!(
                "{}\n{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
            let status = if result.status.success() {
                "success"
            } else {
                "error"
            };
            Ok(Json(
                json!({ "status": status, "logs": logs, "url": format!("ssh://{}", host) }),
            ))
        }
        "kubernetes" | "k8s" => Ok(Json(json!({
            "status": "success",
            "logs": "Kubernetes manifests generated in deploy/k8s/. Apply with: kubectl apply -k deploy/k8s/",
            "url": null
        }))),
        "vercel" => {
            let cwd = output_dir.clone();
            let result = tokio::task::spawn_blocking(move || {
                build_deploy_command("npx")
                    .args(["vercel", "--yes"])
                    .current_dir(&cwd)
                    .output()
            })
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .map_err(|e| ApiError::Internal(e.to_string()))?;

            let mut logs = String::from_utf8_lossy(&result.stdout).to_string();
            logs.push_str(&String::from_utf8_lossy(&result.stderr));
            let url = logs.lines().last().map(|l| l.trim().to_string());
            let status = if result.status.success() {
                "success"
            } else {
                "error"
            };
            Ok(Json(json!({ "status": status, "logs": logs, "url": url })))
        }
        "railway" => {
            let cwd = output_dir.clone();
            let result = tokio::task::spawn_blocking(move || {
                build_deploy_command("railway")
                    .arg("up")
                    .current_dir(&cwd)
                    .output()
            })
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .map_err(|e| ApiError::Internal(e.to_string()))?;

            let mut logs = String::from_utf8_lossy(&result.stdout).to_string();
            logs.push_str(&String::from_utf8_lossy(&result.stderr));
            let status = if result.status.success() {
                "success"
            } else {
                "error"
            };
            Ok(Json(json!({ "status": status, "logs": logs })))
        }
        other => Ok(Json(json!({
            "status": "error",
            "logs": format!("Unknown deploy target: '{}'. Supported: docker, ssh, kubernetes, vercel, railway", other)
        }))),
    }
}
