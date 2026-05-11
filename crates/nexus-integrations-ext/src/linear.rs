use serde::{Deserialize, Serialize};

use crate::error::IntegrationError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearConfig {
    pub api_key: String,
    pub team_id: String,
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearIssue {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub description: Option<String>,
    pub state: String,
    pub priority: u8,
    pub assignee: Option<String>,
    pub labels: Vec<String>,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateIssueRequest {
    pub title: String,
    pub description: Option<String>,
    pub priority: Option<u8>,
    pub assignee_id: Option<String>,
    pub label_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateIssueRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub state_id: Option<String>,
    pub priority: Option<u8>,
    pub assignee_id: Option<String>,
}

pub struct LinearClient {
    config: LinearConfig,
    http: reqwest::Client,
}

impl LinearClient {
    pub fn new(config: LinearConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
        }
    }

    fn api_url(&self) -> String {
        self.config
            .base_url
            .clone()
            .unwrap_or_else(|| "https://api.linear.app/graphql".to_string())
    }

    async fn graphql(
        &self,
        query: &str,
        variables: serde_json::Value,
    ) -> Result<serde_json::Value, IntegrationError> {
        let resp = self
            .http
            .post(self.api_url())
            .header("Authorization", &self.config.api_key)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "query": query,
                "variables": variables,
            }))
            .send()
            .await
            .map_err(|e| IntegrationError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(IntegrationError::from_response(resp).await);
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| IntegrationError::Parse(e.to_string()))?;

        if let Some(errors) = body.get("errors") {
            return Err(IntegrationError::Api {
                status: 200,
                message: errors.to_string(),
            });
        }

        Ok(body)
    }

    pub async fn create_issue(
        &self,
        req: CreateIssueRequest,
    ) -> Result<LinearIssue, IntegrationError> {
        let query = r#"
            mutation CreateIssue($input: IssueCreateInput!) {
                issueCreate(input: $input) {
                    success
                    issue {
                        id
                        identifier
                        title
                        description
                        url
                        priority
                        state { name }
                        assignee { name }
                        labels { nodes { name } }
                    }
                }
            }
        "#;

        let mut input = serde_json::json!({
            "teamId": self.config.team_id,
            "title": req.title,
        });
        if let Some(desc) = &req.description {
            input["description"] = serde_json::json!(desc);
        }
        if let Some(priority) = req.priority {
            input["priority"] = serde_json::json!(priority);
        }
        if let Some(assignee) = &req.assignee_id {
            input["assigneeId"] = serde_json::json!(assignee);
        }
        if !req.label_ids.is_empty() {
            input["labelIds"] = serde_json::json!(req.label_ids);
        }

        let result = self
            .graphql(query, serde_json::json!({ "input": input }))
            .await?;

        let issue_data = result
            .pointer("/data/issueCreate/issue")
            .ok_or_else(|| IntegrationError::Parse("Missing issue in response".to_string()))?;

        parse_linear_issue(issue_data)
    }

    pub async fn update_issue(
        &self,
        issue_id: &str,
        req: UpdateIssueRequest,
    ) -> Result<LinearIssue, IntegrationError> {
        let query = r#"
            mutation UpdateIssue($id: String!, $input: IssueUpdateInput!) {
                issueUpdate(id: $id, input: $input) {
                    success
                    issue {
                        id
                        identifier
                        title
                        description
                        url
                        priority
                        state { name }
                        assignee { name }
                        labels { nodes { name } }
                    }
                }
            }
        "#;

        let mut input = serde_json::json!({});
        if let Some(title) = &req.title {
            input["title"] = serde_json::json!(title);
        }
        if let Some(desc) = &req.description {
            input["description"] = serde_json::json!(desc);
        }
        if let Some(state_id) = &req.state_id {
            input["stateId"] = serde_json::json!(state_id);
        }
        if let Some(priority) = req.priority {
            input["priority"] = serde_json::json!(priority);
        }
        if let Some(assignee) = &req.assignee_id {
            input["assigneeId"] = serde_json::json!(assignee);
        }

        let result = self
            .graphql(
                query,
                serde_json::json!({ "id": issue_id, "input": input }),
            )
            .await?;

        let issue_data = result
            .pointer("/data/issueUpdate/issue")
            .ok_or_else(|| IntegrationError::Parse("Missing issue in response".to_string()))?;

        parse_linear_issue(issue_data)
    }

    pub async fn list_issues(
        &self,
        state_filter: Option<&str>,
    ) -> Result<Vec<LinearIssue>, IntegrationError> {
        let filter = if let Some(state) = state_filter {
            format!(r#", filter: {{ state: {{ name: {{ eq: "{state}" }} }} }}"#)
        } else {
            String::new()
        };

        let query = format!(
            r#"
            query ListIssues($teamId: String!) {{
                team(id: $teamId) {{
                    issues(first: 50{filter}) {{
                        nodes {{
                            id
                            identifier
                            title
                            description
                            url
                            priority
                            state {{ name }}
                            assignee {{ name }}
                            labels {{ nodes {{ name }} }}
                        }}
                    }}
                }}
            }}
        "#
        );

        let result = self
            .graphql(
                &query,
                serde_json::json!({ "teamId": self.config.team_id }),
            )
            .await?;

        let nodes = result
            .pointer("/data/team/issues/nodes")
            .and_then(|v| v.as_array())
            .ok_or_else(|| IntegrationError::Parse("Missing issues in response".to_string()))?;

        nodes.iter().map(parse_linear_issue).collect()
    }
}

fn parse_linear_issue(v: &serde_json::Value) -> Result<LinearIssue, IntegrationError> {
    let labels = v
        .pointer("/labels/nodes")
        .and_then(|n| n.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|l| l.get("name").and_then(|n| n.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();

    Ok(LinearIssue {
        id: v
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        identifier: v
            .get("identifier")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        title: v
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        description: v
            .get("description")
            .and_then(|v| v.as_str())
            .map(String::from),
        state: v
            .pointer("/state/name")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string(),
        priority: v
            .get("priority")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u8,
        assignee: v
            .pointer("/assignee/name")
            .and_then(|v| v.as_str())
            .map(String::from),
        labels,
        url: v
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
    })
}
