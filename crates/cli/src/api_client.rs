//! HTTP client for communicating with the Nexus server.
//!
//! All methods return `anyhow::Result` and communicate with the
//! running `nexus-server` instance over HTTP.

use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::Value;

/// HTTP client wrapper for the Nexus server API.
pub struct NexusClient {
    base_url: String,
    client: Client,
}

impl NexusClient {
    /// Create a new client pointing at the given server URL.
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: Client::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Generation
    // -----------------------------------------------------------------------

    /// Trigger a oneshot generation from a description.
    pub async fn generate(&self, description: &str, interactive: bool) -> Result<Value> {
        let body = serde_json::json!({
            "description": description,
            "interactive": interactive,
        });
        let resp = self
            .client
            .post(format!("{}/oneshot", self.base_url))
            .json(&body)
            .send()
            .await
            .context("Failed to connect to Nexus server")?;

        let status = resp.status();
        let json: Value = resp
            .json()
            .await
            .context("Failed to parse generation response")?;

        if !status.is_success() {
            anyhow::bail!(
                "Generation failed ({}): {}",
                status,
                json.get("error")
                    .and_then(|e| e.as_str())
                    .unwrap_or("unknown error")
            );
        }
        Ok(json)
    }

    // -----------------------------------------------------------------------
    // Teams
    // -----------------------------------------------------------------------

    /// List all team configurations.
    pub async fn list_teams(&self) -> Result<Vec<Value>> {
        self.get_list("/api/teams").await
    }

    /// Create a team from a template.
    pub async fn create_team(&self, template_id: &str) -> Result<Value> {
        let body = serde_json::json!({ "template_id": template_id });
        self.post_json("/api/teams", &body).await
    }

    /// Run a team on a task. Returns the response body.
    pub async fn run_team(&self, team_id: &str, task: &str) -> Result<Value> {
        let body = serde_json::json!({ "task": task });
        self.post_json(&format!("/api/teams/{}/run", team_id), &body)
            .await
    }

    /// List available team templates.
    pub async fn team_templates(&self) -> Result<Vec<Value>> {
        self.get_list("/api/teams/templates").await
    }

    // -----------------------------------------------------------------------
    // Agents
    // -----------------------------------------------------------------------

    /// List agents for a project.
    pub async fn list_agents(&self, project_id: &str) -> Result<Vec<Value>> {
        self.get_list(&format!("/api/projects/{}/agents", project_id))
            .await
    }

    /// Run a specific agent on a task.
    pub async fn run_agent(
        &self,
        project_id: &str,
        agent_id: &str,
        task: &str,
    ) -> Result<Value> {
        let body = serde_json::json!({ "task": task });
        self.post_json(
            &format!("/api/projects/{}/agents/{}/run", project_id, agent_id),
            &body,
        )
        .await
    }

    /// List available tools for an agent.
    pub async fn agent_tools(
        &self,
        project_id: &str,
        agent_id: &str,
    ) -> Result<Vec<Value>> {
        self.get_list(&format!(
            "/api/projects/{}/agents/{}/tools",
            project_id, agent_id
        ))
        .await
    }

    // -----------------------------------------------------------------------
    // App runtime
    // -----------------------------------------------------------------------

    /// Start the app for a project.
    pub async fn app_start(&self, project_id: &str) -> Result<Value> {
        self.post_json(
            &format!("/api/projects/{}/app/start", project_id),
            &serde_json::json!({}),
        )
        .await
    }

    /// Stop the app for a project.
    pub async fn app_stop(&self, project_id: &str) -> Result<Value> {
        self.post_json(
            &format!("/api/projects/{}/app/stop", project_id),
            &serde_json::json!({}),
        )
        .await
    }

    /// Get app status for a project.
    pub async fn app_status(&self, project_id: &str) -> Result<Value> {
        self.get_json(&format!("/api/projects/{}/app/status", project_id))
            .await
    }

    /// Get app logs for a project.
    pub async fn app_logs(&self, project_id: &str, lines: u32) -> Result<String> {
        let resp = self
            .client
            .get(format!(
                "{}/api/projects/{}/app/logs?lines={}",
                self.base_url, project_id, lines
            ))
            .send()
            .await
            .context("Failed to fetch app logs")?;

        let status = resp.status();
        if !status.is_success() {
            anyhow::bail!("Failed to fetch logs ({})", status);
        }
        resp.text().await.context("Failed to read log text")
    }

    // -----------------------------------------------------------------------
    // Taste (quality scoring)
    // -----------------------------------------------------------------------

    /// Get the taste/quality score for a project.
    pub async fn taste_score(&self, project_id: &str) -> Result<Value> {
        self.get_json(&format!("/api/projects/{}/taste/score", project_id))
            .await
    }

    /// Request a taste-based redesign for a project.
    pub async fn taste_redesign(&self, project_id: &str) -> Result<Value> {
        self.post_json(
            &format!("/api/projects/{}/taste/redesign", project_id),
            &serde_json::json!({}),
        )
        .await
    }

    // -----------------------------------------------------------------------
    // Deploy
    // -----------------------------------------------------------------------

    /// Deploy a project to GitHub.
    pub async fn deploy_github(
        &self,
        project_id: &str,
        repo: &str,
    ) -> Result<Value> {
        let body = serde_json::json!({ "target": "github", "repo": repo });
        self.post_json(&format!("/api/projects/{}/deploy", project_id), &body)
            .await
    }

    /// Deploy a project via Docker.
    pub async fn deploy_docker(&self, project_id: &str, tag: &str) -> Result<Value> {
        let body = serde_json::json!({ "target": "docker", "tag": tag });
        self.post_json(&format!("/api/projects/{}/deploy", project_id), &body)
            .await
    }

    /// Deploy a project to the portal.
    pub async fn deploy_portal(&self, project_id: &str) -> Result<Value> {
        let body = serde_json::json!({ "target": "portal" });
        self.post_json(&format!("/api/projects/{}/deploy", project_id), &body)
            .await
    }

    // -----------------------------------------------------------------------
    // Plugins
    // -----------------------------------------------------------------------

    /// List installed plugins.
    pub async fn list_plugins(&self) -> Result<Vec<Value>> {
        self.get_list("/api/plugins").await
    }

    /// Install a plugin by name.
    pub async fn install_plugin(&self, name: &str) -> Result<Value> {
        let body = serde_json::json!({ "name": name });
        self.post_json("/api/plugins/install", &body).await
    }

    /// Search the plugin marketplace.
    pub async fn search_plugins(&self, query: &str) -> Result<Vec<Value>> {
        self.get_list(&format!("/api/plugins/search?q={}", query))
            .await
    }

    // -----------------------------------------------------------------------
    // Config
    // -----------------------------------------------------------------------

    /// Get a config value.
    pub async fn config_get(&self, key: &str) -> Result<Value> {
        self.get_json(&format!("/api/config/{}", key)).await
    }

    /// Set a config value.
    pub async fn config_set(&self, key: &str, value: &str) -> Result<Value> {
        let body = serde_json::json!({ "value": value });
        self.post_json(&format!("/api/config/{}", key), &body).await
    }

    /// List configured providers.
    pub async fn config_providers(&self) -> Result<Vec<Value>> {
        self.get_list("/api/config/providers").await
    }

    // -----------------------------------------------------------------------
    // Server / Health
    // -----------------------------------------------------------------------

    /// Check server health.
    pub async fn health(&self) -> Result<Value> {
        self.get_json("/health").await
    }

    // -----------------------------------------------------------------------
    // Projects
    // -----------------------------------------------------------------------

    /// List all projects.
    pub async fn list_projects(&self) -> Result<Vec<Value>> {
        self.get_list("/api/projects").await
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    async fn get_json(&self, path: &str) -> Result<Value> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {} failed", url))?;

        let status = resp.status();
        let json: Value = resp
            .json()
            .await
            .with_context(|| format!("Failed to parse response from {}", path))?;

        if !status.is_success() {
            anyhow::bail!(
                "Request failed ({}): {}",
                status,
                json.get("error")
                    .and_then(|e| e.as_str())
                    .unwrap_or("unknown error")
            );
        }
        Ok(json)
    }

    async fn get_list(&self, path: &str) -> Result<Vec<Value>> {
        let json = self.get_json(path).await?;
        match json {
            Value::Array(arr) => Ok(arr),
            other => Ok(vec![other]),
        }
    }

    /// Generic GET — deserialises the response as `T`.
    pub async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {} failed", url))?;
        let status = resp.status();
        let json: Value = resp
            .json()
            .await
            .with_context(|| format!("Failed to parse response from {}", path))?;
        if !status.is_success() {
            anyhow::bail!(
                "Request failed ({}): {}",
                status,
                json.get("error").and_then(|e| e.as_str()).unwrap_or("unknown error")
            );
        }
        Ok(serde_json::from_value(json)?)
    }

    /// Generic POST — sends `body` as JSON and deserialises the response as `T`.
    pub async fn post<T: serde::de::DeserializeOwned>(&self, path: &str, body: &Value) -> Result<T> {
        let json = self.post_json(path, body).await?;
        Ok(serde_json::from_value(json)?)
    }

    async fn post_json(&self, path: &str, body: &Value) -> Result<Value> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .post(&url)
            .json(body)
            .send()
            .await
            .with_context(|| format!("POST {} failed", url))?;

        let status = resp.status();
        let json: Value = resp
            .json()
            .await
            .with_context(|| format!("Failed to parse response from {}", path))?;

        if !status.is_success() {
            anyhow::bail!(
                "Request failed ({}): {}",
                status,
                json.get("error")
                    .and_then(|e| e.as_str())
                    .unwrap_or("unknown error")
            );
        }
        Ok(json)
    }
}
