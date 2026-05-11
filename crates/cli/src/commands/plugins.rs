//! `nexus plugins` — plugin management.

use anyhow::Result;
use clap::Subcommand;

use crate::api_client::NexusClient;
use crate::output::{self, OutputFormat};

#[derive(Subcommand, Debug)]
pub enum PluginAction {
    /// List installed plugins.
    List,
    /// Install a plugin by name.
    Install {
        /// Plugin name.
        name: String,
    },
    /// Search the plugin marketplace.
    Search {
        /// Search query.
        query: String,
    },
}

pub async fn run(server: &str, format: &OutputFormat, action: &PluginAction) -> Result<()> {
    let client = NexusClient::new(server);

    match action {
        PluginAction::List => {
            let plugins = client.list_plugins().await?;

            match format {
                OutputFormat::Json => {
                    output::print_json(&serde_json::Value::Array(plugins));
                }
                OutputFormat::Quiet => {
                    for p in &plugins {
                        if let Some(name) = p.get("name").and_then(|v| v.as_str()) {
                            println!("{}", name);
                        }
                    }
                }
                OutputFormat::Human => {
                    if plugins.is_empty() {
                        println!("No plugins installed.");
                        println!("Search for plugins: nexus plugins search <query>");
                        return Ok(());
                    }
                    let headers = &["NAME", "VERSION", "STATUS"];
                    let rows: Vec<Vec<String>> = plugins
                        .iter()
                        .map(|p| {
                            vec![
                                p.get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("-")
                                    .to_string(),
                                p.get("version")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("-")
                                    .to_string(),
                                p.get("status")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("active")
                                    .to_string(),
                            ]
                        })
                        .collect();
                    output::print_table(headers, &rows);
                }
            }
        }

        PluginAction::Install { name } => {
            output::print_progress(&format!("Installing plugin '{}'...", name), 30);
            let result = client.install_plugin(name).await?;

            match format {
                OutputFormat::Json => output::print_json(&result),
                OutputFormat::Quiet => println!("installed"),
                OutputFormat::Human => {
                    output::print_progress("Plugin installed", 100);
                    println!();
                    if let Some(version) = result.get("version").and_then(|v| v.as_str()) {
                        output::print_kv("Version", version);
                    }
                    output::print_success(&format!("Plugin '{}' installed.", name));
                }
            }
        }

        PluginAction::Search { query } => {
            let results = client.search_plugins(query).await?;

            match format {
                OutputFormat::Json => {
                    output::print_json(&serde_json::Value::Array(results));
                }
                OutputFormat::Quiet => {
                    for p in &results {
                        if let Some(name) = p.get("name").and_then(|v| v.as_str()) {
                            println!("{}", name);
                        }
                    }
                }
                OutputFormat::Human => {
                    if results.is_empty() {
                        println!("No plugins found matching '{}'.", query);
                        return Ok(());
                    }
                    output::print_header("Plugin Search Results");
                    let headers = &["NAME", "VERSION", "DESCRIPTION", "DOWNLOADS"];
                    let rows: Vec<Vec<String>> = results
                        .iter()
                        .map(|p| {
                            vec![
                                p.get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("-")
                                    .to_string(),
                                p.get("version")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("-")
                                    .to_string(),
                                p.get("description")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("-")
                                    .to_string(),
                                p.get("downloads")
                                    .and_then(|v| v.as_u64())
                                    .map(|n| n.to_string())
                                    .unwrap_or_else(|| "-".to_string()),
                            ]
                        })
                        .collect();
                    output::print_table(headers, &rows);

                    println!();
                    println!("  Install with: nexus plugins install <NAME>");
                }
            }
        }
    }

    Ok(())
}
