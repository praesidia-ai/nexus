Diagnose and fix a performance problem in nexus-rust.

Read `.claude/skills/debug-performance/SKILL.md` first. Then identify the symptom:

**Requests timing out / server slow under load** → SQLite contention
- Search for `db.lock().await` followed by `.await` in the same scope
- Batch multiple reads into one lock acquisition
- Add missing database indexes

**SSE stream hangs / never completes** → missing terminal event or full channel buffer
- Verify every code path in the spawned task emits `complete` or `error`
- Increase mpsc channel buffer if emitting high-frequency events
- Wrap spawned task body in panic recovery to guarantee terminal event

**LLM calls slow or failing** → rate limits, missing concurrency guard, no caching
- Verify `state.rate_limiter.acquire_llm_slot()` is called before every LLM call in handlers
- Check if the prompt is deterministic — add `state.llm_cache` lookup before calling
- Use cheaper model for classification tasks (gpt-4o-mini, groq)

**Cost spiking** → missing cache, runaway redesign loop, oversized max_tokens
- Check `curl http://localhost:8080/cost/calls?limit=20` to identify the expensive call
- Verify taste redesign loop has `max_redesign_attempts` guard
- Reduce `max_tokens` for the call type (classification: 512, codegen: 8192)

**CPU 100%** → blocking sync call on async thread
- Search: `rg "std::fs::" crates/nexus-http/src/handlers/`
- Wrap with `tokio::task::spawn_blocking(|| ...)` 

**Memory growth** → unbounded Vec/cache, SSE channels not cleaned up
- Check `state.eval_results`, `state.llm_cache`, `state.build_event_bus` sizes

Enable detailed traces: `RUST_LOG=nexus_http=debug cargo run --bin nexus-server`
