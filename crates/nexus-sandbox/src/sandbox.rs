//! Core sandbox trait and shared types.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Trait implemented by all sandbox backends (Docker, process, etc.).
///
/// Each method operates on a [`SandboxInstance`] handle returned by [`Sandbox::create`].
#[async_trait]
pub trait Sandbox: Send + Sync {
    /// Create a new sandbox instance from the given configuration.
    async fn create(&self, config: SandboxConfig) -> Result<SandboxInstance, super::SandboxError>;

    /// Execute a shell command inside the sandbox, returning stdout/stderr and exit code.
    async fn exec(
        &self,
        instance: &SandboxInstance,
        command: &str,
    ) -> Result<ExecResult, super::SandboxError>;

    /// Copy a local file or directory into the sandbox at the given destination path.
    async fn copy_in(
        &self,
        instance: &SandboxInstance,
        src: &std::path::Path,
        dest: &str,
    ) -> Result<(), super::SandboxError>;

    /// Copy a file or directory out of the sandbox to a local path.
    async fn copy_out(
        &self,
        instance: &SandboxInstance,
        src: &str,
        dest: &std::path::Path,
    ) -> Result<(), super::SandboxError>;

    /// Retrieve all logs (stdout + stderr) from the sandbox container or process.
    async fn logs(&self, instance: &SandboxInstance) -> Result<String, super::SandboxError>;

    /// Destroy the sandbox, releasing all resources.
    async fn destroy(&self, instance: &SandboxInstance) -> Result<(), super::SandboxError>;

    /// Get the address (host:port) for reaching a published port on the sandbox.
    async fn address(
        &self,
        instance: &SandboxInstance,
        port: u16,
    ) -> Result<String, super::SandboxError>;

    /// Check whether this sandbox backend is available on the current system.
    fn is_available(&self) -> bool;
}

/// Configuration for creating a new sandbox instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Container image to use (Docker backend) or base environment label (process backend).
    pub image: String,
    /// Memory limit in megabytes.
    pub memory_limit_mb: u64,
    /// CPU core limit (e.g. 1.0 = one full core).
    pub cpu_limit: f32,
    /// Maximum lifetime in seconds before the sandbox is automatically destroyed.
    pub timeout_secs: u64,
    /// Network access policy.
    pub network_policy: NetworkPolicy,
    /// Environment variables to set inside the sandbox.
    pub env_vars: HashMap<String, String>,
    /// Ports to expose from the sandbox.
    pub ports: Vec<u16>,
    /// Working directory inside the sandbox.
    pub working_dir: String,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            image: "node:20-slim".into(),
            memory_limit_mb: 512,
            cpu_limit: 1.0,
            timeout_secs: 300,
            network_policy: NetworkPolicy::Limited(vec![
                "registry.npmjs.org".into(),
                "cdn.jsdelivr.net".into(),
            ]),
            env_vars: HashMap::new(),
            ports: vec![3000],
            working_dir: "/app".into(),
        }
    }
}

/// Controls network access from within the sandbox.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NetworkPolicy {
    /// No network access at all.
    None,
    /// Access limited to the specified list of allowed hostnames.
    Limited(Vec<String>),
    /// Unrestricted network access.
    Full,
}

/// A running (or previously running) sandbox instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxInstance {
    /// Unique identifier for this sandbox (container ID or process tracking ID).
    pub id: String,
    /// Current lifecycle status.
    pub status: SandboxStatus,
    /// The configuration used to create this instance.
    pub config: SandboxConfig,
    /// ISO 8601 timestamp when the sandbox was created.
    pub created_at: String,
}

/// Lifecycle status of a sandbox instance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SandboxStatus {
    /// The sandbox is being created.
    Creating,
    /// The sandbox is running and accepting commands.
    Running,
    /// The sandbox has been stopped.
    Stopped,
    /// The sandbox has been destroyed and cannot be used.
    Destroyed,
    /// The sandbox encountered a fatal error.
    Failed(String),
}

/// Result of executing a command inside a sandbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecResult {
    /// Standard output captured from the command.
    pub stdout: String,
    /// Standard error captured from the command.
    pub stderr: String,
    /// Process exit code (0 = success).
    pub exit_code: i32,
    /// Wall-clock duration of the command in milliseconds.
    pub duration_ms: u64,
    /// Whether the command was killed due to timeout.
    pub timed_out: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_config_defaults() {
        let config = SandboxConfig::default();
        assert_eq!(config.image, "node:20-slim");
        assert_eq!(config.memory_limit_mb, 512);
        assert!((config.cpu_limit - 1.0).abs() < f32::EPSILON);
        assert_eq!(config.timeout_secs, 300);
        assert_eq!(
            config.network_policy,
            NetworkPolicy::Limited(vec![
                "registry.npmjs.org".into(),
                "cdn.jsdelivr.net".into()
            ])
        );
        assert!(config.env_vars.is_empty());
        assert_eq!(config.ports, vec![3000]);
        assert_eq!(config.working_dir, "/app");
    }

    #[test]
    fn sandbox_instance_serialization_roundtrip() {
        let instance = SandboxInstance {
            id: "test-123".into(),
            status: SandboxStatus::Running,
            config: SandboxConfig::default(),
            created_at: "2026-04-04T00:00:00Z".into(),
        };

        let json = serde_json::to_string(&instance).expect("serialize");
        let restored: SandboxInstance = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.id, "test-123");
        assert_eq!(restored.status, SandboxStatus::Running);
        assert_eq!(restored.created_at, "2026-04-04T00:00:00Z");
    }

    #[test]
    fn exec_result_serialization() {
        let result = ExecResult {
            stdout: "hello world\n".into(),
            stderr: String::new(),
            exit_code: 0,
            duration_ms: 42,
            timed_out: false,
        };

        let json = serde_json::to_string(&result).expect("serialize");
        let restored: ExecResult = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.stdout, "hello world\n");
        assert_eq!(restored.exit_code, 0);
        assert!(!restored.timed_out);
    }

    #[test]
    fn network_policy_variants() {
        let none = NetworkPolicy::None;
        let limited = NetworkPolicy::Limited(vec!["example.com".into()]);
        let full = NetworkPolicy::Full;

        assert_ne!(none, full);
        assert_ne!(limited, full);
        assert_eq!(none, NetworkPolicy::None);

        // Serialize each variant
        let none_json = serde_json::to_string(&none).expect("serialize");
        let limited_json = serde_json::to_string(&limited).expect("serialize");
        let full_json = serde_json::to_string(&full).expect("serialize");

        assert!(none_json.contains("None"));
        assert!(limited_json.contains("example.com"));
        assert!(full_json.contains("Full"));
    }

    #[test]
    fn sandbox_status_failed_variant() {
        let failed = SandboxStatus::Failed("out of memory".into());
        let json = serde_json::to_string(&failed).expect("serialize");
        let restored: SandboxStatus = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, SandboxStatus::Failed("out of memory".into()));
    }
}
