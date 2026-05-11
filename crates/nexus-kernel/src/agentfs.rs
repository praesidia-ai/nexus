use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentFsError {
    #[error("access denied: {0}")]
    AccessDenied(String),
    #[error("path not found: {0}")]
    NotFound(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileOperation {
    Read,
    Write,
    Delete,
    List,
    Execute,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: PathBuf,
    pub size: u64,
    pub is_dir: bool,
}

pub struct AgentFS {
    root: PathBuf,
}

impl AgentFS {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn agent_home(&self, pid: &str) -> PathBuf {
        self.root.join("agents").join(pid)
    }

    pub fn project_shared(&self, project_id: &str) -> PathBuf {
        self.root
            .join("projects")
            .join(project_id)
            .join("shared")
    }

    pub fn shared_volume(&self, name: &str) -> PathBuf {
        self.root.join("volumes").join(name)
    }

    pub async fn create_volume(&self, name: &str) -> Result<PathBuf, AgentFsError> {
        let path = self.shared_volume(name);
        tokio::fs::create_dir_all(&path).await?;
        Ok(path)
    }

    pub fn check_access(
        &self,
        pid: &str,
        path: &Path,
        _op: &FileOperation,
    ) -> Result<(), AgentFsError> {
        let home = self.agent_home(pid);
        if path.starts_with(&home) || path.starts_with(self.root.join("volumes")) {
            Ok(())
        } else {
            Err(AgentFsError::AccessDenied(format!(
                "Agent {pid} cannot access {}",
                path.display()
            )))
        }
    }

    pub async fn list_agent_files(&self, pid: &str) -> Result<Vec<FileInfo>, AgentFsError> {
        let home = self.agent_home(pid);
        if !home.exists() {
            return Ok(vec![]);
        }
        let mut files = vec![];
        let mut entries = tokio::fs::read_dir(&home).await?;
        while let Ok(Some(entry)) = entries.next_entry().await {
            let meta = entry.metadata().await?;
            files.push(FileInfo {
                path: entry.path(),
                size: meta.len(),
                is_dir: meta.is_dir(),
            });
        }
        Ok(files)
    }

    pub async fn agent_storage_used(&self, pid: &str) -> Result<u64, AgentFsError> {
        let files = self.list_agent_files(pid).await?;
        Ok(files.iter().map(|f| f.size).sum())
    }

    pub async fn cleanup_agent(&self, pid: &str) -> Result<(), AgentFsError> {
        let home = self.agent_home(pid);
        if home.exists() {
            tokio::fs::remove_dir_all(&home).await?;
        }
        Ok(())
    }
}
