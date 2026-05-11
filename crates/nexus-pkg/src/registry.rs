use crate::error::{PkgError, PkgResult};
use crate::manifest::AgentManifest;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

pub const DEFAULT_REGISTRY: &str = "https://registry.nexus.run";

/// A package entry in the remote registry index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryEntry {
    pub name: String,
    pub latest_version: String,
    pub versions: Vec<VersionRecord>,
    pub description: Option<String>,
    pub keywords: Vec<String>,
    pub categories: Vec<String>,
    pub downloads: u64,
    pub rating: Option<f32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionRecord {
    pub version: String,
    pub tarball_url: String,
    pub checksum_sha256: String,
    pub published_at: DateTime<Utc>,
    pub yanked: bool,
}

/// A locally installed package record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPackage {
    pub name: String,
    pub version: String,
    pub install_path: std::path::PathBuf,
    pub checksum: String,
    pub installed_at: DateTime<Utc>,
    pub manifest: AgentManifest,
}

/// Client for the Nexus package registry.
pub struct RegistryClient {
    base_url: String,
    http: reqwest::Client,
}

impl Default for RegistryClient {
    fn default() -> Self {
        Self::new(DEFAULT_REGISTRY)
    }
}

impl RegistryClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            http: reqwest::Client::new(),
        }
    }

    /// Search for packages matching the query.
    pub async fn search(&self, query: &str, limit: usize) -> PkgResult<Vec<RegistryEntry>> {
        let url = format!("{}/api/v1/search?q={}&limit={}", self.base_url, query, limit);
        let resp = self.http.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(PkgError::Registry(format!("HTTP {}", resp.status())));
        }
        Ok(resp.json::<Vec<RegistryEntry>>().await?)
    }

    /// Fetch metadata for a specific package.
    pub async fn get_entry(&self, name: &str) -> PkgResult<RegistryEntry> {
        let url = format!("{}/api/v1/packages/{}", self.base_url, name);
        let resp = self.http.get(&url).send().await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(PkgError::NotFound(name.to_owned()));
        }
        if !resp.status().is_success() {
            return Err(PkgError::Registry(format!("HTTP {}", resp.status())));
        }
        Ok(resp.json::<RegistryEntry>().await?)
    }

    /// Download a package tarball, verifying the SHA-256 checksum.
    pub async fn download(&self, entry: &VersionRecord) -> PkgResult<Vec<u8>> {
        let bytes = self.http.get(&entry.tarball_url).send().await?.bytes().await?;
        let digest = hex::encode(Sha256::digest(&bytes));
        if digest != entry.checksum_sha256 {
            return Err(PkgError::ChecksumMismatch(entry.version.clone()));
        }
        Ok(bytes.to_vec())
    }

    /// Publish a package to the registry.
    pub async fn publish(
        &self,
        tarball: Vec<u8>,
        manifest: &AgentManifest,
        token: &str,
    ) -> PkgResult<()> {
        let checksum = hex::encode(Sha256::digest(&tarball));
        let meta = serde_json::json!({
            "name": manifest.package.name,
            "version": manifest.package.version,
            "checksum_sha256": checksum,
            "description": manifest.package.description,
            "keywords": manifest.package.keywords,
            "categories": manifest.package.categories,
        });
        let url = format!("{}/api/v1/packages/publish", self.base_url);
        let form = reqwest::multipart::Form::new()
            .text("meta", meta.to_string())
            .part("tarball", reqwest::multipart::Part::bytes(tarball).file_name("agent.tar.gz"));
        let resp = self
            .http
            .post(&url)
            .bearer_auth(token)
            .multipart(form)
            .send()
            .await?;
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(PkgError::Registry(format!("Publish failed: {}", body)));
        }
        Ok(())
    }
}

/// Local package database (stored as JSON sidecar).
pub struct LocalRegistry {
    db_path: std::path::PathBuf,
    packages: HashMap<String, InstalledPackage>,
}

impl LocalRegistry {
    pub fn load(nexus_home: &std::path::Path) -> anyhow::Result<Self> {
        let db_path = nexus_home.join("pkg-db.json");
        let packages = if db_path.exists() {
            let raw = std::fs::read_to_string(&db_path)?;
            serde_json::from_str::<HashMap<String, InstalledPackage>>(&raw)?
        } else {
            HashMap::new()
        };
        Ok(Self { db_path, packages })
    }

    pub fn save(&self) -> anyhow::Result<()> {
        if let Some(parent) = self.db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let raw = serde_json::to_string_pretty(&self.packages)?;
        std::fs::write(&self.db_path, raw)?;
        Ok(())
    }

    pub fn list(&self) -> Vec<&InstalledPackage> {
        self.packages.values().collect()
    }

    pub fn get(&self, name: &str) -> Option<&InstalledPackage> {
        self.packages.get(name)
    }

    pub fn insert(&mut self, pkg: InstalledPackage) {
        self.packages.insert(pkg.name.clone(), pkg);
    }

    pub fn remove(&mut self, name: &str) -> bool {
        self.packages.remove(name).is_some()
    }
}
