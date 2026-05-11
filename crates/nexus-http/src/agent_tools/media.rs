//! Media tools — image generation, placeholders, and SVG icons.
//!
//! Provides tools for generating visual assets. The `ImageGenerateTool`
//! delegates to an external API (OpenAI DALL-E), while the placeholder
//! and icon tools work locally.

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

// ---------------------------------------------------------------------------
// ImageGenerateTool
// ---------------------------------------------------------------------------

/// Generate an image via an external API (OpenAI DALL-E).
pub struct ImageGenerateTool;

#[async_trait]
impl Tool for ImageGenerateTool {
    fn name(&self) -> &str {
        "image_generate"
    }

    fn description(&self) -> &str {
        "Generate an image from a text prompt using DALL-E API. Requires OPENAI_API_KEY."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "prompt": { "type": "string", "description": "Text description of the image to generate" },
                "size": { "type": "string", "description": "Image size: 256x256, 512x512, or 1024x1024 (default: 1024x1024)", "enum": ["256x256", "512x512", "1024x1024"] },
                "model": { "type": "string", "description": "Model to use (default: dall-e-3)" },
                "quality": { "type": "string", "description": "Image quality: standard or hd (default: standard)", "enum": ["standard", "hd"] }
            },
            "required": ["prompt"]
        })
    }

    fn category(&self) -> &str {
        "external_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let prompt = match input.parameters.get("prompt").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return err("Missing required parameter: prompt"),
        };
        let size = input
            .parameters
            .get("size")
            .and_then(|v| v.as_str())
            .unwrap_or("1024x1024");
        let model = input
            .parameters
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("dall-e-3");
        let quality = input
            .parameters
            .get("quality")
            .and_then(|v| v.as_str())
            .unwrap_or("standard");

        let api_key = match std::env::var("OPENAI_API_KEY") {
            Ok(key) => key,
            Err(_) => return err("OPENAI_API_KEY not set. Cannot generate images."),
        };

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
        {
            Ok(c) => c,
            Err(e) => return err(format!("Failed to create HTTP client: {e}")),
        };

        let body = json!({
            "model": model,
            "prompt": prompt,
            "n": 1,
            "size": size,
            "quality": quality,
            "response_format": "url"
        });

        match client
            .post("https://api.openai.com/v1/images/generations")
            .header("Authorization", format!("Bearer {api_key}"))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
        {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let body_text = resp.text().await.unwrap_or_default();

                if status >= 400 {
                    return err(format!("DALL-E API returned HTTP {status}: {body_text}"));
                }

                let body_json: Value = match serde_json::from_str(&body_text) {
                    Ok(v) => v,
                    Err(_) => return err("Failed to parse DALL-E response"),
                };

                let image_url = body_json["data"]
                    .as_array()
                    .and_then(|arr| arr.first())
                    .and_then(|item| item["url"].as_str())
                    .unwrap_or("")
                    .to_string();

                let revised_prompt = body_json["data"]
                    .as_array()
                    .and_then(|arr| arr.first())
                    .and_then(|item| item["revised_prompt"].as_str())
                    .unwrap_or("")
                    .to_string();

                // Image generation: surface as a calls counter; cost in
                // micros is left at 0 since DALL-E is priced per image, not
                // per token.
                crate::http_metrics::record_llm_call(
                    "openai",
                    model,
                    "default",
                    0,
                    0,
                    0,
                    0,
                );

                ok(json!({
                    "url": image_url,
                    "prompt": prompt,
                    "revised_prompt": revised_prompt,
                    "size": size,
                    "model": model
                }))
            }
            Err(e) => err(format!("DALL-E API request failed: {e}")),
        }
    }
}

// ---------------------------------------------------------------------------
// PlaceholderImageTool
// ---------------------------------------------------------------------------

/// Generate a colored SVG placeholder image.
pub struct PlaceholderImageTool;

#[async_trait]
impl Tool for PlaceholderImageTool {
    fn name(&self) -> &str {
        "placeholder_image"
    }

    fn description(&self) -> &str {
        "Generate a colored SVG placeholder image with optional text. Returns SVG markup."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "width": { "type": "integer", "description": "Width in pixels (default: 400)" },
                "height": { "type": "integer", "description": "Height in pixels (default: 300)" },
                "background_color": { "type": "string", "description": "Background color (hex or named, default: #e0e0e0)" },
                "text_color": { "type": "string", "description": "Text color (hex or named, default: #666666)" },
                "text": { "type": "string", "description": "Text to display (default: WxH dimensions)" },
                "output_path": { "type": "string", "description": "Path to save the SVG file (optional)" }
            }
        })
    }

    fn category(&self) -> &str {
        "data_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let width = input.parameters.get("width").and_then(|v| v.as_u64()).unwrap_or(400);
        let height = input.parameters.get("height").and_then(|v| v.as_u64()).unwrap_or(300);
        let bg_color = input.parameters.get("background_color").and_then(|v| v.as_str()).unwrap_or("#e0e0e0");
        let text_color = input.parameters.get("text_color").and_then(|v| v.as_str()).unwrap_or("#666666");
        let default_text = format!("{width}x{height}");
        let text = input.parameters.get("text").and_then(|v| v.as_str()).unwrap_or(&default_text);
        let output_path = input.parameters.get("output_path").and_then(|v| v.as_str());

        let font_size = (width.min(height) / 8).clamp(12, 72);

        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">
  <rect width="100%" height="100%" fill="{bg_color}"/>
  <text x="50%" y="50%" dominant-baseline="middle" text-anchor="middle" font-family="Arial, sans-serif" font-size="{font_size}" fill="{text_color}">{text}</text>
</svg>"#
        );

        if let Some(path) = output_path {
            if let Err(e) = std::fs::write(path, &svg) {
                return err(format!("Failed to write SVG to {path}: {e}"));
            }
        }

        ok(json!({
            "svg": svg,
            "width": width,
            "height": height,
            "output_path": output_path
        }))
    }
}

// ---------------------------------------------------------------------------
// SvgIconTool
// ---------------------------------------------------------------------------

/// Generate a simple SVG icon from a description using an LLM.
pub struct SvgIconTool;

#[async_trait]
impl Tool for SvgIconTool {
    fn name(&self) -> &str {
        "svg_icon"
    }

    fn description(&self) -> &str {
        "Generate a simple SVG icon from a text description using LLM. Returns SVG markup."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "description": { "type": "string", "description": "Description of the icon to generate (e.g., 'a gear/settings icon')" },
                "size": { "type": "integer", "description": "Icon size in pixels (default: 24)" },
                "color": { "type": "string", "description": "Icon color (default: currentColor)" },
                "style": { "type": "string", "description": "Icon style: outline, filled, or duotone (default: outline)", "enum": ["outline", "filled", "duotone"] }
            },
            "required": ["description"]
        })
    }

    fn category(&self) -> &str {
        "external_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let description = match input.parameters.get("description").and_then(|v| v.as_str()) {
            Some(d) => d,
            None => return err("Missing required parameter: description"),
        };
        let size = input.parameters.get("size").and_then(|v| v.as_u64()).unwrap_or(24);
        let color = input.parameters.get("color").and_then(|v| v.as_str()).unwrap_or("currentColor");
        let style = input.parameters.get("style").and_then(|v| v.as_str()).unwrap_or("outline");

        let api_key = std::env::var("OPENAI_API_KEY").or_else(|_| std::env::var("ANTHROPIC_API_KEY"));

        let api_key = match api_key {
            Ok(key) => key,
            Err(_) => {
                // Fall back to a basic SVG circle as placeholder
                let svg = format!(
                    r#"<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" viewBox="0 0 {size} {size}">
  <circle cx="{half}" cy="{half}" r="{radius}" fill="none" stroke="{color}" stroke-width="2"/>
</svg>"#,
                    half = size / 2,
                    radius = size / 2 - 2
                );
                return ok(json!({
                    "svg": svg,
                    "description": description,
                    "size": size,
                    "note": "No API key available. Returned placeholder circle."
                }));
            }
        };

        let prompt = format!(
            "Generate a minimal SVG icon for: {description}\n\
             Requirements:\n\
             - Size: {size}x{size}px with viewBox=\"0 0 {size} {size}\"\n\
             - Style: {style}\n\
             - Color: {color}\n\
             - Clean, simple paths only\n\
             - No text elements\n\
             - Must be valid SVG\n\
             Return ONLY the SVG markup, nothing else."
        );

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
        {
            Ok(c) => c,
            Err(e) => return err(format!("Failed to create HTTP client: {e}")),
        };

        let body = json!({
            "model": "gpt-4o",
            "messages": [
                { "role": "system", "content": "You are an SVG icon generator. Return ONLY valid SVG markup, no explanation." },
                { "role": "user", "content": prompt }
            ],
            "max_tokens": 1024,
            "temperature": 0.3
        });

        match client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {api_key}"))
            .json(&body)
            .send()
            .await
        {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let body_text = resp.text().await.unwrap_or_default();

                if status >= 400 {
                    return err(format!("LLM API returned HTTP {status}"));
                }

                let body_json: Value = match serde_json::from_str(&body_text) {
                    Ok(v) => v,
                    Err(_) => return err("Failed to parse LLM response"),
                };

                let svg_text = body_json["choices"]
                    .as_array()
                    .and_then(|arr| arr.first())
                    .and_then(|choice| choice["message"]["content"].as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();

                let input_tokens = body_json["usage"]["prompt_tokens"].as_u64().unwrap_or(0);
                let output_tokens =
                    body_json["usage"]["completion_tokens"].as_u64().unwrap_or(0);
                let cost_usd = crate::cost_intelligence::estimate_cost_usd(
                    "gpt-4o",
                    input_tokens,
                    output_tokens,
                );
                let cost_micros = crate::budget_brake::dollars_to_micros(cost_usd) as u64;
                crate::http_metrics::record_llm_call(
                    "openai",
                    "gpt-4o",
                    "default",
                    cost_micros,
                    input_tokens,
                    output_tokens,
                    0,
                );

                // Extract just the SVG part if wrapped in markdown
                let svg = if svg_text.contains("<svg") {
                    let start = svg_text.find("<svg").unwrap_or(0);
                    let end = svg_text.find("</svg>").map(|e| e + 6).unwrap_or(svg_text.len());
                    svg_text[start..end].to_string()
                } else {
                    svg_text
                };

                ok(json!({
                    "svg": svg,
                    "description": description,
                    "size": size,
                    "style": style
                }))
            }
            Err(e) => {
                crate::http_metrics::record_llm_error(
                    "openai",
                    "gpt-4o",
                    "default",
                    crate::http_metrics::LlmErrorKind::Other,
                );
                err(format!("LLM API request failed: {e}"))
            }
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
    fn image_generate_tool_metadata() {
        let tool = ImageGenerateTool;
        assert_eq!(tool.name(), "image_generate");
        assert_eq!(tool.category(), "external_ops");
    }

    #[test]
    fn placeholder_image_tool_metadata() {
        let tool = PlaceholderImageTool;
        assert_eq!(tool.name(), "placeholder_image");
        assert_eq!(tool.category(), "data_ops");
    }

    #[test]
    fn svg_icon_tool_metadata() {
        let tool = SvgIconTool;
        assert_eq!(tool.name(), "svg_icon");
        assert_eq!(tool.category(), "external_ops");
    }

    #[tokio::test]
    async fn placeholder_image_default() {
        let tool = PlaceholderImageTool;
        let out = tool
            .execute(ToolInput {
                parameters: json!({}),
            })
            .await;
        assert!(out.success);
        let svg = out.result["svg"].as_str().unwrap();
        assert!(svg.contains("<svg"));
        assert!(svg.contains("400"));
        assert!(svg.contains("300"));
    }

    #[tokio::test]
    async fn placeholder_image_custom() {
        let tool = PlaceholderImageTool;
        let out = tool
            .execute(ToolInput {
                parameters: json!({
                    "width": 200,
                    "height": 100,
                    "background_color": "#ff0000",
                    "text": "Logo"
                }),
            })
            .await;
        assert!(out.success);
        let svg = out.result["svg"].as_str().unwrap();
        assert!(svg.contains("200"));
        assert!(svg.contains("100"));
        assert!(svg.contains("#ff0000"));
        assert!(svg.contains("Logo"));
    }

    #[tokio::test]
    async fn svg_icon_no_api_key_returns_placeholder() {
        // This test works if no API key is set (common in CI)
        let tool = SvgIconTool;
        let out = tool
            .execute(ToolInput {
                parameters: json!({ "description": "a star" }),
            })
            .await;
        // Should succeed either way (API or fallback)
        assert!(out.success);
    }
}
