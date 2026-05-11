---
name: rust-patterns
description: Rust patterns, error handling, AppState access, and SQLite invariants specific to nexus-http. Use when writing new Rust code in any nexus crate.
---

# Rust Patterns for nexus-http

## Error handling

### In handlers — use ApiError

```rust
use crate::error::{ApiError, ApiResult};

// Return early with typed errors
pub async fn my_handler(...) -> ApiResult<Json<Resp>> {
    let item = load_item(&db, &id)
        .map_err(|_| ApiError::NotFound(format!("item {id} not found")))?;

    Ok(Json(Resp { item }))
}
```

`ApiError` variants and their HTTP status codes:
- `NotFound(String)` → 404
- `BadRequest(String)` → 400
- `Unauthorized(String)` → 401
- `Forbidden(String)` → 403
- `TooManyRequests(String)` → 429
- `Internal(String)` → 500 (message is NOT sent to client — only logged)

`From` impls exist for: `anyhow::Error`, `rusqlite::Error`, `StoreError`, `serde_json::Error`, `std::io::Error`.

### In libraries — use anyhow or thiserror

```rust
// Library code: use anyhow for propagation
pub async fn do_complex_work() -> anyhow::Result<Output> {
    let data = read_file("path").context("failed to read input file")?;
    Ok(process(data)?)
}

// Library error types: use thiserror for typed variants
#[derive(Debug, thiserror::Error)]
pub enum MyError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
```

## SQLite / AppState mutex

### The golden rule: never hold the lock across `.await`

```rust
// CORRECT: lock → use → drop, then await
let records = {
    let db = state.db.lock().await;
    nexus_store::list_records(&db, &project_id)?
};
// lock released here
process_async(records).await?;

// WRONG: holding lock across await = potential deadlock
let db = state.db.lock().await;
let records = nexus_store::list_records(&db, &project_id)?;
let processed = expensive_async_call(&records).await?;  // DEADLOCK if another task needs the db
```

### Batch multiple reads in one lock acquisition

```rust
let (projects, agents) = {
    let db = state.db.lock().await;
    let p = nexus_store::list_projects(&db)?;
    let a = nexus_store::list_agents(&db, &project_id)?;
    (p, a)
};
// Both queries share a single lock acquisition
```

## Async patterns

### Spawn background work without blocking the request

```rust
let state_clone = Arc::clone(&state);
let project_id = project_id.clone();
tokio::spawn(async move {
    if let Err(e) = background_task(&state_clone, &project_id).await {
        tracing::error!(error = %e, project_id, "Background task failed");
    }
});
```

### Timeout long operations

```rust
use tokio::time::{timeout, Duration};

let result = timeout(Duration::from_secs(30), long_operation())
    .await
    .map_err(|_| ApiError::Internal("operation timed out".into()))?;
```

### Concurrency with JoinSet

```rust
use tokio::task::JoinSet;

let mut set = JoinSet::new();
for item in items {
    let state = Arc::clone(&state);
    set.spawn(async move { process_item(&state, item).await });
}

while let Some(result) = set.join_next().await {
    match result {
        Ok(Ok(output)) => results.push(output),
        Ok(Err(e)) => tracing::warn!(error = %e, "Item processing failed"),
        Err(e) => tracing::error!(error = %e, "Task panicked"),
    }
}
```

## LLM calls

All LLM calls go through `crate::llm_client`. Use the shared `http_client` from `AppState` — never create new reqwest clients.

```rust
use crate::llm_client;

// Chat completion
let response = llm_client::complete(
    &state,
    "system prompt here",
    "user message here",
).await?;

// With explicit model override
let response = llm_client::complete_with_model(
    &state,
    "gpt-4o",
    "system prompt",
    "user message",
).await?;
```

Cost is tracked automatically by `CostTracker` in AppState.

## Tracing / observability

```rust
use tracing::{debug, error, info, instrument, warn};

// Instrument async functions for automatic span creation
#[instrument(skip(state), fields(project_id = %project_id))]
pub async fn my_function(state: &Arc<AppState>, project_id: &str) -> anyhow::Result<()> {
    info!("starting my_function");
    debug!(project_id, "debug detail");
    warn!(project_id, reason = "something odd", "unexpected state");
    error!(error = %e, "failed");
    Ok(())
}

// Use structured fields — not format strings — for queryable logs
tracing::info!(
    project_id = %id,
    duration_ms = elapsed.as_millis(),
    "generation complete"
);
```

## Plugin hooks

Call plugin hooks at all major decision points:

```rust
use crate::plugin_hooks::{self, HookPoint};

// Call hook and use the (possibly modified) payload
let payload = serde_json::json!({ "intent": intent });
let after = plugin_hooks::run_hook(&state, HookPoint::OnIntentParsed, payload).await;
```

## Serialization conventions

```rust
// Enums: always use snake_case for JSON
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status { Active, Inactive, Pending }

// Tagged enums for event streams
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event { Started { id: String }, Finished { result: String } }

// Structs: use snake_case fields (default in Rust); Deserialize with #[serde(default)] for optional fields
#[derive(Serialize, Deserialize)]
pub struct Request {
    pub required_field: String,
    #[serde(default)]
    pub optional_flag: bool,
}
```

## Clippy — treat warnings as errors

Before every commit:
```
cargo clippy -p nexus-http -- -D warnings
```

Common issues to watch:
- `clippy::needless_pass_by_value` — take `&str` not `String` where you don't need ownership
- `clippy::clone_on_ref_ptr` — use `Arc::clone(&x)` not `x.clone()` for clarity
- `clippy::unwrap_used` — use `?` or explicit error handling instead of `unwrap()`
- `clippy::expect_used` — same; only OK in tests and one-shot startup code

## Module structure

```
my_module.rs        ← single-file module for simple things
my_module/
  mod.rs            ← pub use re-exports
  types.rs          ← shared types
  traits.rs         ← trait definitions
  engine.rs         ← implementation
  tests.rs          ← unit tests (or tests/ directory)
```
