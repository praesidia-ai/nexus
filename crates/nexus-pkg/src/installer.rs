use crate::error::{PkgError, PkgResult};
use crate::manifest::AgentManifest;
use crate::registry::{InstalledPackage, LocalRegistry, RegistryClient};
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};
use tracing::{info, warn};

pub struct Installer {
    nexus_home: PathBuf,
    client: RegistryClient,
}

impl Installer {
    pub fn new(nexus_home: PathBuf, registry_url: impl Into<String>) -> Self {
        Self {
            nexus_home,
            client: RegistryClient::new(registry_url),
        }
    }

    pub fn with_default_registry(nexus_home: PathBuf) -> Self {
        Self {
            nexus_home,
            client: RegistryClient::default(),
        }
    }

    fn packages_dir(&self) -> PathBuf {
        self.nexus_home.join("packages")
    }

    pub async fn install(&self, name: &str, version_req: Option<&str>) -> PkgResult<InstalledPackage> {
        let mut db = LocalRegistry::load(&self.nexus_home)
            .map_err(|e| PkgError::Install(e.to_string()))?;

        let entry = self.client.get_entry(name).await?;

        let record = if let Some(req) = version_req {
            entry
                .versions
                .iter()
                .find(|v| !v.yanked && semver_matches(&v.version, req))
                .ok_or_else(|| PkgError::NotFound(format!("{name}@{req}")))?
        } else {
            entry
                .versions
                .iter()
                .filter(|v| !v.yanked)
                .max_by(|a, b| {
                    let va = semver::Version::parse(&a.version)
                        .unwrap_or(semver::Version::new(0, 0, 0));
                    let vb = semver::Version::parse(&b.version)
                        .unwrap_or(semver::Version::new(0, 0, 0));
                    va.cmp(&vb)
                })
                .ok_or_else(|| PkgError::NotFound(format!("{name}: no versions")))?
        };

        info!("Installing {name}@{}", record.version);
        let tarball = self.client.download(record).await?;
        let install_path = self.extract(&tarball, name, &record.version)?;
        let manifest = load_manifest_from(&install_path)?;
        let checksum = hex::encode(Sha256::digest(&tarball));

        let pkg = InstalledPackage {
            name: name.to_owned(),
            version: record.version.clone(),
            install_path: install_path.clone(),
            checksum,
            installed_at: Utc::now(),
            manifest,
        };

        db.insert(pkg.clone());
        db.save().map_err(|e| PkgError::Install(e.to_string()))?;

        info!("Installed {name}@{} at {}", record.version, install_path.display());
        Ok(pkg)
    }

    pub async fn uninstall(&self, name: &str) -> PkgResult<()> {
        let mut db = LocalRegistry::load(&self.nexus_home)
            .map_err(|e| PkgError::Install(e.to_string()))?;

        let pkg = db.get(name).ok_or_else(|| PkgError::NotFound(name.to_owned()))?;
        let path = pkg.install_path.clone();

        if path.exists() {
            std::fs::remove_dir_all(&path)?;
        }

        db.remove(name);
        db.save().map_err(|e| PkgError::Install(e.to_string()))?;

        info!("Uninstalled {name}");
        Ok(())
    }

    pub async fn update(&self, name: &str) -> PkgResult<InstalledPackage> {
        self.uninstall(name).await.unwrap_or_else(|e| {
            warn!("uninstall before update: {e}");
        });
        self.install(name, None).await
    }

    pub fn list_installed(&self) -> anyhow::Result<Vec<InstalledPackage>> {
        let db = LocalRegistry::load(&self.nexus_home)?;
        Ok(db.list().into_iter().cloned().collect())
    }

    fn extract(&self, tarball: &[u8], name: &str, version: &str) -> PkgResult<PathBuf> {
        use std::io::Cursor;
        let dest = self.packages_dir().join(format!("{name}-{version}"));
        std::fs::create_dir_all(&dest)?;

        // Canonicalize the destination so we can compare against it for each
        // entry. If that fails for some reason (e.g. the path doesn't fully
        // resolve yet), fall back to the raw path — the per-entry check still
        // rejects absolute paths and parent traversal components.
        let dest_canon = dest.canonicalize().unwrap_or_else(|_| dest.clone());

        let gz = flate2::read::GzDecoder::new(Cursor::new(tarball));
        let mut archive = tar::Archive::new(gz);

        // Manual per-entry unpack: the default `archive.unpack(&dest)` does
        // not validate against zip-slip — a crafted tarball with paths like
        // `../../../.env` happily escapes the destination. Every entry is
        // vetted here before it hits the filesystem.
        for entry in archive
            .entries()
            .map_err(|e| PkgError::Install(format!("read archive: {e}")))?
        {
            let mut entry = entry.map_err(|e| PkgError::Install(format!("read entry: {e}")))?;
            let rel = entry
                .path()
                .map_err(|e| PkgError::Install(format!("read entry path: {e}")))?
                .into_owned();

            if rel.is_absolute() {
                return Err(PkgError::Install(format!(
                    "refusing absolute path in package: {}",
                    rel.display()
                )));
            }
            for comp in rel.components() {
                match comp {
                    Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                        return Err(PkgError::Install(format!(
                            "refusing traversal component in package path: {}",
                            rel.display()
                        )));
                    }
                    _ => {}
                }
            }

            let target = dest.join(&rel);
            // Ensure the resolved parent still lives inside `dest`. We check the
            // parent (not the leaf) because the leaf may not exist yet.
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| PkgError::Install(format!("mkdir {}: {e}", parent.display())))?;
                let parent_canon = parent
                    .canonicalize()
                    .map_err(|e| PkgError::Install(format!("canonicalize {}: {e}", parent.display())))?;
                if !parent_canon.starts_with(&dest_canon) {
                    return Err(PkgError::Install(format!(
                        "package entry escapes destination: {}",
                        rel.display()
                    )));
                }
            }

            entry
                .unpack(&target)
                .map_err(|e| PkgError::Install(format!("unpack {}: {e}", rel.display())))?;
        }

        Ok(dest)
    }
}

fn load_manifest_from(dir: &Path) -> PkgResult<AgentManifest> {
    let manifest_path = dir.join("nexus.toml");
    if !manifest_path.exists() {
        return Err(PkgError::Manifest(format!(
            "nexus.toml not found in {}",
            dir.display()
        )));
    }
    let content = std::fs::read_to_string(&manifest_path)?;
    AgentManifest::from_toml(&content).map_err(|e| PkgError::Manifest(e.to_string()))
}

fn semver_matches(version: &str, req: &str) -> bool {
    let Ok(v) = semver::Version::parse(version) else {
        return false;
    };
    let Ok(r) = semver::VersionReq::parse(req) else {
        return false;
    };
    r.matches(&v)
}
