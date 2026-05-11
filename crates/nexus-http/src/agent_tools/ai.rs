//! AI tools — LLM subtasks, embeddings, classification, summarization, translation.
//!
//! These tools delegate to LLM APIs for various AI-powered operations.
//! They build on top of the existing LLM completion infrastructure
//! and provide specialized wrappers for common tasks.

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::agents::tools::registry::{Tool, ToolInput, ToolOutput};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn ok(result: Value) -> ToolOutput {
    ToolOutput {
        result,
        success: true,
        error: None,
    }
}

fn err(msg: impl Into<String>) -> ToolOutput {
    ToolOutput {
        result: json!({}),
        success: false,
        error: Some(msg.into()),
    }
}

/// Make an LLM chat completion request. Returns the response text.
async fn llm_chat(
    system: &str,
    prompt: &str,
    model: &str,
    max_tokens: u64,
    temperature: f64,
) -> Result<(String, u64), String> {
    let api_key = std::env::var("OPENAI_API_KEY")
        .or_else(|_| std::env::var("ANTHROPIC_API_KEY"))
        .map_err(|_| "No LLM API key configured. Set OPENAI_API_KEY or ANTHROPIC_API_KEY.".to_string())?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

    let (url, body, auth_header, auth_value) = if model.starts_with("claude") {
        let body = json!({
            "model": model,
            "max_tokens": max_tokens,
            "system": system,
            "messages": [{ "role": "user", "content": prompt }]
        });
        (
            "https://api.anthropic.com/v1/messages",
            body,
            "x-api-key",
            api_key.clone(),
        )
    } else {
        let body = json!({
            "model": model,
            "max_tokens": max_tokens,
            "temperature": temperature,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": prompt }
            ]
        });
        (
            "https://api.openai.com/v1/chat/completions",
            body,
            "Authorization",
            format!("Bearer {api_key}"),
        )
    };

    let mut req = client.post(url).json(&body).header(auth_header, &auth_value);
    if model.starts_with("claude") {
        req = req
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json");
    }

    let resp = req.send().await.map_err(|e| format!("LLM API request failed: {e}"))?;
    let status = resp.status().as_u16();
    let body_text = resp.text().await.unwrap_or_default();

    if status >= 400 {
        return Err(format!("LLM API returned HTTP {status}: {body_text}"));
    }

    let body_json: Value = serde_json::from_str(&body_text)
        .map_err(|_| "Failed to parse LLM response".to_string())?;

    let (text, input_tokens, output_tokens) = if model.starts_with("claude") {
        let text = body_json["content"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|b| b["text"].as_str())
            .unwrap_or("")
            .to_string();
        let input = body_json["usage"]["input_tokens"].as_u64().unwrap_or(0);
        let output = body_json["usage"]["output_tokens"].as_u64().unwrap_or(0);
        (text, input, output)
    } else {
        let text = body_json["choices"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|c| c["message"]["content"].as_str())
            .unwrap_or("")
            .to_string();
        let input = body_json["usage"]["prompt_tokens"].as_u64().unwrap_or(0);
        let output = body_json["usage"]["completion_tokens"].as_u64().unwrap_or(0);
        (text, input, output)
    };

    // ADR-005 §4: surface bypass agent-tool LLM calls in metrics so they're
    // visible on `/metrics` alongside unified-client traffic.
    let provider = if model.starts_with("claude") { "anthropic" } else { "openai" };
    let cost_usd = crate::cost_intelligence::estimate_cost_usd(model, input_tokens, output_tokens);
    let cost_micros = crate::budget_brake::dollars_to_micros(cost_usd) as u64;
    crate::http_metrics::record_llm_call(
        provider,
        model,
        "default",
        cost_micros,
        input_tokens,
        output_tokens,
        0,
    );

    Ok((text, output_tokens))
}

// ---------------------------------------------------------------------------
// LlmSubtaskTool
// ---------------------------------------------------------------------------

/// Delegate a sub-task to an LLM.
pub struct LlmSubtaskTool;

#[async_trait]
impl Tool for LlmSubtaskTool {
    fn name(&self) -> &str {
        "llm_subtask"
    }

    fn description(&self) -> &str {
        "Delegate a focused sub-task to an LLM: analysis, code review, brainstorming, etc. Returns the LLM response."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task": { "type": "string", "description": "Description of the sub-task" },
                "context": { "type": "string", "description": "Relevant context or data for the task" },
                "model": { "type": "string", "description": "LLM model to use (default: gpt-4o)" },
                "max_tokens": { "type": "integer", "description": "Max tokens in response (default: 4096)" },
                "temperature": { "type": "number", "description": "Sampling temperature (default: 0.7)" }
            },
            "required": ["task"]
        })
    }

    fn category(&self) -> &str {
        "llm_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let task = match input.parameters.get("task").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return err("Missing required parameter: task"),
        };
        let context = input.parameters.get("context").and_then(|v| v.as_str()).unwrap_or("");
        let model = input.parameters.get("model").and_then(|v| v.as_str()).unwrap_or("gpt-4o");
        let max_tokens = input.parameters.get("max_tokens").and_then(|v| v.as_u64()).unwrap_or(4096);
        let temperature = input.parameters.get("temperature").and_then(|v| v.as_f64()).unwrap_or(0.7);

        let prompt = if context.is_empty() {
            task.to_string()
        } else {
            format!("Task: {task}\n\nContext:\n{context}")
        };

        let system = "You are a focused AI assistant performing a specific sub-task. Be precise and thorough.";

        match llm_chat(system, &prompt, model, max_tokens, temperature).await {
            Ok((response, tokens)) => ok(json!({
                "response": response,
                "model": model,
                "tokens_used": tokens,
                "task": task
            })),
            Err(e) => err(e),
        }
    }
}

// ---------------------------------------------------------------------------
// EmbeddingTool
// ---------------------------------------------------------------------------

/// Generate text embeddings via the OpenAI embeddings API.
pub struct EmbeddingTool;

#[async_trait]
impl Tool for EmbeddingTool {
    fn name(&self) -> &str {
        "embedding"
    }

    fn description(&self) -> &str {
        "Generate vector embeddings for text using OpenAI's embedding API. Useful for semantic search and similarity."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "text": { "type": "string", "description": "Text to embed" },
                "model": { "type": "string", "description": "Embedding model (default: text-embedding-3-small)" }
            },
            "required": ["text"]
        })
    }

    fn category(&self) -> &str {
        "llm_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let text = match input.parameters.get("text").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return err("Missing required parameter: text"),
        };
        let model = input
            .parameters
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("text-embedding-3-small");

        let api_key = match std::env::var("OPENAI_API_KEY") {
            Ok(key) => key,
            Err(_) => return err("OPENAI_API_KEY not set. Cannot generate embeddings."),
        };

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
        {
            Ok(c) => c,
            Err(e) => return err(format!("Failed to create HTTP client: {e}")),
        };

        let body = json!({
            "model": model,
            "input": text
        });

        match client
            .post("https://api.openai.com/v1/embeddings")
            .header("Authorization", format!("Bearer {api_key}"))
            .json(&body)
            .send()
            .await
        {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let body_text = resp.text().await.unwrap_or_default();

                if status >= 400 {
                    return err(format!("Embeddings API returned HTTP {status}: {body_text}"));
                }

                let body_json: Value = match serde_json::from_str(&body_text) {
                    Ok(v) => v,
                    Err(_) => return err("Failed to parse embeddings response"),
                };

                let embedding = body_json["data"]
                    .as_array()
                    .and_then(|arr| arr.first())
                    .and_then(|item| item["embedding"].as_array())
                    .cloned()
                    .unwrap_or_default();

                let dimensions = embedding.len();
                let usage = body_json["usage"]["total_tokens"].as_u64().unwrap_or(0);

                // Embeddings priced as input tokens only.
                crate::http_metrics::record_llm_call(
                    "openai",
                    model,
                    "default",
                    0,
                    usage,
                    0,
                    0,
                );

                ok(json!({
                    "embedding": embedding,
                    "dimensions": dimensions,
                    "model": model,
                    "tokens_used": usage
                }))
            }
            Err(e) => {
                crate::http_metrics::record_llm_error(
                    "openai",
                    model,
                    "default",
                    crate::http_metrics::LlmErrorKind::Other,
                );
                err(format!("Embeddings API request failed: {e}"))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ClassifyTextTool
// ---------------------------------------------------------------------------

/// Classify text into categories using an LLM.
pub struct ClassifyTextTool;

#[async_trait]
impl Tool for ClassifyTextTool {
    fn name(&self) -> &str {
        "classify_text"
    }

    fn description(&self) -> &str {
        "Classify text into provided categories using an LLM. Returns the best matching category and confidence."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "text": { "type": "string", "description": "Text to classify" },
                "categories": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of possible categories"
                },
                "model": { "type": "string", "description": "LLM model (default: gpt-4o)" }
            },
            "required": ["text", "categories"]
        })
    }

    fn category(&self) -> &str {
        "llm_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let text = match input.parameters.get("text").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return err("Missing required parameter: text"),
        };
        let categories: Vec<String> = match input.parameters.get("categories").and_then(|v| v.as_array()) {
            Some(arr) => arr.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
            None => return err("Missing required parameter: categories"),
        };
        let model = input.parameters.get("model").and_then(|v| v.as_str()).unwrap_or("gpt-4o");

        if categories.is_empty() {
            return err("Categories array must not be empty");
        }

        let cats_str = categories.join(", ");
        let prompt = format!(
            "Classify the following text into exactly ONE of these categories: [{cats_str}]\n\n\
             Text: {text}\n\n\
             Respond with ONLY a JSON object like: {{\"category\": \"...\", \"confidence\": 0.95, \"reasoning\": \"brief reason\"}}"
        );
        let system = "You are a text classifier. Always respond with valid JSON only.";

        match llm_chat(system, &prompt, model, 256, 0.1).await {
            Ok((response, tokens)) => {
                // Try to parse the LLM response as JSON
                let parsed: Value = serde_json::from_str(response.trim())
                    .or_else(|_| {
                        // Try extracting JSON from markdown code block
                        let trimmed = response.trim();
                        let json_str = if trimmed.starts_with("```") {
                            trimmed
                                .lines()
                                .skip(1)
                                .take_while(|l| !l.starts_with("```"))
                                .collect::<Vec<_>>()
                                .join("\n")
                        } else {
                            trimmed.to_string()
                        };
                        serde_json::from_str(&json_str)
                    })
                    .unwrap_or(json!({
                        "category": response.trim(),
                        "confidence": 0.5,
                        "reasoning": "Could not parse structured response"
                    }));

                ok(json!({
                    "category": parsed["category"],
                    "confidence": parsed["confidence"],
                    "reasoning": parsed["reasoning"],
                    "model": model,
                    "tokens_used": tokens,
                    "available_categories": categories
                }))
            }
            Err(e) => err(e),
        }
    }
}

// ---------------------------------------------------------------------------
// SummarizeTool
// ---------------------------------------------------------------------------

/// Summarize long text using an LLM.
pub struct SummarizeTool;

#[async_trait]
impl Tool for SummarizeTool {
    fn name(&self) -> &str {
        "summarize"
    }

    fn description(&self) -> &str {
        "Summarize long text into a concise summary. Control length and style of the summary."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "text": { "type": "string", "description": "Text to summarize" },
                "max_length": { "type": "string", "description": "Target summary length: short (1-2 sentences), medium (1 paragraph), long (multiple paragraphs)", "enum": ["short", "medium", "long"] },
                "style": { "type": "string", "description": "Summary style: bullet_points, narrative, technical, executive", "enum": ["bullet_points", "narrative", "technical", "executive"] },
                "model": { "type": "string", "description": "LLM model (default: gpt-4o)" }
            },
            "required": ["text"]
        })
    }

    fn category(&self) -> &str {
        "llm_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let text = match input.parameters.get("text").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return err("Missing required parameter: text"),
        };
        let max_length = input.parameters.get("max_length").and_then(|v| v.as_str()).unwrap_or("medium");
        let style = input.parameters.get("style").and_then(|v| v.as_str()).unwrap_or("narrative");
        let model = input.parameters.get("model").and_then(|v| v.as_str()).unwrap_or("gpt-4o");

        let length_instruction = match max_length {
            "short" => "1-2 sentences",
            "long" => "3-5 paragraphs, detailed",
            _ => "1 concise paragraph",
        };

        let style_instruction = match style {
            "bullet_points" => "Use bullet points.",
            "technical" => "Use technical language, be precise.",
            "executive" => "Write an executive summary suitable for stakeholders.",
            _ => "Write in a natural narrative style.",
        };

        let prompt = format!(
            "Summarize the following text in {length_instruction}. {style_instruction}\n\n{text}"
        );

        let max_tokens = match max_length {
            "short" => 128u64,
            "long" => 2048,
            _ => 512,
        };

        let system = "You are a text summarizer. Produce clear, accurate summaries.";

        match llm_chat(system, &prompt, model, max_tokens, 0.3).await {
            Ok((summary, tokens)) => ok(json!({
                "summary": summary,
                "original_length": text.len(),
                "summary_length": summary.len(),
                "compression_ratio": if text.is_empty() { 0.0 } else { summary.len() as f64 / text.len() as f64 },
                "style": style,
                "max_length": max_length,
                "model": model,
                "tokens_used": tokens
            })),
            Err(e) => err(e),
        }
    }
}

// ---------------------------------------------------------------------------
// TranslateTool
// ---------------------------------------------------------------------------

/// Translate text between languages using an LLM.
pub struct TranslateTool;

#[async_trait]
impl Tool for TranslateTool {
    fn name(&self) -> &str {
        "translate"
    }

    fn description(&self) -> &str {
        "Translate text between languages using an LLM. Supports any language pair."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "text": { "type": "string", "description": "Text to translate" },
                "target_language": { "type": "string", "description": "Target language (e.g., 'Spanish', 'French', 'Japanese')" },
                "source_language": { "type": "string", "description": "Source language (optional, auto-detected if omitted)" },
                "tone": { "type": "string", "description": "Translation tone: formal, informal, technical (default: neutral)", "enum": ["formal", "informal", "technical", "neutral"] },
                "model": { "type": "string", "description": "LLM model (default: gpt-4o)" }
            },
            "required": ["text", "target_language"]
        })
    }

    fn category(&self) -> &str {
        "llm_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let text = match input.parameters.get("text").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return err("Missing required parameter: text"),
        };
        let target = match input.parameters.get("target_language").and_then(|v| v.as_str()) {
            Some(l) => l,
            None => return err("Missing required parameter: target_language"),
        };
        let source = input.parameters.get("source_language").and_then(|v| v.as_str());
        let tone = input.parameters.get("tone").and_then(|v| v.as_str()).unwrap_or("neutral");
        let model = input.parameters.get("model").and_then(|v| v.as_str()).unwrap_or("gpt-4o");

        let source_instruction = match source {
            Some(lang) => format!("from {lang} "),
            None => String::new(),
        };

        let tone_instruction = match tone {
            "formal" => " Use formal register.",
            "informal" => " Use informal/casual register.",
            "technical" => " Preserve technical terminology.",
            _ => "",
        };

        let prompt = format!(
            "Translate the following text {source_instruction}to {target}.{tone_instruction}\n\
             Return ONLY the translation, nothing else.\n\n{text}"
        );

        let max_tokens = (text.len() as u64 * 3).clamp(256, 8192);
        let system = "You are a professional translator. Return only the translated text.";

        match llm_chat(system, &prompt, model, max_tokens, 0.3).await {
            Ok((translation, tokens)) => ok(json!({
                "translation": translation,
                "source_language": source.unwrap_or("auto-detected"),
                "target_language": target,
                "tone": tone,
                "model": model,
                "tokens_used": tokens
            })),
            Err(e) => err(e),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llm_subtask_tool_metadata() {
        let tool = LlmSubtaskTool;
        assert_eq!(tool.name(), "llm_subtask");
        assert_eq!(tool.category(), "llm_ops");
    }

    #[test]
    fn embedding_tool_metadata() {
        let tool = EmbeddingTool;
        assert_eq!(tool.name(), "embedding");
        assert_eq!(tool.category(), "llm_ops");
    }

    #[test]
    fn classify_text_tool_metadata() {
        let tool = ClassifyTextTool;
        assert_eq!(tool.name(), "classify_text");
        assert_eq!(tool.category(), "llm_ops");
        let schema = tool.schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("text")));
        assert!(required.contains(&json!("categories")));
    }

    #[test]
    fn summarize_tool_metadata() {
        let tool = SummarizeTool;
        assert_eq!(tool.name(), "summarize");
        assert_eq!(tool.category(), "llm_ops");
    }

    #[test]
    fn translate_tool_metadata() {
        let tool = TranslateTool;
        assert_eq!(tool.name(), "translate");
        assert_eq!(tool.category(), "llm_ops");
        let schema = tool.schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("text")));
        assert!(required.contains(&json!("target_language")));
    }

    #[tokio::test]
    async fn llm_subtask_missing_task() {
        let tool = LlmSubtaskTool;
        let out = tool
            .execute(ToolInput {
                parameters: json!({}),
            })
            .await;
        assert!(!out.success);
        assert!(out.error.unwrap().contains("task"));
    }

    #[tokio::test]
    async fn classify_empty_categories() {
        let tool = ClassifyTextTool;
        let out = tool
            .execute(ToolInput {
                parameters: json!({
                    "text": "hello",
                    "categories": []
                }),
            })
            .await;
        assert!(!out.success);
        assert!(out.error.unwrap().contains("empty"));
    }

    #[tokio::test]
    async fn embedding_no_api_key() {
        let tool = EmbeddingTool;
        let out = tool
            .execute(ToolInput {
                parameters: json!({ "text": "hello" }),
            })
            .await;
        // Either succeeds (key set) or fails gracefully
        if !out.success {
            let err = out.error.unwrap();
            assert!(err.contains("API") || err.contains("key"));
        }
    }
}
