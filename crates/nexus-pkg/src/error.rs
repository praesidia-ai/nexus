use thiserror::Error;

#[derive(Debug, Error)]
pub enum PkgError {
    #[error("manifest error: {0}")]
    Manifest(String),
    #[error("registry error: {0}")]
    Registry(String),
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("package not found: {0}")]
    NotFound(String),
    #[error("version conflict: {0}")]
    VersionConflict(String),
    #[error("checksum mismatch for {0}")]
    ChecksumMismatch(String),
    #[error("install error: {0}")]
    Install(String),
}

pub type PkgResult<T> = Result<T, PkgError>;
