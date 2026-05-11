//! `nexus server` — server management.

use anyhow::Result;
use clap::Subcommand;

use crate::api_client::NexusClient;
use crate::output::{self, OutputFormat};

#[derive(Subcommand, Debug)]
pub enum ServerAction {
    /// Start the Nexus server.
    Start {
        /// Port to listen on.
        #[arg(long, default_value = "8080")]
        port: u16,
        /// Run in the background (detached).
        #[arg(long)]
        detach: bool,
    },
    /// Check server status.
    Status,
}

pub async fn run(server: &str, format: &OutputFormat, action: &ServerAction) -> Result<()> {
    match action {
        ServerAction::Start { port, detach } => {
            println!("Starting Nexus server on port {}...", port);
            println!("  API:    http://localhost:{}", port);
            println!("  Health: http://localhost:{}/health", port);
            println!();

            if *detach {
                println!("Running in background...");
                let mut cmd = tokio::process::Command::new("nexus-server");
                cmd.env("NEXUS_PORT", port.to_string());
                cmd.stdout(std::process::Stdio::null());
                cmd.stderr(std::process::Stdio::null());

                let child = cmd.spawn();
                match child {
                    Ok(c) => {
                        output::print_success(&format!(
                            "Server started in background (PID: {}).",
                            c.id().unwrap_or(0)
                        ));
                        println!("  Check status: nexus server status");
                        println!("  View logs:    nexus app logs --project <ID>");
                    }
                    Err(e) => {
                        output::print_error(&format!("Failed to start server: {}", e));
                        println!();
                        println!("  Make sure 'nexus-server' is in your PATH.");
                        println!("  Build it with: cargo build -p nexus-http --release");
                        anyhow::bail!("Server start failed");
                    }
                }
            } else {
                println!("Press Ctrl+C to stop.");
                println!();

                let status = tokio::process::Command::new("nexus-server")
                    .env("NEXUS_PORT", port.to_string())
                    .status()
                    .await;

                match status {
                    Ok(s) if s.success() => {}
                    Ok(s) => {
                        anyhow::bail!("Server exited with status: {}", s);
                    }
                    Err(e) => {
                        output::print_error(&format!("Failed to start server: {}", e));
                        println!();
                        println!("  Make sure 'nexus-server' is in your PATH.");
                        println!("  Build it with: cargo build -p nexus-http --release");
                        anyhow::bail!("Server start failed");
                    }
                }
            }
        }

        ServerAction::Status => {
            let client = NexusClient::new(server);

            match client.health().await {
                Ok(result) => match format {
                    OutputFormat::Json => output::print_json(&result),
                    OutputFormat::Quiet => println!("ok"),
                    OutputFormat::Human => {
                        output::print_header("Server Status");
                        output::print_kv("URL", server);
                        output::print_kv("Status", "online");

                        if let Some(version) =
                            result.get("version").and_then(|v| v.as_str())
                        {
                            output::print_kv("Version", version);
                        }
                        if let Some(uptime) =
                            result.get("uptime").and_then(|v| v.as_str())
                        {
                            output::print_kv("Uptime", uptime);
                        }
                        if let Some(projects) =
                            result.get("projects").and_then(|v| v.as_u64())
                        {
                            output::print_kv("Projects", &projects.to_string());
                        }
                        println!();
                        output::print_success("Server is healthy.");
                    }
                },
                Err(_) => {
                    match format {
                        OutputFormat::Json => {
                            let val = serde_json::json!({
                                "status": "offline",
                                "url": server,
                            });
                            output::print_json(&val);
                        }
                        OutputFormat::Quiet => println!("offline"),
                        OutputFormat::Human => {
                            output::print_header("Server Status");
                            output::print_kv("URL", server);
                            output::print_kv("Status", "offline");
                            println!();
                            output::print_warning("Server is not reachable.");
                            println!("  Start it with: nexus server start");
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
