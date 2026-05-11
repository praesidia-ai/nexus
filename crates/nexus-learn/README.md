# `nexus-learn`

Self-improving agent intelligence — outcome tracking, pattern extraction, eval-gated skill promotion

## Status

Part of the [Nexus](../../README.md) workspace. See `NEXUS_FEATURE_AUDIT.md` at
the repo root for the per-subsystem maturity verdict and known weaknesses.

## Public API

Top-level exports live in `src/lib.rs` (or `src/main.rs` for binary crates).
Run `cargo doc -p nexus-learn --open` for the rendered API reference.

## How to extend

1. Read the `//!` module-level doc comment at the top of `src/lib.rs`.
2. Skim the relevant ADR(s) under `docs/adr/` if any apply (e.g. ADR-002
   for durable team events, ADR-003 for LLM provider work, ADR-004 for
   plugin sandbox, ADR-005 for cost ledger).
3. Add tests next to the change (unit tests in-file, integration tests
   under `tests/`).
4. Run `cargo test -p nexus-learn` and `cargo clippy -p nexus-learn -- -D warnings`.

## Conventions

- No `unwrap`/`panic` outside `#[cfg(test)]`. The workspace `clippy.toml`
  warns on these.
- No raw `reqwest` calls to LLM provider URLs — go through
  `nexus-providers::LlmProvider` (ADR-003).
- Never hold `db.lock().await` across an `.await` point. Clone what you
  need, drop the guard, then await.

## Build

```bash
cargo build -p nexus-learn
cargo test  -p nexus-learn
```
