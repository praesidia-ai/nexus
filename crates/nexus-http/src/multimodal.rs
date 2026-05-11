//! Multi-modal agent support — vision, audio transcription, and file analysis.
//!
//! Routes content through the appropriate OpenAI model:
//!  - Images → `gpt-4o` vision (base64 or URL)
//!  - Audio  → `whisper-1` for transcription, then optional text agent
//!  - Documents (PDF/text) → chunked text extraction → LLM analysis
//!  - Video frames → sampled images → vision pipeline

use base64::Engine;
use serde::{Deserialize, Serialize};
use tracing::info;

/// The modality of an input part.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Modality {
    Text,
    Image,
    Audio,
    Document,
    VideoFrames,
}

/// A multi-modal message part that can be text, image, audio, or a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiModalPart {
    pub modality: Modality,
    /// For text: the content string.
    pub text: Option<String>,
    /// For images/audio/documents: base64-encoded bytes.
    pub data_base64: Option<String>,
    /// For images: optional public URL (alternative to base64).
    pub url: Option<String>,
    /// MIME type (e.g. "image/png", "audio/mp4", "application/pdf").
    pub mime_type: Option<String>,
}

/// A multi-modal agent request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiModalRequest {
    pub system_prompt: Option<String>,
    pub parts: Vec<MultiModalPart>,
    pub model: Option<String>,
}

/// Response from the multi-modal pipeline.
#[derive(Debug, Serialize)]
pub struct MultiModalResponse {
    pub text: String,
    pub modalities_processed: Vec<Modality>,
    pub tokens_used: u32,
}

/// Process a multi-modal request by routing each part through the correct pipeline.
pub async fn process(
    req: MultiModalRequest,
    openai_api_key: &str,
    http: &reqwest::Client,
) -> anyhow::Result<MultiModalResponse> {
    let model = req.model.as_deref().unwrap_or("gpt-4.1");
    let system = req.system_prompt.as_deref().unwrap_or("You are a helpful AI assistant. Analyse all provided content and respond helpfully.");

    // Build OpenAI messages content
    let mut content: Vec<serde_json::Value> = Vec::new();
    let mut processed: Vec<Modality> = Vec::new();

    for part in &req.parts {
        match &part.modality {
            Modality::Text => {
                if let Some(text) = &part.text {
                    content.push(serde_json::json!({ "type": "text", "text": text }));
                    processed.push(Modality::Text);
                }
            }

            Modality::Image => {
                if let Some(url) = &part.url {
                    content.push(serde_json::json!({
                        "type": "image_url",
                        "image_url": { "url": url, "detail": "auto" },
                    }));
                    processed.push(Modality::Image);
                } else if let Some(b64) = &part.data_base64 {
                    let mime = part.mime_type.as_deref().unwrap_or("image/png");
                    let data_url = format!("data:{mime};base64,{b64}");
                    content.push(serde_json::json!({
                        "type": "image_url",
                        "image_url": { "url": data_url, "detail": "auto" },
                    }));
                    processed.push(Modality::Image);
                }
            }

            Modality::Audio => {
                // Transcribe first with Whisper, then pass text to the LLM
                if let Some(b64) = &part.data_base64 {
                    let mime = part.mime_type.as_deref().unwrap_or("audio/mp4");
                    let transcript = transcribe_audio(b64, mime, openai_api_key, http).await?;
                    info!("Audio transcribed ({} chars)", transcript.len());
                    content.push(serde_json::json!({
                        "type": "text",
                        "text": format!("[Audio transcript]\n{transcript}"),
                    }));
                    processed.push(Modality::Audio);
                }
            }

            Modality::Document => {
                // Extract text from the document and pass it to the LLM
                if let Some(b64) = &part.data_base64 {
                    let mime = part.mime_type.as_deref().unwrap_or("text/plain");
                    let text = extract_document_text(b64, mime)?;
                    content.push(serde_json::json!({
                        "type": "text",
                        "text": format!("[Document content]\n{text}"),
                    }));
                    processed.push(Modality::Document);
                }
            }

            Modality::VideoFrames => {
                // Treat video frames as a sequence of images
                if let Some(b64_list) = part.text.as_deref() {
                    for frame_b64 in b64_list.split(',').filter(|s| !s.is_empty()) {
                        let data_url = format!("data:image/jpeg;base64,{frame_b64}");
                        content.push(serde_json::json!({
                            "type": "image_url",
                            "image_url": { "url": data_url, "detail": "low" },
                        }));
                    }
                    processed.push(Modality::VideoFrames);
                }
            }
        }
    }

    if content.is_empty() {
        anyhow::bail!("No processable content in multi-modal request");
    }

    // Call OpenAI
    let body = serde_json::json!({
        "model": model,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": content },
        ],
        "max_tokens": 4096,
    });

    let resp = http
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(openai_api_key)
        .json(&body)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    let text = resp["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_owned();

    let prompt_tokens = resp["usage"]["prompt_tokens"].as_u64().unwrap_or(0);
    let completion_tokens = resp["usage"]["completion_tokens"].as_u64().unwrap_or(0);
    let tokens_used = (prompt_tokens + completion_tokens) as u32;

    // ADR-005 §4: surface multimodal calls through the same metric families
    // as text-only calls so dashboards stay consistent.
    let cost_usd = crate::cost_intelligence::estimate_cost_usd(model, prompt_tokens, completion_tokens);
    let cost_micros = crate::budget_brake::dollars_to_micros(cost_usd) as u64;
    crate::http_metrics::record_llm_call(
        "openai",
        model,
        "default",
        cost_micros,
        prompt_tokens,
        completion_tokens,
        0,
    );

    Ok(MultiModalResponse { text, modalities_processed: processed, tokens_used })
}

/// Transcribe audio using the Whisper API.
async fn transcribe_audio(
    audio_b64: &str,
    mime_type: &str,
    api_key: &str,
    http: &reqwest::Client,
) -> anyhow::Result<String> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(audio_b64)?;
    let ext = mime_to_ext(mime_type);
    let filename = format!("audio.{ext}");
    let part = reqwest::multipart::Part::bytes(bytes)
        .file_name(filename)
        .mime_str(mime_type)?;
    let form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("model", "whisper-1");

    let resp = http
        .post("https://api.openai.com/v1/audio/transcriptions")
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    // Whisper-1 priced per audio second, not per token. We log a calls-only
    // bump so the metric families still show transcription activity; cost
    // attribution in micros is left at 0 because we don't have audio
    // duration here without re-decoding the file.
    crate::http_metrics::record_llm_call(
        "openai",
        "whisper-1",
        "default",
        0,
        0,
        0,
        0,
    );

    Ok(resp["text"].as_str().unwrap_or("").to_owned())
}

/// Extract text from a document (PDF or plain text).
fn extract_document_text(b64: &str, mime_type: &str) -> anyhow::Result<String> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64)?;
    match mime_type {
        "text/plain" | "text/markdown" | "text/csv" => {
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        }
        "application/json" => {
            let val: serde_json::Value = serde_json::from_slice(&bytes)?;
            Ok(serde_json::to_string_pretty(&val)?)
        }
        _ => {
            // For other formats (e.g. PDF), return a notice; a full PDF parser
            // would be added as an optional feature.
            Ok(format!(
                "[Binary document: {} bytes, type {}. Full extraction requires an optional PDF parser.]",
                bytes.len(),
                mime_type
            ))
        }
    }
}

fn mime_to_ext(mime: &str) -> &'static str {
    match mime {
        "audio/mpeg" | "audio/mp3" => "mp3",
        "audio/mp4" | "audio/m4a" => "m4a",
        "audio/webm" => "webm",
        "audio/wav" => "wav",
        "audio/ogg" => "ogg",
        _ => "mp3",
    }
}
