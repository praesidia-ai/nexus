//! HTTP handlers for multi-modal agent support.
//!
//! SECURITY: every handler in this module spends `app.openai_api_key` (the
//! platform-wide key configured at runtime) on user-supplied input. Without
//! an authenticated caller and a concurrency guard, any authenticated user
//! could drain the platform's OpenAI budget. The shared `enter_multimodal`
//! helper enforces:
//!   1. `AuthContext` extraction (anonymous traffic is rejected by the
//!      global auth middleware, but the explicit extractor surfaces the
//!      contract and gives us the calling tenant for logging).
//!   2. LLM concurrency slot acquisition via `app.rate_limiter` so every
//!      multimodal call counts against the global LLM concurrency budget.
//!
//! Per-tenant rate limiting is the next step (see SEC-017 / TenantRateLimiter).

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use tracing::info;

use crate::multimodal::{MultiModalRequest, MultiModalResponse};
use crate::rate_limiter::ConcurrencyGuard;
use crate::security::auth::AuthContext;
use crate::state::AppState;

/// Common entry guard for every multimodal handler.
///
/// Logs the call (operation + tenant), enforces the per-tenant generation
/// quota, and acquires an LLM concurrency slot. The returned slot guard MUST
/// be held until the HTTP response is constructed.
async fn enter_multimodal(
    app: &Arc<AppState>,
    auth: &AuthContext,
    op: &str,
) -> Result<ConcurrencyGuard, (StatusCode, Json<serde_json::Value>)> {
    info!(
        tenant_id = %auth.tenant_id,
        user_id = %auth.user_id,
        op = op,
        "multimodal request",
    );
    if let Err(reason) = app.tenant_rate_limiter.check(&auth.tenant_id, "generation") {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({ "error": reason })),
        ));
    }
    app.rate_limiter.acquire_llm_slot().await.map_err(|e| {
        (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({ "error": e })),
        )
    })
}

/// `POST /multimodal/analyze` — process text, images, audio, or documents with a multi-modal LLM.
///
/// Accepts a JSON body with `MultiModalRequest` and returns a text response.
/// Requires authentication; counts against the global LLM concurrency slot.
pub async fn analyze(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(req): Json<MultiModalRequest>,
) -> Result<Json<MultiModalResponse>, (StatusCode, Json<serde_json::Value>)> {
    let _slot = enter_multimodal(&app, &auth, "analyze").await?;
    match crate::multimodal::process(req, &app.openai_api_key, &app.http_client).await {
        Ok(resp) => Ok(Json(resp)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )),
    }
}

/// `POST /multimodal/describe-image` — convenience endpoint for single-image description.
/// Requires authentication; counts against the global LLM concurrency slot.
pub async fn describe_image(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _slot = enter_multimodal(&app, &auth, "describe_image").await?;

    let url = body.get("url").and_then(|v| v.as_str());
    let b64 = body.get("data_base64").and_then(|v| v.as_str());
    let prompt = body
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("Describe this image in detail.");

    if url.is_none() && b64.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Provide either `url` or `data_base64`" })),
        ));
    }

    // SSRF guard: a user-supplied image URL would otherwise let the OpenAI
    // request fetch internal/metadata addresses on our behalf when the model
    // chooses to dereference it.
    if let Some(u) = url {
        if let Err(reason) = crate::security::url_guard::is_public_url(u) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "image URL rejected", "reason": reason })),
            ));
        }
    }

    let part = if let Some(url) = url {
        crate::multimodal::MultiModalPart {
            modality: crate::multimodal::Modality::Image,
            text: None,
            data_base64: None,
            url: Some(url.to_owned()),
            mime_type: None,
        }
    } else {
        crate::multimodal::MultiModalPart {
            modality: crate::multimodal::Modality::Image,
            text: None,
            data_base64: b64.map(|s| s.to_owned()),
            url: None,
            mime_type: body
                .get("mime_type")
                .and_then(|v| v.as_str())
                .map(|s| s.to_owned()),
        }
    };

    let req = MultiModalRequest {
        system_prompt: None,
        parts: vec![
            part,
            crate::multimodal::MultiModalPart {
                modality: crate::multimodal::Modality::Text,
                text: Some(prompt.to_owned()),
                data_base64: None,
                url: None,
                mime_type: None,
            },
        ],
        model: None,
    };

    match crate::multimodal::process(req, &app.openai_api_key, &app.http_client).await {
        Ok(resp) => Ok(Json(serde_json::json!({ "description": resp.text }))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )),
    }
}

/// `POST /multimodal/transcribe` — transcribe audio to text.
/// Requires authentication; counts against the global LLM concurrency slot.
pub async fn transcribe(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _slot = enter_multimodal(&app, &auth, "transcribe").await?;

    let b64 = body
        .get("data_base64")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "`data_base64` is required" })),
            )
        })?;

    let mime = body
        .get("mime_type")
        .and_then(|v| v.as_str())
        .unwrap_or("audio/mp4");

    let req = MultiModalRequest {
        system_prompt: Some("Transcribe the audio accurately.".into()),
        parts: vec![crate::multimodal::MultiModalPart {
            modality: crate::multimodal::Modality::Audio,
            text: None,
            data_base64: Some(b64.to_owned()),
            url: None,
            mime_type: Some(mime.to_owned()),
        }],
        model: None,
    };

    match crate::multimodal::process(req, &app.openai_api_key, &app.http_client).await {
        Ok(resp) => Ok(Json(serde_json::json!({ "transcript": resp.text }))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )),
    }
}

/// `POST /multimodal/tts` — text-to-speech via OpenAI TTS API.
///
/// Accepts `{ "text": "...", "voice": "alloy", "model": "tts-1" }`.
/// Returns `{ "audio_base64": "...", "mime_type": "audio/mp3" }`.
/// Requires authentication; counts against the global LLM concurrency slot.
pub async fn text_to_speech(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _slot = enter_multimodal(&app, &auth, "tts").await?;

    let text = body.get("text").and_then(|v| v.as_str()).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "`text` is required" })),
        )
    })?;

    let voice = body
        .get("voice")
        .and_then(|v| v.as_str())
        .unwrap_or("alloy");
    let model = body
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("tts-1");

    let resp = app
        .http_client
        .post("https://api.openai.com/v1/audio/speech")
        .header("Authorization", format!("Bearer {}", app.openai_api_key))
        .json(&serde_json::json!({
            "model": model,
            "input": text,
            "voice": voice,
            "response_format": "mp3",
        }))
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": format!("TTS request failed: {e}") })),
            )
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": format!("TTS API error ({status}): {body_text}") })),
        ));
    }

    let bytes = resp.bytes().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("Failed to read TTS response: {e}") })),
        )
    })?;

    use base64::Engine as _;
    let audio_b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

    Ok(Json(serde_json::json!({
        "audio_base64": audio_b64,
        "mime_type": "audio/mp3",
        "voice": voice,
        "model": model,
        "text_length": text.len(),
    })))
}

/// `POST /multimodal/voice-chat` — full voice pipeline: STT -> LLM -> TTS.
///
/// Accepts audio input, transcribes, runs through LLM, and returns both text
/// and audio responses.
/// Requires authentication; counts against the global LLM concurrency slot
/// (note: this handler issues THREE provider calls — STT + chat + TTS — so
/// holding a single slot for the duration of the request is intentional).
pub async fn voice_chat(
    State(app): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _slot = enter_multimodal(&app, &auth, "voice_chat").await?;

    let audio_b64 = body
        .get("audio_base64")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "`audio_base64` is required" })),
            )
        })?;
    let mime = body
        .get("mime_type")
        .and_then(|v| v.as_str())
        .unwrap_or("audio/mp4");
    let system_prompt = body
        .get("system_prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("You are a helpful voice assistant. Keep responses concise and conversational.");
    let voice = body
        .get("voice")
        .and_then(|v| v.as_str())
        .unwrap_or("alloy");
    let conversation: Vec<serde_json::Value> = body
        .get("messages")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    // Step 1: STT — transcribe the audio
    let stt_req = MultiModalRequest {
        system_prompt: Some("Transcribe the audio accurately.".into()),
        parts: vec![crate::multimodal::MultiModalPart {
            modality: crate::multimodal::Modality::Audio,
            text: None,
            data_base64: Some(audio_b64.to_owned()),
            url: None,
            mime_type: Some(mime.to_owned()),
        }],
        model: None,
    };

    let transcript = crate::multimodal::process(stt_req, &app.openai_api_key, &app.http_client)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("STT failed: {e}") })),
            )
        })?
        .text;

    // Step 2: LLM — generate a response
    let mut messages = vec![serde_json::json!({ "role": "system", "content": system_prompt })];
    messages.extend(conversation);
    messages.push(serde_json::json!({ "role": "user", "content": &transcript }));

    let llm_resp = app
        .http_client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", app.openai_api_key))
        .json(&serde_json::json!({
            "model": "gpt-4.1",
            "messages": messages,
            "max_tokens": 300,
        }))
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": format!("LLM request failed: {e}") })),
            )
        })?;

    let llm_body: serde_json::Value = llm_resp.json().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("Failed to parse LLM response: {e}") })),
        )
    })?;

    let assistant_text = llm_body["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("I couldn't generate a response.")
        .to_owned();

    // Step 3: TTS — convert response to speech
    let tts_resp = app
        .http_client
        .post("https://api.openai.com/v1/audio/speech")
        .header("Authorization", format!("Bearer {}", app.openai_api_key))
        .json(&serde_json::json!({
            "model": "tts-1",
            "input": &assistant_text,
            "voice": voice,
            "response_format": "mp3",
        }))
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": format!("TTS failed: {e}") })),
            )
        })?;

    let tts_bytes = tts_resp.bytes().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("Failed to read TTS audio: {e}") })),
        )
    })?;

    use base64::Engine as _;
    let response_audio_b64 = base64::engine::general_purpose::STANDARD.encode(&tts_bytes);

    Ok(Json(serde_json::json!({
        "transcript": transcript,
        "response_text": assistant_text,
        "response_audio_base64": response_audio_b64,
        "response_mime_type": "audio/mp3",
        "voice": voice,
    })))
}
