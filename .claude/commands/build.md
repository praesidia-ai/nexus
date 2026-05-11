Build the nexus-rust workspace and report all errors.

Steps:
1. Run `cargo build --workspace 2>&1` from the nexus-rust directory
2. If there are errors, group them by crate and show the full error message for each unique error
3. Run `cargo clippy --workspace -- -D warnings 2>&1`
4. Report: number of crates compiled, number of errors, number of clippy warnings
5. If the build failed, identify the root cause crate (the first crate that broke the chain) and suggest a fix

Quick single-crate builds (use these when you only changed one crate):
- `cargo build -p nexus-http` — main server
- `cargo build -p nexus-store` — persistence layer
- `cargo build -p nexus-http --bin nexus-server` — server binary only

Never suggest running `cargo check` as a replacement — always do a full build so link errors are caught.
