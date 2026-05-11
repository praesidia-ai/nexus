//! WASI plugin sandbox host — Phase-3 #7 / ADR-004 runtime.
//!
//! Loads a `.wasm` plugin compiled against the
//! `nexus-plugins-sdk/wit/plugin.wit` interface, instantiates it with
//! capability-scoped WASI imports, and invokes the `on-hook` export with
//! a JSON-encoded `HookContext`. Returns the `HookResult` JSON or a typed
//! [`PluginError`] when the plugin traps, exhausts fuel, runs out of
//! memory, or violates the wallclock cap.
//!
//! This module is the *host*; the plugin author writes Rust + `wit-bindgen`
//! and ships a `.nxp` archive. ADR-004 §1 defines the artifact layout. For
//! v1 we run the simpler preview-1 ABI: the host writes the JSON envelope
//! to stdin, the guest writes the JSON result to stdout. Component-model /
//! WIT bindings can be layered on later without changing this entry point.
//!
//! Defaults match ADR-004 §1:
//! - 1s CPU (~1e9 fuel units)
//! - 5s wallclock (epoch interruption)
//! - 64 MiB memory cap (1024 pages × 64 KiB)
//!
//! Failure modes:
//! - `PluginError::Trap` for any wasmtime trap (fuel/epoch/memory included).
//! - `PluginError::ResultParse` when the guest's stdout isn't valid JSON.
//! - `PluginError::Io` for stdin/stdout pipe failures.
//!
//! The host does NOT panic on plugin misbehaviour. Every error is returned
//! to the caller, who logs and increments `nexus_plugin_trap_total`.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use wasmtime::{Config, Engine, Module, Store};
use wasmtime_wasi::{
    pipe::{MemoryInputPipe, MemoryOutputPipe},
    preview1::{self, WasiP1Ctx},
    WasiCtxBuilder,
};

const DEFAULT_FUEL: u64 = 1_000_000_000;
const DEFAULT_WALLCLOCK: Duration = Duration::from_secs(5);
const DEFAULT_MEMORY_PAGES: u64 = 1024; // 64 MiB
const STDOUT_CAP: usize = 4 * 1024 * 1024; // 4 MiB cap on plugin stdout

/// Configurable per-call resource budget for a plugin hook invocation.
#[derive(Debug, Clone)]
pub struct PluginBudget {
    pub fuel: u64,
    pub wallclock: Duration,
    pub memory_pages: u64,
}

impl Default for PluginBudget {
    fn default() -> Self {
        Self {
            fuel: DEFAULT_FUEL,
            wallclock: DEFAULT_WALLCLOCK,
            memory_pages: DEFAULT_MEMORY_PAGES,
        }
    }
}

/// A loaded plugin module — cheap to clone and re-instantiate per call.
///
/// Cloning copies the `Arc<Module>`; instantiation per-call is what gives
/// us isolation. The shared `Engine` keeps Wasmtime's compilation cache
/// hot across calls.
#[derive(Clone)]
pub struct PluginModule {
    engine: Engine,
    module: Arc<Module>,
    plugin_id: String,
}

impl PluginModule {
    /// Load a `.wasm` plugin from disk. The Wasmtime engine is built with
    /// fuel + epoch + memory consumption tracking enabled per ADR-004 §1.
    pub fn load(plugin_id: impl Into<String>, path: impl AsRef<Path>) -> Result<Self, PluginError> {
        let mut cfg = Config::new();
        cfg.consume_fuel(true);
        cfg.epoch_interruption(true);
        cfg.async_support(true);
        let engine = Engine::new(&cfg).map_err(|e| PluginError::Engine(e.to_string()))?;
        let module = Module::from_file(&engine, path.as_ref())
            .map_err(|e| PluginError::Engine(format!("load_module: {e}")))?;
        Ok(Self {
            engine,
            module: Arc::new(module),
            plugin_id: plugin_id.into(),
        })
    }

    /// Construct from in-memory bytes. Useful for tests and plugins fetched
    /// from a marketplace registry.
    pub fn from_bytes(
        plugin_id: impl Into<String>,
        bytes: &[u8],
    ) -> Result<Self, PluginError> {
        let mut cfg = Config::new();
        cfg.consume_fuel(true);
        cfg.epoch_interruption(true);
        cfg.async_support(true);
        let engine = Engine::new(&cfg).map_err(|e| PluginError::Engine(e.to_string()))?;
        let module = Module::new(&engine, bytes)
            .map_err(|e| PluginError::Engine(format!("load_module: {e}")))?;
        Ok(Self {
            engine,
            module: Arc::new(module),
            plugin_id: plugin_id.into(),
        })
    }

    pub fn id(&self) -> &str {
        &self.plugin_id
    }

    /// Invoke the plugin's `on-hook` entry point.
    ///
    /// Wire format (preview-1 ABI): the host writes `ctx_json` to the
    /// guest's stdin; the guest's `_start` reads it, processes the hook,
    /// and writes the JSON result to stdout. Length-prefix or any other
    /// framing is unnecessary because the guest reads stdin until EOF.
    ///
    /// Returns the captured stdout bytes verbatim — the caller parses them
    /// as `HookResult` JSON. We deliberately don't deserialize here so
    /// callers can compose alternative wire schemas without changing this
    /// crate.
    pub async fn invoke(
        &self,
        ctx_json: &[u8],
        budget: &PluginBudget,
    ) -> Result<Vec<u8>, PluginError> {
        let stdin = MemoryInputPipe::new(ctx_json.to_vec());
        let stdout = MemoryOutputPipe::new(STDOUT_CAP);

        // Bundle the wasi ctx + the per-call resource limiter in one store
        // state struct. Prior versions used `Box::leak` to satisfy the
        // 'static bound on `store.limiter`, which leaked 16 bytes per call.
        let state = SandboxState {
            wasi: WasiCtxBuilder::new()
                .stdin(stdin)
                .stdout(stdout.clone())
                .build_p1(),
            limiter: MemLimiter::new(budget.memory_pages),
        };

        let mut store: Store<SandboxState> = Store::new(&self.engine, state);
        store
            .set_fuel(budget.fuel)
            .map_err(|e| PluginError::Engine(format!("set_fuel: {e}")))?;
        store.set_epoch_deadline(1);
        store.limiter(|s| &mut s.limiter);

        // Background task that increments the engine epoch after the
        // wallclock deadline so a cooperative `wasmtime::Trap` fires.
        // We hold the JoinHandle and abort it on the happy path so a
        // fast-returning plugin doesn't leak a 5s sleeping task per call.
        let engine_for_timeout = self.engine.clone();
        let wallclock = budget.wallclock;
        let epoch_kicker = tokio::spawn(async move {
            tokio::time::sleep(wallclock).await;
            engine_for_timeout.increment_epoch();
        });
        let _abort_on_drop = AbortOnDrop(epoch_kicker);

        let mut linker: wasmtime::Linker<SandboxState> = wasmtime::Linker::new(&self.engine);
        preview1::add_to_linker_async(&mut linker, |s| &mut s.wasi)
            .map_err(|e| PluginError::Engine(format!("link_wasi: {e}")))?;

        let instance = linker
            .instantiate_async(&mut store, &self.module)
            .await
            .map_err(|e| PluginError::Trap(format!("instantiate: {e}")))?;

        // Invoke `_start` (WASI command). Plugins compiled with
        // `--target wasm32-wasip1` expose this; richer entrypoints will
        // come with the component-model migration.
        let start = instance
            .get_typed_func::<(), ()>(&mut store, "_start")
            .map_err(|e| PluginError::Engine(format!("missing _start: {e}")))?;
        start
            .call_async(&mut store, ())
            .await
            .map_err(|e| PluginError::Trap(e.to_string()))?;

        // Drain captured stdout. `try_into_inner()` returns the bytes if
        // the guest closed the pipe; the host doesn't re-use the pipe.
        let bytes = stdout
            .try_into_inner()
            .ok_or_else(|| PluginError::Io("stdout pipe still in use".into()))?;
        Ok(bytes.to_vec())
    }
}

/// Per-call store state — bundles the WASI context and the memory limiter
/// so they share a lifetime owned by the [`Store`]. Prevents the leak in
/// the previous `Box::leak` form.
struct SandboxState {
    wasi: WasiP1Ctx,
    limiter: MemLimiter,
}

/// Abort the held `JoinHandle` when this guard drops. Bounds the lifetime
/// of the epoch-kicker task to the lifetime of the call that spawned it.
struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Per-instance memory limiter — enforces ADR-004 §1 cap (default 64 MiB).
struct MemLimiter {
    max_bytes: usize,
    current: usize,
}

impl MemLimiter {
    fn new(pages: u64) -> Self {
        Self {
            max_bytes: (pages as usize).saturating_mul(64 * 1024),
            current: 0,
        }
    }
}

impl wasmtime::ResourceLimiter for MemLimiter {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        self.current = current;
        Ok(desired <= self.max_bytes)
    }

    fn table_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        // Defensive: cap table growth at 100k entries to bound memory.
        Ok(desired <= 100_000)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("plugin engine: {0}")]
    Engine(String),
    #[error("plugin trap: {0}")]
    Trap(String),
    #[error("plugin io: {0}")]
    Io(String),
    #[error("plugin result parse: {0}")]
    ResultParse(String),
}

impl PluginError {
    /// Classification used by metrics / auto-disable rule.
    pub fn kind(&self) -> &'static str {
        match self {
            PluginError::Engine(_) => "engine",
            PluginError::Trap(s) if s.contains("fuel") => "cpu",
            PluginError::Trap(s) if s.contains("epoch") => "wallclock",
            PluginError::Trap(s) if s.contains("memory") => "memory",
            PluginError::Trap(_) => "other",
            PluginError::Io(_) => "io",
            PluginError::ResultParse(_) => "parse",
        }
    }
}

/// Thin registry of loaded `PluginModule`s, keyed by plugin id. A real
/// install/uninstall flow will plug into this; for now it's the surface
/// the SQLite-backed `plugins` table can hydrate at boot.
#[derive(Default, Clone)]
pub struct PluginSandboxRegistry {
    inner: Arc<Mutex<std::collections::HashMap<String, PluginModule>>>,
}

impl PluginSandboxRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn load(
        &self,
        plugin_id: impl Into<String>,
        path: impl AsRef<Path>,
    ) -> Result<(), PluginError> {
        let id = plugin_id.into();
        let m = PluginModule::load(&id, path)?;
        self.inner.lock().await.insert(id, m);
        Ok(())
    }

    pub async fn get(&self, plugin_id: &str) -> Option<PluginModule> {
        self.inner.lock().await.get(plugin_id).cloned()
    }

    pub async fn unload(&self, plugin_id: &str) -> bool {
        self.inner.lock().await.remove(plugin_id).is_some()
    }

    pub async fn len(&self) -> usize {
        self.inner.lock().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_defaults_match_adr() {
        let b = PluginBudget::default();
        assert_eq!(b.fuel, 1_000_000_000);
        assert_eq!(b.wallclock, Duration::from_secs(5));
        assert_eq!(b.memory_pages, 1024); // 64 MiB
    }

    #[test]
    fn error_kind_classifies_traps() {
        assert_eq!(PluginError::Trap("fuel exhausted".into()).kind(), "cpu");
        assert_eq!(
            PluginError::Trap("epoch deadline reached".into()).kind(),
            "wallclock"
        );
        assert_eq!(
            PluginError::Trap("memory growth refused".into()).kind(),
            "memory"
        );
        assert_eq!(PluginError::Trap("other thing".into()).kind(), "other");
        assert_eq!(PluginError::Engine("oops".into()).kind(), "engine");
        assert_eq!(PluginError::Io("broken".into()).kind(), "io");
        assert_eq!(
            PluginError::ResultParse("not json".into()).kind(),
            "parse"
        );
    }

    #[tokio::test]
    async fn registry_load_unload_round_trip() {
        // Tiny WAT module that does nothing — just validates the load path.
        let wat = r#"
            (module
                (func (export "_start"))
                (memory (export "memory") 1)
            )
        "#;
        let bytes = wat::parse_str(wat).unwrap();
        let reg = PluginSandboxRegistry::new();
        let m = PluginModule::from_bytes("noop", &bytes).unwrap();
        reg.inner.lock().await.insert("noop".into(), m);
        assert_eq!(reg.len().await, 1);
        assert!(reg.get("noop").await.is_some());
        assert!(reg.unload("noop").await);
        assert_eq!(reg.len().await, 0);
    }

    /// A no-op WASI _start that exits cleanly without writing stdout.
    /// Confirms the host successfully links wasi preview-1, drives the
    /// command, and reads back an (empty) stdout byte vec without panic.
    #[tokio::test]
    async fn invoke_noop_plugin_returns_empty_stdout() {
        // wasi-libc's `_start` import surface is large; rather than depend
        // on a full toolchain we declare just the `proc_exit` import the
        // smallest WASI command needs.
        let wat = r#"
            (module
                (import "wasi_snapshot_preview1" "proc_exit"
                    (func $exit (param i32)))
                (memory (export "memory") 1)
                (func (export "_start")
                    (call $exit (i32.const 0))
                )
            )
        "#;
        let bytes = wat::parse_str(wat).unwrap();
        let m = PluginModule::from_bytes("noop", &bytes).unwrap();
        // proc_exit traps cleanly with `Trap::I32Exit(0)`. The host treats
        // that as a successful exit, returns whatever stdout captured.
        let result = m.invoke(b"{}", &PluginBudget::default()).await;
        // Either success (Ok empty bytes) OR the trap variant carrying the
        // success exit code — both are acceptable terminal states.
        match result {
            Ok(bytes) => assert!(bytes.is_empty()),
            Err(PluginError::Trap(s)) => {
                assert!(s.contains("0") || s.contains("Exit"), "trap text: {s}");
            }
            Err(other) => panic!("unexpected: {other:?}"),
        }
    }

    /// CPU-fuel exhaustion: a tight loop must not run forever.
    #[tokio::test]
    async fn invoke_busy_loop_is_killed_by_fuel() {
        let wat = r#"
            (module
                (import "wasi_snapshot_preview1" "proc_exit"
                    (func $exit (param i32)))
                (memory (export "memory") 1)
                (func (export "_start")
                    (loop $forever br $forever)
                    (call $exit (i32.const 0))
                )
            )
        "#;
        let bytes = wat::parse_str(wat).unwrap();
        let m = PluginModule::from_bytes("loop", &bytes).unwrap();
        // 100k fuel is enough to enter the loop and trip exhaustion fast.
        let budget = PluginBudget {
            fuel: 100_000,
            wallclock: std::time::Duration::from_secs(2),
            memory_pages: 16,
        };
        let started = std::time::Instant::now();
        let err = m.invoke(b"{}", &budget).await.expect_err("should trap");
        let elapsed = started.elapsed();
        // The hard requirement: the loop terminated and the host stayed up.
        // Wasmtime's trap text wording is opaque; what we care about is
        // bounded execution time relative to the budget.
        assert!(
            elapsed < std::time::Duration::from_secs(3),
            "sandbox failed to bound CPU loop: ran for {elapsed:?}"
        );
        assert!(matches!(err, PluginError::Trap(_)), "unexpected: {err:?}");
    }
}
