//! nexus-sandbox — Sandboxed code execution for generated applications.
//!
//! Provides two sandbox backends:
//! - [`DockerSandbox`] — full container isolation via Docker CLI (preferred).
//! - [`ProcessSandbox`] — lightweight fallback using OS processes and temp directories.

pub mod docker;
pub mod error;
pub mod process;
pub mod sandbox;
pub mod wasm;

pub use error::SandboxError;
pub use sandbox::{ExecResult, NetworkPolicy, Sandbox, SandboxConfig, SandboxInstance, SandboxStatus};
pub use wasm::{WasmCapabilities, WasmExecResult, WasmSandbox, WasmSandboxConfig};
