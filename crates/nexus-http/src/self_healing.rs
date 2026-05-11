//! Self-healing system — auto-detects crashes, diagnoses errors, and fixes code.

use std::sync::Arc;

use tracing::{error, info};

use crate::agent_loop::{self, AgentConfig, AgentEvent};
use crate::handlers::agent_run::{api_base_for_provider, default_model_for_provider};
use crate::state::AppState;

use nexus_store::ProjectService;

/// Start the self-healing loop for a project.
/// Called from the app runner watchdog when a crash is detected.
pub async fn attempt_heal(
    app: Arc<AppState>,
    project_id: String,
    instance_id: String,
    error_log: String,
) {
    let healing_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    info!(
        project = %project_id,
        healing_id = %healing_id,
        "Self-healing triggered"
    );

    // Store healing event
    {
        let db = app.db.lock().await;
        let _ = db.execute(
            "INSERT OR IGNORE INTO healing_events (id, project_id, instance_id, trigger, error_log, status, created_at)
             VALUES (?1, ?2, ?3, 'crash', ?4, 'diagnosing', ?5)",
            rusqlite::params![healing_id, project_id, instance_id, error_log, now],
        );
    }

    // Resolve LLM config — self-healing runs in the background without a
    // caller AuthContext, so we derive the tenant from the project row and
    // read the API key via the tenant-aware resolver.
    let (provider, model, api_key, api_base) = {
        let db = app.db.lock().await;
        let svc = ProjectService::new(&db);
        let tenant_id: Option<String> = db
            .prepare("SELECT tenant_id FROM projects WHERE id = ?1")
            .ok()
            .and_then(|mut stmt| {
                stmt.query_row([&project_id], |row| row.get::<_, Option<String>>(0))
                    .ok()
                    .flatten()
            });
        let provider = svc
            .get_setting("llm.default.provider")
            .ok()
            .flatten()
            .unwrap_or_else(|| "ollama".to_string());
        let model = svc
            .get_setting("llm.default.model")
            .ok()
            .flatten()
            .unwrap_or_else(|| default_model_for_provider(&provider));
        let api_key = crate::handlers::agent_run::resolve_api_key_for_tenant(
            &svc,
            tenant_id.as_deref(),
            &provider,
            &app,
        );
        let api_base = api_base_for_provider(&provider);
        (provider, model, api_key, api_base)
    };

    let project_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");

    let heal_prompt = format!(
        r#"The application has CRASHED. Your job is to diagnose and fix the error.

## Error Log
```
{}
```

## Instructions
1. Read the error carefully — identify the root cause
2. Find the file(s) that caused the error using grep/file_read
3. Fix the bug using file_edit
4. If there are missing dependencies, note them
5. Verify the fix by reading the modified file

Be surgical — fix ONLY the bug that caused the crash. Do not refactor or improve other code."#,
        error_log.chars().take(3000).collect::<String>()
    );

    let config = AgentConfig {
        max_iterations: 15,
        system_prompt: "You are a diagnostic agent. Your only job is to find and fix the bug that caused the app crash. Be precise and minimal.".to_string(),
        project_dir,
        provider,
        model,
        api_key,
        api_base,
        project_id: Some(project_id.clone()),
        // Self-healing runs as a background system task; no caller tenant
        // context. Cost is recorded against the global "default" bucket.
        tenant_id: None,
    };

    let (tx, mut rx) = tokio::sync::mpsc::channel::<AgentEvent>(50);

    // Run the healing agent
    let app_bg = app.clone();
    tokio::spawn(async move {
        agent_loop::run_agent_loop(config, heal_prompt, tx, app_bg).await;
    });

    // Collect results
    let mut fix_summary = String::new();
    let mut healed = false;

    while let Some(event) = rx.recv().await {
        match event {
            AgentEvent::Done { summary, .. } => {
                fix_summary = summary;
                healed = true;
                break;
            }
            AgentEvent::Error { message } => {
                fix_summary = format!("Healing failed: {}", message);
                break;
            }
            _ => {}
        }
    }

    // Update healing event
    let status = if healed { "healed" } else { "failed" };
    {
        let db = app.db.lock().await;
        let _ = db.execute(
            "UPDATE healing_events SET status = ?1, fix_summary = ?2 WHERE id = ?3",
            rusqlite::params![status, fix_summary, healing_id],
        );
    }

    if healed {
        info!(healing_id = %healing_id, "Self-healing succeeded, restarting app");
        // The app_runner watchdog will handle the restart
    } else {
        error!(healing_id = %healing_id, "Self-healing failed");
    }
}
