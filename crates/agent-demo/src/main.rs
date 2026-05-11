//! `nexus-demo` — end-to-end demonstration of the Nexus workflow engine.
//!
//! Runs one of the built-in pipelines using the ZeroClaw agent team and
//! prints each step's output to stdout.
//!
//! # Usage
//!
//! ```bash
//! # Full app-build pipeline (default)
//! cargo run -p nexus-demo
//!
//! # Code review pipeline
//! cargo run -p nexus-demo -- --pipeline code-review \
//!   --input code="fn add(a: i32, b: i32) -> i32 { a + b }"
//!
//! # Ask a single agent
//! cargo run -p nexus-demo -- --ask nova "Write a Rust function to parse a CSV file"
//!
//! # List pipelines
//! cargo run -p nexus-demo -- --list-pipelines
//!
//! # Dry-run (stub LLM, no API key needed)
//! ANTHROPIC_API_KEY="" OPENAI_API_KEY="" cargo run -p nexus-demo
//! ```

use std::path::PathBuf;

use clap::Parser;
use tracing::info;

use nexus_core::{
    AgentName, NexusState, WorkflowContext, PIPELINE_NAMES, pipeline_by_name,
};

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(name = "nexus-demo")]
#[command(about = "Nexus workflow demo — build apps from scratch with a team of ZeroClaw AI agents")]
#[command(version = "0.1.0")]
struct Args {
    /// Pipeline to run (default: app-build).
    #[arg(long, default_value = "app-build")]
    pipeline: String,

    /// Input key=value pairs fed into the workflow context.
    #[arg(long = "input", value_name = "KEY=VALUE")]
    inputs: Vec<String>,

    /// Instead of running a pipeline, send a single task to this agent.
    #[arg(long, value_name = "AGENT_NAME")]
    ask: Option<String>,

    /// The task to send when using --ask.
    #[arg(long, default_value = "Introduce yourself and describe what you can do")]
    task: String,

    /// List available pipelines and exit.
    #[arg(long, default_value_t = false)]
    list_pipelines: bool,

    /// Show the agents in the roster and exit.
    #[arg(long, default_value_t = false)]
    list_agents: bool,

    /// Nexus data directory.
    #[arg(long, default_value = ".nexus")]
    data_dir: PathBuf,

    /// Save the final workflow output to a file.
    #[arg(long)]
    output: Option<PathBuf>,
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    // --list-pipelines
    if args.list_pipelines {
        println!("Available pipelines:");
        for name in PIPELINE_NAMES {
            if let Some(dag) = pipeline_by_name(name) {
                let agents: Vec<_> = dag.steps.iter()
                    .map(|s| format!("{}", s.agent))
                    .collect::<std::collections::LinkedList<_>>()
                    .into_iter()
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();
                println!("  {:20} {} steps  agents: {}",
                    dag.name, dag.steps.len(), agents.join(", "));
                println!("    {}", dag.description);
            }
        }
        return Ok(());
    }

    // --list-agents
    if args.list_agents {
        println!("Nexus agent roster:");
        for name in AgentName::all() {
            let role = name.role();
            println!("  {:6}  {:30}  {}", name, role.domain, role.tagline);
        }
        return Ok(());
    }

    // Initialise state
    let state = NexusState::init(&args.data_dir).await?;
    let config = &state.roster_config;

    info!(
        provider = %config.provider,
        workspace = %config.workspace_dir.display(),
        "Nexus ready"
    );

    // --ask <agent>
    if let Some(ref agent_str) = args.ask {
        let agent: AgentName = agent_str
            .parse()
            .map_err(|e| anyhow::anyhow!("Unknown agent '{}': {}", agent_str, e))?;

        println!("Asking {} ({}): {}\n", agent, agent.role().domain, args.task);

        let reply = state.ask_agent(agent, &args.task).await?;
        println!("{}", reply);
        return Ok(());
    }

    // Run pipeline
    if pipeline_by_name(&args.pipeline).is_none() {
        anyhow::bail!(
            "Unknown pipeline '{}'. Available: {}",
            args.pipeline,
            PIPELINE_NAMES.join(", ")
        );
    }

    // Parse inputs
    let mut ctx = WorkflowContext::new();

    // Default context for app-build demo when no inputs provided
    if args.inputs.is_empty() && args.pipeline == "app-build" {
        ctx = ctx
            .with("requirements",
                "Build a production-ready task management REST API with user auth, \
                 projects, tasks (CRUD), due dates, and priority levels.")
            .with("stack", "Rust + Axum + PostgreSQL");
        println!("Using default demo requirements (pass --input to customise).\n");
    }

    for kv in &args.inputs {
        if let Some((k, v)) = kv.split_once('=') {
            ctx = ctx.with(k.trim(), v.trim());
        } else {
            anyhow::bail!("Invalid --input format: '{}' (expected key=value)", kv);
        }
    }

    println!("Running pipeline: {}", args.pipeline);
    println!("{}", "=".repeat(60));

    let result = state.run_pipeline(&args.pipeline, ctx).await?;

    println!("\n{}", "=".repeat(60));
    println!("Pipeline '{}' completed — {} steps", args.pipeline, result.steps.len());
    println!("{}", "=".repeat(60));

    for step in &result.steps {
        println!("\n[ {} ] Agent: {}", step.step_id, step.agent);
        println!("{}", "-".repeat(40));
        println!("{}", step.output);
    }

    println!("\n{}", "=".repeat(60));
    println!("FINAL OUTPUT");
    println!("{}", "=".repeat(60));
    println!("{}", result.final_output);

    if let Some(path) = args.output {
        std::fs::write(&path, &result.final_output)?;
        println!("\nSaved to: {}", path.display());
    }

    Ok(())
}
