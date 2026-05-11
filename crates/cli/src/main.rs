//! `nexus` — the Nexus command-line interface.
//!
//! AI-native application generation and agent teams.
//!
//! ```text
//! USAGE:
//!   nexus generate "Build a SaaS CRM"       Generate an app from a description
//!   nexus team list                          List teams
//!   nexus team run --team <ID> "task"        Run a team on a task
//!   nexus agent list --project <ID>          List agents
//!   nexus agent run --project <ID> ...       Run an agent
//!   nexus app start --project <ID>           Start the app
//!   nexus app logs --project <ID>            View app logs
//!   nexus taste score --project <ID>         Quality score
//!   nexus deploy github --project <ID> ...   Deploy to GitHub
//!   nexus plugins list                       List plugins
//!   nexus config set <key> <value>           Set config
//!   nexus server start                       Start the server
//! ```

mod api_client;
mod commands;
mod output;
#[cfg(test)]
mod tests;

use clap::{Parser, Subcommand};

use crate::commands::acp::AcpAction;
use crate::commands::agent::AgentAction;
use crate::commands::app::AppAction;
use crate::commands::config::ConfigAction;
use crate::commands::deploy::DeployAction;
use crate::commands::mcp::McpAction;
use crate::commands::pkg::PkgAction;
use crate::commands::plugins::PluginAction;
use crate::commands::server::ServerAction;
use crate::commands::taste::TasteAction;
use crate::commands::team::TeamAction;
use crate::commands::workflow::WorkflowAction;
use crate::output::OutputFormat;

// ---------------------------------------------------------------------------
// CLI definition
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(
    name = "nexus",
    about = "Nexus — AI-native application generation and agent teams",
    version = "0.1.0",
    long_about = "Nexus is an AI-powered platform for generating, managing, and deploying\n\
                  full-stack applications using coordinated agent teams.\n\n\
                  Start with:\n  \
                  nexus generate \"Build a task management app with React and Node.js\"\n\n\
                  Or manage the server:\n  \
                  nexus server start\n  \
                  nexus server status"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output format: human, json, quiet.
    #[arg(long, global = true, default_value = "human")]
    format: OutputFormat,

    /// Nexus server URL.
    #[arg(long, global = true, default_value = "http://localhost:8020")]
    server: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate an app from a natural-language description.
    Generate {
        /// App description (e.g. "Build a SaaS CRM with Stripe billing").
        description: String,
        /// Interactive mode — ask clarifying questions before generating.
        #[arg(long)]
        interactive: bool,
    },

    /// Team management and execution.
    #[command(subcommand_required = true)]
    Team {
        #[command(subcommand)]
        action: TeamAction,
    },

    /// Agent management and execution.
    #[command(subcommand_required = true)]
    Agent {
        #[command(subcommand)]
        action: AgentAction,
    },

    /// App runtime management (start, stop, logs, status).
    #[command(subcommand_required = true)]
    App {
        #[command(subcommand)]
        action: AppAction,
    },

    /// Quality scoring and taste-based redesign.
    #[command(subcommand_required = true)]
    Taste {
        #[command(subcommand)]
        action: TasteAction,
    },

    /// Deploy to GitHub, Docker, or the Nexus portal.
    #[command(subcommand_required = true)]
    Deploy {
        #[command(subcommand)]
        action: DeployAction,
    },

    /// Plugin management (list, install, search).
    #[command(subcommand_required = true)]
    Plugins {
        #[command(subcommand)]
        action: PluginAction,
    },

    /// Configuration management.
    #[command(subcommand_required = true)]
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Server management (start, status).
    #[command(subcommand_required = true)]
    Server {
        #[command(subcommand)]
        action: ServerAction,
    },

    /// Agent package manager (install, publish, search community agents).
    #[command(subcommand_required = true)]
    Pkg {
        #[command(subcommand)]
        action: PkgAction,
    },

    /// Workflow management (list, run, status, cancel).
    #[command(subcommand_required = true)]
    Workflow {
        #[command(subcommand)]
        action: WorkflowAction,
    },

    /// MCP (Model Context Protocol) — run Nexus as an MCP server so
    /// Claude Desktop / Claude Code / Cursor / Zed can drive it.
    #[command(subcommand_required = true)]
    Mcp {
        #[command(subcommand)]
        action: McpAction,
    },

    /// ACP (Agent Client Protocol) — run Nexus as an ACP server so
    /// Zed + JetBrains ACP Agent Registry + any ACP-aware editor can
    /// drive it over JSON-RPC on stdio.
    #[command(subcommand_required = true)]
    Acp {
        #[command(subcommand)]
        action: AcpAction,
    },

    /// List all projects.
    Projects,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result = run_command(cli).await;

    if let Err(e) = result {
        output::print_error(&format!("{:#}", e));
        std::process::exit(1);
    }
}

async fn run_command(cli: Cli) -> anyhow::Result<()> {
    let server = &cli.server;
    let format = &cli.format;

    match cli.command {
        Commands::Generate {
            description,
            interactive,
        } => {
            commands::generate::run(server, format, &description, interactive).await
        }

        Commands::Team { action } => commands::team::run(server, format, &action).await,

        Commands::Agent { action } => commands::agent::run(server, format, &action).await,

        Commands::App { action } => commands::app::run(server, format, &action).await,

        Commands::Taste { action } => commands::taste::run(server, format, &action).await,

        Commands::Deploy { action } => commands::deploy::run(server, format, &action).await,

        Commands::Plugins { action } => commands::plugins::run(server, format, &action).await,

        Commands::Config { action } => commands::config::run(server, format, &action).await,

        Commands::Server { action } => commands::server::run(server, format, &action).await,

        Commands::Pkg { action } => commands::pkg::run(server, format, &action).await,

        Commands::Workflow { action } => commands::workflow::run(server, format, &action).await,

        Commands::Mcp { action } => commands::mcp::run(server, format, &action).await,

        Commands::Acp { action } => commands::acp::run(server, format, &action).await,

        Commands::Projects => {
            let client = api_client::NexusClient::new(server);
            let projects = client.list_projects().await?;

            match format {
                OutputFormat::Json => {
                    output::print_json(&serde_json::Value::Array(projects));
                }
                OutputFormat::Quiet => {
                    for p in &projects {
                        if let Some(id) = p.get("id").and_then(|v| v.as_str()) {
                            println!("{}", id);
                        }
                    }
                }
                OutputFormat::Human => {
                    if projects.is_empty() {
                        println!("No projects found.");
                        println!();
                        println!("  Create one with: nexus generate \"Your app description\"");
                        return Ok(());
                    }
                    output::print_header("Projects");
                    let headers = &["ID", "NAME", "STATUS", "CREATED"];
                    let rows: Vec<Vec<String>> = projects
                        .iter()
                        .map(|p| {
                            vec![
                                p.get("id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("-")
                                    .to_string(),
                                p.get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("-")
                                    .to_string(),
                                p.get("status")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("-")
                                    .to_string(),
                                p.get("created_at")
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
