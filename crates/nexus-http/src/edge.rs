//! Edge and embedded deployment support.
//!
//! Provides resource-constrained deployment profiles, offline LLM routing
//! (Ollama / llama.cpp), and adaptive degradation for low-memory devices.

use serde::{Deserialize, Serialize};
use tracing::info;

/// A named resource profile for a deployment environment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentProfile {
    /// Full cloud deployment — no restrictions.
    Cloud,
    /// Edge server (e.g. Raspberry Pi 4, Jetson) — moderate constraints.
    Edge,
    /// Micro embedded device — severe constraints, offline-only.
    Micro,
    /// Custom — explicit limits set by the user.
    Custom,
}

/// Resource limits for a deployment profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceProfile {
    pub name: DeploymentProfile,
    /// Max concurrent agent tasks.
    pub max_concurrent_tasks: usize,
    /// Max memory per agent task (MB).
    pub max_task_memory_mb: u32,
    /// Max LLM context window tokens to request.
    pub max_context_tokens: u32,
    /// Max vector search results returned.
    pub max_vector_results: usize,
    /// Whether to allow cloud LLM calls.
    pub allow_cloud_llm: bool,
    /// Local LLM base URL (Ollama / llama.cpp), if any.
    pub local_llm_url: Option<String>,
    /// Local LLM model name (e.g. "llama3.2", "mistral").
    pub local_llm_model: Option<String>,
    /// Whether to disable the Web UI (serve API only).
    pub headless: bool,
}

impl ResourceProfile {
    pub fn cloud() -> Self {
        Self {
            name: DeploymentProfile::Cloud,
            max_concurrent_tasks: 32,
            max_task_memory_mb: 4096,
            max_context_tokens: 128_000,
            max_vector_results: 50,
            allow_cloud_llm: true,
            local_llm_url: None,
            local_llm_model: None,
            headless: false,
        }
    }

    pub fn edge() -> Self {
        Self {
            name: DeploymentProfile::Edge,
            max_concurrent_tasks: 4,
            max_task_memory_mb: 512,
            max_context_tokens: 8_192,
            max_vector_results: 10,
            allow_cloud_llm: true,
            local_llm_url: std::env::var("NEXUS_LOCAL_LLM_URL").ok(),
            local_llm_model: std::env::var("NEXUS_LOCAL_LLM_MODEL").ok(),
            headless: false,
        }
    }

    pub fn micro() -> Self {
        Self {
            name: DeploymentProfile::Micro,
            max_concurrent_tasks: 1,
            max_task_memory_mb: 128,
            max_context_tokens: 2_048,
            max_vector_results: 5,
            allow_cloud_llm: false,
            local_llm_url: std::env::var("NEXUS_LOCAL_LLM_URL").ok()
                .or_else(|| Some("http://localhost:11434".into())),
            local_llm_model: std::env::var("NEXUS_LOCAL_LLM_MODEL").ok()
                .or_else(|| Some("llama3.2:1b".into())),
            headless: true,
        }
    }

    /// Load from environment.
    pub fn from_env() -> Self {
        match std::env::var("NEXUS_PROFILE").as_deref() {
            Ok("edge") => Self::edge(),
            Ok("micro") => Self::micro(),
            _ => Self::cloud(),
        }
    }

    /// Whether we should use the local LLM for this profile.
    pub fn use_local_llm(&self) -> bool {
        !self.allow_cloud_llm || self.local_llm_url.is_some()
    }
}

/// Global edge profile (set once at startup).
static PROFILE: std::sync::OnceLock<ResourceProfile> = std::sync::OnceLock::new();

pub fn init_profile(profile: ResourceProfile) {
    let name = format!("{:?}", profile.name);
    PROFILE.set(profile).ok();
    info!("Deployment profile: {name}");
}

pub fn current_profile() -> &'static ResourceProfile {
    PROFILE.get_or_init(ResourceProfile::from_env)
}

/// Build an OpenAI-compatible chat completion payload.
/// If the profile mandates local LLM, rewrites the model and base URL.
pub fn resolve_llm_endpoint(model: &str) -> (String, String) {
    let profile = current_profile();
    if profile.use_local_llm() {
        if let (Some(url), Some(local_model)) =
            (&profile.local_llm_url, &profile.local_llm_model)
        {
            let endpoint = format!("{}/v1/chat/completions", url.trim_end_matches('/'));
            return (endpoint, local_model.clone());
        }
    }
    (
        "https://api.openai.com/v1/chat/completions".into(),
        model.to_owned(),
    )
}

/// An adaptive LLM caller that respects the active resource profile.
///
/// Falls back to local LLM if the profile disallows cloud calls.
pub async fn call_llm(
    messages: serde_json::Value,
    preferred_model: &str,
    api_key: &str,
    http: &reqwest::Client,
) -> anyhow::Result<String> {
    let profile = current_profile();
    let (endpoint, model) = resolve_llm_endpoint(preferred_model);

    let max_tokens = profile.max_context_tokens.min(4096);

    let body = serde_json::json!({
        "model": model,
        "messages": messages,
        "max_tokens": max_tokens,
        "temperature": 0.7,
    });

    // For local LLM (Ollama), no auth header needed
    let req = if endpoint.contains("openai.com") {
        http.post(&endpoint).bearer_auth(api_key).json(&body)
    } else {
        http.post(&endpoint).json(&body)
    };

    let resp = req.send().await?.json::<serde_json::Value>().await?;

    if let Some(err) = resp.get("error") {
        crate::http_metrics::record_llm_error(
            "openai",
            &model,
            "default",
            crate::http_metrics::LlmErrorKind::Other,
        );
        anyhow::bail!("LLM error: {err}");
    }

    // ADR-005 §4: surface every LLM call through Prometheus, even ones
    // that bypass the unified client for legacy reasons. Closes audit
    // weakness §1.6 #18 for this site.
    let usage = resp.get("usage");
    let input_tokens = usage
        .and_then(|u| u.get("prompt_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let output_tokens = usage
        .and_then(|u| u.get("completion_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cost_usd = crate::cost_intelligence::estimate_cost_usd(&model, input_tokens, output_tokens);
    let cost_micros = crate::budget_brake::dollars_to_micros(cost_usd) as u64;
    crate::http_metrics::record_llm_call(
        "openai",
        &model,
        "default",
        cost_micros,
        input_tokens,
        output_tokens,
        0,
    );

    Ok(resp["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_owned())
}
