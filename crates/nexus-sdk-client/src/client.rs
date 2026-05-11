//! HTTP client for the Nexus API.

use serde_json::json;

use crate::error::SdkError;
use crate::types::*;

/// Typed HTTP client for the Nexus REST API.
///
/// Wraps all Nexus endpoints (generation, agents, teams, quality, intent,
/// memory, health) behind ergonomic async methods.
///
/// # Example
/// ```rust,ignore
/// let client = NexusClient::new("http://localhost:8020", "my-api-key");
/// let gen = client.generate("Build a todo app").await?;
/// println!("Taste: {}", gen.taste_score);
/// ```
pub struct NexusClient {
    /// Base URL of the Nexus server (no trailing slash).
    base_url: String,
    /// API key for authentication.
    api_key: String,
    /// Shared reqwest HTTP client.
    client: reqwest::Client,
}

impl NexusClient {
    /// Create a new client pointed at the given Nexus server.
    ///
    /// # Arguments
    /// * `base_url` — Root URL of the Nexus HTTP server (e.g. `"http://localhost:8020"`).
    /// * `api_key` — API key obtained from the Nexus security settings.
    pub fn new(base_url: &str, api_key: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            client: reqwest::Client::new(),
        }
    }

    // ── Generation ──────────────────────────────────────────────────────────

    /// Generate a full application from a natural-language description.
    ///
    /// Calls `POST /oneshot/sync` with the provided description and returns
    /// the generation result including project ID, taste score, and file count.
    pub async fn generate(&self, description: &str) -> Result<GenerationResult, SdkError> {
        let body = json!({ "description": description });
        let resp = self.post("/oneshot/sync", &body).await?;
        serde_json::from_value(resp).map_err(|e| SdkError::Parse(e.to_string()))
    }

    // ── Agents ──────────────────────────────────────────────────────────────

    /// Run a single agent on a task within a project context.
    ///
    /// # Arguments
    /// * `role` — Agent role (e.g. "coder", "reviewer", "tester").
    /// * `task` — Natural-language task description.
    pub async fn run_agent(&self, role: &str, task: &str) -> Result<AgentResult, SdkError> {
        let body = json!({ "role": role, "task": task });
        let resp = self.post("/coding-agent/run", &body).await?;
        serde_json::from_value(resp).map_err(|e| SdkError::Parse(e.to_string()))
    }

    // ── Teams ───────────────────────────────────────────────────────────────

    /// Run a multi-agent team using a named template.
    ///
    /// # Arguments
    /// * `template` — Team template name (e.g. "full-stack", "review-board").
    /// * `task` — Natural-language task for the team.
    pub async fn run_team(&self, template: &str, task: &str) -> Result<TeamResult, SdkError> {
        let body = json!({ "template": template, "task": task });
        let resp = self.post("/teams/run", &body).await?;
        serde_json::from_value(resp).map_err(|e| SdkError::Parse(e.to_string()))
    }

    // ── Quality ─────────────────────────────────────────────────────────────

    /// Score the quality of a generated project using the taste engine.
    ///
    /// # Arguments
    /// * `project_id` — ID of the project to evaluate.
    pub async fn score_quality(&self, project_id: &str) -> Result<QualityScore, SdkError> {
        let path = format!("/projects/{}/taste", project_id);
        let resp = self.get(&path).await?;
        serde_json::from_value(resp).map_err(|e| SdkError::Parse(e.to_string()))
    }

    // ── Intent ──────────────────────────────────────────────────────────────

    /// Analyze a natural-language description to extract intent metadata.
    ///
    /// Returns the detected app type, complexity, domain, and confidence.
    pub async fn analyze_intent(&self, description: &str) -> Result<IntentResult, SdkError> {
        let body = json!({ "description": description });
        let resp = self.post("/intent/analyze", &body).await?;
        serde_json::from_value(resp).map_err(|e| SdkError::Parse(e.to_string()))
    }

    // ── Memory ──────────────────────────────────────────────────────────────

    /// Store content in the persistent memory system with optional tags.
    ///
    /// # Arguments
    /// * `content` — The content to remember.
    /// * `tags` — Categorization tags for later retrieval.
    pub async fn remember(&self, content: &str, tags: &[&str]) -> Result<(), SdkError> {
        let body = json!({ "content": content, "tags": tags });
        self.post("/memory", &body).await?;
        Ok(())
    }

    /// Recall memories matching a semantic query.
    ///
    /// # Arguments
    /// * `query` — Natural-language search query.
    /// * `limit` — Maximum number of entries to return.
    pub async fn recall(&self, query: &str, limit: usize) -> Result<Vec<MemoryEntry>, SdkError> {
        let body = json!({ "query": query, "limit": limit });
        let resp = self.post("/memory/knowledge/query", &body).await?;
        // The API may return entries under a "results" key or as a top-level array.
        if let Some(arr) = resp.as_array() {
            let entries: Vec<MemoryEntry> = arr
                .iter()
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect();
            return Ok(entries);
        }
        if let Some(arr) = resp.get("results").and_then(|v| v.as_array()) {
            let entries: Vec<MemoryEntry> = arr
                .iter()
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect();
            return Ok(entries);
        }
        Ok(Vec::new())
    }

    // ── Health ───────────────────────────────────────────────────────────────

    /// Check the health of the Nexus server.
    ///
    /// Returns the raw JSON health response.
    pub async fn health(&self) -> Result<serde_json::Value, SdkError> {
        self.get("/health").await
    }

    // ── Internal helpers ────────────────────────────────────────────────────

    /// Send a GET request to the given API path.
    async fn get(&self, path: &str) -> Result<serde_json::Value, SdkError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;

        self.handle_response(resp).await
    }

    /// Send a POST request with a JSON body to the given API path.
    async fn post(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, SdkError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(body)
            .send()
            .await?;

        self.handle_response(resp).await
    }

    /// Parse a response, mapping non-2xx status codes to `SdkError::Api` or `SdkError::Auth`.
    async fn handle_response(
        &self,
        resp: reqwest::Response,
    ) -> Result<serde_json::Value, SdkError> {
        let status = resp.status().as_u16();

        if status == 401 || status == 403 {
            let message = resp.text().await.unwrap_or_default();
            return Err(SdkError::Auth(message));
        }

        if !resp.status().is_success() {
            let message = resp.text().await.unwrap_or_default();
            return Err(SdkError::Api { status, message });
        }

        let body = resp.text().await?;
        if body.is_empty() {
            return Ok(serde_json::Value::Null);
        }
        serde_json::from_str(&body).map_err(|e| SdkError::Parse(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructs_with_trimmed_url() {
        let client = NexusClient::new("http://localhost:8020/", "key-123");
        assert_eq!(client.base_url, "http://localhost:8020");
        assert_eq!(client.api_key, "key-123");
    }

    #[test]
    fn constructs_without_trailing_slash() {
        let client = NexusClient::new("http://example.com", "abc");
        assert_eq!(client.base_url, "http://example.com");
    }

    #[test]
    fn url_building() {
        let client = NexusClient::new("http://localhost:8020", "key");
        let url = format!("{}{}", client.base_url, "/health");
        assert_eq!(url, "http://localhost:8020/health");
    }

    #[test]
    fn url_building_with_project() {
        let client = NexusClient::new("http://localhost:8020", "key");
        let project_id = "proj-abc";
        let path = format!("/projects/{}/taste", project_id);
        let url = format!("{}{}", client.base_url, path);
        assert_eq!(url, "http://localhost:8020/projects/proj-abc/taste");
    }
}
