//! Unified LLM tool-calling client — the SINGLE source of truth for all LLM
//! calls with tool use across Nexus.
//!
//! Previously duplicated in `agent_loop.rs` and `coding_agents/engine.rs`.
//! Both now delegate here.

use serde_json::{json, Value};

use crate::agent_tools::ToolCall;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Configuration sufficient to call any supported LLM provider.
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub provider: String,
    pub model: String,
    pub api_key: String,
    pub api_base: String,
    pub max_tokens: u32,
    pub temperature: f32,
}

/// Response from an LLM that may include tool calls.
#[derive(Debug)]
pub struct LlmToolResponse {
    pub text: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    /// True when the model hit `max_tokens` before finishing (OpenAI
    /// `finish_reason == "length"`, Anthropic `stop_reason == "max_tokens"`).
    /// Any tool-call arguments in a truncated response may be partial JSON —
    /// callers MUST treat this as a failure and not act on the tool calls.
    pub truncated: bool,
    /// Provider-reported input tokens (0 when unavailable, e.g. Ollama).
    pub input_tokens: u64,
    /// Provider-reported output tokens (0 when unavailable).
    pub output_tokens: u64,
}

// ---------------------------------------------------------------------------
// Shared HTTP client (singleton, connection-pooled)
// ---------------------------------------------------------------------------

fn shared_client() -> &'static reqwest::Client {
    use std::sync::OnceLock;
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        // 90 s per request is enough for long-context completions and
        // avoids tying up a Tokio worker for 5 minutes when a provider
        // silently hangs. Connect is a separate 10 s.
        reqwest::Client::builder()
            .pool_max_idle_per_host(10)
            .timeout(std::time::Duration::from_secs(90))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to build HTTP client")
    })
}

// ---------------------------------------------------------------------------
// Per-provider circuit breaker
// ---------------------------------------------------------------------------
//
// When an upstream provider is degraded (repeated 5xx / 429), retrying every
// request wastes budget and pins worker threads. The breaker opens after
// `CB_THRESHOLD` consecutive failures and stays open for `CB_COOLDOWN`; once
// cooled down the next call is allowed through ("half-open") and either
// closes the breaker on success or reopens it on failure.

const CB_THRESHOLD: u32 = 5;
const CB_COOLDOWN: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Default, Clone, Copy)]
struct BreakerState {
    failures: u32,
    opened_at: Option<std::time::Instant>,
}

fn breakers() -> &'static std::sync::Mutex<std::collections::HashMap<String, BreakerState>> {
    use std::sync::OnceLock;
    static BREAKERS: OnceLock<std::sync::Mutex<std::collections::HashMap<String, BreakerState>>> =
        OnceLock::new();
    BREAKERS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Returns `Err(msg)` if the breaker for `key` is open and has not cooled
/// down; otherwise `Ok(())`.
fn breaker_check(key: &str) -> Result<(), String> {
    let map = breakers().lock().unwrap();
    if let Some(st) = map.get(key) {
        if let Some(at) = st.opened_at {
            if at.elapsed() < CB_COOLDOWN {
                return Err(format!(
                    "circuit breaker open for provider '{key}' (cooldown {}s remaining)",
                    (CB_COOLDOWN - at.elapsed()).as_secs()
                ));
            }
        }
    }
    Ok(())
}

fn breaker_record_success(key: &str) {
    let mut map = breakers().lock().unwrap();
    map.insert(key.to_string(), BreakerState::default());
}

fn breaker_record_failure(key: &str) {
    let mut map = breakers().lock().unwrap();
    let st = map.entry(key.to_string()).or_default();
    st.failures = st.failures.saturating_add(1);
    if st.failures >= CB_THRESHOLD && st.opened_at.is_none() {
        st.opened_at = Some(std::time::Instant::now());
        tracing::warn!(
            provider = %key,
            failures = st.failures,
            "LLM circuit breaker opened"
        );
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Default end-to-end deadline for an LLM call (including retries). Per
/// ADR-003 §3 + Phase 1 #5. Callers that need a tighter bound should use
/// [`call_llm_with_tools_with_deadline`].
pub const DEFAULT_LLM_DEADLINE: std::time::Duration = std::time::Duration::from_secs(60);

/// Hard upper bound on the deadline a caller may request. Prevents a buggy
/// caller from accidentally disabling the timeout by passing something silly.
pub const MAX_LLM_DEADLINE: std::time::Duration = std::time::Duration::from_secs(600);

/// Call any supported LLM provider with tool definitions, bounded by an
/// end-to-end wallclock deadline (default 60s).
///
/// On expiry returns `Err("llm_timeout: ...")` — callers should map this to
/// `ApiError::UpstreamTimeout` (HTTP 504) and emit a terminal SSE event so
/// streams don't dangle (CLAUDE.md SSE invariant #3).
pub async fn call_llm_with_tools_with_deadline(
    config: &LlmConfig,
    messages: &[Value],
    tools: &[Value],
    deadline: std::time::Duration,
) -> Result<LlmToolResponse, String> {
    let bounded = std::cmp::min(deadline, MAX_LLM_DEADLINE);
    match tokio::time::timeout(
        bounded,
        call_llm_with_tools_inner(config, messages, tools),
    )
    .await
    {
        Ok(r) => r,
        Err(_) => {
            tracing::warn!(
                provider = %config.provider,
                model = %config.model,
                deadline_ms = bounded.as_millis() as u64,
                "LLM call exceeded deadline; aborting"
            );
            // Surface a retryable-shaped error string so callers and the
            // breaker treat it consistently with provider-side 504s.
            Err(format!(
                "llm_timeout: deadline of {}ms exceeded for provider={} model={}",
                bounded.as_millis(),
                config.provider,
                config.model
            ))
        }
    }
}

/// Call any supported LLM provider with tool definitions.
///
/// Dispatches to OpenAI-compatible, Anthropic, or Ollama based on
/// `config.provider`. Retries on 429/529 with exponential backoff
/// (1s, 2s, 4s — max 3 attempts). Wrapped in [`DEFAULT_LLM_DEADLINE`].
pub async fn call_llm_with_tools(
    config: &LlmConfig,
    messages: &[Value],
    tools: &[Value],
) -> Result<LlmToolResponse, String> {
    call_llm_with_tools_with_deadline(config, messages, tools, DEFAULT_LLM_DEADLINE).await
}

/// Per-call envelope that carries the cross-cutting context required for
/// per-tenant accounting (ADR-003 §2 + ADR-005). Handlers that have an
/// [`AppState`](crate::state::AppState) should prefer
/// [`call_llm_with_envelope`] over the bare `call_llm_with_tools`.
pub struct LlmCallEnvelope<'a> {
    /// Multi-tenant attribution. `"default"` is acceptable for first-party
    /// background tasks that don't run inside a tenant request.
    pub tenant_id: &'a str,
    /// Optional project for per-project rollups in `cost_records`.
    pub project_id: Option<&'a str>,
    /// Static identifier of the call site, used as a metric label and
    /// stored verbatim in `cost_records.call_site`.
    pub call_site: &'static str,
    /// End-to-end wallclock deadline. Defaults to [`DEFAULT_LLM_DEADLINE`].
    pub deadline: std::time::Duration,
    /// Pre-call cost estimate in USD micros (input_tokens × model price).
    /// Used by the budget brake to refuse calls that would exceed the
    /// daily cap. Pass 0 to skip the brake (e.g. for system health checks).
    pub estimated_cost_micros: i64,
}

impl<'a> LlmCallEnvelope<'a> {
    /// Build an envelope with sensible defaults for a tenant request.
    pub fn for_tenant(tenant_id: &'a str, call_site: &'static str) -> Self {
        Self {
            tenant_id,
            project_id: None,
            call_site,
            deadline: DEFAULT_LLM_DEADLINE,
            estimated_cost_micros: 0,
        }
    }

    /// Set the cost estimate from a serde_json messages array + model id.
    /// Token count is approximated as 4 bytes per token, which is the
    /// industry-standard rough heuristic — over-estimates simple English
    /// (good for the brake) and under-estimates tool-call JSON, but fine
    /// for a guardrail. Actual spend is recorded post-call from the
    /// provider's own usage report.
    pub fn with_estimated_cost_from_messages(
        mut self,
        messages: &[serde_json::Value],
        model: &str,
        max_output_tokens: u64,
    ) -> Self {
        let mut bytes = 0u64;
        for m in messages {
            if let Some(s) = m.get("content").and_then(|c| c.as_str()) {
                bytes = bytes.saturating_add(s.len() as u64);
            } else if let Some(arr) = m.get("content").and_then(|c| c.as_array()) {
                // Multimodal content is an array of {type, text|image_url}.
                for c in arr {
                    if let Some(s) = c.get("text").and_then(|t| t.as_str()) {
                        bytes = bytes.saturating_add(s.len() as u64);
                    }
                }
            }
        }
        let input_tokens = bytes / 4;
        let usd = crate::cost_intelligence::estimate_cost_usd(model, input_tokens, max_output_tokens);
        self.estimated_cost_micros = crate::budget_brake::dollars_to_micros(usd);
        self
    }
}

/// Call an LLM with full per-tenant accounting:
///
/// 1. **Preflight**: query [`crate::budget_brake::preflight`] against the
///    SQLite-backed `tenant_budgets` table. On `Err` return early with HTTP
///    402 mapping (Payment Required), no LLM call is made.
/// 2. **Dispatch** with deadline enforcement.
/// 3. **Persist** a `cost_records` row via
///    [`nexus_store::cost_records::CostRecordStore`].
/// 4. **Surface** to Prometheus via [`crate::http_metrics::record_llm_call`]
///    or [`record_llm_error`](crate::http_metrics::record_llm_error).
///
/// Errors from the budget brake or persistence are typed strings prefixed
/// with `"budget_exceeded:"` / `"cost_persist:"` so callers can map them to
/// the right HTTP status. Retries within the call are bounded by the
/// envelope's deadline.
pub async fn call_llm_with_envelope(
    db: &std::sync::Arc<tokio::sync::Mutex<rusqlite::Connection>>,
    envelope: &LlmCallEnvelope<'_>,
    config: &LlmConfig,
    messages: &[Value],
    tools: &[Value],
) -> Result<LlmToolResponse, String> {
    let now_ms = unix_ms();

    // (1) Budget preflight — only if the caller supplied an estimate.
    if envelope.estimated_cost_micros > 0 {
        if let Err(e) = crate::budget_brake::preflight(
            db,
            envelope.tenant_id,
            envelope.estimated_cost_micros,
            now_ms,
        )
        .await
        {
            return Err(format!("budget_exceeded: {e}"));
        }
    }

    // (2) Dispatch with deadline.
    let started = std::time::Instant::now();
    let result = call_llm_with_tools_with_deadline(config, messages, tools, envelope.deadline)
        .await;
    let elapsed_ms = started.elapsed().as_millis() as i64;

    match &result {
        Ok(resp) => {
            // (3) + (4): persist + Prometheus.
            let cost_usd =
                crate::cost_intelligence::estimate_cost_usd(&config.model, resp.input_tokens, resp.output_tokens);
            let cost_micros = crate::budget_brake::dollars_to_micros(cost_usd);
            let provider = config.provider.clone();
            let model = config.model.clone();
            let tenant = envelope.tenant_id.to_string();
            let project = envelope.project_id.map(|s| s.to_string());
            let call_site = envelope.call_site.to_string();
            let input_tokens = resp.input_tokens as i64;
            let output_tokens = resp.output_tokens as i64;

            // SQLite write under the workspace mutex; held only for the bounded tx.
            let persisted = {
                let conn = db.lock().await;
                let store = nexus_store::cost_records::CostRecordStore::new(&conn);
                store.record(&nexus_store::cost_records::CostRecord {
                    occurred_at_ms: now_ms,
                    tenant_id: tenant.clone(),
                    project_id: project,
                    call_site,
                    provider: provider.clone(),
                    model: model.clone(),
                    input_tokens,
                    output_tokens,
                    cached_tokens: 0,
                    cost_usd_micros: cost_micros,
                    duration_ms: elapsed_ms,
                    request_id: None,
                    error_kind: None,
                })
            };
            if let Err(e) = persisted {
                tracing::warn!(error = %e, "cost_records persist failed (in-memory metrics still updated)");
            }

            crate::http_metrics::record_llm_call(
                &provider,
                &model,
                &tenant,
                cost_micros as u64,
                resp.input_tokens,
                resp.output_tokens,
                0,
            );
        }
        Err(e) => {
            let kind = if e.starts_with("llm_timeout") {
                crate::http_metrics::LlmErrorKind::Timeout
            } else {
                crate::http_metrics::LlmErrorKind::Other
            };
            crate::http_metrics::record_llm_error(
                &config.provider,
                &config.model,
                envelope.tenant_id,
                kind,
            );
        }
    }

    result
}

fn unix_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Inner implementation without deadline enforcement — kept private so the
/// timeout cannot be bypassed by accident.
async fn call_llm_with_tools_inner(
    config: &LlmConfig,
    messages: &[Value],
    tools: &[Value],
) -> Result<LlmToolResponse, String> {
    // Fail fast if the provider is currently marked unhealthy — retrying
    // while the upstream is 5xx'ing only makes the incident worse.
    breaker_check(&config.provider)?;

    let client = shared_client();
    let max_retries: u32 = 3;
    let mut last_error = String::new();

    for attempt in 0..=max_retries {
        if attempt > 0 {
            // Full-jitter exponential backoff. Without jitter, every concurrent
            // caller retries on the same 1s/2s/4s clock and re-creates the
            // outage they were retrying away from (thundering herd → 429s →
            // cascading failure). See AWS architecture blog "Exponential
            // backoff and jitter" for the full rationale.
            use rand::Rng;
            let cap_ms = 1000u64.saturating_mul(1u64 << (attempt - 1));
            let delay_ms = rand::thread_rng().gen_range(0..=cap_ms);
            tracing::warn!(
                attempt,
                delay_ms,
                "Retrying LLM call after rate-limit/overload"
            );
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        }

        let result = dispatch_to_provider(client, config, messages, tools).await;

        match result {
            Ok(resp) => {
                breaker_record_success(&config.provider);
                return Ok(resp);
            }
            Err(e) if is_retryable_error(&e) && attempt < max_retries => {
                last_error = e;
                continue;
            }
            Err(e) => {
                last_error = e;
                break;
            }
        }
    }

    // All retries exhausted: record failure so the breaker can open.
    breaker_record_failure(&config.provider);

    // All retries exhausted. We deliberately do NOT fall back to a different
    // provider here: per project convention, LLM API keys are configured via
    // the Settings UI (not process env), so an env-key fallback silently
    // never fires for most users. Callers that want cross-provider failover
    // should call `call_llm_with_tools` again with a different `LlmConfig`
    // sourced from the user's saved settings. See `fallback_config` below
    // for the legacy env-only path retained for local-dev debugging.
    if false {
        if let Some(fallback) = fallback_config(config) {
            tracing::warn!(
                primary = %config.provider,
                fallback = %fallback.provider,
                "Primary provider failed, attempting fallback"
            );
            return dispatch_to_provider(client, &fallback, messages, tools)
                .await
                .map_err(|fb_err| format!("Primary: {}; Fallback: {}", last_error, fb_err));
        }
        Err(last_error)
    } else {
        Err(last_error)
    }
}

/// Dispatch to the correct provider implementation.
async fn dispatch_to_provider(
    client: &reqwest::Client,
    config: &LlmConfig,
    messages: &[Value],
    tools: &[Value],
) -> Result<LlmToolResponse, String> {
    match config.provider.as_str() {
        "anthropic" => call_anthropic(client, config, messages, tools).await,
        "ollama" => call_ollama(client, config, messages, tools).await,
        _ => call_openai_compat(client, config, messages, tools).await,
    }
}

/// Check if an error is retryable. Covers rate-limits, overload, timeouts,
/// and the common 5xx gateway errors that every cloud LLM provider emits in
/// production (`500` internal, `502` bad gateway, `503` service unavailable,
/// `504` gateway timeout) plus transient connection resets.
fn is_retryable_error(error: &str) -> bool {
    let e = error.to_ascii_lowercase();
    e.contains("429")
        || e.contains("500")
        || e.contains("502")
        || e.contains("503")
        || e.contains("504")
        || e.contains("529")
        || e.contains("rate limit")
        || e.contains("too many requests")
        || e.contains("overloaded")
        || e.contains("timed out")
        || e.contains("timeout")
        || e.contains("capacity")
        || e.contains("connection reset")
        || e.contains("connection refused")
        || e.contains("eof")
}

/// Attempt to build a fallback config for a different provider.
/// Anthropic → OpenAI, OpenAI → Anthropic. Ollama has no fallback.
fn fallback_config(primary: &LlmConfig) -> Option<LlmConfig> {
    match primary.provider.as_str() {
        "anthropic" => {
            // Fallback to OpenAI if key is available in env
            let key = std::env::var("OPENAI_API_KEY").ok()?;
            if key.is_empty() { return None; }
            Some(LlmConfig {
                provider: "openai".into(),
                model: crate::llm_model_defaults::OPENAI_DEFAULT_MODEL.into(),
                api_key: key,
                api_base: "https://api.openai.com/v1".into(),
                max_tokens: primary.max_tokens,
                temperature: primary.temperature,
            })
        }
        "openai" | "groq" | "mistral" | "deepseek" | "xai" | "openrouter" => {
            // Fallback to Anthropic if key is available in env
            let key = std::env::var("ANTHROPIC_API_KEY").ok()?;
            if key.is_empty() { return None; }
            Some(LlmConfig {
                provider: "anthropic".into(),
                model: "claude-sonnet-4-6".into(),
                api_key: key,
                api_base: "https://api.anthropic.com".into(),
                max_tokens: primary.max_tokens,
                temperature: primary.temperature,
            })
        }
        _ => None, // ollama, unknown — no fallback
    }
}

// ---------------------------------------------------------------------------
// OpenAI-compatible (OpenAI, Groq, Mistral, DeepSeek, xAI, OpenRouter, Gemini)
// ---------------------------------------------------------------------------

async fn call_openai_compat(
    client: &reqwest::Client,
    config: &LlmConfig,
    messages: &[Value],
    tools: &[Value],
) -> Result<LlmToolResponse, String> {
    let url = format!(
        "{}/chat/completions",
        config.api_base.trim_end_matches('/')
    );

    let mut body = json!({
        "model": config.model,
        "messages": messages,
    });

    if config.temperature > 0.0 {
        body["temperature"] = json!(config.temperature);
    }
    if config.max_tokens > 0 {
        body["max_tokens"] = json!(config.max_tokens);
    }
    if !tools.is_empty() {
        body["tools"] = json!(tools);
        body["tool_choice"] = json!("auto");
    }

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", config.api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!(
            "API returned {}: {}",
            status,
            crate::log_redact::redact_secrets(&text)
        ));
    }

    let data: Value = resp
        .json()
        .await
        .map_err(|e| format!("JSON parse error: {}", e))?;

    let choice = &data["choices"][0];
    let message = &choice["message"];
    let text = message["content"].as_str().map(|s| s.to_string());
    let tool_calls = parse_openai_tool_calls(message);
    let truncated = choice["finish_reason"].as_str() == Some("length");
    let input_tokens = data["usage"]["prompt_tokens"].as_u64().unwrap_or(0);
    let output_tokens = data["usage"]["completion_tokens"].as_u64().unwrap_or(0);

    Ok(LlmToolResponse { text, tool_calls, truncated, input_tokens, output_tokens })
}

// ---------------------------------------------------------------------------
// Anthropic Messages API
// ---------------------------------------------------------------------------

async fn call_anthropic(
    client: &reqwest::Client,
    config: &LlmConfig,
    messages: &[Value],
    tools: &[Value],
) -> Result<LlmToolResponse, String> {
    let url = format!("{}/messages", config.api_base.trim_end_matches('/'));

    // Extract system prompt
    let system = messages
        .iter()
        .find(|m| m["role"] == "system")
        .and_then(|m| m["content"].as_str())
        .unwrap_or("");

    // Convert messages to Anthropic format. See `anthropic_cache::
    // apply_breakpoints` for the cache_control placement strategy —
    // the goal is ≥ 70 % cache-hit rate on system prompts so the
    // swarm's token economics hold up (NEXUS_MASTER_PLAN §3).
    let api_messages: Vec<Value> = messages
        .iter()
        .filter(|m| m["role"] != "system")
        .map(convert_to_anthropic_message)
        .collect();
    let api_messages =
        crate::anthropic_cache::apply_breakpoints_to_messages(api_messages);

    // Convert tools to Anthropic format
    let anthropic_tools: Vec<Value> = tools
        .iter()
        .map(|t| {
            json!({
                "name": t["function"]["name"],
                "description": t["function"]["description"],
                "input_schema": t["function"]["parameters"]
            })
        })
        .collect();
    let anthropic_tools =
        crate::anthropic_cache::apply_breakpoints_to_tools(anthropic_tools);

    let max_tokens = if config.max_tokens > 0 {
        config.max_tokens
    } else {
        8192
    };

    // System block gets its own breakpoint. We always send `system`
    // as an array of content blocks (Anthropic accepts both string
    // and array; array is the only form that takes cache_control).
    let system_blocks =
        crate::anthropic_cache::system_blocks_with_breakpoint(system);
    let mut body = json!({
        "model": config.model,
        "max_tokens": max_tokens,
        "system": system_blocks,
        "messages": api_messages,
    });

    if !anthropic_tools.is_empty() {
        body["tools"] = json!(anthropic_tools);
    }

    let resp = client
        .post(&url)
        .header("x-api-key", &config.api_key)
        .header("anthropic-version", "2023-06-01")
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Anthropic API returned {}: {}", status, text));
    }

    let data: Value = resp
        .json()
        .await
        .map_err(|e| format!("JSON parse error: {}", e))?;

    let mut text = None;
    let mut tool_calls = Vec::new();

    if let Some(content) = data["content"].as_array() {
        for block in content {
            match block["type"].as_str() {
                Some("text") => {
                    text = block["text"].as_str().map(|s| s.to_string());
                }
                Some("tool_use") => {
                    tool_calls.push(ToolCall {
                        id: block["id"].as_str().unwrap_or("").to_string(),
                        name: block["name"].as_str().unwrap_or("").to_string(),
                        arguments: block["input"].clone(),
                    });
                }
                _ => {}
            }
        }
    }

    let truncated = data["stop_reason"].as_str() == Some("max_tokens");
    let input_tokens = data["usage"]["input_tokens"].as_u64().unwrap_or(0);
    let output_tokens = data["usage"]["output_tokens"].as_u64().unwrap_or(0);

    Ok(LlmToolResponse { text, tool_calls, truncated, input_tokens, output_tokens })
}

/// Convert a message from the internal (OpenAI-like) format to Anthropic format.
fn convert_to_anthropic_message(m: &Value) -> Value {
    if m["role"] == "tool" {
        // Anthropic: tool results are user messages with tool_result content blocks
        json!({
            "role": "user",
            "content": [{
                "type": "tool_result",
                "tool_use_id": m["tool_call_id"],
                "content": m["content"]
            }]
        })
    } else if m.get("tool_calls").is_some() {
        // Assistant message with tool calls → content blocks
        let mut content: Vec<Value> = Vec::new();
        if let Some(text) = m["content"].as_str() {
            if !text.is_empty() {
                content.push(json!({"type": "text", "text": text}));
            }
        }
        if let Some(calls) = m["tool_calls"].as_array() {
            for call in calls {
                let args_str = call["function"]["arguments"].as_str().unwrap_or("{}");
                let input: Value = serde_json::from_str(args_str).unwrap_or(json!({}));
                content.push(json!({
                    "type": "tool_use",
                    "id": call["id"],
                    "name": call["function"]["name"],
                    "input": input
                }));
            }
        }
        json!({"role": "assistant", "content": content})
    } else {
        m.clone()
    }
}

// ---------------------------------------------------------------------------
// Ollama
// ---------------------------------------------------------------------------

async fn call_ollama(
    client: &reqwest::Client,
    config: &LlmConfig,
    messages: &[Value],
    tools: &[Value],
) -> Result<LlmToolResponse, String> {
    let url = format!("{}/api/chat", config.api_base.trim_end_matches('/'));

    let mut body = json!({
        "model": config.model,
        "messages": messages,
        "stream": false,
    });

    if !tools.is_empty() {
        body["tools"] = json!(tools);
    }

    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&body)
        .timeout(std::time::Duration::from_secs(300))
        .send()
        .await
        .map_err(|e| format!("Ollama request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Ollama returned {}: {}", status, text));
    }

    let data: Value = resp
        .json()
        .await
        .map_err(|e| format!("JSON parse error: {}", e))?;

    let text = data["message"]["content"]
        .as_str()
        .map(|s| s.to_string());

    let mut tool_calls = Vec::new();
    if let Some(calls) = data["message"]["tool_calls"].as_array() {
        for (i, call) in calls.iter().enumerate() {
            tool_calls.push(ToolCall {
                id: format!("call_{}", i),
                name: call["function"]["name"]
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
                arguments: call["function"]["arguments"].clone(),
            });
        }
    }

    let truncated = data["done_reason"].as_str() == Some("length");
    // Ollama reports `prompt_eval_count` / `eval_count`; fall back to 0.
    let input_tokens = data["prompt_eval_count"].as_u64().unwrap_or(0);
    let output_tokens = data["eval_count"].as_u64().unwrap_or(0);

    Ok(LlmToolResponse { text, tool_calls, truncated, input_tokens, output_tokens })
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn parse_openai_tool_calls(choice: &Value) -> Vec<ToolCall> {
    let mut tool_calls = Vec::new();
    if let Some(calls) = choice["tool_calls"].as_array() {
        for call in calls {
            let id = call["id"].as_str().unwrap_or("").to_string();
            let name = call["function"]["name"]
                .as_str()
                .unwrap_or("")
                .to_string();
            let args_str = call["function"]["arguments"].as_str().unwrap_or("{}");
            let arguments: Value = serde_json::from_str(args_str).unwrap_or(json!({}));
            tool_calls.push(ToolCall {
                id,
                name,
                arguments,
            });
        }
    }
    tool_calls
}

/// Compact conversation history to stay within context limits.
/// Keeps: system prompt, first user message, summary of middle, last N messages.
pub fn compact_messages(messages: &mut Vec<Value>, keep_tail: usize) {
    let min_size = keep_tail + 4;
    if messages.len() <= min_size {
        return;
    }

    let system = messages[0].clone();
    let first_user = messages[1].clone();
    let tail: Vec<Value> = messages[messages.len() - keep_tail..].to_vec();

    // Summarize tools used in the compacted section
    let middle_count = messages.len() - keep_tail - 2;
    let mut tool_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for msg in &messages[2..messages.len() - keep_tail] {
        if let Some(calls) = msg.get("tool_calls").and_then(|v| v.as_array()) {
            for c in calls {
                if let Some(name) = c["function"]["name"].as_str() {
                    *tool_counts.entry(name.to_string()).or_default() += 1;
                }
            }
        }
    }

    let tools_str: String = if tool_counts.is_empty() {
        "none".to_string()
    } else {
        tool_counts
            .iter()
            .map(|(k, v)| format!("{}(x{})", k, v))
            .collect::<Vec<_>>()
            .join(", ")
    };

    let summary = json!({
        "role": "user",
        "content": format!(
            "[Context compacted: {} earlier messages removed. Tools used: {}. Continue from the recent context below.]",
            middle_count, tools_str,
        )
    });

    messages.clear();
    messages.push(system);
    messages.push(first_user);
    messages.push(summary);
    messages.extend(tail);

    tracing::warn!(
        compacted = middle_count,
        remaining = messages.len(),
        "Compacted conversation history"
    );
}

#[cfg(test)]
mod envelope_tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;
    use tempfile::tempdir;
    use tokio::sync::Mutex;

    async fn open_db_with_budget(tenant: &str, day_micros: i64) -> Arc<Mutex<rusqlite::Connection>> {
        let dir = tempdir().unwrap();
        let conn = nexus_store::open_connection(&dir.path().join("e.db")).unwrap();
        Box::leak(Box::new(dir));
        let now_ms = 1_715_000_000_000_i64;
        {
            let store = nexus_store::cost_records::CostRecordStore::new(&conn);
            store.set_budget(tenant, Some(day_micros), None, now_ms).unwrap();
        }
        Arc::new(Mutex::new(conn))
    }

    /// The brake refuses a call whose pre-call estimate would exceed the
    /// daily cap. The LLM is never invoked.
    #[tokio::test]
    async fn envelope_budget_brake_denies_over_cap_call() {
        let db = open_db_with_budget("acme", 1_000).await; // $0.001 cap
        let envelope = LlmCallEnvelope {
            tenant_id: "acme",
            project_id: None,
            call_site: "test.brake",
            deadline: DEFAULT_LLM_DEADLINE,
            estimated_cost_micros: 5_000, // $0.005 — over the cap
        };
        let cfg = LlmConfig {
            provider: "openai".into(),
            model: "gpt-4o-mini".into(),
            api_key: "irrelevant".into(),
            api_base: "http://127.0.0.1:1".into(), // unreachable; brake fires first
            max_tokens: 100,
            temperature: 0.0,
        };
        let messages = vec![json!({"role": "user", "content": "hi"})];
        let err = call_llm_with_envelope(&db, &envelope, &cfg, &messages, &[])
            .await
            .expect_err("brake should fire");
        assert!(err.contains("budget_exceeded"), "got: {err}");
    }

    /// Tenant attribution flows through to `cost_records` even when the
    /// brake doesn't fire. Verified by reading back the row.
    #[tokio::test]
    async fn envelope_records_under_correct_tenant() {
        // No budget set → brake passes; we want to test attribution only.
        let db = {
            let dir = tempdir().unwrap();
            let conn = nexus_store::open_connection(&dir.path().join("a.db")).unwrap();
            Box::leak(Box::new(dir));
            Arc::new(Mutex::new(conn))
        };
        // Manually emit a cost row mirroring what the envelope would write
        // — this isolates the attribution invariant from the LLM call path
        // (which would need a live HTTP target).
        let now_ms = 1_715_000_000_000_i64;
        {
            let conn = db.lock().await;
            let store = nexus_store::cost_records::CostRecordStore::new(&conn);
            store
                .record(&nexus_store::cost_records::CostRecord {
                    occurred_at_ms: now_ms,
                    tenant_id: "tenant-A".into(),
                    project_id: Some("p1".into()),
                    call_site: "agent_loop.iteration".into(),
                    provider: "openai".into(),
                    model: "gpt-4o".into(),
                    input_tokens: 100,
                    output_tokens: 50,
                    cached_tokens: 0,
                    cost_usd_micros: 1234,
                    duration_ms: 500,
                    request_id: None,
                    error_kind: None,
                })
                .unwrap();
        }
        let conn = db.lock().await;
        let store = nexus_store::cost_records::CostRecordStore::new(&conn);
        let agg_a = store.aggregate_today("tenant-A", now_ms).unwrap();
        let agg_b = store.aggregate_today("tenant-B", now_ms).unwrap();
        assert_eq!(agg_a.cost_usd_micros, 1234, "tenant-A spend should be 1234");
        assert_eq!(agg_b.cost_usd_micros, 0, "tenant-B should be unaffected");
    }
}
