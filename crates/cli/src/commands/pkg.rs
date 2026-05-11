use clap::Subcommand;
use crate::output::OutputFormat;

#[derive(Subcommand)]
pub enum PkgAction {
    /// Initialise a nexus.toml in the current directory
    Init {
        name: Option<String>,
    },
    /// Install an agent package (name[@version])
    Install {
        package: String,
        #[arg(long, env = "NEXUS_REGISTRY")]
        registry: Option<String>,
    },
    /// Uninstall an agent package
    Uninstall { package: String },
    /// Update an installed package to its latest version
    Update { package: String },
    /// List installed packages
    List,
    /// Search the registry
    Search {
        query: String,
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },
    /// Show detailed info for a package
    Info { package: String },
    /// Publish the current package to the registry
    Publish {
        #[arg(long, env = "NEXUS_REGISTRY_TOKEN")]
        token: String,
    },
}

pub async fn run(_server: &str, _format: &OutputFormat, action: &PkgAction) -> anyhow::Result<()> {
    match action {
        PkgAction::Init { name } => {
            nexus_pkg::commands::cmd_init(name.clone()).await?;
        }

        PkgAction::Install { package, registry } => {
            if let Some(url) = registry {
                unsafe { std::env::set_var("NEXUS_REGISTRY", url) };
            }
            let (name, version) = split_pkg(package);
            nexus_pkg::commands::cmd_install(name, version.as_deref()).await?;
        }

        PkgAction::Uninstall { package } => {
            nexus_pkg::commands::cmd_uninstall(package).await?;
        }

        PkgAction::Update { package } => {
            nexus_pkg::commands::cmd_update(package).await?;
        }

        PkgAction::List => {
            nexus_pkg::commands::cmd_list().await?;
        }

        PkgAction::Search { query, limit } => {
            nexus_pkg::commands::cmd_search(query, *limit).await?;
        }

        PkgAction::Info { package } => {
            nexus_pkg::commands::cmd_info(package).await?;
        }

        PkgAction::Publish { token } => {
            nexus_pkg::commands::cmd_publish(token).await?;
        }
    }
    Ok(())
}

fn split_pkg(input: &str) -> (&str, Option<String>) {
    if let Some(idx) = input.rfind('@') {
        (&input[..idx], Some(input[idx + 1..].to_owned()))
    } else {
        (input, None)
    }
}
