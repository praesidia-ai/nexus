//! `nexus deploy` — deployment to various targets.

use anyhow::Result;
use clap::Subcommand;

use crate::api_client::NexusClient;
use crate::output::{self, OutputFormat};

#[derive(Subcommand, Debug)]
pub enum DeployAction {
    /// Deploy to a GitHub repository.
    Github {
        /// Project ID.
        #[arg(long)]
        project: String,
        /// Target GitHub repo (e.g. "user/repo").
        #[arg(long)]
        repo: String,
    },
    /// Build and push a Docker image.
    Docker {
        /// Project ID.
        #[arg(long)]
        project: String,
        /// Docker image tag.
        #[arg(long, default_value = "latest")]
        tag: String,
    },
    /// Publish to the Nexus portal.
    Portal {
        /// Project ID.
        #[arg(long)]
        project: String,
    },
}

pub async fn run(server: &str, format: &OutputFormat, action: &DeployAction) -> Result<()> {
    let client = NexusClient::new(server);

    match action {
        DeployAction::Github { project, repo } => {
            output::print_progress("Deploying to GitHub...", 20);
            let result = client.deploy_github(project, repo).await?;

            match format {
                OutputFormat::Json => output::print_json(&result),
                OutputFormat::Quiet => {
                    if let Some(url) = result.get("url").and_then(|v| v.as_str()) {
                        println!("{}", url);
                    }
                }
                OutputFormat::Human => {
                    output::print_progress("Deployed to GitHub", 100);
                    println!();
                    if let Some(url) = result.get("url").and_then(|v| v.as_str()) {
                        output::print_kv("Repository", url);
                    }
                    if let Some(commit) = result.get("commit").and_then(|v| v.as_str()) {
                        output::print_kv("Commit", commit);
                    }
                    output::print_success("GitHub deployment complete.");
                }
            }
        }

        DeployAction::Docker { project, tag } => {
            output::print_progress("Building Docker image...", 20);
            let result = client.deploy_docker(project, tag).await?;

            match format {
                OutputFormat::Json => output::print_json(&result),
                OutputFormat::Quiet => {
                    if let Some(image) = result.get("image").and_then(|v| v.as_str()) {
                        println!("{}", image);
                    }
                }
                OutputFormat::Human => {
                    output::print_progress("Docker image built", 100);
                    println!();
                    if let Some(image) = result.get("image").and_then(|v| v.as_str()) {
                        output::print_kv("Image", image);
                    }
                    if let Some(size) = result.get("size").and_then(|v| v.as_str()) {
                        output::print_kv("Size", size);
                    }
                    output::print_success("Docker deployment complete.");
                }
            }
        }

        DeployAction::Portal { project } => {
            output::print_progress("Publishing to portal...", 20);
            let result = client.deploy_portal(project).await?;

            match format {
                OutputFormat::Json => output::print_json(&result),
                OutputFormat::Quiet => {
                    if let Some(url) = result.get("url").and_then(|v| v.as_str()) {
                        println!("{}", url);
                    }
                }
                OutputFormat::Human => {
                    output::print_progress("Published to portal", 100);
                    println!();
                    if let Some(url) = result.get("url").and_then(|v| v.as_str()) {
                        output::print_kv("Portal URL", url);
                    }
                    if let Some(slug) = result.get("slug").and_then(|v| v.as_str()) {
                        output::print_kv("Slug", slug);
                    }
                    output::print_success("Portal deployment complete.");
                }
            }
        }
    }

    Ok(())
}
