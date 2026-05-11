//! Communication tools — webhooks and GitHub API integration.
//!
//! These tools enable agents to interact with external services:
//! posting to webhooks, creating GitHub issues and pull requests.

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::agents::tools::registry::{Tool, ToolInput, ToolOutput};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn ok(result: Value) -> ToolOutput {
    ToolOutput {
        result,
        success: true,
        error: None,
    }
}

fn err(msg: impl Into<String>) -> ToolOutput {
    ToolOutput {
        result: json!({}),
        success: false,
        error: Some(msg.into()),
    }
}

// ---------------------------------------------------------------------------
// WebhookTool
// ---------------------------------------------------------------------------

/// POST data to a webhook URL.
pub struct WebhookTool;

#[async_trait]
impl Tool for WebhookTool {
    fn name(&self) -> &str {
        "webhook"
    }

    fn description(&self) -> &str {
        "Send a POST request to a webhook URL with a JSON payload. Useful for notifications and integrations."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "Webhook URL to POST to" },
                "payload": { "description": "JSON payload to send" },
                "headers": {
                    "type": "object",
                    "description": "Additional HTTP headers",
                    "additionalProperties": { "type": "string" }
                },
                "secret": { "type": "string", "description": "Signing secret for HMAC-SHA256 signature (optional)" }
            },
            "required": ["url", "payload"]
        })
    }

    fn category(&self) -> &str {
        "external_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let url = match input.parameters.get("url").and_then(|v| v.as_str()) {
            Some(u) => u,
            None => return err("Missing required parameter: url"),
        };
        let payload = match input.parameters.get("payload") {
            Some(p) => p,
            None => return err("Missing required parameter: payload"),
        };

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
        {
            Ok(c) => c,
            Err(e) => return err(format!("Failed to create HTTP client: {e}")),
        };

        let payload_str = serde_json::to_string(payload).unwrap_or_default();

        let mut req = client
            .post(url)
            .header("Content-Type", "application/json");

        // Add custom headers
        if let Some(headers) = input.parameters.get("headers").and_then(|v| v.as_object()) {
            for (key, val) in headers {
                if let Some(v) = val.as_str() {
                    req = req.header(key.as_str(), v);
                }
            }
        }

        // Add HMAC signature if secret provided
        if let Some(secret) = input.parameters.get("secret").and_then(|v| v.as_str()) {
            use hmac::{Hmac, Mac};
            use sha2::Sha256;

            type HmacSha256 = Hmac<Sha256>;
            match HmacSha256::new_from_slice(secret.as_bytes()) {
                Ok(mut mac) => {
                    mac.update(payload_str.as_bytes());
                    let signature = hex::encode(mac.finalize().into_bytes());
                    req = req.header("X-Signature-256", format!("sha256={signature}"));
                }
                Err(e) => return err(format!("Failed to create HMAC: {e}")),
            }
        }

        req = req.body(payload_str);

        match req.send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                let truncated = body.len() > 10_000;
                let body_out = if truncated {
                    body[..10_000].to_string()
                } else {
                    body
                };

                ok(json!({
                    "url": url,
                    "status": status,
                    "success": status < 400,
                    "response_body": body_out,
                    "truncated": truncated
                }))
            }
            Err(e) => err(format!("Webhook request failed: {e}")),
        }
    }
}

// ---------------------------------------------------------------------------
// GitHubIssueTool
// ---------------------------------------------------------------------------

/// Create a GitHub issue via the API.
pub struct GitHubIssueTool;

#[async_trait]
impl Tool for GitHubIssueTool {
    fn name(&self) -> &str {
        "github_issue"
    }

    fn description(&self) -> &str {
        "Create a GitHub issue. Requires GITHUB_TOKEN env var or a token parameter."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "owner": { "type": "string", "description": "Repository owner (user or org)" },
                "repo": { "type": "string", "description": "Repository name" },
                "title": { "type": "string", "description": "Issue title" },
                "body": { "type": "string", "description": "Issue body (markdown)" },
                "labels": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Labels to apply"
                },
                "assignees": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "GitHub usernames to assign"
                },
                "token": { "type": "string", "description": "GitHub token (overrides GITHUB_TOKEN env var)" }
            },
            "required": ["owner", "repo", "title"]
        })
    }

    fn category(&self) -> &str {
        "external_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let owner = match input.parameters.get("owner").and_then(|v| v.as_str()) {
            Some(o) => o,
            None => return err("Missing required parameter: owner"),
        };
        let repo = match input.parameters.get("repo").and_then(|v| v.as_str()) {
            Some(r) => r,
            None => return err("Missing required parameter: repo"),
        };
        let title = match input.parameters.get("title").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return err("Missing required parameter: title"),
        };
        let body = input.parameters.get("body").and_then(|v| v.as_str()).unwrap_or("");
        let labels: Vec<String> = input
            .parameters
            .get("labels")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let assignees: Vec<String> = input
            .parameters
            .get("assignees")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        let token = input
            .parameters
            .get("token")
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| std::env::var("GITHUB_TOKEN").ok());

        let token = match token {
            Some(t) => t,
            None => return err("No GitHub token. Set GITHUB_TOKEN env var or provide 'token' parameter."),
        };

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
        {
            Ok(c) => c,
            Err(e) => return err(format!("Failed to create HTTP client: {e}")),
        };

        let api_body = json!({
            "title": title,
            "body": body,
            "labels": labels,
            "assignees": assignees
        });

        let url = format!("https://api.github.com/repos/{owner}/{repo}/issues");

        match client
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "NexusAgent/1.0")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .json(&api_body)
            .send()
            .await
        {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let body_text = resp.text().await.unwrap_or_default();

                if status >= 400 {
                    return err(format!("GitHub API returned HTTP {status}: {body_text}"));
                }

                let body_json: Value = serde_json::from_str(&body_text).unwrap_or(json!({}));

                ok(json!({
                    "issue_number": body_json["number"],
                    "url": body_json["html_url"],
                    "title": title,
                    "state": body_json["state"]
                }))
            }
            Err(e) => err(format!("GitHub API request failed: {e}")),
        }
    }
}

// ---------------------------------------------------------------------------
// GitHubPrTool
// ---------------------------------------------------------------------------

/// Create a GitHub pull request via the API.
pub struct GitHubPrTool;

#[async_trait]
impl Tool for GitHubPrTool {
    fn name(&self) -> &str {
        "github_pr"
    }

    fn description(&self) -> &str {
        "Create a GitHub pull request. Requires GITHUB_TOKEN env var or a token parameter."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "owner": { "type": "string", "description": "Repository owner (user or org)" },
                "repo": { "type": "string", "description": "Repository name" },
                "title": { "type": "string", "description": "PR title" },
                "body": { "type": "string", "description": "PR body (markdown)" },
                "head": { "type": "string", "description": "Source branch name" },
                "base": { "type": "string", "description": "Target branch name (default: main)" },
                "draft": { "type": "boolean", "description": "Create as draft PR (default: false)" },
                "token": { "type": "string", "description": "GitHub token (overrides GITHUB_TOKEN env var)" }
            },
            "required": ["owner", "repo", "title", "head"]
        })
    }

    fn category(&self) -> &str {
        "external_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let owner = match input.parameters.get("owner").and_then(|v| v.as_str()) {
            Some(o) => o,
            None => return err("Missing required parameter: owner"),
        };
        let repo = match input.parameters.get("repo").and_then(|v| v.as_str()) {
            Some(r) => r,
            None => return err("Missing required parameter: repo"),
        };
        let title = match input.parameters.get("title").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return err("Missing required parameter: title"),
        };
        let head = match input.parameters.get("head").and_then(|v| v.as_str()) {
            Some(h) => h,
            None => return err("Missing required parameter: head"),
        };
        let body = input.parameters.get("body").and_then(|v| v.as_str()).unwrap_or("");
        let base = input.parameters.get("base").and_then(|v| v.as_str()).unwrap_or("main");
        let draft = input.parameters.get("draft").and_then(|v| v.as_bool()).unwrap_or(false);

        let token = input
            .parameters
            .get("token")
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| std::env::var("GITHUB_TOKEN").ok());

        let token = match token {
            Some(t) => t,
            None => return err("No GitHub token. Set GITHUB_TOKEN env var or provide 'token' parameter."),
        };

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
        {
            Ok(c) => c,
            Err(e) => return err(format!("Failed to create HTTP client: {e}")),
        };

        let api_body = json!({
            "title": title,
            "body": body,
            "head": head,
            "base": base,
            "draft": draft
        });

        let url = format!("https://api.github.com/repos/{owner}/{repo}/pulls");

        match client
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "NexusAgent/1.0")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .json(&api_body)
            .send()
            .await
        {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let body_text = resp.text().await.unwrap_or_default();

                if status >= 400 {
                    return err(format!("GitHub API returned HTTP {status}: {body_text}"));
                }

                let body_json: Value = serde_json::from_str(&body_text).unwrap_or(json!({}));

                ok(json!({
                    "pr_number": body_json["number"],
                    "url": body_json["html_url"],
                    "title": title,
                    "head": head,
                    "base": base,
                    "state": body_json["state"],
                    "draft": draft
                }))
            }
            Err(e) => err(format!("GitHub API request failed: {e}")),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webhook_tool_metadata() {
        let tool = WebhookTool;
        assert_eq!(tool.name(), "webhook");
        assert_eq!(tool.category(), "external_ops");
        let schema = tool.schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("url")));
        assert!(required.contains(&json!("payload")));
    }

    #[test]
    fn github_issue_tool_metadata() {
        let tool = GitHubIssueTool;
        assert_eq!(tool.name(), "github_issue");
        assert_eq!(tool.category(), "external_ops");
    }

    #[test]
    fn github_pr_tool_metadata() {
        let tool = GitHubPrTool;
        assert_eq!(tool.name(), "github_pr");
        assert_eq!(tool.category(), "external_ops");
        let schema = tool.schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("head")));
    }

    #[tokio::test]
    async fn webhook_missing_url() {
        let tool = WebhookTool;
        let out = tool
            .execute(ToolInput {
                parameters: json!({ "payload": {} }),
            })
            .await;
        assert!(!out.success);
        assert!(out.error.unwrap().contains("url"));
    }

    #[tokio::test]
    async fn github_issue_no_token() {
        let tool = GitHubIssueTool;
        let out = tool
            .execute(ToolInput {
                parameters: json!({
                    "owner": "test",
                    "repo": "test",
                    "title": "Test issue"
                }),
            })
            .await;
        // Should either fail with no token or succeed if GITHUB_TOKEN is set
        if !out.success {
            let err = out.error.unwrap();
            assert!(err.contains("token") || err.contains("GitHub"));
        }
    }
}
