//! LLM provider settings — manage API keys and model preferences.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use tracing::info;

use nexus_store::ProjectService;

use crate::{
    error::{ApiError, ApiResult},
    security::auth::{AuthContext, Scope},
    state::AppState,
};

/// Build the settings-table key where a tenant's LLM API key is stored.
///
/// For the legacy single-tenant default tenant we still write to the bare
/// `llm.<provider>.api_key` row so existing installs keep working without a
/// data migration. Real multi-tenant installs namespace by tenant id.
pub fn llm_key_row_for_write(tenant_id: &str, provider: &str) -> String {
    if tenant_id.is_empty() || tenant_id == "default" {
        format!("llm.{provider}.api_key")
    } else {
        format!("llm.tenant:{tenant_id}.{provider}.api_key")
    }
}

/// Keys to try, in order, when reading an API key for a given caller. A real
/// multi-tenant caller reads the tenant-scoped row first and only falls back
/// to the legacy global row if no tenant-scoped key is configured. Single-
/// tenant callers (tenant == "default" or unknown) only read the legacy row.
pub fn llm_key_rows_for_read(tenant_id: Option<&str>, provider: &str) -> Vec<String> {
    let mut out = Vec::with_capacity(2);
    if let Some(t) = tenant_id {
        if !t.is_empty() && t != "default" {
            out.push(format!("llm.tenant:{t}.{provider}.api_key"));
        }
    }
    out.push(format!("llm.{provider}.api_key"));
    out
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmProvider {
    pub id: String,
    pub name: String,
    pub api_base: String,
    pub requires_key: bool,
    pub models: Vec<LlmModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmModel {
    pub id: String,
    pub name: String,
    pub context_window: u32,
    pub supports_streaming: bool,
}

/// GET /settings/providers — list all available LLM providers and their models
pub async fn list_providers(
    State(_app): State<Arc<AppState>>,
) -> ApiResult<Json<serde_json::Value>> {
    let m = |id: &str, name: &str, ctx: u32| LlmModel {
        id: id.into(), name: name.into(), context_window: ctx, supports_streaming: true,
    };

    let providers = vec![
        // ── OpenAI ──────────────────────────────────────────────────
        LlmProvider {
            id: "openai".into(),
            name: "OpenAI".into(),
            api_base: "https://api.openai.com/v1".into(),
            requires_key: true,
            models: vec![
                m("gpt-4.1",            "GPT-4.1",              1_047_576),
                m("gpt-4.1-mini",       "GPT-4.1 Mini",         1_047_576),
                m("gpt-4.1-nano",       "GPT-4.1 Nano",         1_047_576),
                m("gpt-4o",             "GPT-4o",               128_000),
                m("o3-mini",            "o3-mini",              200_000),
            ],
        },
        // ── Anthropic ───────────────────────────────────────────────
        LlmProvider {
            id: "anthropic".into(),
            name: "Anthropic".into(),
            api_base: "https://api.anthropic.com/v1".into(),
            requires_key: true,
            models: vec![
                m("claude-sonnet-4-6",           "Claude Sonnet 4.6",   1_000_000),
                m("claude-opus-4-6",             "Claude Opus 4.6",     1_000_000),
                m("claude-haiku-4-5-20251001",   "Claude Haiku 4.5",    200_000),
                m("claude-sonnet-4-5-20250929",  "Claude Sonnet 4.5",   200_000),
                m("claude-opus-4-5-20251101",    "Claude Opus 4.5",     200_000),
                m("claude-sonnet-4-20250514",    "Claude Sonnet 4",     200_000),
                m("claude-opus-4-20250514",      "Claude Opus 4",       200_000),
            ],
        },
        // ── Google Gemini ───────────────────────────────────────────
        LlmProvider {
            id: "gemini".into(),
            name: "Google Gemini".into(),
            api_base: "https://generativelanguage.googleapis.com/v1beta".into(),
            requires_key: true,
            models: vec![
                m("gemini-2.5-pro",        "Gemini 2.5 Pro",       1_048_576),
                m("gemini-2.5-flash",      "Gemini 2.5 Flash",     1_048_576),
                m("gemini-2.5-flash-lite", "Gemini 2.5 Flash-Lite",1_048_576),
            ],
        },
        // ── Mistral ────────────────────────────────────────────────
        LlmProvider {
            id: "mistral".into(),
            name: "Mistral AI".into(),
            api_base: "https://api.mistral.ai/v1".into(),
            requires_key: true,
            models: vec![
                m("mistral-large-latest",  "Mistral Large 3",     256_000),
                m("mistral-small-latest",  "Mistral Small 4",     256_000),
                m("codestral-latest",      "Codestral",           256_000),
                m("devstral-latest",       "Devstral 2",          256_000),
                m("ministral-8b-latest",   "Ministral 8B",        256_000),
            ],
        },
        // ── DeepSeek ───────────────────────────────────────────────
        LlmProvider {
            id: "deepseek".into(),
            name: "DeepSeek".into(),
            api_base: "https://api.deepseek.com/v1".into(),
            requires_key: true,
            models: vec![
                m("deepseek-chat",      "DeepSeek V3 (Chat)",     128_000),
                m("deepseek-reasoner",  "DeepSeek V3 (Reasoning)",128_000),
            ],
        },
        // ── xAI (Grok) ─────────────────────────────────────────────
        LlmProvider {
            id: "xai".into(),
            name: "xAI (Grok)".into(),
            api_base: "https://api.x.ai/v1".into(),
            requires_key: true,
            models: vec![
                m("grok-3-fast",  "Grok 3 Fast",   131_072),
                m("grok-3-mini-fast", "Grok 3 Mini Fast", 131_072),
            ],
        },
        // ── Groq ───────────────────────────────────────────────────
        LlmProvider {
            id: "groq".into(),
            name: "Groq".into(),
            api_base: "https://api.groq.com/openai/v1".into(),
            requires_key: true,
            models: vec![
                m("llama-3.3-70b-versatile",                     "Llama 3.3 70B",       131_072),
                m("llama-3.1-8b-instant",                        "Llama 3.1 8B",        131_072),
                m("meta-llama/llama-4-scout-17b-16e-instruct",   "Llama 4 Scout 17B",   131_072),
                m("qwen/qwen3-32b",                              "Qwen3 32B",           131_072),
            ],
        },
        // ── Ollama (Local) ──────────────────────────────────────────
        LlmProvider {
            id: "ollama".into(),
            name: "Ollama (Local)".into(),
            api_base: "http://localhost:11434/v1".into(),
            requires_key: false,
            models: vec![
                m("llama3.3",          "Llama 3.3 70B",         128_000),
                m("llama3.2",          "Llama 3.2 3B",          128_000),
                m("llama3.1",          "Llama 3.1 8B",          128_000),
                m("qwen3",             "Qwen3",                 128_000),
                m("qwen2.5-coder",     "Qwen 2.5 Coder",       128_000),
                m("gemma3",            "Gemma 3",               128_000),
                m("mistral",           "Mistral 7B",            32_000),
                m("deepseek-r1",       "DeepSeek R1",           128_000),
                m("phi4",              "Phi-4 14B",             16_000),
                m("codellama",         "Code Llama",            16_000),
            ],
        },
        // ── OpenRouter (Aggregator) ─────────────────────────────────
        LlmProvider {
            id: "openrouter".into(),
            name: "OpenRouter".into(),
            api_base: "https://openrouter.ai/api/v1".into(),
            requires_key: true,
            models: vec![
                m("anthropic/claude-sonnet-4-6",    "Claude Sonnet 4.6",    1_000_000),
                m("openai/gpt-4.1",                 "GPT-4.1",              1_047_576),
                m("google/gemini-2.5-flash",        "Gemini 2.5 Flash",     1_048_576),
                m("google/gemini-2.5-pro",          "Gemini 2.5 Pro",       1_048_576),
                m("deepseek/deepseek-chat",         "DeepSeek V3",          128_000),
                m("mistralai/mistral-large-2512",   "Mistral Large 3",      256_000),
                m("x-ai/grok-3-fast",               "Grok 3 Fast",         131_072),
                m("meta-llama/llama-3.3-70b-instruct", "Llama 3.3 70B",    131_072),
            ],
        },
    ];

    Ok(Json(serde_json::to_value(providers)?))
}

#[derive(Deserialize)]
pub struct SetApiKeyReq {
    pub provider: String,
    pub api_key: String,
}

/// POST /settings/api-keys — store an API key for a provider (admin-only).
pub async fn set_api_key(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<SetApiKeyReq>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::SettingsManage)
        .map_err(|_| ApiError::Forbidden("settings:manage scope required".into()))?;
    {
        let db = app.db.lock().await;
        let svc = ProjectService::new(&db);
        let key_name = llm_key_row_for_write(&auth.tenant_id, &body.provider);
        svc.set_setting(&key_name, &body.api_key)?;
    }
    info!(
        provider = %body.provider,
        actor = %auth.user_id,
        tenant = %auth.tenant_id,
        "API key stored"
    );

    // ADR-003 §5: registry must reflect the new key without a server restart.
    // Single-tenant installs use the legacy global registry; multi-tenant
    // installs need per-tenant registries (tracked separately).
    if auth.tenant_id.is_empty() || auth.tenant_id == "default" {
        let cfg = read_builtin_provider_config_from_settings(&app, &auth.tenant_id).await;
        app.reload_provider_registry(&cfg).await;
    }

    Ok(Json(serde_json::json!({ "status": "ok", "provider": body.provider })))
}

/// GET /settings/api-keys — list which providers have keys configured
pub async fn list_api_keys(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::SettingsManage)
        .map_err(|_| ApiError::Forbidden("settings:manage scope required".into()))?;
    let db = app.db.lock().await;
    let svc = ProjectService::new(&db);

    let providers = [
        "openai", "anthropic", "gemini", "mistral", "deepseek",
        "xai", "groq", "ollama", "openrouter",
    ];
    let mut configured = Vec::new();

    for p in providers {
        // Check tenant-scoped row first, then fall back to legacy global.
        let has_key = llm_key_rows_for_read(Some(&auth.tenant_id), p)
            .iter()
            .any(|k| matches!(svc.get_setting(k), Ok(Some(_))));
        // Also check env vars as fallback
        let has_env = match p {
            "openai" => app.openai_api_key != "sk-placeholder" && !app.openai_api_key.is_empty(),
            "anthropic" => app.anthropic_api_key.is_some(),
            "ollama" => true, // no key needed
            _ => false,
        };
        configured.push(serde_json::json!({
            "provider": p,
            "configured": has_key || has_env,
            "source": if has_key { "settings" } else if has_env { "env" } else { "none" }
        }));
    }

    Ok(Json(serde_json::json!({ "providers": configured })))
}

/// DELETE /settings/api-keys/:provider — remove an API key (admin-only).
pub async fn delete_api_key(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Path(provider): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::SettingsManage)
        .map_err(|_| ApiError::Forbidden("settings:manage scope required".into()))?;
    {
        let db = app.db.lock().await;
        // Only delete *this* tenant's key — never the global/legacy key belonging
        // to someone else. Admins can still delete the legacy row via the
        // `default` tenant account.
        let key_name = llm_key_row_for_write(&auth.tenant_id, &provider);
        db.execute(
            "DELETE FROM settings WHERE key = ?1",
            rusqlite::params![key_name],
        )?;
    }
    info!(
        provider = %provider,
        actor = %auth.user_id,
        tenant = %auth.tenant_id,
        "API key deleted"
    );

    // ADR-003 §5: rebuild the registry so the deleted provider stops serving.
    if auth.tenant_id.is_empty() || auth.tenant_id == "default" {
        let cfg = read_builtin_provider_config_from_settings(&app, &auth.tenant_id).await;
        app.reload_provider_registry(&cfg).await;
    }

    Ok(Json(serde_json::json!({ "status": "ok", "provider": provider })))
}

/// Read the live API keys for `tenant_id` from the settings table, with a
/// fall-through to env for the global tenant. Returns the
/// [`BuiltinProviderConfig`] that [`AppState::reload_provider_registry`]
/// expects.
pub async fn read_builtin_provider_config_from_settings(
    app: &Arc<AppState>,
    tenant_id: &str,
) -> nexus_providers::BuiltinProviderConfig {
    let db = app.db.lock().await;
    let svc = ProjectService::new(&db);
    let read = |provider: &str| -> Option<String> {
        for k in llm_key_rows_for_read(Some(tenant_id), provider) {
            if let Ok(Some(v)) = svc.get_setting(&k) {
                if !v.is_empty() {
                    return Some(v);
                }
            }
        }
        None
    };

    let openai_api_key = read("openai").or_else(|| {
        if !app.openai_api_key.is_empty() && app.openai_api_key != "sk-placeholder" {
            Some(app.openai_api_key.clone())
        } else {
            None
        }
    });
    let anthropic_api_key = read("anthropic").or_else(|| app.anthropic_api_key.clone());

    nexus_providers::BuiltinProviderConfig {
        openai_api_key,
        openai_base_url: None,
        anthropic_api_key,
        ollama_base_url: None,
    }
}

/// GET /settings/default-model — get the default model
pub async fn get_default_model(
    State(app): State<Arc<AppState>>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    let svc = ProjectService::new(&db);
    let provider = svc.get_setting("llm.default.provider")?.unwrap_or_else(|| "openai".to_string());
    let model = svc.get_setting("llm.default.model")?.unwrap_or_else(|| app.model.clone());
    Ok(Json(serde_json::json!({ "provider": provider, "model": model })))
}

#[derive(Deserialize)]
pub struct SetDefaultModelReq {
    pub provider: String,
    pub model: String,
}

/// POST /settings/default-model — set the default model (admin-only).
pub async fn set_default_model(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<SetDefaultModelReq>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::SettingsManage)
        .map_err(|_| ApiError::Forbidden("settings:manage scope required".into()))?;
    let db = app.db.lock().await;
    let svc = ProjectService::new(&db);
    svc.set_setting("llm.default.provider", &body.provider)?;
    svc.set_setting("llm.default.model", &body.model)?;
    info!(provider = %body.provider, model = %body.model, "Default model updated");
    Ok(Json(serde_json::json!({ "status": "ok", "provider": body.provider, "model": body.model })))
}

// ---------------------------------------------------------------------------
//  Sandbox settings
// ---------------------------------------------------------------------------

/// GET /settings/sandbox — get sandbox configuration
pub async fn get_sandbox_settings(
    State(app): State<Arc<AppState>>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = app.db.lock().await;
    let svc = ProjectService::new(&db);

    // Default to sandboxed execution. Users can opt out explicitly (and should
    // only do so on trusted local hardware) by setting this to "false" in the
    // Settings UI. Otherwise any authenticated `runtime:manage` caller could
    // run untrusted npm install scripts on the host.
    let enabled = svc.get_setting("sandbox.enabled")?
        .map(|v| v == "true")
        .unwrap_or(true);

    // Check if Docker is available on the host
    let docker_available = nexus_store::app_runner::check_docker_available().is_ok();

    Ok(Json(serde_json::json!({
        "enabled": enabled,
        "docker_available": docker_available,
    })))
}

#[derive(Deserialize)]
pub struct SetSandboxReq {
    pub enabled: bool,
}

/// POST /settings/sandbox — enable or disable sandbox mode by default (admin-only).
pub async fn set_sandbox_settings(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<SetSandboxReq>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_scope(&Scope::SettingsManage)
        .map_err(|_| ApiError::Forbidden("settings:manage scope required".into()))?;
    // Validate Docker is available before enabling
    if body.enabled {
        if let Err(e) = nexus_store::app_runner::check_docker_available() {
            return Err(crate::error::ApiError::BadRequest(format!(
                "Cannot enable sandbox: {}", e
            )));
        }
    }

    let db = app.db.lock().await;
    let svc = ProjectService::new(&db);
    svc.set_setting("sandbox.enabled", if body.enabled { "true" } else { "false" })?;
    info!(enabled = body.enabled, "Sandbox setting updated");
    Ok(Json(serde_json::json!({ "status": "ok", "enabled": body.enabled })))
}
