# ADR-003 — `LlmProvider` trait surface

- **Status:** accepted (2026-04-26)
- **Owners:** Nova (backend), Rex (observability)
- **Unblocks roadmap:** #5 LLM timeout, #9 wire `nexus-providers`, #12 cost tracking + budget
- **Closes audit weaknesses:** §1.6 #18 (LLM calls bypass `cost_tracker` + `rate_limiter`), #22 (`nexus-providers` is dead code), #32 (no global LLM timeout); audit §1.3 #5 (provider abstraction `experimental`)

## Context

`crates/nexus-providers/` defines an `LlmProvider` trait that nothing implements outside `stub_provider.rs`. Real provider dispatch lives in `crates/nexus-http/src/llm_client.rs` + `model_router.rs` + `anthropic_cache.rs`, with bespoke `reqwest` calls scattered through `edge.rs`, `multimodal.rs`, `agent_tools/ai.rs:73`, `agent_tools/media.rs:110,321` that **bypass** the cost tracker and rate limiter entirely. Adding a 7th provider today means editing five files in `nexus-http`; new contributors have no clear plug-in point.

This ADR fixes the contract first so #5, #9, and #12 can land in parallel without re-shaping the trait three times.

## Decision

### 1. Single trait, in `nexus-providers`, async-trait

```rust
// crates/nexus-providers/src/provider.rs
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync + 'static {
    /// Stable identifier used in `model_router` and `provider_registry` keys.
    fn id(&self) -> ProviderId;

    /// Models this provider can serve, with capability flags (tools, vision, json_mode, etc.).
    async fn list_models(&self) -> Result<Vec<ModelInfo>, LlmError>;

    /// Non-streaming completion. MUST honour `req.deadline` (see §3 below).
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError>;

    /// Streaming completion. The returned stream MUST emit a terminal
    /// `StreamChunk::Done { usage }` or `StreamChunk::Error { … }` in every
    /// path — required by SSE invariant #3 in CLAUDE.md.
    async fn stream(
        &self,
        req: CompletionRequest,
    ) -> Result<BoxStream<'static, Result<StreamChunk, LlmError>>, LlmError>;

    /// Optional capabilities — return `None` if unsupported. The dispatcher
    /// chooses a fallback provider if the requested capability is absent.
    async fn embed(&self, _req: EmbedRequest) -> Result<EmbedResponse, LlmError> {
        Err(LlmError::Unsupported("embed"))
    }
    async fn rerank(&self, _req: RerankRequest) -> Result<RerankResponse, LlmError> {
        Err(LlmError::Unsupported("rerank"))
    }

    /// Lightweight liveness probe. Used by `/health/detailed` and the model router's
    /// fallback policy. MUST NOT charge tokens; cheapest no-op the API supports.
    async fn health_check(&self) -> ProviderHealth;
}
```

### 2. `CompletionRequest` carries *all* enforcement context

```rust
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSpec>,
    pub response_format: ResponseFormat,
    pub temperature: Option<f32>,
    pub max_output_tokens: Option<u32>,
    pub stop: Vec<String>,
    pub seed: Option<u64>,

    // Cross-cutting context — providers MUST use these.
    pub deadline: Instant,                  // hard timeout, see §3
    pub tenant_id: TenantId,                // for cost + rate-limit attribution
    pub project_id: Option<ProjectId>,
    pub call_site: &'static str,            // e.g. "oneshot.intent_phase"
    pub trace_id: TraceId,                  // for observability
    pub idempotency_key: Option<String>,    // optional caller dedupe
}
```

The `call_site` field is a `&'static str` so it appears verbatim in metrics labels with zero allocation; new strings require code review.

### 3. Timeout discipline — provider-level **and** dispatcher-level

- `req.deadline = Instant::now() + Duration::from_secs(60)` is the default; callers may shorten, **never** lengthen past 600s.
- Every provider impl wraps its outbound `reqwest` call in `tokio::time::timeout_at(req.deadline, …)` — not in `complete()`'s caller. Reason: provider knows the right granularity (e.g. SSE chunk-level vs whole-call).
- The dispatcher (`llm_client::dispatch`) also enforces the deadline as a backstop using `tokio::time::timeout_at`.
- Streaming: every chunk read is bounded by the remaining deadline budget. On expiry, emit `StreamChunk::Error { kind: Timeout, partial_tokens: N }` and close.
- Error type: `LlmError::Timeout { elapsed_ms, deadline_ms }`. This is mapped to `ApiError::UpstreamTimeout` (HTTP 504) by handlers.

### 4. Cost + rate enforcement happens **outside** the trait, but **mandatorily**

Direct provider calls are forbidden. The only public entrypoint is:

```rust
// crates/nexus-http/src/llm_client.rs
pub struct LlmClient {
    providers: Arc<ProviderRegistry>,
    rate_limiter: Arc<RateLimiter>,
    cost_tracker: Arc<CostTracker>,
    cache: Arc<LlmCache>,
    router: Arc<ModelRouter>,
}

impl LlmClient {
    pub async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, ApiError> {
        let provider = self.router.pick(&req)?;
        self.rate_limiter.acquire_slot(&req.tenant_id, provider.id()).await?;
        if let Some(hit) = self.cache.lookup(&req).await { return Ok(hit); }
        let resp = provider.complete(req.clone()).await
            .map_err(ApiError::from_llm_err)?;
        self.cost_tracker.record(&req, &resp).await; // see ADR-005
        self.cache.put(&req, &resp).await;
        Ok(resp)
    }
    // analogous `stream()` with chunk-level cost accumulation
}
```

The four bypass call sites (`edge.rs`, `multimodal.rs`, `agent_tools/ai.rs:73`, `agent_tools/media.rs:110,321`) are **deleted** and replaced with `LlmClient::complete` / `stream` calls. There is no "raw provider" public escape hatch.

### 5. Provider registry — explicit, ordered

```rust
// crates/nexus-providers/src/registry.rs
pub struct ProviderRegistry {
    providers: HashMap<ProviderId, Arc<dyn LlmProvider>>,
}

impl ProviderRegistry {
    pub fn from_settings(settings: &Settings) -> Result<Self, RegistryError> { /* … */ }
    pub fn get(&self, id: ProviderId) -> Option<&Arc<dyn LlmProvider>> { /* … */ }
    pub fn iter(&self) -> impl Iterator<Item = (&ProviderId, &Arc<dyn LlmProvider>)> { /* … */ }
}
```

Registration is **explicit** in `nexus-providers/src/lib.rs::register_builtins(&mut Registry)`. Adding a 7th provider is two edits:
1. New file `crates/nexus-providers/src/<name>.rs` implementing the trait.
2. One line in `register_builtins`.

### 6. What moves where

| moved from | moved to |
|---|---|
| `nexus-http/src/llm_client.rs` provider-specific branches | `nexus-providers/src/{openai,anthropic,ollama,groq,together,cerebras,openrouter}.rs` |
| `nexus-http/src/model_router.rs` (provider lookup) | stays — but consumes the registry, no provider strings hardcoded |
| `nexus-http/src/anthropic_cache.rs` | folded into `nexus-providers/src/anthropic.rs` as a private module |
| `nexus-http/src/edge.rs` direct `api.openai.com` POST | `LlmClient::complete` |
| `nexus-http/src/multimodal.rs` direct calls | `LlmClient::stream` (text) + `LlmClient::audio` once added |
| `nexus-http/src/agent_tools/ai.rs:73`, `media.rs:110,321` | `LlmClient::complete` |
| `nexus-providers/src/stub_provider.rs` | stays as `cfg(test)` only — not in default registry |

**Replace in place. No `legacy_llm_client` module.**

### 7. Capability discovery (replaces audit §1.3 #5 "no model-capability discovery")

`ModelInfo` returned by `list_models` includes:

```rust
pub struct ModelCapabilities {
    pub context_window: u32,
    pub max_output_tokens: u32,
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub supports_json_mode: bool,
    pub supports_streaming: bool,
    pub input_cost_per_mtok_usd: f64,
    pub output_cost_per_mtok_usd: f64,
}
```

`model_router::pick` uses these for provider fallback when a capability is requested.

## Consequences

**Positive**
- One enforcement point — every LLM call goes through `LlmClient`, so cost tracking, rate limiting, caching, and timeout are not optional.
- Adding a provider is a 50-line PR, testable in isolation against the trait.
- `nexus-providers` stops being dead code; the dependency graph reflects reality.
- Capabilities-aware routing replaces the current static defaults.

**Negative**
- Mechanical sweep of ~10 call sites; high diff volume in one PR but trivial reviews.
- Embedding/rerank features now explicit `Unsupported` errors when calling Ollama for them; existing flaky behaviour becomes loud.

**Neutral**
- Anthropic prompt cache logic moves crate but its on-the-wire format does not change; integration tests confirm parity.

## Alternatives considered

- **Keep provider dispatch in `nexus-http`, delete `nexus-providers`.** Considered. Rejected: closes the door on `nexus-sdk-client` and external embedders consuming Nexus as a library. The trait is the public boundary.
- **Generic over `Provider: LlmProvider` instead of `Arc<dyn LlmProvider>`.** Rejected: forces every consumer to know the concrete type at compile time; runtime registry needs dyn dispatch.
- **Adopt LangChain/LiteLLM's wire format.** Rejected: their format is OpenAI-shaped + extensions; we already have one in `llm_client.rs`. Trait stays internal, gateways translate.
- **Allow callers to bypass `LlmClient` for "system" calls (health checks).** Rejected: that's what `health_check()` on the trait is for, and it must be free.

## Acceptance test

1. Adding `crates/nexus-providers/src/mistral.rs` (a real 7th provider) is a self-contained PR ≤ 80 LOC + 1 line in `register_builtins` + 1 cookbook entry.
2. `cargo test -p nexus-providers --features integration-stubs` exercises every trait method against a mocked HTTP server with deadline expiry, malformed JSON, partial stream, and cost-extraction edge cases.
3. `grep -rn "reqwest::Client.*api\\.openai\\|api\\.anthropic" crates/` returns matches **only** inside `crates/nexus-providers/src/`. Anywhere else fails CI.
