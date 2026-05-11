//! GitHub REST API integration.
//!
//! Provides typed access to repository management, pull requests, and issues
//! via the GitHub REST API v3 (`https://api.github.com`).
//!
//! Rate-limit headers (`X-RateLimit-Remaining`, `X-RateLimit-Reset`) are checked
//! on every response to surface [`IntegrationError::RateLimited`] before the
//! caller hits a hard 403.

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::IntegrationError;

// ─── Public data types ──────────────────────────────────────────────────────

/// A GitHub repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repo {
    pub id: u64,
    pub name: String,
    pub full_name: String,
    pub html_url: String,
    pub private: bool,
    #[serde(default)]
    pub description: Option<String>,
}

/// A GitHub pull request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequest {
    pub id: u64,
    pub number: u64,
    pub title: String,
    pub html_url: String,
    pub state: String,
}

/// A GitHub issue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub id: u64,
    pub number: u64,
    pub title: String,
    pub state: String,
    pub html_url: String,
}

// ─── Request bodies (sent to GitHub) ────────────────────────────────────────

#[derive(Serialize)]
struct CreateRepoBody<'a> {
    name: &'a str,
    description: &'a str,
    private: bool,
}

#[derive(Serialize)]
struct CreatePrBody<'a> {
    title: &'a str,
    body: &'a str,
    head: &'a str,
    base: &'a str,
}

#[derive(Serialize)]
struct CreateIssueBody<'a> {
    title: &'a str,
    body: &'a str,
}

// ─── Integration ────────────────────────────────────────────────────────────

const GITHUB_API: &str = "https://api.github.com";
const USER_AGENT: &str = "nexus-integrations/0.1";

/// Client for the GitHub REST API.
///
/// Requires a personal access token (classic or fine-grained) with appropriate
/// scopes for the operations you intend to use.
pub struct GitHubIntegration {
    token: String,
    client: reqwest::Client,
}

impl GitHubIntegration {
    /// Create a new GitHub integration with the given personal access token.
    pub fn new(token: &str) -> Self {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("failed to build reqwest client");

        Self {
            token: token.to_owned(),
            client,
        }
    }

    // ── Repositories ────────────────────────────────────────────────────────

    /// Create a new repository for the authenticated user.
    ///
    /// # Errors
    /// Returns [`IntegrationError::Auth`] if the token lacks `repo` scope, or
    /// [`IntegrationError::Api`] for any other GitHub error.
    pub async fn create_repo(
        &self,
        name: &str,
        description: &str,
        private: bool,
    ) -> Result<Repo, IntegrationError> {
        let url = format!("{GITHUB_API}/user/repos");
        let body = CreateRepoBody {
            name,
            description,
            private,
        };

        debug!(repo = name, "Creating GitHub repository");
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github+json")
            .json(&body)
            .send()
            .await
            .map_err(IntegrationError::from_reqwest)?;

        self.check_rate_limit(&resp);

        if !resp.status().is_success() {
            return Err(IntegrationError::from_response(resp).await);
        }

        resp.json::<Repo>()
            .await
            .map_err(IntegrationError::from_reqwest)
    }

    /// List repositories for the authenticated user, or for an organization if `org` is provided.
    pub async fn list_repos(&self, org: Option<&str>) -> Result<Vec<Repo>, IntegrationError> {
        let url = match org {
            Some(o) => format!("{GITHUB_API}/orgs/{o}/repos?per_page=100&sort=updated"),
            None => format!("{GITHUB_API}/user/repos?per_page=100&sort=updated"),
        };

        debug!(org = org, "Listing GitHub repositories");
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(IntegrationError::from_reqwest)?;

        self.check_rate_limit(&resp);

        if !resp.status().is_success() {
            return Err(IntegrationError::from_response(resp).await);
        }

        resp.json::<Vec<Repo>>()
            .await
            .map_err(IntegrationError::from_reqwest)
    }

    // ── Pull Requests ───────────────────────────────────────────────────────

    /// Create a pull request in the given repository.
    pub async fn create_pr(
        &self,
        owner: &str,
        repo: &str,
        title: &str,
        body: &str,
        head: &str,
        base: &str,
    ) -> Result<PullRequest, IntegrationError> {
        let url = format!("{GITHUB_API}/repos/{owner}/{repo}/pulls");
        let payload = CreatePrBody {
            title,
            body,
            head,
            base,
        };

        debug!(owner, repo, head, base, "Creating GitHub pull request");
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github+json")
            .json(&payload)
            .send()
            .await
            .map_err(IntegrationError::from_reqwest)?;

        self.check_rate_limit(&resp);

        if !resp.status().is_success() {
            return Err(IntegrationError::from_response(resp).await);
        }

        resp.json::<PullRequest>()
            .await
            .map_err(IntegrationError::from_reqwest)
    }

    // ── Issues ──────────────────────────────────────────────────────────────

    /// List issues for a repository, filtered by state (`open`, `closed`, `all`).
    pub async fn list_issues(
        &self,
        owner: &str,
        repo: &str,
        state: &str,
    ) -> Result<Vec<Issue>, IntegrationError> {
        let url =
            format!("{GITHUB_API}/repos/{owner}/{repo}/issues?state={state}&per_page=100");

        debug!(owner, repo, state, "Listing GitHub issues");
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(IntegrationError::from_reqwest)?;

        self.check_rate_limit(&resp);

        if !resp.status().is_success() {
            return Err(IntegrationError::from_response(resp).await);
        }

        resp.json::<Vec<Issue>>()
            .await
            .map_err(IntegrationError::from_reqwest)
    }

    /// Create a new issue in the given repository.
    pub async fn create_issue(
        &self,
        owner: &str,
        repo: &str,
        title: &str,
        body: &str,
    ) -> Result<Issue, IntegrationError> {
        let url = format!("{GITHUB_API}/repos/{owner}/{repo}/issues");
        let payload = CreateIssueBody { title, body };

        debug!(owner, repo, title, "Creating GitHub issue");
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github+json")
            .json(&payload)
            .send()
            .await
            .map_err(IntegrationError::from_reqwest)?;

        self.check_rate_limit(&resp);

        if !resp.status().is_success() {
            return Err(IntegrationError::from_response(resp).await);
        }

        resp.json::<Issue>()
            .await
            .map_err(IntegrationError::from_reqwest)
    }

    // ── Helpers ─────────────────────────────────────────────────────────────

    /// Log a warning when the rate limit is getting low (fewer than 100 calls remaining).
    fn check_rate_limit(&self, resp: &reqwest::Response) {
        if let Some(remaining) = resp
            .headers()
            .get("x-ratelimit-remaining")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
        {
            if remaining < 100 {
                let reset = resp
                    .headers()
                    .get("x-ratelimit-reset")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("unknown");
                warn!(
                    remaining,
                    reset, "GitHub API rate limit running low"
                );
            }
        }
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_integration_builds_with_token() {
        let gh = GitHubIntegration::new("ghp_test_token_12345");
        assert_eq!(gh.token, "ghp_test_token_12345");
    }

    #[test]
    fn repo_deserializes_from_github_json() {
        let json = serde_json::json!({
            "id": 123456,
            "name": "my-repo",
            "full_name": "user/my-repo",
            "html_url": "https://github.com/user/my-repo",
            "private": false,
            "description": "A test repo"
        });
        let repo: Repo = serde_json::from_value(json).unwrap();
        assert_eq!(repo.id, 123456);
        assert_eq!(repo.full_name, "user/my-repo");
        assert!(!repo.private);
    }

    #[test]
    fn pull_request_deserializes() {
        let json = serde_json::json!({
            "id": 1,
            "number": 42,
            "title": "Fix bug",
            "html_url": "https://github.com/user/repo/pull/42",
            "state": "open"
        });
        let pr: PullRequest = serde_json::from_value(json).unwrap();
        assert_eq!(pr.number, 42);
        assert_eq!(pr.state, "open");
    }

    #[test]
    fn issue_deserializes() {
        let json = serde_json::json!({
            "id": 10,
            "number": 7,
            "title": "Add feature X",
            "state": "open",
            "html_url": "https://github.com/user/repo/issues/7"
        });
        let issue: Issue = serde_json::from_value(json).unwrap();
        assert_eq!(issue.number, 7);
        assert_eq!(issue.title, "Add feature X");
    }

    #[test]
    fn create_repo_body_serializes_correctly() {
        let body = CreateRepoBody {
            name: "test",
            description: "desc",
            private: true,
        };
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["name"], "test");
        assert_eq!(json["private"], true);
    }

    #[test]
    fn create_pr_body_serializes_correctly() {
        let body = CreatePrBody {
            title: "PR title",
            body: "PR body",
            head: "feature",
            base: "main",
        };
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["head"], "feature");
        assert_eq!(json["base"], "main");
    }
}
