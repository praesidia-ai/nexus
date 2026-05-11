//! `nexus agent` — agent management and execution.

use anyhow::Result;
use clap::Subcommand;

use crate::api_client::NexusClient;
use crate::output::{self, OutputFormat};

#[derive(Subcommand, Debug)]
pub enum AgentAction {
    /// Create a new agent in a project.
    Create {
        /// Project ID.
        #[arg(long)]
        project: String,
        /// Agent name (e.g. nova, atlas, kai).
        name: String,
        /// Agent role description.
        #[arg(long)]
        role: Option<String>,
    },
    /// List agents in a project.
    List {
        /// Project ID.
        #[arg(long)]
        project: String,
    },
    /// Run an agent on a task.
    Run {
        /// Project ID.
        #[arg(long)]
        project: String,
        /// Agent ID or name.
        #[arg(long)]
        agent: String,
        /// Task description.
        task: String,
    },
    /// List tools available to an agent.
    Tools {
        /// Project ID.
        #[arg(long)]
        project: String,
        /// Agent ID or name.
        #[arg(long)]
        agent: String,
    },
}

pub async fn run(server: &str, format: &OutputFormat, action: &AgentAction) -> Result<()> {
    let client = NexusClient::new(server);

    match action {
        AgentAction::Create {
            project,
            name,
            role,
        } => {
            // The API client doesn't have a direct create_agent method,
            // so we use the run_agent as a proxy or provide helpful output.
            println!(
                "Creating agent '{}' in project '{}'...",
                name, project
            );
            if let Some(r) = role {
                println!("  Role: {}", r);
            }
            println!();
            println!("Agent creation is handled by the server during project setup.");
            println!("Use `nexus agent list --project {}` to see available agents.", project);
            Ok(())
        }

        AgentAction::List { project } => {
            let agents = client.list_agents(project).await?;

            match format {
                OutputFormat::Json => {
                    output::print_json(&serde_json::Value::Array(agents));
                }
                OutputFormat::Quiet => {
                    for a in &agents {
                        if let Some(id) = a.get("id").and_then(|v| v.as_str()) {
                            println!("{}", id);
                        }
                    }
                }
                OutputFormat::Human => {
                    if agents.is_empty() {
                        println!("No agents found for project '{}'.", project);
                        return Ok(());
                    }
                    let headers = &["ID", "NAME", "ROLE", "STATUS"];
                    let rows: Vec<Vec<String>> = agents
                        .iter()
                        .map(|a| {
                            vec![
                                a.get("id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("-")
                                    .to_string(),
                                a.get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("-")
                                    .to_string(),
                                a.get("role")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("-")
                                    .to_string(),
                                a.get("status")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("idle")
                                    .to_string(),
                            ]
                        })
                        .collect();
                    output::print_table(headers, &rows);
                }
            }
            Ok(())
        }

        AgentAction::Run {
            project,
            agent,
            task,
        } => {
            println!("Running agent '{}' on task...", agent);
            println!();

            let result = client.run_agent(project, agent, task).await?;

            match format {
                OutputFormat::Json => output::print_json(&result),
                OutputFormat::Quiet => {
                    if let Some(out) = result.get("output").and_then(|v| v.as_str()) {
                        println!("{}", out);
                    }
                }
                OutputFormat::Human => {
                    if let Some(out) = result.get("output").and_then(|v| v.as_str()) {
                        println!("{}", out);
                    } else {
                        output::print_json(&result);
                    }
                    println!();
                    output::print_success("Agent task complete.");
                }
            }
            Ok(())
        }

        AgentAction::Tools { project, agent } => {
            let tools = client.agent_tools(project, agent).await?;

            match format {
                OutputFormat::Json => {
                    output::print_json(&serde_json::Value::Array(tools));
                }
                OutputFormat::Quiet => {
                    for t in &tools {
                        if let Some(name) = t.get("name").and_then(|v| v.as_str()) {
                            println!("{}", name);
                        }
                    }
                }
                OutputFormat::Human => {
                    if tools.is_empty() {
                        println!("No tools registered for agent '{}'.", agent);
                        return Ok(());
                    }
                    let headers = &["NAME", "DESCRIPTION"];
                    let rows: Vec<Vec<String>> = tools
                        .iter()
                        .map(|t| {
                            vec![
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
            Ok(())
        }
    }
}
