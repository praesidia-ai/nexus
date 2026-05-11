//! `nexus generate` — generate an app from a natural-language description.

use anyhow::Result;

use crate::api_client::NexusClient;
use crate::output::{self, OutputFormat};

pub async fn run(
    server: &str,
    format: &OutputFormat,
    description: &str,
    interactive: bool,
) -> Result<()> {
    let client = NexusClient::new(server);

    output::print_header("Generating Application");
    output::print_kv("Description", description);
    output::print_kv("Interactive", if interactive { "yes" } else { "no" });
    println!();

    output::print_progress("Sending to Nexus server...", 10);

    let result = client.generate(description, interactive).await?;

    match format {
        OutputFormat::Json => output::print_json(&result),
        OutputFormat::Quiet => {
            if let Some(id) = result.get("project_id").and_then(|v| v.as_str()) {
                println!("{}", id);
            }
        }
        OutputFormat::Human => {
            output::print_progress("Generation complete", 100);
            println!();

            if let Some(id) = result.get("project_id").and_then(|v| v.as_str()) {
                output::print_kv("Project ID", id);
            }
            if let Some(name) = result.get("name").and_then(|v| v.as_str()) {
                output::print_kv("Name", name);
            }
            if let Some(status) = result.get("status").and_then(|v| v.as_str()) {
                output::print_kv("Status", status);
            }

            println!();
            output::print_success("Application generated successfully.");
            println!();
            println!("  Next steps:");
            println!("    nexus app start --project <PROJECT_ID>");
            println!("    nexus taste score --project <PROJECT_ID>");
        }
    }

    Ok(())
}
