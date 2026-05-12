# Contributing to Nexus

Thank you for considering contributing to Nexus. This document covers the development setup, crate structure, and guidelines for submitting changes.

## Development Setup

### Prerequisites

- Rust 1.82+ (install via [rustup](https://rustup.rs/))
- An OpenAI or Anthropic API key (configure at runtime via the Settings page — no env var required)
- Node.js 18+ (for the frontend in `web/`)

### Building

```bash
# Build all crates
cargo build

# Run tests
cargo test

# Run clippy lints
cargo clippy --all-targets

# Run the server in development mode
OPENAI_API_KEY="sk-..." cargo run --bin nexus-http

# Run the frontend
cd web && npm install && npm run dev
```

## Crate Overview

| Crate | Purpose | Depends On |
|-------|---------|------------|
| `nexus-intelligence` | Trait definitions for intent, decisions, product, learning | (standalone) |
| `nexus-agents-core` | Agent definitions, events, tools, teams | (standalone) |
| `nexus-quality-core` | Quality scoring, guarantees, fix plans | (standalone) |
| `nexus-providers` | LLM provider trait abstraction | (standalone) |
| `nexus-plugins-sdk` | Plugin manifest, hooks, capabilities | (standalone) |
| `nexus-core` | Core IR and app state | nexus-intelligence |
| `nexus-store` | SQLite persistence | nexus-core |
| `nexus-http` | HTTP server with all engines | all of the above |
| `nexus-workflow` | Workflow DAG runner | nexus-core |
| `nexus-zeroclaw` | ZeroClaw agent integration | nexus-agents-core |
| `cli` | CLI interface | nexus-http |

## How to Add Things

### Adding a New Agent Tool

1. Implement the `Tool` trait from `nexus-agents-core::tools`.
2. Register the tool in the tool registry (`nexus-http/src/agents/tools/registry.rs`).
3. Add the tool name to the appropriate agent definitions.

### Adding a Domain Pack

1. Create a `DomainPack` struct (from `nexus-intelligence::domain_packs`).
2. Register it in the domain pack registry or provide it via a plugin.

### Adding a Plugin

1. Create a `nexus-plugin.json` manifest following the `PluginManifest` schema.
2. Implement hooks using the `PluginHook` trait from `nexus-plugins-sdk::hooks`.
3. Declare capabilities in your manifest.

### Adding an LLM Provider

1. Implement the `LlmProvider` trait from `nexus-providers::provider`.
2. Register the provider in the model router.

### Publishing an Agent Package

1. Create a `nexus.toml` manifest in your agent directory (`nexus-pkg init`).
2. Implement your agent (WASM, A2A card, or native Rust).
3. Test locally: `nexus-pkg install --registry http://localhost:8020`.
4. Publish: `nexus-pkg publish --token $NEXUS_REGISTRY_TOKEN`.

### Adding a Multi-Modal Tool

1. Add a new `Modality` variant to `nexus-http/src/multimodal.rs`.
2. Handle the variant in the `process()` function.
3. Register any new API endpoint in `server.rs`.

## Code Style

- No `unwrap()` in production code (use `?` or `expect("...")` with a clear failure message). Tests are exempt; the workspace `clippy.toml` warns on the rest.
- All public items must have doc comments (`///`).
- Use `thiserror` for typed error enums, `anyhow` only in binaries.
- Follow existing naming: `snake_case` for files and functions, `PascalCase` for types.
- Keep traits in the `-core`/`-sdk` crates, implementations in `nexus-http` or domain crates.
- Never hold `db.lock().await` across an `.await` point — clone what you need, drop the guard, then await.

## Testing philosophy — no coverage targets

We don't enforce a coverage percentage and we don't think you should chase one for an AI orchestrator. Lots of Nexus's most important behaviour involves live LLM credits, real WASI plugins, and real browser automation — coverage tools can't see most of that, and burning provider credit on every CI run is wasteful.

**What we do enforce:**

- **The hard invariants are tested end-to-end.** Plugin sandbox bounds a hostile loop. Boot reconciler heals orphaned runs. Budget brake denies over-cap calls. Tenant attribution doesn't leak. Migrations are idempotent. SSE streams emit terminal events.
- **Every new bug fix lands with a regression test** that would have caught it.
- **Every new public API ships with at least one happy-path test** that exercises the real code, not a mock.
- **Live-API tests are feature-flagged** (`--features integration-live`) so contributors can run them with their own keys; CI doesn't.

If you find an integration path that isn't tested and ought to be, open an issue tagged `test-gap`. Real engineering judgement beats a coverage badge.

## Pull Request Process

1. Fork the repository and create a feature branch.
2. Make your changes; add a regression test if you fixed a bug.
3. Run the gate: `cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo test --workspace`.
4. Write a clear PR description: what changed, why, what you tested.
5. PRs require one review approval before merging. No CLA. No paperwork.

## Bounty Program

We run a community bounty program. Issues tagged `bounty` carry a reward (in USD or USDC):

- `bounty:small` — $25–$100 (typo fixes, small features)
- `bounty:medium` — $100–$500 (new agent tools, SDK improvements)
- `bounty:large` — $500–$2000 (new crates, major features)

To claim a bounty, comment on the issue with your solution and link your PR.

## Community Channels

- **Discord**: https://discord.gg/praesidia
- **Discussions**: GitHub Discussions (ideas, questions, showcase)
- **Releases**: Monthly releases on the 1st of each month. Watch the repo for release notes.
- **Security**: Report vulnerabilities via GitHub Security Advisories (not public issues).

## License

By contributing, you agree that your contributions will be licensed under the MIT license.
