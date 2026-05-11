Run the nexus-rust test suite and summarize results.

Steps:
1. Run `cargo test --workspace --no-fail-fast 2>&1` from the nexus-rust directory
2. Parse the output and report:
   - Total tests run / passed / failed / ignored
   - For each failing test: crate name, test name, failure message (first 20 lines)
   - Doctests separately (they often have different failure patterns)
3. If any tests failed, identify whether the failure is:
   - A logic error in the implementation
   - A SQLite/database issue (check for migration errors)
   - An async/runtime issue (look for tokio panics)
   - A missing environment variable (OPENAI_API_KEY, etc.)
4. Run `cargo test --workspace -- --list 2>&1 | grep "test$" | wc -l` to show total test count

Quick single-crate test runs:
- `cargo test -p nexus-http` — only server tests
- `cargo test -p nexus-store` — only persistence tests
- `cargo test -p nexus-http <test_name>` — run a specific test by name

For integration tests that need environment variables:
```
OPENAI_API_KEY=test NEXUS_DATA_DIR=/tmp/nexus-test cargo test -p nexus-http
```
