use serde::{Deserialize, Serialize};

use crate::error::IntegrationError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubConfig {
    pub token: String,
    pub owner: String,
    pub repo: String,
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub head: String,
    pub base: String,
    pub state: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub labels: Vec<String>,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePrRequest {
    pub title: String,
    pub body: String,
    pub head: String,
    pub base: String,
    pub draft: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriageResult {
    pub issue_number: u64,
    pub suggested_labels: Vec<String>,
    pub priority: String,
    pub suggested_assignee: Option<String>,
    pub summary: String,
}

pub struct GitHubClient {
    config: GitHubConfig,
    http: reqwest::Client,
}

impl GitHubClient {
    pub fn new(config: GitHubConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
        }
    }

    fn api_url(&self, path: &str) -> String {
        let base = self
            .config
            .base_url
            .as_deref()
            .unwrap_or("https://api.github.com");
        format!(
            "{base}/repos/{}/{}{path}",
            self.config.owner, self.config.repo
        )
    }

    pub async fn create_pr(&self, req: CreatePrRequest) -> Result<PullRequest, IntegrationError> {
        let url = self.api_url("/pulls");
        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.token))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "nexus-agent")
            .json(&serde_json::json!({
                "title": req.title,
                "body": req.body,
                "head": req.head,
                "base": req.base,
                "draft": req.draft,
            }))
            .send()
            .await
            .map_err(|e| IntegrationError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(IntegrationError::from_response(resp).await);
        }

        let pr: PullRequest = resp
            .json()
            .await
            .map_err(|e| IntegrationError::Parse(e.to_string()))?;
        Ok(pr)
    }

    pub async fn list_issues(
        &self,
        state: &str,
        labels: &[&str],
    ) -> Result<Vec<Issue>, IntegrationError> {
        let label_str = labels.join(",");
        let url = format!(
            "{}?state={state}&labels={label_str}",
            self.api_url("/issues")
        );
        let resp = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.config.token))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "nexus-agent")
            .send()
            .await
            .map_err(|e| IntegrationError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(IntegrationError::from_response(resp).await);
        }

        let issues: Vec<Issue> = resp
            .json()
            .await
            .map_err(|e| IntegrationError::Parse(e.to_string()))?;
        Ok(issues)
    }

    pub async fn add_comment(
        &self,
        issue_number: u64,
        body: &str,
    ) -> Result<(), IntegrationError> {
        let url = format!(
            "{}/comments",
            self.api_url(&format!("/issues/{issue_number}"))
        );
        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.token))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "nexus-agent")
            .json(&serde_json::json!({ "body": body }))
            .send()
            .await
            .map_err(|e| IntegrationError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(IntegrationError::from_response(resp).await);
        }

        Ok(())
    }
}
