use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Debug, Error)]
pub enum PackageError {
    #[error("package not found: {0}")]
    NotFound(String),
    #[error("invalid package: {0}")]
    Invalid(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NxaManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub license: String,
    pub tags: Vec<String>,
    pub category: String,
    pub required_tools: Vec<String>,
    pub min_nexus_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NxaPackage {
    pub manifest: NxaManifest,
    pub agent_definition: serde_json::Value,
    pub reactive_config: Option<serde_json::Value>,
    pub prompts: HashMap<String, String>,
    pub readme: String,
}

pub struct PackageManager {
    packages_dir: PathBuf,
    installed: RwLock<HashMap<String, NxaPackage>>,
}

impl PackageManager {
    pub fn new(packages_dir: impl Into<PathBuf>) -> Self {
        Self {
            packages_dir: packages_dir.into(),
            installed: RwLock::new(HashMap::new()),
        }
    }

    pub async fn install(&self, package: NxaPackage) -> Result<(), PackageError> {
        let name = package.manifest.name.clone();
        let pkg_dir = self.packages_dir.join(&name);
        tokio::fs::create_dir_all(&pkg_dir).await?;
        let manifest_json = serde_json::to_string_pretty(&package)?;
        tokio::fs::write(pkg_dir.join("package.json"), manifest_json).await?;
        self.installed.write().await.insert(name, package);
        Ok(())
    }

    pub async fn uninstall(&self, name: &str) -> Result<(), PackageError> {
        self.installed
            .write()
            .await
            .remove(name)
            .ok_or_else(|| PackageError::NotFound(name.into()))?;
        let pkg_dir = self.packages_dir.join(name);
        if pkg_dir.exists() {
            tokio::fs::remove_dir_all(&pkg_dir).await?;
        }
        Ok(())
    }

    pub async fn list_installed(&self) -> Vec<NxaManifest> {
        self.installed
            .read()
            .await
            .values()
            .map(|p| p.manifest.clone())
            .collect()
    }

    pub async fn get_installed(&self, name: &str) -> Option<NxaPackage> {
        self.installed.read().await.get(name).cloned()
    }
}
