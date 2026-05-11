//! Local app runner — spawns generated apps as child processes or Docker containers.
//!
//! After `CodeGenMaterializer` produces a project directory, this module:
//! 1. Runs `npm install` to install dependencies (bare-metal) or builds a Docker image (sandbox)
//! 2. Spawns `npm run dev` or `docker run` on an available port
//! 3. Health-checks until the app responds
//! 4. Tracks running processes/containers for cleanup

use std::path::Path;
use std::process::Command;

use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;

/// Status of a running app instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInstance {
    pub id: String,
    pub project_id: String,
    pub port: u16,
    pub pid: Option<u32>,
    /// "installing" | "starting" | "running" | "stopped" | "failed" | "crashed"
    pub status: String,
    pub output_dir: String,
    pub error: Option<String>,
    pub started_at: String,
    pub stopped_at: Option<String>,
    /// Whether this instance runs inside a Docker sandbox.
    pub sandbox: bool,
    /// Docker container ID (only set when sandbox = true).
    pub container_id: Option<String>,
    /// Build/install logs captured during startup.
    #[serde(default)]
    pub logs: String,
    /// Health check status: "unknown" | "healthy" | "unhealthy"
    #[serde(default = "default_health_status")]
    pub health_status: String,
    /// Number of auto-restarts performed.
    #[serde(default)]
    pub restart_count: i32,
    /// Whether auto-restart on crash is enabled.
    #[serde(default = "default_auto_restart")]
    pub auto_restart: bool,
}

fn default_health_status() -> String {
    "unknown".to_string()
}

fn default_auto_restart() -> bool {
    true
}

/// A build step within the app startup process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildStep {
    pub id: String,
    pub instance_id: String,
    pub step_name: String,
    pub label: String,
    pub status: String, // "pending" | "running" | "completed" | "failed" | "skipped"
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub duration_ms: Option<i64>,
    pub output: String,
    pub error: Option<String>,
}

/// An environment variable for a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVar {
    pub id: String,
    pub project_id: String,
    pub key: String,
    pub value: String,
    pub created_at: String,
}

pub struct AppRunnerService<'c> {
    conn: &'c Connection,
}

impl<'c> AppRunnerService<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Find the next available port starting from 4100.
    ///
    /// Caps the allocation at 5100 (1000-port window) so a runaway spawn
    /// loop cannot exhaust the ephemeral-port space or overflow the u16
    /// bound. When every port in the window is in use, callers get a
    /// clean error instead of wrap-around or panic. Ports from stopped
    /// instances are recycled because the query only considers
    /// running-style statuses.
    pub fn next_available_port(&self) -> Result<u16> {
        const PORT_BASE: u16 = 4100;
        const PORT_CEILING: u16 = 5100;

        let max_port: Option<i64> = self
            .conn
            .query_row(
                "SELECT MAX(port) FROM app_instances \
                 WHERE status IN ('running', 'starting', 'installing') \
                   AND port >= ?1 AND port < ?2",
                [PORT_BASE as i64, PORT_CEILING as i64],
                |row| row.get(0),
            )
            .unwrap_or(None);

        let candidate = max_port
            .map(|p| (p as u16).saturating_add(1))
            .unwrap_or(PORT_BASE);

        if candidate < PORT_CEILING {
            return Ok(candidate);
        }

        // Window is contiguous-full from above; scan for a recycled slot.
        let mut stmt = self.conn.prepare(
            "SELECT port FROM app_instances \
             WHERE status IN ('running', 'starting', 'installing') \
               AND port >= ?1 AND port < ?2",
        )?;
        let used: std::collections::HashSet<i64> = stmt
            .query_map([PORT_BASE as i64, PORT_CEILING as i64], |row| row.get(0))?
            .filter_map(std::result::Result::ok)
            .collect();
        for p in PORT_BASE..PORT_CEILING {
            if !used.contains(&(p as i64)) {
                return Ok(p);
            }
        }
        Err(crate::error::StoreError::Msg(format!(
            "app port window exhausted ({}..{})",
            PORT_BASE, PORT_CEILING
        )))
    }

    /// Record a new app instance (before starting).
    pub fn create_instance(
        &self,
        project_id: &str,
        port: u16,
        output_dir: &str,
        sandbox: bool,
    ) -> Result<AppInstance> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO app_instances (id, project_id, port, status, output_dir, started_at, sandbox)
             VALUES (?1, ?2, ?3, 'installing', ?4, ?5, ?6)",
            params![id, project_id, port as i64, output_dir, now, sandbox as i64],
        )?;
        Ok(AppInstance {
            id,
            project_id: project_id.to_string(),
            port,
            pid: None,
            status: "installing".to_string(),
            output_dir: output_dir.to_string(),
            error: None,
            started_at: now,
            stopped_at: None,
            sandbox,
            container_id: None,
            logs: String::new(),
            health_status: "unknown".to_string(),
            restart_count: 0,
            auto_restart: true,
        })
    }

    /// Update instance status and optionally set PID.
    pub fn update_status(
        &self,
        instance_id: &str,
        status: &str,
        pid: Option<u32>,
        error: Option<&str>,
    ) -> Result<()> {
        let stopped_at = if status == "stopped" || status == "failed" {
            Some(Utc::now().to_rfc3339())
        } else {
            None
        };
        self.conn.execute(
            "UPDATE app_instances SET status = ?1, pid = ?2, error = ?3, stopped_at = ?4
             WHERE id = ?5",
            params![status, pid.map(|p| p as i64), error, stopped_at, instance_id],
        )?;
        Ok(())
    }

    /// Set the Docker container ID for a sandbox instance.
    pub fn set_container_id(
        &self,
        instance_id: &str,
        container_id: &str,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE app_instances SET container_id = ?1 WHERE id = ?2",
            params![container_id, instance_id],
        )?;
        Ok(())
    }

    fn row_to_instance(row: &rusqlite::Row) -> rusqlite::Result<AppInstance> {
        Ok(AppInstance {
            id: row.get(0)?,
            project_id: row.get(1)?,
            port: row.get::<_, i64>(2)? as u16,
            pid: row.get::<_, Option<i64>>(3)?.map(|p| p as u32),
            status: row.get(4)?,
            output_dir: row.get(5)?,
            error: row.get(6)?,
            started_at: row.get(7)?,
            stopped_at: row.get(8)?,
            sandbox: row.get::<_, i64>(9).unwrap_or(0) != 0,
            container_id: row.get(10)?,
            logs: row.get::<_, String>(11).unwrap_or_default(),
            health_status: row.get::<_, String>(12).unwrap_or_else(|_| "unknown".to_string()),
            restart_count: row.get::<_, i64>(13).unwrap_or(0) as i32,
            auto_restart: row.get::<_, i64>(14).unwrap_or(1) != 0,
        })
    }

    /// Get the running instance for a project (if any).
    pub fn get_running_instance(&self, project_id: &str) -> Result<Option<AppInstance>> {
        let result = self.conn.query_row(
            "SELECT id, project_id, port, pid, status, output_dir, error, started_at, stopped_at, sandbox, container_id, logs, health_status, restart_count, auto_restart
             FROM app_instances WHERE project_id = ?1 AND status IN ('running', 'starting', 'installing')
             ORDER BY started_at DESC LIMIT 1",
            params![project_id],
            Self::row_to_instance,
        );
        match result {
            Ok(instance) => Ok(Some(instance)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Append a log line to an instance's logs.
    pub fn append_logs(&self, instance_id: &str, line: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE app_instances SET logs = logs || ?1 WHERE id = ?2",
            params![format!("{}\n", line), instance_id],
        )?;
        Ok(())
    }

    /// List all instances for a project.
    pub fn list_instances(&self, project_id: &str) -> Result<Vec<AppInstance>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, port, pid, status, output_dir, error, started_at, stopped_at, sandbox, container_id, logs, health_status, restart_count, auto_restart
             FROM app_instances WHERE project_id = ?1 ORDER BY started_at DESC",
        )?;
        let rows = stmt.query_map(params![project_id], Self::row_to_instance)?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    // -----------------------------------------------------------------------
    // Build steps
    // -----------------------------------------------------------------------

    /// Create a new build step in "pending" status.
    pub fn create_build_step(
        &self,
        instance_id: &str,
        step_name: &str,
        label: &str,
    ) -> Result<BuildStep> {
        let id = Uuid::new_v4().to_string();
        self.conn.execute(
            "INSERT INTO app_build_steps (id, instance_id, step_name, label, status) VALUES (?1, ?2, ?3, ?4, 'pending')",
            params![id, instance_id, step_name, label],
        )?;
        Ok(BuildStep {
            id,
            instance_id: instance_id.to_string(),
            step_name: step_name.to_string(),
            label: label.to_string(),
            status: "pending".to_string(),
            started_at: None,
            completed_at: None,
            duration_ms: None,
            output: String::new(),
            error: None,
        })
    }

    /// Update a build step's status and optionally append output or set error.
    pub fn update_build_step(
        &self,
        step_id: &str,
        status: &str,
        output: Option<&str>,
        error: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let started_at = if status == "running" {
            Some(now.clone())
        } else {
            None
        };
        if let Some(out) = output {
            self.conn.execute(
                "UPDATE app_build_steps SET status = ?1, output = output || ?2, error = COALESCE(?3, error), started_at = COALESCE(?4, started_at) WHERE id = ?5",
                params![status, out, error, started_at, step_id],
            )?;
        } else {
            self.conn.execute(
                "UPDATE app_build_steps SET status = ?1, error = COALESCE(?2, error), started_at = COALESCE(?3, started_at) WHERE id = ?4",
                params![status, error, started_at, step_id],
            )?;
        }
        Ok(())
    }

    /// Mark a build step as completed or failed, computing duration.
    pub fn complete_build_step(
        &self,
        step_id: &str,
        success: bool,
        output: Option<&str>,
        error: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let status = if success { "completed" } else { "failed" };
        self.conn.execute(
            "UPDATE app_build_steps SET status = ?1, completed_at = ?2, output = output || COALESCE(?3, ''), error = COALESCE(?4, error),
             duration_ms = CAST((julianday(?2) - julianday(COALESCE(started_at, ?2))) * 86400000 AS INTEGER)
             WHERE id = ?5",
            params![status, now, output, error, step_id],
        )?;
        Ok(())
    }

    /// List all build steps for an instance, ordered by rowid (insertion order).
    pub fn list_build_steps(&self, instance_id: &str) -> Result<Vec<BuildStep>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, instance_id, step_name, label, status, started_at, completed_at, duration_ms, output, error
             FROM app_build_steps WHERE instance_id = ?1 ORDER BY rowid ASC",
        )?;
        let rows = stmt.query_map(params![instance_id], |row| {
            Ok(BuildStep {
                id: row.get(0)?,
                instance_id: row.get(1)?,
                step_name: row.get(2)?,
                label: row.get(3)?,
                status: row.get(4)?,
                started_at: row.get(5)?,
                completed_at: row.get(6)?,
                duration_ms: row.get(7)?,
                output: row.get::<_, String>(8).unwrap_or_default(),
                error: row.get(9)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    // -----------------------------------------------------------------------
    // Environment variables
    // -----------------------------------------------------------------------

    /// Set (upsert) an environment variable for a project.
    pub fn set_env_var(&self, project_id: &str, key: &str, value: &str) -> Result<EnvVar> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO app_env_vars (id, project_id, key, value, created_at) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(project_id, key) DO UPDATE SET value = excluded.value",
            params![id, project_id, key, value, now],
        )?;
        // Return the current row (may be the old id on upsert)
        let row = self.conn.query_row(
            "SELECT id, project_id, key, value, created_at FROM app_env_vars WHERE project_id = ?1 AND key = ?2",
            params![project_id, key],
            |row| {
                Ok(EnvVar {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    key: row.get(2)?,
                    value: row.get(3)?,
                    created_at: row.get(4)?,
                })
            },
        )?;
        Ok(row)
    }

    /// List all environment variables for a project.
    pub fn list_env_vars(&self, project_id: &str) -> Result<Vec<EnvVar>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, key, value, created_at FROM app_env_vars WHERE project_id = ?1 ORDER BY key ASC",
        )?;
        let rows = stmt.query_map(params![project_id], |row| {
            Ok(EnvVar {
                id: row.get(0)?,
                project_id: row.get(1)?,
                key: row.get(2)?,
                value: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Delete an environment variable.
    pub fn delete_env_var(&self, project_id: &str, key: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM app_env_vars WHERE project_id = ?1 AND key = ?2",
            params![project_id, key],
        )?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Health & auto-restart
    // -----------------------------------------------------------------------

    /// Update the health status of an instance.
    pub fn update_health_status(&self, instance_id: &str, status: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE app_instances SET health_status = ?1 WHERE id = ?2",
            params![status, instance_id],
        )?;
        Ok(())
    }

    /// Increment the restart count and return the new value.
    pub fn increment_restart_count(&self, instance_id: &str) -> Result<i32> {
        self.conn.execute(
            "UPDATE app_instances SET restart_count = restart_count + 1 WHERE id = ?1",
            params![instance_id],
        )?;
        let count: i64 = self.conn.query_row(
            "SELECT restart_count FROM app_instances WHERE id = ?1",
            params![instance_id],
            |row| row.get(0),
        )?;
        Ok(count as i32)
    }

    /// Enable or disable auto-restart for an instance.
    pub fn set_auto_restart(&self, instance_id: &str, enabled: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE app_instances SET auto_restart = ?1 WHERE id = ?2",
            params![enabled as i64, instance_id],
        )?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
//  Bare-metal helpers (existing)
// ---------------------------------------------------------------------------

/// Check if npm is available on the system.
pub fn check_npm_available() -> std::result::Result<(), String> {
    match Command::new("npm").arg("--version").output() {
        Ok(output) if output.status.success() => Ok(()),
        Ok(_) => Err("npm is installed but returned an error. Check your Node.js installation.".into()),
        Err(_) => Err(
            "npm not found. Please install Node.js from https://nodejs.org (includes npm)."
                .into(),
        ),
    }
}

/// Run `npm install` in the given directory.
/// Returns (stdout+stderr combined, success).
///
/// Bounded by a hard timeout so a hung `npm install` (network stall, lockfile
/// contention, post-install script deadlock) cannot block auto-start
/// indefinitely. The timeout is overridable via `NEXUS_NPM_INSTALL_TIMEOUT_SECS`.
pub fn npm_install(project_dir: &Path) -> std::result::Result<String, String> {
    check_npm_available()?;

    let timeout_secs: u64 = std::env::var("NEXUS_NPM_INSTALL_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300); // 5 minutes default

    let mut child = Command::new("npm")
        .arg("install")
        .arg("--no-audit")
        .arg("--no-fund")
        .current_dir(project_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn npm install: {}", e))?;

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    let child_pid = child.id();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let out = child.wait_with_output().unwrap_or_else(|_| std::process::Output {
                    status,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                });
                let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                let combined = format!("{}{}", stdout, stderr);
                return if status.success() {
                    Ok(combined)
                } else {
                    Err(combined)
                };
            }
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    // Kill the hung process — best-effort.
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "npm install timed out after {}s (pid {}). \
                         Raise NEXUS_NPM_INSTALL_TIMEOUT_SECS or check your network.",
                        timeout_secs, child_pid
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
            Err(e) => return Err(format!("npm install wait failed: {e}")),
        }
    }
}

/// Spawn a dev server for the generated project.
///
/// Detects the project type from package.json:
/// - If `next` is a dependency → `npx next dev --port <port>`
/// - If `scripts.dev` exists → `npm run dev` with PORT env var
/// - If `scripts.start` exists → `npm start` with PORT env var
/// - Fallback → `npx next dev --port <port>`
///
/// Returns the child's PID on success.
pub fn spawn_dev_server(project_dir: &Path, port: u16) -> std::result::Result<u32, String> {
    let (cmd, args, env_port) = detect_start_command(project_dir, port);

    let mut command = Command::new(&cmd);
    for arg in &args {
        command.arg(arg);
    }
    if let Some((key, val)) = &env_port {
        command.env(key, val);
    }

    // Capture stdout + stderr into a rotating log file under the project dir
    // so users can see why their app died. `Stdio::null()` previously silently
    // discarded every error from `next dev` / `node`, making crashes
    // impossible to diagnose from the UI.
    let log_dir = project_dir.join(".nexus");
    std::fs::create_dir_all(&log_dir)
        .map_err(|e| format!("Failed to create log dir: {e}"))?;
    let log_path = log_dir.join("dev-server.log");
    let stdout_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("Failed to open dev-server.log: {e}"))?;
    let stderr_file = stdout_file
        .try_clone()
        .map_err(|e| format!("Failed to clone log handle: {e}"))?;

    let child = command
        .current_dir(project_dir)
        .stdout(std::process::Stdio::from(stdout_file))
        .stderr(std::process::Stdio::from(stderr_file))
        .spawn()
        .map_err(|e| format!("Failed to spawn dev server ({}): {}", cmd, e))?;

    Ok(child.id())
}

/// Detect how to start the generated app based on its package.json.
fn detect_start_command(project_dir: &Path, port: u16) -> (String, Vec<String>, Option<(String, String)>) {
    let pkg_path = project_dir.join("package.json");
    if let Ok(content) = std::fs::read_to_string(&pkg_path) {
        if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content) {
            // Check if it's a Next.js project
            let has_next = pkg.get("dependencies")
                .and_then(|d| d.get("next"))
                .is_some();

            if has_next {
                return (
                    "npx".into(),
                    vec!["next".into(), "dev".into(), "--port".into(), port.to_string()],
                    None,
                );
            }

            // Check scripts in priority order
            let scripts = pkg.get("scripts");

            // Check for "dev" script
            if let Some(dev_script) = scripts.and_then(|s| s.get("dev")).and_then(|s| s.as_str()) {
                // If dev script already contains next, use npx next dev
                if dev_script.contains("next") {
                    return (
                        "npx".into(),
                        vec!["next".into(), "dev".into(), "--port".into(), port.to_string()],
                        None,
                    );
                }
                return (
                    "npm".into(),
                    vec!["run".into(), "dev".into()],
                    Some(("PORT".into(), port.to_string())),
                );
            }

            // Check for "start" script
            if scripts.and_then(|s| s.get("start")).is_some() {
                return (
                    "npm".into(),
                    vec!["start".into()],
                    Some(("PORT".into(), port.to_string())),
                );
            }

            // Check if server.js or index.js exists
            if project_dir.join("server.js").exists() {
                return (
                    "node".into(),
                    vec!["server.js".into()],
                    Some(("PORT".into(), port.to_string())),
                );
            }
            if project_dir.join("index.js").exists() {
                return (
                    "node".into(),
                    vec!["index.js".into()],
                    Some(("PORT".into(), port.to_string())),
                );
            }
        }
    }

    // Fallback: assume Next.js
    (
        "npx".into(),
        vec!["next".into(), "dev".into(), "--port".into(), port.to_string()],
        None,
    )
}

/// Stop a process by PID. Best-effort — returns Ok even if process already exited.
pub fn stop_process(pid: u32) -> std::result::Result<(), String> {
    #[cfg(unix)]
    {
        // Send SIGTERM
        let _ = Command::new("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .output();
    }
    #[cfg(not(unix))]
    {
        let _ = Command::new("taskkill")
            .args(&["/PID", &pid.to_string(), "/F"])
            .output();
    }
    Ok(())
}

// ---------------------------------------------------------------------------
//  Docker sandbox helpers
// ---------------------------------------------------------------------------

/// Check if Docker is available on the system.
pub fn check_docker_available() -> std::result::Result<(), String> {
    match Command::new("docker").arg("version").arg("--format").arg("{{.Server.Version}}").output() {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("Docker is installed but returned an error: {}. Is the Docker daemon running?", stderr.trim()))
        }
        Err(_) => Err(
            "Docker not found. Please install Docker from https://docs.docker.com/get-docker/"
                .into(),
        ),
    }
}

/// Detect the project type from files in the directory.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProjectType {
    NextJs,
    Python,
    Unknown,
}

pub fn detect_project_type(project_dir: &Path) -> ProjectType {
    if project_dir.join("package.json").exists() {
        ProjectType::NextJs
    } else if project_dir.join("requirements.txt").exists() || project_dir.join("app.py").exists() {
        ProjectType::Python
    } else {
        ProjectType::Unknown
    }
}

/// Write a Dockerfile into the project directory, auto-detecting the framework.
pub fn write_sandbox_dockerfile(project_dir: &Path) -> std::result::Result<ProjectType, String> {
    let project_type = detect_project_type(project_dir);

    let (dockerfile, dockerignore) = match project_type {
        ProjectType::NextJs => (
            r#"FROM node:20-alpine
WORKDIR /app
COPY package.json package-lock.json* ./
RUN npm install --no-audit --no-fund
COPY . .
EXPOSE 3000
ENV PORT=3000
ENV NODE_ENV=development
CMD ["npx", "next", "dev", "--port", "3000"]
"#.to_string(),
            "node_modules\n.next\n.git\nDockerfile\n.dockerignore\n".to_string(),
        ),
        ProjectType::Python => {
            // Detect if it's Flask (app.py) or generic Python
            let has_app_py = project_dir.join("app.py").exists();
            let cmd = if has_app_py {
                r#"CMD ["python", "app.py"]"#
            } else {
                r#"CMD ["python", "-m", "http.server", "3000"]"#
            };
            (
                format!(
                    r#"FROM python:3.12-slim
WORKDIR /app
COPY requirements.txt* ./
RUN if [ -f requirements.txt ]; then pip install --no-cache-dir -r requirements.txt; fi
COPY . .
EXPOSE 3000
ENV PORT=3000
ENV FLASK_APP=app.py
ENV FLASK_RUN_HOST=0.0.0.0
ENV FLASK_RUN_PORT=3000
{cmd}
"#
                ),
                "__pycache__\n*.pyc\n.git\nvenv\n.venv\nDockerfile\n.dockerignore\ndata.db\n"
                    .to_string(),
            )
        }
        ProjectType::Unknown => {
            return Err(
                "Cannot detect project type. Expected package.json (Node.js) or requirements.txt/app.py (Python)."
                    .into(),
            );
        }
    };

    std::fs::write(project_dir.join("Dockerfile"), &dockerfile)
        .map_err(|e| format!("Failed to write Dockerfile: {}", e))?;
    std::fs::write(project_dir.join(".dockerignore"), &dockerignore)
        .map_err(|e| format!("Failed to write .dockerignore: {}", e))?;
    Ok(project_type)
}

/// Build a Docker image for the sandbox. Returns the detected project type.
pub fn docker_build(project_dir: &Path, image_tag: &str) -> std::result::Result<ProjectType, String> {
    check_docker_available()?;
    let project_type = write_sandbox_dockerfile(project_dir)?;

    let output = Command::new("docker")
        .args(["build", "-t", image_tag, "."])
        .current_dir(project_dir)
        .output()
        .map_err(|e| format!("Failed to spawn docker build: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("docker build failed: {}", stderr));
    }
    Ok(project_type)
}

/// Run a Docker container from the given image, mapping `host_port` -> container port 3000.
/// Security constraints:
/// - No new privileges
/// - Memory and CPU limits
/// - PID limit
///
/// Returns the container ID.
pub fn docker_run(
    image_tag: &str,
    host_port: u16,
    container_name: &str,
    project_type: ProjectType,
) -> std::result::Result<String, String> {
    // Remove any stale container with the same name (best-effort)
    let _ = Command::new("docker").args(["rm", "-f", container_name]).output();

    let mut args = vec![
        "run".to_string(),
        "-d".to_string(),
        "--name".to_string(), container_name.to_string(),
        "-p".to_string(), format!("{}:3000", host_port),
        // Sandbox security constraints
        "--security-opt".to_string(), "no-new-privileges:true".to_string(),
        "--memory".to_string(), "512m".to_string(),
        "--cpus".to_string(), "1.0".to_string(),
        "--pids-limit".to_string(), "256".to_string(),
    ];

    // Add tmpfs mounts based on project type
    match project_type {
        ProjectType::NextJs => {
            args.extend([
                "--tmpfs".to_string(), "/tmp:rw,exec,size=128m".to_string(),
            ]);
        }
        ProjectType::Python => {
            args.extend([
                "--tmpfs".to_string(), "/tmp:rw,noexec,nosuid,size=64m".to_string(),
            ]);
        }
        ProjectType::Unknown => {}
    }

    args.push(image_tag.to_string());

    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let output = Command::new("docker")
        .args(&args_ref)
        .output()
        .map_err(|e| format!("Failed to spawn docker run: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("docker run failed: {}", stderr));
    }

    let container_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(container_id)
}

/// Stop and remove a Docker container by ID or name. Best-effort.
pub fn docker_stop(container_id: &str) -> std::result::Result<(), String> {
    // Stop the container (graceful 10s timeout)
    let _ = Command::new("docker")
        .args(["stop", "-t", "10", container_id])
        .output();
    // Remove it
    let _ = Command::new("docker")
        .args(["rm", "-f", container_id])
        .output();
    Ok(())
}

/// Check if a Docker container is running and healthy.
pub fn docker_inspect_running(container_id: &str) -> std::result::Result<bool, String> {
    let output = Command::new("docker")
        .args(["inspect", "-f", "{{.State.Running}}", container_id])
        .output()
        .map_err(|e| format!("docker inspect failed: {}", e))?;

    if !output.status.success() {
        return Ok(false);
    }

    let running = String::from_utf8_lossy(&output.stdout).trim() == "true";
    Ok(running)
}

/// Get container logs (last N lines). Useful for error diagnostics.
pub fn docker_logs(container_id: &str, tail: u32) -> std::result::Result<String, String> {
    let output = Command::new("docker")
        .args(["logs", "--tail", &tail.to_string(), container_id])
        .output()
        .map_err(|e| format!("docker logs failed: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Ok(format!("{}{}", stdout, stderr))
}
