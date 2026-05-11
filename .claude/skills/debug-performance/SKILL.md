---
name: debug-performance
description: Diagnose and fix performance problems in nexus-rust — SQLite contention, LLM latency spikes, SSE backpressure, cost runaway, and slow handlers. Use when something is slow, hanging, or burning money.
---

# Performance Debugging in nexus-rust

## Symptom → Root cause map

| Symptom | Most likely cause | Jump to |
|---------|-------------------|---------|
| Requests queue up / timeout under load | SQLite mutex contention | §1 |
| Handler responds but stream hangs | SSE channel backpressure or missing terminal event | §2 |
| LLM calls slow or failing | Provider rate limits, missing concurrency guard, or no caching | §3 |
| Cost spiking unexpectedly | Missing cache, wrong model for task, duplicate calls | §4 |
| Server uses 100% CPU | Busy-loop in async task, blocking sync call in async context | §5 |
| Memory growing over time | SSE channels never closed, `Arc` reference cycles | §6 |

---

## §1 — SQLite mutex contention

**Symptom**: requests pile up, `db.lock().await` shows high wait time in traces, server slows to a crawl under concurrent load.

**Diagnosis**:
```rust
// Add temporary timing around every lock acquisition in the hot path
let t = std::time::Instant::now();
let db = state.db.lock().await;
tracing::warn!(waited_ms = t.elapsed().as_millis(), "acquired db lock");
```

**Root cause checklist**:

1. **Lock held across `.await`** — the most common cause:
```rust
// BAD — lock held while awaiting an LLM call (can take 30s+)
let db = state.db.lock().await;
let project = load_project(&db, &id)?;
let result = call_llm_with_tools(&config, &messages, &[]).await?;  // 30s wait with lock held
save_result(&db, &result)?;

// CORRECT — two separate lock acquisitions
let project = { let db = state.db.lock().await; load_project(&db, &id)? };
let result = call_llm_with_tools(&config, &messages, &[]).await?;
{ let db = state.db.lock().await; save_result(&db, &result)?; }
```

2. **Too many sequential lock acquisitions in a request** — batch reads:
```rust
// BAD — 3 separate lock acquisitions
let project = { state.db.lock().await; load_project(&db, &id)? };
let agents  = { state.db.lock().await; list_agents(&db, &id)? };
let history = { state.db.lock().await; load_history(&db, &id)? };

// CORRECT — one lock acquisition
let (project, agents, history) = {
    let db = state.db.lock().await;
    (load_project(&db, &id)?, list_agents(&db, &id)?, load_history(&db, &id)?)
};
```

3. **Missing indexes** — add to the next migration (`nexus-store/migrations/`):
```sql
-- Check slow queries with EXPLAIN QUERY PLAN
EXPLAIN QUERY PLAN SELECT * FROM my_table WHERE project_id = 'x' ORDER BY created_at DESC;
-- If it shows "SCAN my_table" instead of "SEARCH", add an index
CREATE INDEX IF NOT EXISTS idx_my_table_project_created ON my_table(project_id, created_at DESC);
```

---

## §2 — SSE stream hangs or never completes

**Symptom**: frontend spinner never stops, stream open in devtools but no more events.

**Diagnosis**:
```rust
// Check the channel capacity — if the send buffer is full, sends block
let (tx, mut rx) = mpsc::channel::<MyEvent>(32);  // 32 slots
// If the consumer is slow and 32 slots fill up, tx.send().await will block indefinitely
```

**Root cause checklist**:

1. **Missing terminal event** — most common. The async task panicked or returned early without sending `complete`/`error`:
```rust
// CORRECT pattern — terminal event guaranteed
tokio::spawn(async move {
    let result = std::panic::AssertUnwindSafe(do_work(&state, &tx))
        .catch_unwind()
        .await;
    match result {
        Ok(Ok(_)) => { tx.send(Event::Complete).await.ok(); }
        Ok(Err(e)) => { tx.send(Event::Error { message: e.to_string() }).await.ok(); }
        Err(_)    => { tx.send(Event::Error { message: "internal panic".into() }).await.ok(); }
    }
});
```

2. **Channel buffer too small** — increase capacity for high-frequency events:
```rust
// For events emitted per-file (can be hundreds)
let (tx, mut rx) = mpsc::channel(256);  // not 32
```

3. **Rx dropped before stream ends** — the stream consumer (Axum SSE) was dropped (client disconnected) but the spawn continues sending to a dead channel. This is harmless but wastes CPU — check with `.is_closed()`:
```rust
if tx.is_closed() {
    tracing::debug!("Client disconnected, stopping work");
    return Ok(());
}
```

---

## §3 — LLM latency spikes

**Symptom**: some requests take 60s+, others are instant.

**Diagnosis**:
```bash
# Check for rate limit 429s in server logs
cargo run --bin nexus-server 2>&1 | grep -i "429\|rate\|retry\|overload"

# Check cost dashboard for recent call volumes
curl http://localhost:8080/cost/summary | jq '.recent_calls_per_minute'
```

**Root cause checklist**:

1. **Missing concurrency guard** — without it, N parallel requests all hit the LLM at once and all get rate-limited:
```rust
// Every LLM-heavy handler must acquire a slot FIRST
let _slot = state.rate_limiter.acquire_llm_slot().await
    .map_err(|e| ApiError::TooManyRequests(e))?;
// LLM call now — slot released when _slot drops at end of scope
```

2. **Cache miss on deterministic prompts** — check if the prompt is the same across requests:
```rust
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

let mut h = DefaultHasher::new();
format!("{}{}", config.model, user_prompt).hash(&mut h);
let key = format!("{:x}", h.finish());

if let Some(cached) = state.llm_cache.get(&key).await {
    return Ok(cached);
}
```

3. **Wrong model for the task** — heavy models for simple classification:
```
gpt-4o / claude-sonnet  →  complex multi-step reasoning, code generation
gpt-4o-mini             →  classification, extraction, short responses
groq/llama              →  ultra-fast, use for latency-critical paths
ollama                  →  local dev, no rate limits
```

4. **Provider fallback triggering repeatedly** — means the primary provider is persistently down. Check `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` validity:
```bash
curl https://api.openai.com/v1/models -H "Authorization: Bearer $OPENAI_API_KEY" | jq '.error'
```

---

## §4 — Cost runaway

**Symptom**: `cost_tracker` shows unexpectedly high spend.

**Diagnosis**:
```bash
# Check recent LLM call log
curl http://localhost:8080/cost/calls?limit=20 | jq '.[] | {model, purpose, cost_usd, input_tokens}'

# Check per-project spend today
curl http://localhost:8080/cost/by-project | jq '.'
```

**Root cause checklist**:

1. **Taste redesign loop without bound** — the taste engine re-runs codegen until score ≥ threshold. If score never reaches threshold, it loops:
```rust
// In oneshot.rs — check `max_redesign_attempts` is honored
if redesign_count >= req.max_redesign_attempts.unwrap_or(3) {
    break;  // stop even if taste score is still low
}
```

2. **Intent engine falling through to LLM** — the deterministic layer should handle 90%+ of inputs. If it's consistently using the LLM fallback, the keyword rules need expanding:
```rust
// Check the AnalysisSource in intent results
if result.source == AnalysisSource::Semantic {
    tracing::info!(input = %input, confidence = result.confidence, "Semantic fallback used");
}
```

3. **Duplicate inflight calls** — the inflight deduplicator in AppState (`state.inflight`) prevents this for identical concurrent calls. Verify it's being used in the hot path.

4. **max_tokens set too high** — use the minimum needed for the task:
```
Classification / intent: max_tokens = 512
Short JSON response:      max_tokens = 1024
Code generation:          max_tokens = 8192
Long context reasoning:   max_tokens = 16384
```

---

## §5 — CPU spike / blocking in async

**Symptom**: Tokio runtime thread pool saturated, tracing shows long task latency.

**Root cause**: a blocking synchronous call on the async runtime thread.

```rust
// WRONG — blocks the Tokio thread for the entire file read
let content = std::fs::read_to_string(path)?;

// CORRECT — offload to blocking thread pool
let content = tokio::task::spawn_blocking(|| std::fs::read_to_string(path))
    .await??;

// WRONG — CPU-heavy work on async thread
let result = expensive_cpu_computation(&data);

// CORRECT
let result = tokio::task::spawn_blocking(move || expensive_cpu_computation(&data))
    .await?;
```

Search for blocking calls in async contexts:
```bash
rg "std::fs::" crates/nexus-http/src/handlers/ --glob "*.rs"
rg "\.join\(\)" crates/nexus-http/src/ --glob "*.rs"  # thread::spawn().join() blocks
```

---

## §6 — Memory growth

**Symptom**: RSS grows over hours without leveling off.

**Root cause checklist**:

1. **SSE broadcast channels never cleaned up** — `BuildEventBus` entries for deleted projects stay alive. Check the cleanup path when a project is deleted.

2. **`llm_cache` unbounded** — the LLM cache has no eviction policy. Add a max-size check or TTL if cache entries accumulate.

3. **`eval_results` grows forever** — `state.eval_results` is a `Vec` with no bound. Trim after a threshold:
```rust
let mut results = state.eval_results.lock().await;
if results.len() > 1000 {
    results.drain(0..500);  // keep last 500
}
```

---

## General performance tracing

Enable `RUST_LOG=nexus_http=debug` to see handler timing. Key spans to watch:

- `db_lock_wait_ms` — SQLite contention
- `llm_call_ms` — LLM round-trip
- `hook_execution_ms` — plugin hook overhead
- `taste_score_ms` — taste engine
- `codegen_ms` — code generation total
