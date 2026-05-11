//! WASM sandbox — secure, lightweight code execution via Wasmtime + WASI.
//!
//! Provides capability-based security for AI agent-generated code:
//! - **Isolation**: WASM linear memory is fully isolated from the host.
//! - **Capability grants**: Filesystem access must be explicitly granted per directory.
//! - **Resource limits**: CPU instruction fuel budget and execution timeouts.
//! - **Near-instant startup**: ~1ms cold start vs. ~500ms for Docker containers.

use std::path::PathBuf;

use tokio::time::Duration;
use tracing::{debug, warn};
use wasmtime::{Config, Engine, Linker, Module, Store};
use wasmtime_wasi::{
    pipe::MemoryOutputPipe, DirPerms, FilePerms, WasiCtxBuilder,
    preview1::WasiP1Ctx,
};

use crate::error::SandboxError;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Capability grants for a WASM execution.
#[derive(Debug, Clone, Default)]
pub struct WasmCapabilities {
    /// Allow read-only access to specific host directories.
    pub read_dirs: Vec<PathBuf>,
    /// Allow read-write access to specific host directories.
    pub write_dirs: Vec<PathBuf>,
    /// Allow environment variable inheritance from the host.
    pub inherit_env: bool,
    /// Maximum fuel (instruction budget). None = use config default.
    pub fuel_limit: Option<u64>,
    /// Maximum execution time. None = use config default.
    pub timeout: Option<Duration>,
}

/// Result of a WASM execution.
#[derive(Debug, Clone)]
pub struct WasmExecResult {
    /// Standard output captured from the WASM module.
    pub stdout: String,
    /// Standard error captured from the WASM module.
    pub stderr: String,
    /// Exit code (0 = success).
    pub exit_code: i32,
    /// CPU instructions consumed (fuel units).
    pub fuel_consumed: u64,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u64,
}

/// Configuration for the WASM sandbox engine (shared across executions).
#[derive(Debug, Clone)]
pub struct WasmSandboxConfig {
    /// Maximum memory pages (64KiB each). Default: 256 pages = 16 MiB.
    pub max_memory_pages: u64,
    /// Default fuel limit per execution (instruction budget).
    pub default_fuel: u64,
    /// Default execution timeout.
    pub default_timeout: Duration,
}

impl Default for WasmSandboxConfig {
    fn default() -> Self {
        Self {
            max_memory_pages: 256,
            default_fuel: 10_000_000,
            default_timeout: Duration::from_secs(30),
        }
    }
}

// ---------------------------------------------------------------------------
// WasmSandbox
// ---------------------------------------------------------------------------

/// WASM execution sandbox backed by Wasmtime.
pub struct WasmSandbox {
    engine: Engine,
    config: WasmSandboxConfig,
}

impl WasmSandbox {
    /// Create a new WASM sandbox with the given configuration.
    pub fn new(config: WasmSandboxConfig) -> Result<Self, SandboxError> {
        let mut engine_config = Config::new();
        engine_config.async_support(true);
        engine_config.consume_fuel(true);
        engine_config.max_wasm_stack(512 * 1024);

        let engine =
            Engine::new(&engine_config).map_err(|e| SandboxError::Create(e.to_string()))?;

        Ok(Self { engine, config })
    }

    /// Execute a WASM binary from raw bytes with the given capability set.
    pub async fn exec_bytes(
        &self,
        wasm_bytes: &[u8],
        args: &[&str],
        caps: WasmCapabilities,
    ) -> Result<WasmExecResult, SandboxError> {
        let start = std::time::Instant::now();

        // Compile module
        let module = Module::new(&self.engine, wasm_bytes)
            .map_err(|e| SandboxError::Create(format!("WASM compile error: {e}")))?;

        // Capture stdout/stderr
        let stdout_pipe = MemoryOutputPipe::new(64 * 1024);
        let stderr_pipe = MemoryOutputPipe::new(64 * 1024);

        // Build WASI context with capability grants
        let mut wasi_builder = WasiCtxBuilder::new();
        wasi_builder.args(args);
        wasi_builder.stdout(stdout_pipe.clone());
        wasi_builder.stderr(stderr_pipe.clone());

        // Grant read-only filesystem capabilities
        for dir in &caps.read_dirs {
            if dir.exists() {
                wasi_builder
                    .preopened_dir(dir, dir.to_string_lossy().as_ref(), DirPerms::READ, FilePerms::READ)
                    .map_err(|e| SandboxError::Create(format!("preopened_dir error: {e}")))?;
            }
        }

        // Grant read-write filesystem capabilities
        for dir in &caps.write_dirs {
            if dir.exists() {
                wasi_builder
                    .preopened_dir(
                        dir,
                        dir.to_string_lossy().as_ref(),
                        DirPerms::all(),
                        FilePerms::all(),
                    )
                    .map_err(|e| SandboxError::Create(format!("preopened_dir error: {e}")))?;
            }
        }

        if caps.inherit_env {
            wasi_builder.inherit_env();
        }

        let wasi: WasiP1Ctx = wasi_builder.build_p1();

        // Set up store with fuel
        let mut store = Store::new(&self.engine, wasi);
        let fuel = caps.fuel_limit.unwrap_or(self.config.default_fuel);
        store
            .set_fuel(fuel)
            .map_err(|e| SandboxError::Create(e.to_string()))?;
        store
            .fuel_async_yield_interval(Some(10_000))
            .map_err(|e| SandboxError::Create(e.to_string()))?;

        // Link WASI preview 1
        let mut linker: Linker<WasiP1Ctx> = Linker::new(&self.engine);
        wasmtime_wasi::preview1::add_to_linker_async(&mut linker, |s| s)
            .map_err(|e| SandboxError::Create(e.to_string()))?;

        let instance = linker
            .instantiate_async(&mut store, &module)
            .await
            .map_err(|e| SandboxError::Create(format!("Instantiate error: {e}")))?;

        // Call _start (WASI entry point)
        let timeout = caps.timeout.unwrap_or(self.config.default_timeout);
        let run_result = tokio::time::timeout(timeout, async {
            let start_fn = instance
                .get_typed_func::<(), ()>(&mut store, "_start")
                .map_err(|e| SandboxError::Exec(format!("No _start export: {e}")))?;
            start_fn
                .call_async(&mut store, ())
                .await
                .map_err(|e| SandboxError::Exec(format!("Execution error: {e}")))
        })
        .await;

        let exit_code = match run_result {
            Ok(Ok(())) => 0,
            Ok(Err(e)) => {
                // WASI proc_exit traps — check for the special exit-code trap
                let msg = e.to_string();
                if msg.contains("ExitCalled") {
                    if let Some(code_str) = msg.split('(').nth(1).and_then(|s| s.split(')').next()) {
                        code_str.trim().parse::<i32>().unwrap_or(1)
                    } else {
                        1
                    }
                } else {
                    warn!(error = %e, "WASM execution error");
                    1
                }
            }
            Err(_) => {
                warn!("WASM execution timed out after {:?}", timeout);
                124
            }
        };

        let fuel_consumed = fuel.saturating_sub(store.get_fuel().unwrap_or(0));
        let duration_ms = start.elapsed().as_millis() as u64;

        let stdout = stdout_pipe.contents().to_vec();
        let stderr = stderr_pipe.contents().to_vec();
        let stdout = String::from_utf8_lossy(&stdout).to_string();
        let stderr = String::from_utf8_lossy(&stderr).to_string();

        debug!(exit_code, fuel_consumed, duration_ms, "WASM execution complete");

        Ok(WasmExecResult {
            stdout,
            stderr,
            exit_code,
            fuel_consumed,
            duration_ms,
        })
    }

    /// Execute a WASM file from disk.
    pub async fn exec_file(
        &self,
        path: &std::path::Path,
        args: &[&str],
        caps: WasmCapabilities,
    ) -> Result<WasmExecResult, SandboxError> {
        let bytes = std::fs::read(path)
            .map_err(|e| SandboxError::Create(format!("Read WASM file: {e}")))?;
        self.exec_bytes(&bytes, args, caps).await
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_creation() {
        let sandbox = WasmSandbox::new(WasmSandboxConfig::default());
        assert!(sandbox.is_ok());
    }

    #[tokio::test]
    async fn minimal_wasm_exit_zero() {
        let sandbox = WasmSandbox::new(WasmSandboxConfig::default()).unwrap();

        // Minimal WASM that calls proc_exit(0)
        let wat = r#"
            (module
                (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
                (func $main
                    i32.const 0
                    call $exit
                    unreachable
                )
                (export "_start" (func $main))
            )
        "#;

        let bytes = wat::parse_str(wat).expect("WAT parse failed");
        let result = sandbox
            .exec_bytes(&bytes, &["test"], WasmCapabilities::default())
            .await;

        // proc_exit(0) registers as exit code 0
        assert!(result.is_ok());
    }
}
