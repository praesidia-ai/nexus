Run a full quality check on the nexus-rust workspace before committing.

Run all steps in order. Stop at the first hard failure.

## Step 1 — Clippy (hard failure)
```
cargo clippy --workspace -- -D warnings 2>&1
```
All clippy warnings are treated as errors. Fix every one before proceeding.

## Step 2 — Test suite
```
cargo test --workspace --no-fail-fast 2>&1
```
Report failures. Failing tests must be fixed.

## Step 3 — Format check
```
cargo fmt --check 2>&1
```
If there are formatting issues, run `cargo fmt` to fix them automatically.

## Step 4 — Code smell scan
Search the workspace source for:
- `unwrap()` calls outside of tests: `rg "\.unwrap()" crates/ --glob "*.rs" -l`
- `TODO`/`FIXME`/`HACK` comments: `rg "TODO|FIXME|HACK" crates/ --glob "*.rs"`
- Missing doc comments on `pub` functions in library crates: check `crates/nexus-store/src/` and `crates/nexus-core/src/`

## Step 5 — Invariant audit
Check these project-specific invariants manually in any files you changed:
- [ ] No `db.lock().await` held across an `.await` point — clone what you need and drop the guard first
- [ ] Every SSE stream emits a terminal `complete` or `error` event
- [ ] Plugin hooks called in both oneshot AND pipeline paths (if modifying codegen flow)
- [ ] New migrations added to `crates/nexus-store/src/db.rs` `run_migrations` with monotonic version number
- [ ] New handlers registered in `crates/nexus-http/src/handlers/mod.rs` AND wired in `server.rs`
- [ ] All project-scoped queries pass through `security/project_access.rs` guard
- [ ] LLM-bearing endpoints run through `rate_limiter`
- [ ] Governed actions write to `audit_log` (Ed25519-signed Merkle chain)

## Step 6 — Build check
```
cargo build --workspace 2>&1
```
Confirm the workspace compiles clean after all fixes.

## Summary output
Report:
- Clippy: N warnings (0 = pass)
- Tests: N/N passed
- Format: clean / N files to reformat
- Smells: list any found
- Invariant violations: list any found
- Overall: PASS or FAIL with what to fix
