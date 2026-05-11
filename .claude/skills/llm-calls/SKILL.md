---
name: llm-calls
description: Make LLM calls correctly in nexus-http — tool calling, provider dispatch, retry, caching, cost tracking, rate limiting. Use whenever writing code that calls an LLM.
---

# LLM Calls in nexus-http

## The only entry point: `llm_client::call_llm_with_tools`

Never create a new `reqwest::Client` for LLM calls. Never call provider APIs directly. All LLM calls go through:

```rust
use crate::llm_client::{call_llm_with_tools, LlmConfig, LlmToolResponse};
```

This gives you: automatic retry (3 attempts, exponential backoff on 429/529/timeout), provider fallback (Anthropic → OpenAI, OpenAI → Anthropic), and connection pooling.

## Building LlmConfig

```rust
use crate::llm_client::LlmConfig;

// From AppState defaults (most common path)
fn config_from_state(state: &AppState) -> LlmConfig {
    LlmConfig {
        provider: "openai".into(),          // or "anthropic", "ollama", "groq", "mistral"
        model: state.model.clone(),         // e.g. "gpt-4o"
        api_key: state.openai_api_key.clone(),
        api_base: "https://api.openai.com/v1".into(),
        max_tokens: 4096,
        temperature: 0.7,
    }
}

// Anthropic variant
fn anthropic_config(state: &AppState) -> Option<LlmConfig> {
    let key = state.anthropic_api_key.clone()?;
    Some(LlmConfig {
        provider: "anthropic".into(),
        model: "claude-sonnet-4-20250514".into(),
        api_key: key,
        api_base: "https://api.anthropic.com".into(),
        max_tokens: 8096,
        temperature: 0.7,
    })
}

// Ollama (local, no key needed)
fn ollama_config(model: &str) -> LlmConfig {
    LlmConfig {
        provider: "ollama".into(),
        model: model.into(),
        api_key: String::new(),
        api_base: "http://localhost:11434".into(),
        max_tokens: 4096,
        temperature: 0.7,
    }
}
```

## Simple completion (no tools)

Pass an empty `tools` slice:

```rust
use serde_json::json;

let messages = vec![
    json!({ "role": "system", "content": "You are a helpful assistant." }),
    json!({ "role": "user", "content": user_prompt }),
];

let response = call_llm_with_tools(&config, &messages, &[]).await
    .map_err(|e| ApiError::Internal(format!("LLM call failed: {e}")))?;

let text = response.text.unwrap_or_default();
```

## Tool-calling completion

Define tools as JSON following the OpenAI function-calling schema (works for all providers — the client translates for Anthropic):

```rust
let tools = vec![
    json!({
        "type": "function",
        "function": {
            "name": "write_file",
            "description": "Write content to a file path",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path" },
                    "content": { "type": "string", "description": "File content" }
                },
                "required": ["path", "content"]
            }
        }
    }),
];

let response = call_llm_with_tools(&config, &messages, &tools).await
    .map_err(|e| ApiError::Internal(e))?;

// Process tool calls
for tool_call in &response.tool_calls {
    match tool_call.name.as_str() {
        "write_file" => {
            let path = tool_call.arguments["path"].as_str().unwrap_or_default();
            let content = tool_call.arguments["content"].as_str().unwrap_or_default();
            std::fs::write(path, content)?;
        }
        _ => tracing::warn!(name = %tool_call.name, "Unknown tool call"),
    }
}
```

## Rate limiting — always acquire a slot for LLM-heavy handlers

```rust
// Acquire a concurrency slot BEFORE the LLM call
// This prevents thundering-herd when N requests hit at once
let _slot = state.rate_limiter.acquire_llm_slot().await
    .map_err(|e| ApiError::TooManyRequests(e))?;

let response = call_llm_with_tools(&config, &messages, &[]).await?;
// _slot is dropped here, freeing the concurrency permit
```

For non-LLM endpoints the slot is not needed.

## LLM response cache — avoid duplicate calls

The `LlmCache` in AppState deduplicates identical prompt + model combinations. Use it for deterministic, idempotent calls (not conversational ones):

```rust
use crate::cache::LlmCache;

let cache_key = format!("{model}:{}", sha256_of_prompt);

if let Some(cached) = state.llm_cache.get(&cache_key).await {
    return Ok(Json(MyResponse { output: cached }));
}

let response = call_llm_with_tools(&config, &messages, &[]).await?;
let text = response.text.unwrap_or_default();

state.llm_cache.set(cache_key, text.clone()).await;
Ok(Json(MyResponse { output: text }))
```

## Cost tracking — record every LLM call

```rust
use crate::cost_intelligence::LlmCallRecord;

let start = std::time::Instant::now();
let response = call_llm_with_tools(&config, &messages, &[]).await?;
let latency_ms = start.elapsed().as_millis() as u64;

// Record for cost dashboard and budget enforcement
state.cost_tracker.record(LlmCallRecord {
    id: uuid::Uuid::new_v4().to_string(),
    project_id: Some(project_id.clone()),
    model: config.model.clone(),
    provider: config.provider.clone(),
    input_tokens: 0,   // fill from response headers if available
    output_tokens: 0,
    total_tokens: 0,
    cost_usd: 0.0,     // cost_tracker can estimate from model+tokens
    latency_ms,
    purpose: "my_feature".into(),
    timestamp: chrono::Utc::now().to_rfc3339(),
}).await;
```

## Multi-turn conversation messages

Build conversation history properly:

```rust
let mut messages: Vec<Value> = vec![
    json!({ "role": "system", "content": system_prompt }),
];

// Add previous conversation turns
for msg in &conversation_history {
    messages.push(json!({ "role": msg.role, "content": msg.content }));
}

// Add the new user message
messages.push(json!({ "role": "user", "content": user_message }));
```

## Provider capabilities and limits

| Provider | Max tokens | Tools | Streaming | Notes |
|----------|-----------|-------|-----------|-------|
| `openai` | 128k context | Yes | Yes | Default. `gpt-4o` standard. |
| `anthropic` | 200k context | Yes | Yes | `claude-sonnet-4-20250514`. Best for long context. |
| `ollama` | Model-dependent | No | Yes | Local only. No fallback. |
| `groq` | 8k | Yes | Yes | Ultra-fast, use for latency-sensitive paths. |
| `mistral` | 32k | Yes | Yes | Good cost/quality ratio. |
| `deepseek` | 64k | Yes | Yes | Cost-efficient. |

## Critical rules

- Never create a `reqwest::Client` — use `call_llm_with_tools` which uses the pooled singleton
- Never call provider REST APIs directly — the client handles auth headers, body format, and response parsing for all providers
- Always acquire a rate-limiter slot for LLM calls in handlers
- Always record cost after each LLM call
- Use `temperature: 0.0` for deterministic classification tasks; `0.7-1.0` for creative/generation tasks
- For JSON-only responses, append `"Respond only in valid JSON."` to the system prompt — don't rely on `response_format`
