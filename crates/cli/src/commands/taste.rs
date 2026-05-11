//! `nexus taste` — quality scoring and redesign.

use anyhow::Result;
use clap::Subcommand;

use crate::api_client::NexusClient;
use crate::output::{self, OutputFormat};

#[derive(Subcommand, Debug)]
pub enum TasteAction {
    /// Get the quality score for a project.
    Score {
        /// Project ID.
        #[arg(long)]
        project: String,
    },
    /// Request an AI-driven redesign based on taste analysis.
    Redesign {
        /// Project ID.
        #[arg(long)]
        project: String,
    },
}

pub async fn run(server: &str, format: &OutputFormat, action: &TasteAction) -> Result<()> {
    let client = NexusClient::new(server);

    match action {
        TasteAction::Score { project } => {
            output::print_progress("Analyzing project quality...", 30);
            let result = client.taste_score(project).await?;

            match format {
                OutputFormat::Json => output::print_json(&result),
                OutputFormat::Quiet => {
                    if let Some(score) = result.get("overall_score").and_then(|v| v.as_f64()) {
                        println!("{:.1}", score);
                    }
                }
                OutputFormat::Human => {
                    output::print_header("Quality Score");

                    if let Some(score) = result.get("overall_score").and_then(|v| v.as_f64()) {
                        output::print_kv("Overall", &format!("{:.1}/100", score));
                    }

                    // Print category breakdowns if available
                    if let Some(categories) =
                        result.get("categories").and_then(|v| v.as_object())
                    {
                        println!();
                        let headers = &["CATEGORY", "SCORE"];
                        let rows: Vec<Vec<String>> = categories
                            .iter()
                            .map(|(k, v)| {
                                vec![
                                    k.clone(),
                                    v.as_f64()
                                        .map(|s| format!("{:.1}", s))
                                        .unwrap_or_else(|| "-".to_string()),
                                ]
                            })
                            .collect();
                        output::print_table(headers, &rows);
                    }

                    // Print issues if available
                    if let Some(issues) = result.get("issues").and_then(|v| v.as_array()) {
                        if !issues.is_empty() {
                            println!();
                            println!("  Issues:");
                            for issue in issues {
                                if let Some(msg) = issue.as_str() {
                                    println!("    - {}", msg);
                                } else if let Some(msg) =
                                    issue.get("message").and_then(|v| v.as_str())
                                {
                                    let severity = issue
                                        .get("severity")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("info");
                                    println!("    [{}] {}", severity.to_uppercase(), msg);
                                }
                            }
                        }
                    }

                    println!();
                    println!("  To auto-fix issues: nexus taste redesign --project {}", project);
                }
            }
        }

        TasteAction::Redesign { project } => {
            output::print_progress("Requesting taste-based redesign...", 20);
            let result = client.taste_redesign(project).await?;

            match format {
                OutputFormat::Json => output::print_json(&result),
                OutputFormat::Quiet => {
                    if let Some(status) = result.get("status").and_then(|v| v.as_str()) {
                        println!("{}", status);
                    }
                }
                OutputFormat::Human => {
                    output::print_progress("Redesign complete", 100);
                    println!();

                    if let Some(changes) =
                        result.get("changes_applied").and_then(|v| v.as_u64())
                    {
                        output::print_kv("Changes", &changes.to_string());
                    }
                    if let Some(new_score) =
                        result.get("new_score").and_then(|v| v.as_f64())
                    {
                        output::print_kv("New Score", &format!("{:.1}/100", new_score));
                    }
                    output::print_success("Redesign applied successfully.");
                }
            }
        }
    }

    Ok(())
}
