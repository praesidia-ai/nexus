//! `nexus app` — app runtime management.

use anyhow::Result;
use clap::Subcommand;

use crate::api_client::NexusClient;
use crate::output::{self, OutputFormat};

#[derive(Subcommand, Debug)]
pub enum AppAction {
    /// Start the app for a project.
    Start {
        /// Project ID.
        #[arg(long)]
        project: String,
    },
    /// Stop the app for a project.
    Stop {
        /// Project ID.
        #[arg(long)]
        project: String,
    },
    /// Show app logs.
    Logs {
        /// Project ID.
        #[arg(long)]
        project: String,
        /// Number of log lines to show.
        #[arg(long, default_value = "50")]
        lines: u32,
        /// Follow logs in real time.
        #[arg(long, short)]
        follow: bool,
    },
    /// Show app status.
    Status {
        /// Project ID.
        #[arg(long)]
        project: String,
    },
}

pub async fn run(server: &str, format: &OutputFormat, action: &AppAction) -> Result<()> {
    let client = NexusClient::new(server);

    match action {
        AppAction::Start { project } => {
            output::print_progress("Starting app...", 20);
            let result = client.app_start(project).await?;

            match format {
                OutputFormat::Json => output::print_json(&result),
                OutputFormat::Quiet => {
                    if let Some(url) = result.get("url").and_then(|v| v.as_str()) {
                        println!("{}", url);
                    }
                }
                OutputFormat::Human => {
                    output::print_progress("App started", 100);
                    println!();
                    if let Some(url) = result.get("url").and_then(|v| v.as_str()) {
                        output::print_kv("URL", url);
                    }
                    if let Some(port) = result.get("port").and_then(|v| v.as_u64()) {
                        output::print_kv("Port", &port.to_string());
                    }
                    if let Some(pid) = result.get("pid").and_then(|v| v.as_u64()) {
                        output::print_kv("PID", &pid.to_string());
                    }
                    output::print_success("App is running.");
                }
            }
        }

        AppAction::Stop { project } => {
            let result = client.app_stop(project).await?;

            match format {
                OutputFormat::Json => output::print_json(&result),
                OutputFormat::Quiet => println!("stopped"),
                OutputFormat::Human => {
                    output::print_success("App stopped.");
                }
            }
        }

        AppAction::Logs {
            project,
            lines,
            follow,
        } => {
            if *follow {
                println!("(Following logs — press Ctrl+C to stop)");
                println!();
            }

            let logs = client.app_logs(project, *lines).await?;

            match format {
                OutputFormat::Json => {
                    let val = serde_json::json!({ "logs": logs });
                    output::print_json(&val);
                }
                _ => {
                    println!("{}", logs);
                }
            }
        }

        AppAction::Status { project } => {
            let result = client.app_status(project).await?;

            match format {
                OutputFormat::Json => output::print_json(&result),
                OutputFormat::Quiet => {
                    if let Some(status) = result.get("status").and_then(|v| v.as_str()) {
                        println!("{}", status);
                    }
                }
                OutputFormat::Human => {
                    output::print_header("App Status");
                    if let Some(status) = result.get("status").and_then(|v| v.as_str()) {
                        output::print_kv("Status", status);
                    }
                    if let Some(url) = result.get("url").and_then(|v| v.as_str()) {
                        output::print_kv("URL", url);
                    }
                    if let Some(port) = result.get("port").and_then(|v| v.as_u64()) {
                        output::print_kv("Port", &port.to_string());
                    }
                    if let Some(uptime) = result.get("uptime").and_then(|v| v.as_str()) {
                        output::print_kv("Uptime", uptime);
                    }
                    if let Some(pid) = result.get("pid").and_then(|v| v.as_u64()) {
                        output::print_kv("PID", &pid.to_string());
                    }
                }
            }
        }
    }

    Ok(())
}
