use clap::{Parser, Subcommand};
use nexus_pkg::commands;

#[derive(Parser)]
#[command(
    name = "nexus-pkg",
    about = "Nexus Agent Package Manager",
    version,
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialise a new agent package in the current directory
    Init {
        /// Package name (defaults to directory name)
        name: Option<String>,
    },
    /// Install an agent package
    Install {
        /// Package name, optionally with @version (e.g. my-agent@1.0.0)
        package: String,
    },
    /// Uninstall an agent package
    Uninstall {
        package: String,
    },
    /// Update an installed agent package to the latest version
    Update {
        package: String,
    },
    /// List all installed agent packages
    List,
    /// Search the registry for agent packages
    Search {
        query: String,
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },
    /// Show detailed info about a registry package
    Info {
        package: String,
    },
    /// Publish the current agent package to the registry
    Publish {
        /// Registry authentication token
        #[arg(long, env = "NEXUS_REGISTRY_TOKEN")]
        token: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { name } => commands::cmd_init(name).await?,
        Commands::Install { package } => {
            let (name, version) = split_package_version(&package);
            commands::cmd_install(name, version.as_deref()).await?;
        }
        Commands::Uninstall { package } => commands::cmd_uninstall(&package).await?,
        Commands::Update { package } => commands::cmd_update(&package).await?,
        Commands::List => commands::cmd_list().await?,
        Commands::Search { query, limit } => commands::cmd_search(&query, limit).await?,
        Commands::Info { package } => commands::cmd_info(&package).await?,
        Commands::Publish { token } => commands::cmd_publish(&token).await?,
    }

    Ok(())
}

fn split_package_version(input: &str) -> (&str, Option<String>) {
    if let Some(idx) = input.rfind('@') {
        (&input[..idx], Some(input[idx + 1..].to_owned()))
    } else {
        (input, None)
    }
}
