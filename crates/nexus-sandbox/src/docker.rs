//! Docker-based sandbox backend.
//!
//! Uses the Docker CLI (`docker`) via [`tokio::process::Command`] to create and manage
//! isolated containers. This avoids a dependency on `bollard` while providing full
//! container isolation with resource limits, network policies, and port mapping.

use async_trait::async_trait;
use std::path::Path;
use tokio::process::Command;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::{
    sandbox::{ExecResult, NetworkPolicy, Sandbox, SandboxConfig, SandboxInstance, SandboxStatus},
    SandboxError,
};

/// Sandbox backend that runs commands inside Docker containers.
///
/// Requires Docker to be installed and the `docker` CLI to be on `$PATH`.
pub struct DockerSandbox;

impl DockerSandbox {
    /// Create a new Docker sandbox backend.
    pub fn new() -> Self {
        Self
    }

    /// Check whether Docker is installed and the daemon is reachable.
    fn is_docker_available() -> bool {
        std::process::Command::new("docker")
            .arg("version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Build the `docker create` argument list from a [`SandboxConfig`].
    fn build_create_args(container_name: &str, config: &SandboxConfig) -> Vec<String> {
        let mut args = vec![
            "create".to_string(),
            "--name".to_string(),
            container_name.to_string(),
            // Resource limits
            "--memory".to_string(),
            format!("{}m", config.memory_limit_mb),
            "--cpus".to_string(),
            format!("{:.2}", config.cpu_limit),
            // Working directory
            "--workdir".to_string(),
            config.working_dir.clone(),
            // Keep stdin open so the container stays alive
            "-i".to_string(),
        ];

        // Network policy
        match &config.network_policy {
            NetworkPolicy::None => {
                args.push("--network".to_string());
                args.push("none".to_string());
            }
            NetworkPolicy::Limited(_hosts) => {
                // Docker doesn't natively support host-level allowlists without iptables.
                // We use the default bridge network; the caller should set up firewall rules
                // or a custom network plugin for fine-grained host filtering.
                // For now, we log the restriction for observability.
                debug!(
                    allowed_hosts = ?_hosts,
                    "limited network policy requested — using default bridge network"
                );
            }
            NetworkPolicy::Full => {
                // Default bridge network, no restrictions.
            }
        }

        // Port mappings — map container port to a random host port
        for port in &config.ports {
            args.push("-p".to_string());
            args.push(format!("0:{}", port));
        }

        // Environment variables
        for (key, value) in &config.env_vars {
            args.push("-e".to_string());
            args.push(format!("{}={}", key, value));
        }

        // Image
        args.push(config.image.clone());

        // Keep the container alive with a long sleep so we can docker exec into it
        args.push("sleep".to_string());
        args.push(config.timeout_secs.to_string());

        args
    }
}

impl Default for DockerSandbox {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Sandbox for DockerSandbox {
    async fn create(&self, config: SandboxConfig) -> Result<SandboxInstance, SandboxError> {
        if !Self::is_docker_available() {
            return Err(SandboxError::NotAvailable(
                "Docker is not installed or the daemon is not running".into(),
            ));
        }

        let sandbox_id = format!("nexus-sandbox-{}", Uuid::new_v4());
        let args = Self::build_create_args(&sandbox_id, &config);

        info!(sandbox_id = %sandbox_id, image = %config.image, "creating docker sandbox");

        let output = Command::new("docker")
            .args(&args)
            .output()
            .await
            .map_err(|e| SandboxError::Create(format!("failed to invoke docker: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SandboxError::Create(format!(
                "docker create failed: {}",
                stderr.trim()
            )));
        }

        // Start the container
        let start_output = Command::new("docker")
            .args(["start", &sandbox_id])
            .output()
            .await
            .map_err(|e| SandboxError::Create(format!("failed to start container: {}", e)))?;

        if !start_output.status.success() {
            let stderr = String::from_utf8_lossy(&start_output.stderr);
            // Clean up the created container
            let _ = Command::new("docker")
                .args(["rm", "-f", &sandbox_id])
                .output()
                .await;
            return Err(SandboxError::Create(format!(
                "docker start failed: {}",
                stderr.trim()
            )));
        }

        let now = chrono::Utc::now().to_rfc3339();

        info!(sandbox_id = %sandbox_id, "docker sandbox created and started");

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

        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(instance.config.timeout_secs);

        debug!(sandbox_id = %instance.id, command = %command, "executing in docker sandbox");

        let child = Command::new("docker")
            .args([
                "exec",
                &instance.id,
                "sh",
                "-c",
                command,
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| SandboxError::Exec(format!("failed to spawn docker exec: {}", e)))?;

        let result = tokio::time::timeout(timeout, child.wait_with_output()).await;

        match result {
            Ok(Ok(output)) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                Ok(ExecResult {
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                    exit_code: output.status.code().unwrap_or(-1),
                    duration_ms,
                    timed_out: false,
                })
            }
            Ok(Err(e)) => Err(SandboxError::Exec(format!("docker exec failed: {}", e))),
            Err(_) => {
                warn!(
                    sandbox_id = %instance.id,
                    timeout_secs = instance.config.timeout_secs,
                    "command execution timed out"
                );
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

        let src_str = src.to_string_lossy();
        let target = format!("{}:{}", instance.id, dest);

        debug!(sandbox_id = %instance.id, src = %src_str, dest = %dest, "copying file into sandbox");

        let output = Command::new("docker")
            .args(["cp", &src_str, &target])
            .output()
            .await
            .map_err(|e| SandboxError::CopyFailed(format!("failed to invoke docker cp: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SandboxError::CopyFailed(format!(
                "docker cp into sandbox failed: {}",
                stderr.trim()
            )));
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

        let source = format!("{}:{}", instance.id, src);
        let dest_str = dest.to_string_lossy();

        debug!(sandbox_id = %instance.id, src = %src, dest = %dest_str, "copying file out of sandbox");

        let output = Command::new("docker")
            .args(["cp", &source, &dest_str])
            .output()
            .await
            .map_err(|e| SandboxError::CopyFailed(format!("failed to invoke docker cp: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SandboxError::CopyFailed(format!(
                "docker cp out of sandbox failed: {}",
                stderr.trim()
            )));
        }

        Ok(())
    }

    async fn logs(&self, instance: &SandboxInstance) -> Result<String, SandboxError> {
        let output = Command::new("docker")
            .args(["logs", &instance.id])
            .output()
            .await
            .map_err(|e| SandboxError::Exec(format!("failed to get docker logs: {}", e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Combine stdout and stderr as docker logs outputs both
        let mut combined = stdout.to_string();
        if !stderr.is_empty() {
            combined.push_str(&stderr);
        }

        Ok(combined)
    }

    async fn destroy(&self, instance: &SandboxInstance) -> Result<(), SandboxError> {
        info!(sandbox_id = %instance.id, "destroying docker sandbox");

        let output = Command::new("docker")
            .args(["rm", "-f", &instance.id])
            .output()
            .await
            .map_err(|e| SandboxError::Exec(format!("failed to destroy container: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(
                sandbox_id = %instance.id,
                stderr = %stderr.trim(),
                "docker rm returned non-zero (container may already be removed)"
            );
        }

        Ok(())
    }

    async fn address(
        &self,
        instance: &SandboxInstance,
        port: u16,
    ) -> Result<String, SandboxError> {
        if instance.status == SandboxStatus::Destroyed {
            return Err(SandboxError::Destroyed(instance.id.clone()));
        }

        // Use docker port to find the mapped host port
        let output = Command::new("docker")
            .args(["port", &instance.id, &port.to_string()])
            .output()
            .await
            .map_err(|e| {
                SandboxError::Exec(format!("failed to query docker port mapping: {}", e))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SandboxError::Exec(format!(
                "docker port query failed: {}",
                stderr.trim()
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        // Output is like "0.0.0.0:32768" or ":::32768" — take the first line
        let address = stdout
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .to_string();

        if address.is_empty() {
            return Err(SandboxError::Exec(format!(
                "no port mapping found for port {}",
                port
            )));
        }

        // Normalize IPv6 wildcard to localhost
        let normalized = address
            .replace("0.0.0.0", "127.0.0.1")
            .replace(":::", "127.0.0.1:");

        Ok(normalized)
    }

    fn is_available(&self) -> bool {
        Self::is_docker_available()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_availability_detection() {
        // This test just verifies the function runs without panicking.
        // The result depends on whether Docker is installed on the test machine.
        let _available = DockerSandbox::is_docker_available();
    }

    #[test]
    fn build_create_args_default_config() {
        let config = SandboxConfig::default();
        let args = DockerSandbox::build_create_args("test-container", &config);

        assert!(args.contains(&"create".to_string()));
        assert!(args.contains(&"--name".to_string()));
        assert!(args.contains(&"test-container".to_string()));
        assert!(args.contains(&"--memory".to_string()));
        assert!(args.contains(&"512m".to_string()));
        assert!(args.contains(&"--cpus".to_string()));
        assert!(args.contains(&"node:20-slim".to_string()));
        // Should map port 3000
        assert!(args.contains(&"-p".to_string()));
        assert!(args.contains(&"0:3000".to_string()));
    }

    #[test]
    fn build_create_args_no_network() {
        let config = SandboxConfig {
            network_policy: NetworkPolicy::None,
            ..SandboxConfig::default()
        };
        let args = DockerSandbox::build_create_args("test", &config);
        assert!(args.contains(&"--network".to_string()));
        assert!(args.contains(&"none".to_string()));
    }

    #[test]
    fn build_create_args_with_env_vars() {
        let mut env = std::collections::HashMap::new();
        env.insert("NODE_ENV".to_string(), "production".to_string());

        let config = SandboxConfig {
            env_vars: env,
            ..SandboxConfig::default()
        };
        let args = DockerSandbox::build_create_args("test", &config);
        assert!(args.contains(&"-e".to_string()));
        assert!(args.contains(&"NODE_ENV=production".to_string()));
    }

    #[test]
    fn build_create_args_multiple_ports() {
        let config = SandboxConfig {
            ports: vec![3000, 5432, 6379],
            ..SandboxConfig::default()
        };
        let args = DockerSandbox::build_create_args("test", &config);

        let port_count = args.iter().filter(|a| a.as_str() == "-p").count();
        assert_eq!(port_count, 3);
        assert!(args.contains(&"0:3000".to_string()));
        assert!(args.contains(&"0:5432".to_string()));
        assert!(args.contains(&"0:6379".to_string()));
    }
}
