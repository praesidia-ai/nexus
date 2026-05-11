//! `nexus config` — configuration management.

use anyhow::Result;
use clap::Subcommand;

use crate::api_client::NexusClient;
use crate::output::{self, OutputFormat};

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Get a configuration value.
    Get {
        /// Config key (e.g. "model", "api_key").
        key: String,
    },
    /// Set a configuration value.
    Set {
        /// Config key.
        key: String,
        /// Config value.
        value: String,
    },
    /// List configured LLM providers.
    Providers,
}

pub async fn run(server: &str, format: &OutputFormat, action: &ConfigAction) -> Result<()> {
    let client = NexusClient::new(server);

    match action {
        ConfigAction::Get { key } => {
            let result = client.config_get(key).await?;

            match format {
                OutputFormat::Json => output::print_json(&result),
                OutputFormat::Quiet | OutputFormat::Human => {
                    if let Some(val) = result.get("value") {
                        match val {
                            serde_json::Value::String(s) => println!("{}", s),
                            other => println!("{}", other),
                        }
                    } else {
                        output::print_json(&result);
                    }
                }
            }
        }

        ConfigAction::Set { key, value } => {
            let result = client.config_set(key, value).await?;

            match format {
                OutputFormat::Json => output::print_json(&result),
                OutputFormat::Quiet => {}
                OutputFormat::Human => {
                    output::print_success(&format!("Config '{}' set to '{}'.", key, value));
                }
            }
        }

        ConfigAction::Providers => {
            let providers = client.config_providers().await?;

            match format {
                OutputFormat::Json => {
                    output::print_json(&serde_json::Value::Array(providers));
                }
                OutputFormat::Quiet => {
                    for p in &providers {
                        if let Some(name) = p.get("name").and_then(|v| v.as_str()) {
                            println!("{}", name);
                        }
                    }
                }
                OutputFormat::Human => {
                    if providers.is_empty() {
                        println!("No LLM providers configured.");
                        println!();
                        println!("  Set an API key:");
                        println!("    nexus config set openai_api_key sk-...");
                        println!("    nexus config set anthropic_api_key sk-ant-...");
                        return Ok(());
                    }
                    output::print_header("LLM Providers");
                    let headers = &["PROVIDER", "MODEL", "STATUS"];
                    let rows: Vec<Vec<String>> = providers
                        .iter()
                        .map(|p| {
                            vec![
                                p.get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("-")
                                    .to_string(),
                                p.get("model")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("-")
                                    .to_string(),
                                p.get("status")
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
    }

    Ok(())
}
