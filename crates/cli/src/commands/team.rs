//! `nexus team` — team management and execution.

use anyhow::Result;
use clap::Subcommand;

use crate::api_client::NexusClient;
use crate::output::{self, OutputFormat};

#[derive(Subcommand, Debug)]
pub enum TeamAction {
    /// Create a new team from a template.
    Create {
        /// Template ID to use.
        #[arg(long)]
        template: String,
    },
    /// List all teams.
    List,
    /// Run a team on a task.
    Run {
        /// Team ID.
        #[arg(long)]
        team: String,
        /// Task description.
        task: String,
        /// Watch live progress via SSE.
        #[arg(long)]
        watch: bool,
    },
    /// List available team templates.
    Templates,
}

pub async fn run(server: &str, format: &OutputFormat, action: &TeamAction) -> Result<()> {
    let client = NexusClient::new(server);

    match action {
        TeamAction::Create { template } => {
            output::print_progress("Creating team...", 30);
            let result = client.create_team(template).await?;

            match format {
                OutputFormat::Json => output::print_json(&result),
                OutputFormat::Quiet => {
                    if let Some(id) = result.get("id").and_then(|v| v.as_str()) {
                        println!("{}", id);
                    }
                }
                OutputFormat::Human => {
                    output::print_success("Team created.");
                    if let Some(id) = result.get("id").and_then(|v| v.as_str()) {
                        output::print_kv("Team ID", id);
                    }
                    if let Some(name) = result.get("name").and_then(|v| v.as_str()) {
                        output::print_kv("Name", name);
                    }
                }
            }
        }

        TeamAction::List => {
            let teams = client.list_teams().await?;

            match format {
                OutputFormat::Json => {
                    output::print_json(&serde_json::Value::Array(teams));
                }
                OutputFormat::Quiet => {
                    for t in &teams {
                        if let Some(id) = t.get("id").and_then(|v| v.as_str()) {
                            println!("{}", id);
                        }
                    }
                }
                OutputFormat::Human => {
                    if teams.is_empty() {
                        println!("No teams found. Create one with: nexus team create --template <ID>");
                        return Ok(());
                    }
                    let headers = &["ID", "NAME", "AGENTS", "STATUS"];
                    let rows: Vec<Vec<String>> = teams
                        .iter()
                        .map(|t| {
                            vec![
                                t.get("id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("-")
                                    .to_string(),
                                t.get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("-")
                                    .to_string(),
                                t.get("agent_count")
                                    .and_then(|v| v.as_u64())
                                    .map(|n| n.to_string())
                                    .unwrap_or_else(|| "-".to_string()),
                                t.get("status")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("unknown")
                                    .to_string(),
                            ]
                        })
                        .collect();
                    output::print_table(headers, &rows);
                }
            }
        }

        TeamAction::Run { team, task, watch } => {
            if *watch {
                println!("Starting team run (live mode)...");
                println!();
            }

            let result = client.run_team(team, task).await?;

            match format {
                OutputFormat::Json => output::print_json(&result),
                OutputFormat::Quiet => {
                    if let Some(status) = result.get("status").and_then(|v| v.as_str()) {
                        println!("{}", status);
                    }
                }
                OutputFormat::Human => {
                    if let Some(events) = result.get("events").and_then(|v| v.as_array()) {
                        for event in events {
                            let agent = event
                                .get("agent")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown");
                            let msg = event
                                .get("message")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            println!("[{}] {}", agent, msg);
                        }
                    }

                    if let Some(status) = result.get("status").and_then(|v| v.as_str()) {
                        println!();
                        output::print_kv("Status", status);
                    }
                    if let Some(output_val) = result.get("output").and_then(|v| v.as_str()) {
                        println!();
                        println!("{}", output_val);
                    }
                    output::print_success("Team run complete.");
                }
            }
        }

        TeamAction::Templates => {
            let templates = client.team_templates().await?;

            match format {
                OutputFormat::Json => {
                    output::print_json(&serde_json::Value::Array(templates));
                }
                OutputFormat::Quiet => {
                    for t in &templates {
                        if let Some(id) = t.get("id").and_then(|v| v.as_str()) {
                            println!("{}", id);
                        }
                    }
                }
                OutputFormat::Human => {
                    if templates.is_empty() {
                        println!("No templates available.");
                        return Ok(());
                    }
                    output::print_header("Team Templates");
                    let headers = &["ID", "NAME", "DESCRIPTION"];
                    let rows: Vec<Vec<String>> = templates
                        .iter()
                        .map(|t| {
                            vec![
                                t.get("id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("-")
                                    .to_string(),
                                t.get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("-")
                                    .to_string(),
                                t.get("description")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("-")
                                    .to_string(),
                            ]
                        })
                        .collect();
                    output::print_table(headers, &rows);
                }
            }
        }
    }

    Ok(())
}
