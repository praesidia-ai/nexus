use clap::Subcommand;
use crate::api_client::NexusClient;
use crate::output::{print_header, print_json, print_table, OutputFormat};

#[derive(Subcommand)]
pub enum WorkflowAction {
    /// List all workflow runs
    List {
        #[arg(long)]
        project: String,
    },
    /// Trigger a workflow run
    Run {
        #[arg(long)]
        project: String,
        /// Workflow name
        name: String,
        /// JSON payload (optional)
        #[arg(long)]
        payload: Option<String>,
    },
    /// Show status of a workflow run
    Status {
        #[arg(long)]
        project: String,
        run_id: String,
    },
    /// Cancel a running workflow
    Cancel {
        #[arg(long)]
        project: String,
        run_id: String,
    },
}

pub async fn run(server: &str, format: &OutputFormat, action: &WorkflowAction) -> anyhow::Result<()> {
    let client = NexusClient::new(server);

    match action {
        WorkflowAction::List { project } => {
            let url = format!("/projects/{project}/workflows");
            let runs: Vec<serde_json::Value> = client.get(&url).await?;
            match format {
                OutputFormat::Json => print_json(&serde_json::Value::Array(runs)),
                OutputFormat::Quiet => {
                    for r in &runs {
                        if let Some(id) = r.get("id").and_then(|v| v.as_str()) {
                            println!("{id}");
                        }
                    }
                }
                OutputFormat::Human => {
                    if runs.is_empty() {
                        println!("No workflow runs.");
                        return Ok(());
                    }
                    print_header("Workflow Runs");
                    let headers = &["ID", "NAME", "STATUS", "STARTED"];
                    let rows: Vec<Vec<String>> = runs
                        .iter()
                        .map(|r| {
                            vec![
                                field(r, "id"),
                                field(r, "name"),
                                field(r, "status"),
                                field(r, "started_at"),
                            ]
                        })
                        .collect();
                    print_table(headers, &rows);
                }
            }
        }

        WorkflowAction::Run { project, name, payload } => {
            let url = format!("/projects/{project}/workflows/run");
            let payload_val: serde_json::Value = if let Some(p) = payload {
                serde_json::from_str(p).unwrap_or(serde_json::Value::Null)
            } else {
                serde_json::Value::Null
            };
            let body = serde_json::json!({ "name": name, "payload": payload_val });
            let result: serde_json::Value = client.post(&url, &body).await?;
            match format {
                OutputFormat::Json => print_json(&result),
                _ => {
                    let run_id = result.get("id").and_then(|v| v.as_str()).unwrap_or("-");
                    println!("Started workflow run: {run_id}");
                }
            }
        }

        WorkflowAction::Status { project, run_id } => {
            let url = format!("/projects/{project}/workflows/{run_id}");
            let result: serde_json::Value = client.get(&url).await?;
            match format {
                OutputFormat::Json => print_json(&result),
                _ => {
                    let status = result.get("status").and_then(|v| v.as_str()).unwrap_or("-");
                    println!("Run {run_id}: {status}");
                }
            }
        }

        WorkflowAction::Cancel { project, run_id } => {
            let url = format!("/projects/{project}/workflows/{run_id}/cancel");
            let _result: serde_json::Value = client.post(&url, &serde_json::json!({})).await?;
            println!("Cancelled: {run_id}");
        }
    }
    Ok(())
}

fn field(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("-")
        .to_owned()
}
