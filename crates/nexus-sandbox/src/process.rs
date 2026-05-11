//! Process-based sandbox backend (fallback).
//!
//! Runs commands in isolated temporary directories using OS processes.
//! Does not provide container-level isolation, but is always available
//! and useful for development or environments without Docker.

use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::{
    sandbox::{ExecResult, Sandbox, SandboxConfig, SandboxInstance, SandboxStatus},
    SandboxError,
};

/// Sandbox backend that runs commands as local processes in temporary directories.
///
/// This backend is always available and serves as a fallback when Docker is not
/// installed. It provides directory-level isolation but not true container isolation.
pub struct ProcessSandbox {
    /// Root directory where sandbox working directories are created.
    base_dir: PathBuf,
}

impl ProcessSandbox {
    /// Create a new process sandbox backend using the system temp directory.
    pub fn new() -> Self {
        Self {
            base_dir: std::env::temp_dir().join("nexus-sandboxes"),
        }
    }

    /// Create a new process sandbox backend with a custom base directory.
    pub fn with_base_dir(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    /// Get the working directory path for a sandbox instance.
    fn sandbox_dir(&self, sandbox_id: &str) -> PathBuf {
        self.base_dir.join(sandbox_id)
    }

    /// Get the log file path for a sandbox instance.
    fn log_file(&self, sandbox_id: &str) -> PathBuf {
        self.sandbox_dir(sandbox_id).join(".nexus-sandbox.log")
    }

    /// Append a message to the sandbox log file.
    async fn append_log(&self, sandbox_id: &str, message: &str) {
        use tokio::io::AsyncWriteExt;

        let log_path = self.log_file(sandbox_id);
        if let Ok(mut file) = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .await
        {
            let timestamped = format!(
                "[{}] {}\n",
                chrono::Utc::now().to_rfc3339(),
                message
            );
            let _ = file.write_all(timestamped.as_bytes()).await;
        }
    }
}

impl Default for ProcessSandbox {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Sandbox for ProcessSandbox {
    async fn create(&self, config: SandboxConfig) -> Result<SandboxInstance, SandboxError> {
        let sandbox_id = format!("nexus-proc-{}", Uuid::new_v4());
        let sandbox_dir = self.sandbox_dir(&sandbox_id);

        info!(sandbox_id = %sandbox_id, dir = %sandbox_dir.display(), "creating process sandbox");

        tokio::fs::create_dir_all(&sandbox_dir)
            .await
            .map_err(|e| {
                SandboxError::Create(format!(
                    "failed to create sandbox directory {}: {}",
                    sandbox_dir.display(),
                    e
                ))
            })?;

        // Write config to a metadata file for later reference
        let meta_path = sandbox_dir.join(".nexus-sandbox-meta.json");
        let meta_json = serde_json::to_string_pretty(&config).unwrap_or_default();
        tokio::fs::write(&meta_path, meta_json).await.map_err(|e| {
            SandboxError::Create(format!("failed to write sandbox metadata: {}", e))
        })?;

        self.append_log(&sandbox_id, "sandbox created").await;

        let now = chrono::Utc::now().to_rfc3339();

        Ok(SandboxInstance {
            id: sandbox_id,
            status: SandboxStatus::Running,
            config,
            created_at: now,
        })
    }

    async fn exec(
        &self,
        instance: &SandboxInstance,
        command: &str,
    ) -> Result<ExecResult, SandboxError> {
        if instance.status == SandboxStatus::Destroyed {
            return Err(SandboxError::Destroyed(instance.id.clone()));
        }

        let sandbox_dir = self.sandbox_dir(&instance.id);
        if !sandbox_dir.exists() {
            return Err(SandboxError::Destroyed(instance.id.clone()));
        }

        let timeout = std::time::Duration::from_secs(instance.config.timeout_secs);
        let start = std::time::Instant::now();

        debug!(
            sandbox_id = %instance.id,
            command = %command,
            dir = %sandbox_dir.display(),
            "executing in process sandbox"
        );

        self.append_log(&instance.id, &format!("exec: {}", command))
            .await;

        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg(command)
            .current_dir(&sandbox_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        // Set environment variables from config
        for (key, value) in &instance.config.env_vars {
            cmd.env(key, value);
        }

        let child = cmd
            .spawn()
            .map_err(|e| SandboxError::Exec(format!("failed to spawn process: {}", e)))?;

        let result = tokio::time::timeout(timeout, child.wait_with_output()).await;

        match result {
            Ok(Ok(output)) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let exit_code = output.status.code().unwrap_or(-1);

                self.append_log(
                    &instance.id,
                    &format!(
                        "exec completed: exit_code={}, duration_ms={}",
                        exit_code, duration_ms
                    ),
                )
                .await;

                Ok(ExecResult {
                    stdout,
                    stderr,
                    exit_code,
                    duration_ms,
                    timed_out: false,
                })
            }
            Ok(Err(e)) => Err(SandboxError::Exec(format!("process execution failed: {}", e))),
            Err(_) => {
                warn!(
                    sandbox_id = %instance.id,
                    timeout_secs = instance.config.timeout_secs,
                    "command execution timed out"
                );
                self.append_log(&instance.id, "exec timed out").await;
                Err(SandboxError::Timeout(instance.config.timeout_secs))
            }
        }
    }

    async fn copy_in(
        &self,
        instance: &SandboxInstance,
        src: &Path,
        dest: &str,
    ) -> Result<(), SandboxError> {
        if instance.status == SandboxStatus::Destroyed {
            return Err(SandboxError::Destroyed(instance.id.clone()));
        }

        let sandbox_dir = self.sandbox_dir(&instance.id);
        // Resolve dest relative to sandbox dir, stripping leading slash
        let dest_clean = dest.strip_prefix('/').unwrap_or(dest);
        let dest_path = sandbox_dir.join(dest_clean);

        debug!(
            sandbox_id = %instance.id,
            src = %src.display(),
            dest = %dest_path.display(),
            "copying file into process sandbox"
        );

        // Create parent directories
        if let Some(parent) = dest_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                SandboxError::CopyFailed(format!("failed to create destination directory: {}", e))
            })?;
        }

        if src.is_dir() {
            copy_dir_recursive(src, &dest_path).await.map_err(|e| {
                SandboxError::CopyFailed(format!("failed to copy directory: {}", e))
            })?;
        } else {
            tokio::fs::copy(src, &dest_path).await.map_err(|e| {
                SandboxError::CopyFailed(format!("failed to copy file: {}", e))
            })?;
        }

        Ok(())
    }

    async fn copy_out(
        &self,
        instance: &SandboxInstance,
        src: &str,
        dest: &Path,
    ) -> Result<(), SandboxError> {
        if instance.status == SandboxStatus::Destroyed {
            return Err(SandboxError::Destroyed(instance.id.clone()));
        }

        let sandbox_dir = self.sandbox_dir(&instance.id);
        let src_clean = src.strip_prefix('/').unwrap_or(src);
        let src_path = sandbox_dir.join(src_clean);

        debug!(
            sandbox_id = %instance.id,
            src = %src_path.display(),
            dest = %dest.display(),
            "copying file out of process sandbox"
        );

        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                SandboxError::CopyFailed(format!("failed to create destination directory: {}", e))
            })?;
        }

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, dest).await.map_err(|e| {
                SandboxError::CopyFailed(format!("failed to copy directory: {}", e))
            })?;
        } else {
            tokio::fs::copy(&src_path, dest).await.map_err(|e| {
                SandboxError::CopyFailed(format!("failed to copy file: {}", e))
            })?;
        }

        Ok(())
    }

    async fn logs(&self, instance: &SandboxInstance) -> Result<String, SandboxError> {
        let log_path = self.log_file(&instance.id);

        match tokio::fs::read_to_string(&log_path).await {
            Ok(contents) => Ok(contents),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(SandboxError::Exec(format!(
                "failed to read sandbox logs: {}",
                e
            ))),
        }
    }

    async fn destroy(&self, instance: &SandboxInstance) -> Result<(), SandboxError> {
        let sandbox_dir = self.sandbox_dir(&instance.id);

        info!(sandbox_id = %instance.id, dir = %sandbox_dir.display(), "destroying process sandbox");

        if sandbox_dir.exists() {
            tokio::fs::remove_dir_all(&sandbox_dir).await.map_err(|e| {
                SandboxError::Exec(format!(
                    "failed to remove sandbox directory {}: {}",
                    sandbox_dir.display(),
                    e
                ))
            })?;
        }

        Ok(())
    }

    async fn address(
        &self,
        _instance: &SandboxInstance,
        port: u16,
    ) -> Result<String, SandboxError> {
        // Process sandbox runs on localhost — just return the requested port.
        Ok(format!("127.0.0.1:{}", port))
    }

    fn is_available(&self) -> bool {
        true
    }
}

/// Recursively copy a directory from `src` to `dest`.
async fn copy_dir_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
    tokio::fs::create_dir_all(dest).await?;

    let mut entries = tokio::fs::read_dir(src).await?;
    while let Some(entry) = entries.next_entry().await? {
        let file_type = entry.file_type().await?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());

        if file_type.is_dir() {
            Box::pin(copy_dir_recursive(&src_path, &dest_path)).await?;
        } else {
            tokio::fs::copy(&src_path, &dest_path).await?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn process_sandbox_creates_temp_dir() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let sandbox = ProcessSandbox::with_base_dir(tmp.path().to_path_buf());

        let config = SandboxConfig::default();
        let instance = sandbox.create(config).await.expect("create sandbox");

        assert!(instance.id.starts_with("nexus-proc-"));
        assert_eq!(instance.status, SandboxStatus::Running);

        // Verify the directory was created
        let dir = sandbox.sandbox_dir(&instance.id);
        assert!(dir.exists());

        // Verify metadata file exists
        let meta = dir.join(".nexus-sandbox-meta.json");
        assert!(meta.exists());

        // Clean up
        sandbox.destroy(&instance).await.expect("destroy");
        assert!(!dir.exists());
    }

    #[tokio::test]
    async fn process_sandbox_executes_commands() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let sandbox = ProcessSandbox::with_base_dir(tmp.path().to_path_buf());

        let config = SandboxConfig::default();
        let instance = sandbox.create(config).await.expect("create sandbox");

        let result = sandbox
            .exec(&instance, "echo hello world")
            .await
            .expect("exec");

        assert_eq!(result.stdout.trim(), "hello world");
        assert_eq!(result.exit_code, 0);
        assert!(!result.timed_out);
        assert!(result.duration_ms < 5000);

        sandbox.destroy(&instance).await.expect("destroy");
    }

    #[tokio::test]
    async fn process_sandbox_captures_stderr() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let sandbox = ProcessSandbox::with_base_dir(tmp.path().to_path_buf());

        let config = SandboxConfig::default();
        let instance = sandbox.create(config).await.expect("create sandbox");

        let result = sandbox
            .exec(&instance, "echo error >&2")
            .await
            .expect("exec");

        assert_eq!(result.stderr.trim(), "error");

        sandbox.destroy(&instance).await.expect("destroy");
    }

    #[tokio::test]
    async fn process_sandbox_nonzero_exit_code() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let sandbox = ProcessSandbox::with_base_dir(tmp.path().to_path_buf());

        let config = SandboxConfig::default();
        let instance = sandbox.create(config).await.expect("create sandbox");

        let result = sandbox
            .exec(&instance, "exit 42")
            .await
            .expect("exec");

        assert_eq!(result.exit_code, 42);

        sandbox.destroy(&instance).await.expect("destroy");
    }

    #[tokio::test]
    async fn process_sandbox_copy_in_and_out() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let sandbox = ProcessSandbox::with_base_dir(tmp.path().to_path_buf());

        let config = SandboxConfig::default();
        let instance = sandbox.create(config).await.expect("create sandbox");

        // Create a source file
        let src_dir = tempfile::tempdir().expect("create src dir");
        let src_file = src_dir.path().join("test.txt");
        tokio::fs::write(&src_file, "hello from outside")
            .await
            .expect("write src");

        // Copy into sandbox
        sandbox
            .copy_in(&instance, &src_file, "/test.txt")
            .await
            .expect("copy in");

        // Verify it exists inside
        let result = sandbox
            .exec(&instance, "cat test.txt")
            .await
            .expect("exec cat");
        assert_eq!(result.stdout.trim(), "hello from outside");

        // Copy out
        let out_dir = tempfile::tempdir().expect("create out dir");
        let out_file = out_dir.path().join("copied.txt");
        sandbox
            .copy_out(&instance, "/test.txt", &out_file)
            .await
            .expect("copy out");

        let contents = tokio::fs::read_to_string(&out_file)
            .await
            .expect("read copied file");
        assert_eq!(contents, "hello from outside");

        sandbox.destroy(&instance).await.expect("destroy");
    }

    #[tokio::test]
    async fn process_sandbox_logs() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let sandbox = ProcessSandbox::with_base_dir(tmp.path().to_path_buf());

        let config = SandboxConfig::default();
        let instance = sandbox.create(config).await.expect("create sandbox");

        sandbox.exec(&instance, "echo test").await.expect("exec");

        let logs = sandbox.logs(&instance).await.expect("logs");
        assert!(logs.contains("sandbox created"));
        assert!(logs.contains("exec: echo test"));
        assert!(logs.contains("exec completed"));

        sandbox.destroy(&instance).await.expect("destroy");
    }

    #[tokio::test]
    async fn process_sandbox_address() {
        let sandbox = ProcessSandbox::new();
        let config = SandboxConfig::default();
        let instance = sandbox.create(config).await.expect("create sandbox");

        let addr = sandbox.address(&instance, 3000).await.expect("address");
        assert_eq!(addr, "127.0.0.1:3000");

        sandbox.destroy(&instance).await.expect("destroy");
    }

    #[test]
    fn process_sandbox_always_available() {
        let sandbox = ProcessSandbox::new();
        assert!(sandbox.is_available());
    }

    #[tokio::test]
    async fn process_sandbox_env_vars() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let sandbox = ProcessSandbox::with_base_dir(tmp.path().to_path_buf());

        let mut env = std::collections::HashMap::new();
        env.insert("MY_VAR".to_string(), "my_value".to_string());

        let config = SandboxConfig {
            env_vars: env,
            ..SandboxConfig::default()
        };
        let instance = sandbox.create(config).await.expect("create sandbox");

        let result = sandbox
            .exec(&instance, "echo $MY_VAR")
            .await
            .expect("exec");
        assert_eq!(result.stdout.trim(), "my_value");

        sandbox.destroy(&instance).await.expect("destroy");
    }
}
