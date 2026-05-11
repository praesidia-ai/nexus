use crate::installer::Installer;
use crate::manifest::AgentManifest;
use crate::registry::RegistryClient;
use std::path::PathBuf;
use tracing::info;

fn nexus_home() -> PathBuf {
    dirs_next::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("nexus")
}

fn registry_url() -> String {
    std::env::var("NEXUS_REGISTRY").unwrap_or_else(|_| crate::registry::DEFAULT_REGISTRY.to_owned())
}

pub async fn cmd_init(name: Option<String>) -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;
    let agent_name = name.unwrap_or_else(|| {
        cwd.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("my-agent")
            .to_owned()
    });
    let manifest_path = cwd.join("nexus.toml");
    if manifest_path.exists() {
        anyhow::bail!("nexus.toml already exists in the current directory");
    }
    let template = AgentManifest::default_template(&agent_name);
    std::fs::write(&manifest_path, &template)?;
    println!("Created nexus.toml for agent '{agent_name}'");
    Ok(())
}

pub async fn cmd_install(package: &str, version: Option<&str>) -> anyhow::Result<()> {
    let installer = Installer::new(nexus_home(), registry_url());
    let pkg = installer.install(package, version).await?;
    println!("Installed {}@{} -> {}", pkg.name, pkg.version, pkg.install_path.display());
    Ok(())
}

pub async fn cmd_uninstall(package: &str) -> anyhow::Result<()> {
    let installer = Installer::new(nexus_home(), registry_url());
    installer.uninstall(package).await?;
    println!("Uninstalled {package}");
    Ok(())
}

pub async fn cmd_update(package: &str) -> anyhow::Result<()> {
    let installer = Installer::new(nexus_home(), registry_url());
    let pkg = installer.update(package).await?;
    println!("Updated {} to v{}", pkg.name, pkg.version);
    Ok(())
}

pub async fn cmd_list() -> anyhow::Result<()> {
    let installer = Installer::new(nexus_home(), registry_url());
    let packages = installer.list_installed()?;
    if packages.is_empty() {
        println!("No packages installed.");
    } else {
        println!("{:<32} {:<16} PATH", "NAME", "VERSION");
        println!("{}", "-".repeat(80));
        for pkg in &packages {
            println!(
                "{:<32} {:<16} {}",
                pkg.name,
                pkg.version,
                pkg.install_path.display()
            );
        }
    }
    Ok(())
}

pub async fn cmd_search(query: &str, limit: usize) -> anyhow::Result<()> {
    let client = RegistryClient::new(registry_url());
    let results = client.search(query, limit).await?;
    if results.is_empty() {
        println!("No results for '{query}'");
    } else {
        println!("{:<32} {:<12} DOWNLOADS  DESCRIPTION", "NAME", "VERSION");
        println!("{}", "-".repeat(80));
        for entry in &results {
            println!(
                "{:<32} {:<12} {:<10} {}",
                entry.name,
                entry.latest_version,
                entry.downloads,
                entry.description.as_deref().unwrap_or("-")
            );
        }
    }
    Ok(())
}

pub async fn cmd_info(package: &str) -> anyhow::Result<()> {
    let client = RegistryClient::new(registry_url());
    let entry = client.get_entry(package).await?;
    println!("Name:        {}", entry.name);
    println!("Latest:      {}", entry.latest_version);
    println!("Downloads:   {}", entry.downloads);
    if let Some(r) = entry.rating {
        println!("Rating:      {:.1}/5", r);
    }
    println!("Description: {}", entry.description.as_deref().unwrap_or("-"));
    if !entry.keywords.is_empty() {
        println!("Keywords:    {}", entry.keywords.join(", "));
    }
    println!("\nVersions:");
    for v in &entry.versions {
        let yanked = if v.yanked { " [yanked]" } else { "" };
        println!("  {}  ({}){}", v.version, v.published_at.format("%Y-%m-%d"), yanked);
    }
    Ok(())
}

pub async fn cmd_publish(token: &str) -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;
    let manifest_path = cwd.join("nexus.toml");
    if !manifest_path.exists() {
        anyhow::bail!("nexus.toml not found. Run `nexus-pkg init` first.");
    }
    let content = std::fs::read_to_string(&manifest_path)?;
    let manifest = AgentManifest::from_toml(&content)?;

    // Pack the current directory into a tarball
    let tarball = pack_directory(&cwd)?;
    let client = RegistryClient::new(registry_url());
    client.publish(tarball, &manifest, token).await?;

    info!("Published {}@{}", manifest.package.name, manifest.package.version);
    println!("Published {}@{}", manifest.package.name, manifest.package.version);
    Ok(())
}

fn pack_directory(dir: &std::path::Path) -> anyhow::Result<Vec<u8>> {
    let buf = Vec::new();
    let enc = flate2::write::GzEncoder::new(buf, flate2::Compression::default());
    let mut tar = tar::Builder::new(enc);
    tar.append_dir_all(".", dir)?;
    let gz = tar.into_inner()?;
    Ok(gz.finish()?)
}
