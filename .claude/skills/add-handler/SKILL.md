---
name: add-handler
description: Add a new Axum HTTP handler to nexus-http. Use when creating a new endpoint, handler module, or route.
---

# Adding a New Handler to nexus-http

## Location rules
- Handler file: `crates/nexus-http/src/handlers/<name>_handler.rs` (or `<name>.rs` for resource-style handlers)
- Register in: `crates/nexus-http/src/handlers/mod.rs`
- Wire routes in: `crates/nexus-http/src/server.rs`

## Step 1 — Create the handler file

```rust
//! Brief description of what this handler does.

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{
    error::{ApiError, ApiResult},
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct MyRequest {
    // fields with #[serde(default)] for optional ones
}

#[derive(Debug, Serialize)]
pub struct MyResponse {
    // fields
}

pub async fn my_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MyRequest>,
) -> ApiResult<Json<MyResponse>> {
    // implementation
    Ok(Json(MyResponse { /* ... */ }))
}
```

## Step 2 — Register in handlers/mod.rs

Add `pub mod <name>_handler;` in alphabetical order inside `crates/nexus-http/src/handlers/mod.rs`.

## Step 3 — Wire the route in server.rs

Inside the `create_router` function in `crates/nexus-http/src/server.rs`, add the import to the `use crate::handlers::{ ... }` block and register the route:

```rust
.route("/my-resource", get(my_handler::list).post(my_handler::create))
.route("/my-resource/:id", get(my_handler::get).delete(my_handler::delete))
```

Group the route with semantically similar routes — use the existing section comments (`// ── Projects ──`, `// ── Agents ──`, etc.) as a guide.

## Error handling

Always use `ApiError` from `crate::error`:

```rust
// Not found
return Err(ApiError::NotFound(format!("Item {} not found", id)));

// Bad input
return Err(ApiError::BadRequest("description must not be empty".into()));

// Auth
return Err(ApiError::Unauthorized("valid token required".into()));

// Anyhow / internal
some_fallible_call().map_err(|e| ApiError::Internal(e.to_string()))?;
```

`ApiResult<T>` = `Result<T, ApiError>`. Use `?` freely — `ApiError` implements `From<anyhow::Error>`, `From<rusqlite::Error>`, and `From<StoreError>`.

## Database access

**Critical invariant: never hold the SQLite mutex lock across an `.await` point.**

```rust
// CORRECT — drop the lock before any await
let result = {
    let db = state.db.lock().await;
    some_store_call(&db, &req.id)?
};
// db lock dropped here — safe to await
some_async_call(result).await?;

// WRONG — holding lock across await
let db = state.db.lock().await;
let result = some_async_call(&db).await?; // DEADLOCK RISK
```

## Path parameters

```rust
use axum::extract::Path;

pub async fn get_item(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<MyResponse>> { ... }
```

## Query parameters

```rust
use axum::extract::Query;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_limit")]
    pub limit: usize,
    pub cursor: Option<String>,
}

fn default_limit() -> usize { 50 }

pub async fn list_items(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Vec<MyItem>>> { ... }
```

## Project-scoped endpoints — use `ProjectAccess`

Any handler operating on `:id` (project ID) MUST use the `ProjectAccess` extractor instead of `Path<String>`. It enforces auth + tenant ownership in one step — impossible to forget.

```rust
use crate::security::{auth::Scope, project_access::ProjectAccess};

pub async fn my_project_handler(
    State(state): State<Arc<AppState>>,
    access: ProjectAccess,                 // replaces Path<String>
    Json(req): Json<MyRequest>,
) -> ApiResult<Json<MyResponse>> {
    access.require_scope(&Scope::ProjectWrite)
        .map_err(|r| ApiError::Forbidden(format!("{:?}", r)))?;

    // safe to use: access.project_id, access.tenant_id, access.user_id
    let project_id = &access.project_id;
    // ... store calls scoped by project_id
}
```

For non-project endpoints that still need auth, add the `auth_middleware` to the route group in `server.rs` and pull `AuthContext` via `extract_auth(req.extensions())`.

## Rate limiting + cost tracking

Endpoints that call LLMs MUST:
1. Acquire a slot: `let _slot = state.rate_limiter.acquire_llm_slot().await.map_err(ApiError::TooManyRequests)?;`
2. Record the call via `state.cost_tracker` after the response.

See `.claude/skills/llm-calls/SKILL.md` for the full pattern.

## Input validation

For request bodies that contain free-form strings or structured payloads, validate against `crate::input_limits::*` (max field lengths, max array sizes). Reject oversized inputs with `ApiError::BadRequest` before any LLM or DB work.

## Response shape

All responses must be structured JSON. Prefer typed response structs over `serde_json::Value`. Error responses are handled automatically by `ApiError::into_response`.

## After adding the handler

1. `cargo build -p nexus-http` — verify it compiles
2. `cargo clippy -p nexus-http -- -D warnings` — fix all warnings before committing
3. Add a test in `crates/nexus-http/tests/` if the logic is non-trivial
4. Add the route to the auth-required group in `server.rs` unless it's intentionally public (see `is_public_path` in `security/auth.rs`)
