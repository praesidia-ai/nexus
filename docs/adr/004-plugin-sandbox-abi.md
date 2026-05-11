# ADR-004 — Plugin sandbox ABI (WASI)

- **Status:** accepted (2026-04-26)
- **Owners:** Orion (security), Nova (backend)
- **Unblocks roadmap:** #7 sandbox plugins
- **Closes audit weaknesses:** §1.3 #8 (plugins run in-process unsandboxed, no timeout, conflict detection manifest-only); §1.6 #24 (`CustomTool` capability registers names only — actual code is not loaded)

## Context

Today there are **two** plugin registries (legacy file-based scan of `~/.nexus/plugins/` and the new `plugin_system::PluginRegistry`), and neither sandboxes the plugin's code. A community plugin executes inside the `nexus-server` process with full access to the SQLite handle, the audit keypair, and outbound network. A `while {}` plugin freezes the entire pipeline. The marketplace strategy (`nexus.praesidia.ai/marketplace` per project memory) cannot ship community plugins on this substrate without giving every contributor production root.

We already have `crates/nexus-sandbox` with Wasmtime + WASI for sandboxed exec of generated apps. We extend it for plugins.

## Decision

### 1. WASI-preview-1 + component-model preview, single artifact format

Plugins are distributed as `.nxp` (Nexus Plugin) bundles:
```
my-plugin.nxp                     # zip
├── manifest.toml                  # plugin metadata, capabilities, hooks
├── plugin.wasm                    # WASI component, the only executable artifact
├── README.md                      # required
└── assets/                        # optional static files, read-only at runtime
```

Wasmtime engine config:
- Fuel metering enabled, default budget **1_000_000_000** units (~1s CPU on average hardware).
- Epoch interruption enabled, deadline **5s wallclock per hook call**.
- Memory limit **64 MiB** per instance.
- WASI capabilities: **none by default**. Network, filesystem, env, and clock are all denied; explicit `manifest.capabilities` opens them with scoped allowlists.
- Component model imports/exports per §2 below.

### 2. Hook ABI (component-model interface)

```wit
// crates/nexus-plugins-sdk/wit/plugin.wit
package nexus:plugin@0.1.0;

interface hooks {
    record hook-context {
        hook-point: string,        // e.g. "OnAfterGeneration"
        run-id: string,
        project-id: string,
        tenant-id: string,
        payload: list<u8>,         // canonical JSON of the hook-specific context
        deadline-ms: u64,          // absolute ms epoch
    }

    record hook-modification {
        actions: list<u8>,         // canonical JSON of HookAction[]
        log: list<string>,
        skip-rest: bool,
    }

    variant hook-result {
        ok(hook-modification),
        err(string),
        skip,
    }

    /// The plugin implements this single export. Multiple hook points share
    /// the entrypoint; the hook-point string in the context selects behaviour.
    on-hook: func(ctx: hook-context) -> hook-result;
}

interface host {
    /// Structured logging back to the host (writes to tracing).
    log: func(level: string, msg: string);

    /// Read a single key from the plugin's scoped data dir
    /// (~/.nexus/plugins/<plugin_id>/data/). Path traversal denied.
    data-read: func(key: string) -> result<list<u8>, string>;
    data-write: func(key: string, value: list<u8>) -> result<_, string>;
    data-delete: func(key: string) -> result<_, string>;

    /// Optional capability — only available if manifest declares net:fetch with
    /// matching allow-list. Subject to host-side rate limit and 5s timeout.
    fetch: func(req: list<u8>) -> result<list<u8>, string>;
}

world plugin {
    import host;
    export hooks;
}
```

The host serializes `HookContext` to a canonical JSON blob (`payload: list<u8>`). Plugins are not given Rust/serde types directly — the JSON envelope is the stable wire format.

### 3. Manifest schema

```toml
# manifest.toml
schema_version = 1
id = "com.example.taste-extras"
name = "Taste Extras"
version = "1.2.0"
nexus_compat = ">=0.5.0,<0.7.0"
license = "MIT"
authors = ["Example <hi@example.com>"]
homepage = "https://github.com/example/taste-extras"

# Declared capabilities — host enforces these, manifest is not load-bearing for security.
[[capabilities]]
kind = "design-system"
slot = "tailwind-themes"

[[capabilities]]
kind = "quality-rule"
slot = "taste-score"

[[hooks]]
point = "OnTasteScore"
priority = 50    # 0–100, lower = earlier

[[hooks]]
point = "OnAfterGeneration"
priority = 80

# Optional capabilities the plugin requests. Each must be approved at install.
[permissions]
data = true                              # ~/.nexus/plugins/<id>/data/  R/W
net.fetch = ["https://api.example.com"]  # explicit allowlist; * is rejected
clock = true                             # wall-clock read access
```

`nexus plugins validate <file.nxp>` runs the manifest through a strict JSON-schema validator + Wasmtime dry instantiation before any state mutation.

### 4. Resource enforcement (host-side, not advisory)

| Resource | Cap | Enforcement |
|---|---|---|
| CPU per `on-hook` call | 1s | Wasmtime fuel exhaustion → trap → `HookResult::err("cpu_budget_exhausted")` |
| Wallclock per `on-hook` call | 5s | Epoch interruption → trap |
| Memory | 64 MiB | Linear-memory cap configured at instantiation |
| Outbound HTTP | 5s timeout, 1 MiB response cap | Host wraps `fetch` import; each request also charges the tenant's `rate_limiter` slot |
| Plugin data dir | 100 MiB total per plugin | Quota enforced in `host::data-write` |
| Concurrent `on-hook` calls | 4 per plugin | Semaphore in the host plugin handle |

A trap from any cause → host emits `tracing::warn!` + counter `nexus_plugin_trap_total{plugin_id, hook_point, kind}`, returns `HookResult::err` to the calling pipeline. **The host process is unaffected.**

### 5. Single registry — delete the legacy path

The legacy file-scan registry alongside `plugin_system::PluginRegistry` is **deleted**. There is one registry, backed by SQLite tables `plugins` and `plugin_hook_bindings` (already exist; extended with `wasm_blake3` and `manifest_json` columns via migration `008_plugins_wasi.sql`). All in-process plugin code paths are replaced.

### 6. First-party plugins migration

Every existing first-party plugin in-tree gets a sibling `*.wasm` build (Rust `--target wasm32-wasip1` + `wit-bindgen`). The original Rust code is kept only as the source the WASI artifact is built from. CI builds and signs each `*.nxp`; release pipeline publishes them to `marketplace.praesidia.ai`.

### 7. Hook execution rules

- Hooks for a given point fire in `priority` order, then by lex(`plugin_id`).
- If a hook returns `skip`, the host skips that plugin only — does not skip subsequent plugins on the same point.
- If a hook returns `ok(mod)` with `skip_rest: true`, **and** the manifest declares `kind = "decision-override"`, subsequent plugins on the same point are skipped. Otherwise `skip_rest` is ignored with a WARN log.
- Hooks **cannot** see one another's modifications during the same hook-point invocation; the host applies them in order after all plugins return.

### 8. Versioning + compatibility

`nexus_compat` semver constraint is checked at install **and** at each load. Mismatch → plugin is marked `disabled` with `error = "incompat: nexus 0.7 not in <0.7.0"`; user sees this in the marketplace UI.

## Consequences

**Positive**
- Marketplace can host community plugins safely; this is the precondition for the §2.2 #7 "massive integration count" pattern.
- A runaway plugin no longer takes down the demo instance.
- Single registry — debugging, install, and uninstall paths collapse to one code path.
- The plugin SDK (`nexus-plugins-sdk`) becomes a real public artifact: `wit-bindgen` + Rust crate users can target.

**Negative**
- Hooks now incur WASI instantiation cost (~1ms first call, ~10μs subsequent with module pooling). For per-token hooks this would be unacceptable — but no current hook fires more than once per pipeline phase.
- First-party plugins must be rebuilt as WASI components — moderate one-time effort, automated in CI thereafter.
- Plugin authors must use the JSON envelope, not direct Rust types; this is documented as the stable wire format.

**Neutral**
- The legacy `~/.nexus/plugins/` directory still exists but only as the storage location for installed `*.nxp` archives + their data dirs.

## Alternatives considered

- **Keep in-process plugins, add timeout + thread-isolation.** Rejected: timeouts on cooperative tokio tasks can't kill a tight CPU loop; thread isolation doesn't sandbox memory access; no path to multi-tenant safety.
- **Use Lua / QuickJS / Deno for plugins.** Rejected: forces a second runtime, no Rust authoring path, weaker static guarantees than WASI components.
- **Use eBPF or process-level sandboxing (`unshare`, `seccomp`).** Rejected: not portable to macOS / Windows; community plugin authors should target one ABI.
- **Skip `host::fetch` and force plugins to be pure functions.** Rejected: marketplace plugins (e.g. PostHog integration, Slack notifier) need outbound HTTP; the explicit allowlist is the right axis.
- **Delay plugin sandboxing until v1.0.** Rejected: the marketplace narrative is one of the public surfaces (`nexus.praesidia.ai/marketplace`); shipping unsafe plugins to it would be reputation-damaging.

## Acceptance test

1. `crates/nexus-plugins-sdk/examples/runaway-plugin/` builds to `runaway.nxp`. `nexus plugins install runaway.nxp` succeeds; first hook invocation traps within 5s; host returns `HookResult::err`; metric `nexus_plugin_trap_total` increments; subsequent runs of the same hook continue to trap (plugin is auto-disabled after 3 consecutive traps in 60s).
2. `crates/nexus-plugins-sdk/examples/oom-plugin/` allocates 200 MiB; instantiation fails with `MemoryGrowthFailed`.
3. `crates/nexus-plugins-sdk/examples/exfil-plugin/` declares no `net.fetch`; calling `host::fetch` returns `Err("capability_denied")`. With `net.fetch = ["https://example.com"]` in manifest, calling `https://evil.com` returns `Err("not_in_allowlist")`.
4. A passing first-party plugin (`taste-extras`) runs end-to-end through a `/oneshot` request and contributes a `HookModification` that is visible in the SSE stream.
