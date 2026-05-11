Add or fix an LLM call in nexus-http.

Read `.claude/skills/llm-calls/SKILL.md` first, then:

1. Build `LlmConfig` from `AppState` — use `state.model`, `state.openai_api_key`
2. Use `call_llm_with_tools(&config, &messages, &tools).await` — never call provider APIs directly
3. Acquire `state.rate_limiter.acquire_llm_slot().await` BEFORE the LLM call in any handler
4. For deterministic prompts (same input → same output), add a cache lookup via `state.llm_cache`
5. Record cost after each call via `state.cost_tracker.record(...)`

**Model selection guide**:
- Complex reasoning / code generation → `gpt-4o` or `claude-sonnet-4-20250514`
- Classification / extraction / short JSON → `gpt-4o-mini`
- Latency-critical path → `groq` provider (ultra-fast)
- Local dev / no rate limits → `ollama`

**Temperature**:
- Deterministic tasks (classification, extraction) → `0.0`
- Generation / creative tasks → `0.7`

**For JSON-only responses**: append `"Respond only in valid JSON."` to the system prompt.

**Retry + fallback are automatic** — `call_llm_with_tools` retries 3 times on 429/529/timeout and falls back to the other provider if configured.

After changes: `cargo build -p nexus-http && cargo clippy -p nexus-http -- -D warnings`
