Systematically fix Rust compilation errors and clippy warnings in nexus-rust.

Steps:
1. Run `cargo build --workspace 2>&1` to capture all current errors
2. Group errors by type:
   - **Type mismatch** — check `From`/`Into` impls; use `.map_err(ApiError::from)` pattern
   - **Borrow checker** — check for lock held across await (use the block pattern from `rust-patterns` skill)
   - **Missing trait impl** — add `#[derive(...)]` or implement manually
   - **Unused imports/variables** — prefix with `_` or remove
   - **Lifetime errors** — usually means you need `Arc<T>` instead of `&T` for async code
3. Fix errors starting from the lowest-level crate (nexus-store → nexus-core → nexus-http)
4. After each fix, re-run `cargo build -p <crate>` for fast feedback
5. Run `cargo clippy --workspace -- -D warnings 2>&1` after all errors are resolved
6. Fix clippy warnings:
   - `needless_pass_by_value` → change `String` param to `&str` or `impl AsRef<str>`
   - `clone_on_ref_ptr` → use `Arc::clone(&x)` instead of `x.clone()`
   - `unwrap_used` → use `?` operator or explicit match
   - `map_err` patterns → use `?` with a `From` impl if possible
7. Run `cargo fmt` to fix formatting
8. Final check: `cargo build --workspace && cargo test --workspace`

Read `.claude/skills/rust-patterns/SKILL.md` for patterns specific to this codebase.
