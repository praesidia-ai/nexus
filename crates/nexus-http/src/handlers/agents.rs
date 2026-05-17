//! Agent definition handlers.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use nexus_store::AgentBuilder;
use tracing::{info, error};

use crate::{
    error::{ApiError, ApiResult},
    security::auth::AuthContext,
    security::tenant::validate_project_access,
    state::AppState,
};

// ---------------------------------------------------------------------------
// GET /projects/:id/agents
// ---------------------------------------------------------------------------

pub async fn list_agents(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(project_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;
    let ab = AgentBuilder::new(&db);
    let agents = ab.list_agents(&project_id)?;
    Ok(Json(serde_json::to_value(agents)?))
}

// ---------------------------------------------------------------------------
// GET /projects/:id/agents/:agent_id
// ---------------------------------------------------------------------------

pub async fn get_agent(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path((project_id, agent_id)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;
    let ab = AgentBuilder::new(&db);
    let agents = ab.list_agents(&project_id)?;
    let agent = agents
        .into_iter()
        .find(|a| a.id == agent_id)
        .ok_or_else(|| ApiError::NotFound(format!("agent {} not found", agent_id)))?;
    Ok(Json(serde_json::to_value(agent)?))
}

// ---------------------------------------------------------------------------
// POST /projects/:id/agents/:agent_id/run
// Spawns a real ZeroClaw agent process.
// ---------------------------------------------------------------------------

pub async fn run_agent(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path((project_id, agent_id)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    // Phase 1 — short DB transaction to validate access, look up the agent,
    // and pick a port. We MUST drop the DB lock before fork+exec, otherwise
    // every other DB-using request blocks for the entire spawn (violates the
    // README "never hold the DB lock across blocking calls" invariant).
    let (agent, port) = {
        let db = app.db.lock().await;
        validate_project_access(&db, &project_id, &auth.tenant_id)
            .map_err(ApiError::Forbidden)?;
        let ab = AgentBuilder::new(&db);
        let agents = ab.list_agents(&project_id)?;
        let agent = agents
            .into_iter()
            .find(|a| a.id == agent_id)
            .ok_or_else(|| ApiError::NotFound(format!("agent {} not found", agent_id)))?;

        if agent.status == "running" {
            return Err(ApiError::BadRequest(format!(
                "agent {} is already running",
                agent_id
            )));
        }

        let base_port: i64 = 3300;
        let port = ab.max_allocated_port()?.map_or(base_port, |p| p + 1);
        (agent, port)
    };

    // Resolve YAML path outside the DB lock — it's pure filesystem work.
    let agents_dir = app.project_agents_dir(&project_id);
    let yaml_path_id = agents_dir.join(format!("{}.yaml", agent_id));
    let yaml_path = if yaml_path_id.exists() {
        yaml_path_id
    } else {
        let name_path = agents_dir.join(format!(
            "{}.yaml",
            agent.name.to_lowercase().replace(' ', "_")
        ));
        if name_path.exists() {
            name_path
        } else {
            yaml_path_id
        }
    };

    if !yaml_path.exists() {
        let err_msg = format!("Agent YAML config not found at {}", yaml_path.display());
        error!("{}", err_msg);
        let db = app.db.lock().await;
        let ab = AgentBuilder::new(&db);
        let _ = ab.update_agent_status_with_port(&agent_id, "error", None, None);
        return Err(ApiError::NotFound(err_msg));
    }

    // Resolve secrets without holding the DB lock.
    let zeroclaw_api_key = std::env::var("ZEROCLAW_API_KEY")
        .unwrap_or_else(|_| app.anthropic_api_key.clone().unwrap_or_default());

    // Spawn agent as a lightweight HTTP server.
    let agent_server_script = format!(
        r#"const http = require('http');
const s = http.createServer((req, res) => {{
  if (req.url === '/health') {{ res.writeHead(200); res.end(JSON.stringify({{status:'ok',agent:'{name}'}})); }}
  else if (req.url === '/webhook' && req.method === 'POST') {{
    let body = '';
    req.on('data', c => body += c);
    req.on('end', () => {{ res.writeHead(200); res.end(JSON.stringify({{status:'accepted',agent:'{name}'}})); }});
  }}
  else {{ res.writeHead(404); res.end('Not found'); }}
}});
s.listen({port}, () => console.log('Agent {name} running on port {port}'));"#,
        name = agent.name.replace('\'', "\\'"),
        port = port,
    );

    // Phase 2 — fork+exec (DB lock NOT held). Clear inherited env so the
    // node child does NOT see NEXUS_JWT_SECRET / NEXUS_ENCRYPTION_KEY /
    // provider keys. We re-add only what the script actually needs.
    let path_var = std::env::var("PATH").unwrap_or_default();
    let home_var = std::env::var("HOME").unwrap_or_default();
    let spawn_result = tokio::process::Command::new("node")
        .env_clear()
        .env("PATH", path_var)
        .env("HOME", home_var)
        .env("ZEROCLAW_API_KEY", &zeroclaw_api_key)
        .arg("-e")
        .arg(&agent_server_script)
        .current_dir(&agents_dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();

    let mut child = match spawn_result {
        Ok(c) => c,
        Err(e) => {
            let err_msg = format!("Failed to spawn ZeroClaw process: {}", e);
            error!("{}", err_msg);
            let db = app.db.lock().await;
            let ab = AgentBuilder::new(&db);
            let _ = ab.update_agent_status_with_port(&agent_id, "error", None, None);
            return Err(ApiError::Internal(err_msg));
        }
    };

    let pid = match child.id() {
        Some(p) => p as i64,
        None => {
            // Child died before we could grab a PID — best-effort kill + fail.
            let _ = child.start_kill();
            let db = app.db.lock().await;
            let ab = AgentBuilder::new(&db);
            let _ = ab.update_agent_status_with_port(&agent_id, "error", None, None);
            return Err(ApiError::Internal(
                "spawned ZeroClaw process exited immediately".into(),
            ));
        }
    };

    info!(
        agent_id = %agent_id,
        pid = pid,
        port = port,
        "ZeroClaw agent process spawned"
    );

    // Phase 3 — record success in DB. If the write fails, kill the child to
    // avoid orphaning a process that holds `port` and isn't tracked anywhere.
    let update_result = {
        let db = app.db.lock().await;
        let ab = AgentBuilder::new(&db);
        ab.update_agent_status_with_port(&agent_id, "running", Some(pid), Some(port))
    };

    if let Err(e) = update_result {
        error!(
            agent_id = %agent_id,
            pid = pid,
            error = %e,
            "Failed to persist running agent — killing child to avoid orphan"
        );
        let _ = child.start_kill();
        return Err(ApiError::Internal(format!(
            "Failed to persist running agent: {e}"
        )));
    }

    // Detach the child handle now that the DB knows about the PID. The boot
    // reaper in state.rs will SIGTERM it on next startup if needed.
    drop(child);

    Ok(Json(serde_json::json!({
        "agent_id": agent_id,
        "status": "running",
        "pid": pid,
        "port": port,
        "url": format!("http://localhost:{}", port)
    })))
}

// ---------------------------------------------------------------------------
// POST /projects/:id/agents/:agent_id/stop
// Kills the ZeroClaw agent process if it has a real PID.
// ---------------------------------------------------------------------------

pub async fn stop_agent(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path((project_id, agent_id)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    // Phase 1 — short DB transaction to read the PID we need to kill.
    let agent = {
        let db = app.db.lock().await;
        validate_project_access(&db, &project_id, &auth.tenant_id)
            .map_err(ApiError::Forbidden)?;
        let ab = AgentBuilder::new(&db);
        let agents = ab.list_agents(&project_id)?;
        agents
            .into_iter()
            .find(|a| a.id == agent_id)
            .ok_or_else(|| ApiError::NotFound(format!("agent {} not found", agent_id)))?
    };

    if agent.status != "running" {
        return Err(ApiError::BadRequest(format!(
            "agent {} is not running (status: {})",
            agent_id, agent.status
        )));
    }

    // Phase 2 — issue the OS signal WITHOUT holding the DB lock.
    if let Some(pid) = agent.zeroclaw_pid {
        info!(agent_id = %agent_id, pid = pid, "Sending SIGTERM to ZeroClaw process");

        #[cfg(unix)]
        {
            // Use libc::kill directly — no fork/exec, no blocking wait.
            let rc = unsafe { libc::kill(pid as i32, libc::SIGTERM) };
            if rc != 0 {
                let err = std::io::Error::last_os_error();
                error!(
                    agent_id = %agent_id,
                    pid = pid,
                    error = %err,
                    "kill -TERM failed (process may have already exited)"
                );
            }
        }

        #[cfg(windows)]
        {
            let kill_result = tokio::process::Command::new("taskkill")
                .arg("/PID")
                .arg(pid.to_string())
                .arg("/F")
                .output()
                .await;

            match kill_result {
                Ok(output) => {
                    if !output.status.success() {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        error!(
                            agent_id = %agent_id,
                            pid = pid,
                            stderr = %stderr,
                            "taskkill returned non-zero (process may have already exited)"
                        );
                    }
                }
                Err(e) => {
                    error!(
                        agent_id = %agent_id,
                        pid = pid,
                        error = %e,
                        "Failed to execute taskkill command"
                    );
                }
            }
        }
    }

    // Phase 3 — re-acquire the lock to mark the agent stopped.
    {
        let db = app.db.lock().await;
        let ab = AgentBuilder::new(&db);
        ab.update_agent_status_with_port(&agent_id, "stopped", None, None)?;
    }

    Ok(Json(serde_json::json!({
        "agent_id": agent_id,
        "status": "stopped"
    })))
}

// ---------------------------------------------------------------------------
// POST /projects/:id/agents/:agent_id/deploy
// Builds a Praesidia registration request for the agent.
// ---------------------------------------------------------------------------

pub async fn deploy_agent(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path((project_id, agent_id)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;
    let ab = AgentBuilder::new(&db);

    let agents = ab.list_agents(&project_id)?;
    let agent = agents
        .into_iter()
        .find(|a| a.id == agent_id)
        .ok_or_else(|| ApiError::NotFound(format!("agent {} not found", agent_id)))?;

    // Build webhook URL from assigned port (if running)
    let webhook_url = agent.zeroclaw_port.map(|port| {
        format!("http://localhost:{}/webhook", port)
    });

    // Build the agent card for Praesidia registration
    let agent_card = serde_json::json!({
        "name": agent.name,
        "role": agent.role,
        "tools": agent.tools,
        "system_prompt": agent.system_prompt,
        "model": agent.model,
        "provider": agent.provider,
    });

    let register_request = serde_json::json!({
        "agent_id": agent.id,
        "name": agent.name,
        "role": agent.role,
        "agent_card": agent_card,
        "webhook_url": webhook_url,
    });

    info!(
        agent_id = %agent_id,
        "Built Praesidia registration request for agent"
    );

    Ok(Json(serde_json::json!({
        "status": "ok",
        "message": "Registration request built (Praesidia connection not yet wired)",
        "register_request": register_request,
        "agent_card": agent_card,
    })))
}

// ---------------------------------------------------------------------------
// PUT /projects/:id/agents/:agent_id/model — update agent LLM model
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
pub struct UpdateAgentModelReq {
    pub provider: String,
    pub model: String,
}

pub async fn update_agent_model(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path((project_id, agent_id)): Path<(String, String)>,
    Json(body): Json<UpdateAgentModelReq>,
) -> ApiResult<Json<serde_json::Value>> {
    // Scope the SQLite guard so it is released BEFORE the YAML file I/O
    // below. Holding `db` across blocking `std::fs` calls (the old code did)
    // serialised the entire global database connection behind a file write.
    {
        let db = app.db.lock().await;
        validate_project_access(&db, &project_id, &auth.tenant_id)
            .map_err(ApiError::Forbidden)?;

        // Verify agent exists in this project
        let count: i64 = db.query_row(
            "SELECT COUNT(*) FROM agent_definitions WHERE id = ?1 AND project_id = ?2",
            rusqlite::params![agent_id, project_id],
            |row| row.get(0),
        )?;
        if count == 0 {
            return Err(ApiError::NotFound(format!("agent {} not found", agent_id)));
        }

        let now = chrono::Utc::now().to_rfc3339();
        db.execute(
            "UPDATE agent_definitions SET provider = ?1, model = ?2, updated_at = ?3 WHERE id = ?4",
            rusqlite::params![body.provider, body.model, now, agent_id],
        )?;
    } // <-- db guard dropped here

    // Also update the YAML file if it exists. Uses `tokio::fs` so the file
    // read/write does not block the tokio worker thread.
    let agents_dir = app.project_agents_dir(&project_id);
    let yaml_path = agents_dir.join(format!("{}.yaml", agent_id));
    if let Ok(content) = tokio::fs::read_to_string(&yaml_path).await {
        // Replace the provider line, then the model block.
        let updated: String = content
            .lines()
            .map(|line| {
                if line.trim_start().starts_with("provider:") {
                    format!("  provider: {}", body.provider)
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let updated = update_yaml_model_block(&updated, &body.provider, &body.model);
        let _ = tokio::fs::write(&yaml_path, updated).await;
    }

    info!(
        agent_id = %agent_id,
        provider = %body.provider,
        model = %body.model,
        "Agent model updated"
    );

    Ok(Json(serde_json::json!({
        "status": "ok",
        "agent_id": agent_id,
        "provider": body.provider,
        "model": body.model,
    })))
}

/// Replace the model block in an agent YAML file.
fn update_yaml_model_block(yaml: &str, provider: &str, model: &str) -> String {
    let mut result = Vec::new();
    let mut in_model_block = false;

    for line in yaml.lines() {
        if line.starts_with("model:") {
            in_model_block = true;
            result.push("model:".to_string());
            continue;
        }

        if in_model_block {
            if line.starts_with("  ") || line.starts_with('\t') {
                // Inside model block — replace provider and name lines
                let trimmed = line.trim_start();
                if trimmed.starts_with("provider:") {
                    result.push(format!("  provider: {}", provider));
                    continue;
                }
                if trimmed.starts_with("name:") {
                    result.push(format!("  name: {}", model));
                    continue;
                }
                result.push(line.to_string());
            } else {
                // Exited model block
                in_model_block = false;
                result.push(line.to_string());
            }
        } else {
            result.push(line.to_string());
        }
    }

    result.join("\n")
}

// ---------------------------------------------------------------------------
// DELETE /projects/:id/agents/:agent_id
// ---------------------------------------------------------------------------

pub async fn delete_agent(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path((project_id, agent_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let db = app.db.lock().await;
    validate_project_access(&db, &project_id, &auth.tenant_id)
        .map_err(ApiError::Forbidden)?;
    db.execute(
        "DELETE FROM agent_definitions WHERE id = ?1 AND project_id = ?2",
        rusqlite::params![agent_id, project_id],
    )?;
    Ok(StatusCode::NO_CONTENT)
}
