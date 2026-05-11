---
name: sse-streaming
description: Implement SSE (Server-Sent Events) streaming endpoints in nexus-http. Use when adding real-time progress feedback, live-build streams, or any long-running operation that needs to stream output.
---

# SSE Streaming in nexus-http

## Critical invariant

**Every SSE stream MUST emit a terminal event before the stream closes.**

Terminal events are `complete`, `error`, or `done` depending on the handler convention. A stream that ends silently will leave the frontend in a hung state.

## Standard SSE handler pattern

Use an `mpsc` channel to decouple the async work from the SSE stream:

```rust
use std::convert::Infallible;
use std::sync::Arc;

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::{error::ApiResult, state::AppState};

// ---------------------------------------------------------------------------
// Event enum — always tag with `#[serde(tag = "type", rename_all = "snake_case")]`
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MyStreamEvent {
    Phase {
        phase: String,
        status: String,
    },
    Progress {
        message: String,
        percent: u8,
    },
    Complete {
        result: String,
    },
    Error {
        message: String,
    },
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

pub async fn my_streaming_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MyRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, mut rx) = mpsc::channel::<MyStreamEvent>(32);

    // Spawn the work in the background — the SSE stream returns immediately
    tokio::spawn(async move {
        let result = do_work(&state, &req, &tx).await;

        // Always emit a terminal event
        match result {
            Ok(output) => {
                tx.send(MyStreamEvent::Complete { result: output }).await.ok();
            }
            Err(e) => {
                tx.send(MyStreamEvent::Error { message: e.to_string() }).await.ok();
            }
        }
    });

    let stream = stream::unfold(rx, |mut rx| async move {
        let event = rx.recv().await?;
        let data = serde_json::to_string(&event).unwrap_or_default();
        Some((Ok(Event::default().data(data)), rx))
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn do_work(
    state: &Arc<AppState>,
    req: &MyRequest,
    tx: &mpsc::Sender<MyStreamEvent>,
) -> anyhow::Result<String> {
    tx.send(MyStreamEvent::Phase {
        phase: "init".into(),
        status: "starting".into(),
    }).await.ok();

    // ... do work ...

    tx.send(MyStreamEvent::Progress {
        message: "halfway there".into(),
        percent: 50,
    }).await.ok();

    Ok("done".into())
}
```

## Per-project build event bus pattern

For live-build streams that must survive reconnects, use the `BuildEventBus` in `AppState`:

```rust
// Send to the bus (from anywhere)
if let Some(bus) = state.build_event_bus.get(&project_id) {
    bus.send(my_event).ok();  // broadcast to all subscribers
}

// Subscribe in a handler
let mut rx = state.build_event_bus
    .entry(project_id.clone())
    .or_insert_with(|| tokio::sync::broadcast::channel(256).0)
    .subscribe();

let stream = stream::unfold(rx, |mut rx| async move {
    match rx.recv().await {
        Ok(event) => {
            let data = serde_json::to_string(&event).unwrap_or_default();
            Some((Ok(Event::default().data(data)), rx))
        }
        Err(_) => None,  // channel closed = stream ends
    }
});
```

## Event naming conventions

Follow the existing conventions from `handlers/oneshot.rs`:

| Event type | Meaning |
|-----------|---------|
| `phase` | Major pipeline phase started (init, analyze, codegen, etc.) |
| `progress` | Incremental progress update within a phase |
| `thinking` | LLM is reasoning (show spinner to user) |
| `file_written` | A file was produced |
| `complete` | Stream finished successfully |
| `error` | Stream ended with an error |
| `done` | Alias for complete used in some handlers |

## Route registration

SSE endpoints must use `get` (not `post`) if they only receive path/query params, or `post` if they accept a JSON body:

```rust
// GET-based SSE (params via query string)
.route("/projects/:id/my-stream", get(my_handler::my_streaming_handler))

// POST-based SSE (params via JSON body) — used by oneshot
.route("/projects/:id/my-op", post(my_handler::my_streaming_handler))
```

## Frontend consumption

The frontend SSE client is in `web/lib/api.ts`. When adding a new SSE endpoint, also update the API client:

```typescript
// web/lib/api.ts
export function streamMyOperation(
  projectId: string,
  params: MyParams,
  onEvent: (event: MyEvent) => void,
  onDone: () => void,
): EventSource {
  const es = new EventSource(`/api/projects/${projectId}/my-op?...`);
  es.onmessage = (e) => {
    const event = JSON.parse(e.data);
    if (event.type === 'complete' || event.type === 'error') {
      onDone();
      es.close();
    } else {
      onEvent(event);
    }
  };
  return es;
}
```

## Checklist before shipping an SSE handler

- [ ] Terminal event emitted in all code paths (success AND error)
- [ ] `tokio::spawn` used so the handler returns the stream immediately
- [ ] `mpsc` buffer sized appropriately (32 is a sensible default; increase for high-frequency events)
- [ ] `KeepAlive::default()` attached to prevent proxy timeouts
- [ ] All event variants implement `Serialize`
- [ ] Route registered in `server.rs`
