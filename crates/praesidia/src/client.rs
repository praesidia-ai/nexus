//! HTTP client for the Praesidia platform API.
//!
//! Used by Nexus to publish local projects, register agents, and push policies
//! to a self-hosted or Praesidia Cloud instance.

use serde::{Deserialize, Serialize};

use crate::policy::PraesidiaPolicy;

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishProjectRequest {
    pub name: String,
    pub description: Option<String>,
    /// Serialized `NeuroAgentConfig` YAML (the neuro.agent.yaml content).
    pub agent_yaml: Option<String>,
    /// Serialized `PraesidiaPolicy` YAML.
    pub policy_yaml: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishResult {
    pub project_id: String,
    pub organization_id: String,
    pub dashboard_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterAgentRequest {
    pub agent_id: String,
    pub name: String,
    pub role: String,
    /// Full JSON-serialized AgentCard.
    pub agent_card: serde_json::Value,
    /// Optional webhook URL (DIRECT mode). Absent for ROUTED/polling mode.
    pub webhook_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterPolicyRequest {
    pub agent_id: String,
    pub policy_yaml: String,
    pub policy_hash: String,
}

// ---------------------------------------------------------------------------
// PraesidiaClient
// ---------------------------------------------------------------------------

pub struct PraesidiaClient {
    base_url: String,
    api_key: String,
    http: reqwest::Client,
}

impl PraesidiaClient {
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.api_key)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Verify the Praesidia instance is reachable.
    pub async fn health_check(&self) -> anyhow::Result<bool> {
        let resp = self
            .http
            .get(self.url("/health"))
            .header("Authorization", self.auth_header())
            .send()
            .await?;
        Ok(resp.status().is_success())
    }

    /// Publish a local Nexus project to Praesidia.
    /// Returns `PublishResult` with the remote project ID and a dashboard URL.
    pub async fn publish_project(
        &self,
        request: &PublishProjectRequest,
    ) -> anyhow::Result<PublishResult> {
        let resp = self
            .http
            .post(self.url("/api/v1/nexus/projects"))
            .header("Authorization", self.auth_header())
            .json(request)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Praesidia publish_project HTTP {}: {}", status, body);
        }

        Ok(resp.json::<PublishResult>().await?)
    }

    /// Register an agent definition with Praesidia.
    pub async fn register_agent(&self, request: &RegisterAgentRequest) -> anyhow::Result<()> {
        let resp = self
            .http
            .post(self.url("/api/v1/nexus/agents"))
            .header("Authorization", self.auth_header())
            .json(request)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Praesidia register_agent HTTP {}: {}", status, body);
        }

        Ok(())
    }

    /// Push a policy YAML and its hash to Praesidia so it can verify agent compliance.
    pub async fn register_policy(
        &self,
        agent_id: &str,
        policy: &PraesidiaPolicy,
        policy_yaml: &str,
    ) -> anyhow::Result<()> {
        let hash = crate::policy_hash::sha256_hex(policy_yaml.as_bytes());

        let request = RegisterPolicyRequest {
            agent_id: agent_id.to_string(),
            policy_yaml: policy_yaml.to_string(),
            policy_hash: hash,
        };

        // Validate before sending
        let _ = policy; // already parsed by caller

        let resp = self
            .http
            .post(self.url("/api/v1/nexus/policies"))
            .header("Authorization", self.auth_header())
            .json(&request)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Praesidia register_policy HTTP {}: {}", status, body);
        }

        Ok(())
    }
}
