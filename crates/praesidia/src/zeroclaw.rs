//! ZeroClaw runtime registry at `.neurogent/runtime.json`.
//!
//! Compatible with the TypeScript Neurogent CLI: agents started via `neurogent start`
//! write this file so shells and other tools can discover local HTTP endpoints.
//!
//! Fields: `agents[]` with `id`, `name`, `url`, `port`, optional `pid`, `startedAt`;
//! top-level `startedAt` for the registry.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZeroClawAgentEntry {
    pub id: String,
    pub name: String,
    pub url: String,
    pub port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(rename = "startedAt")]
    pub started_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZeroClawRegistration {
    pub agents: Vec<ZeroClawAgentEntry>,
    #[serde(rename = "startedAt")]
    pub started_at: String,
}

/// Default registry path under `base/.neurogent/runtime.json`.
pub fn runtime_file_at_base(base: impl AsRef<std::path::Path>) -> PathBuf {
    let mut dir = base.as_ref().to_path_buf();
    dir.push(".neurogent");
    dir.push("runtime.json");
    dir
}

fn get_runtime_file() -> PathBuf {
    runtime_file_at_base(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

pub fn load_runtime() -> Option<ZeroClawRegistration> {
    let path = get_runtime_file();
    if let Ok(raw) = fs::read_to_string(path) {
        serde_json::from_str(&raw).ok()
    } else {
        None
    }
}

pub fn save_runtime(reg: &ZeroClawRegistration) -> Result<(), std::io::Error> {
    let path = get_runtime_file();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_string_pretty(reg)?;
    fs::write(path, data)
}

pub fn clear_runtime() {
    let _ = fs::remove_file(get_runtime_file());
}

pub fn get_agent_url(agent_id: &str) -> Option<String> {
    let runtime = load_runtime()?;
    runtime.agents.into_iter().find(|a| a.id == agent_id).map(|a| a.url)
}

/// All entries from the current working directory registry, if present.
pub fn list_registered_agents() -> Vec<ZeroClawAgentEntry> {
    load_runtime()
        .map(|r| r.agents)
        .unwrap_or_default()
}

/// Load registry relative to `base` (project root).
pub fn load_runtime_at(base: &std::path::Path) -> Option<ZeroClawRegistration> {
    let path = runtime_file_at_base(base);
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub alive: bool,
    pub agent: Option<String>,
    pub uptime: Option<u64>,
    pub error: Option<String>,
}

pub async fn ping_agent(url: &str) -> HealthResponse {
    let client = reqwest::Client::new();
    match client.get(format!("{}/health", url))
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
    {
        Ok(res) if res.status().is_success() => {
            if let Ok(data) = res.json::<serde_json::Value>().await {
                HealthResponse {
                    alive: true,
                    agent: data.get("agent").and_then(|v| v.as_str()).map(String::from),
                    uptime: data.get("uptime").and_then(|v| v.as_u64()),
                    error: None,
                }
            } else {
                HealthResponse { alive: false, error: Some("Parse err".into()), agent: None, uptime: None }
            }
        }
        Ok(res) => HealthResponse { alive: false, error: Some(format!("HTTP {}", res.status())), agent: None, uptime: None },
        Err(e) => HealthResponse { alive: false, error: Some(e.to_string()), agent: None, uptime: None },
    }
}
