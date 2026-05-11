//! Web tools — HTTP requests and URL content fetching.
//!
//! Uses `reqwest` for all HTTP operations. The [`HttpRequestTool`] handles
//! arbitrary HTTP methods/headers, while [`WebFetchTool`] is a simplified
//! GET-and-extract-text helper.
//!
//! Every outbound request is screened by [`validate_egress_url`] to block
//! loopback, link-local, cloud-metadata, and private-range targets — an
//! LLM agent that a user tricks into calling `http://169.254.169.254/…`
//! must not be able to exfiltrate instance credentials.

use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::agents::tools::registry::{Tool, ToolInput, ToolOutput};

/// Reject URLs that point at loopback, link-local, private, unspecified, or
/// cloud-metadata endpoints. Also restricts schemes to http/https.
///
/// Note: this only checks the literal host the caller supplied. A fully
/// robust SSRF defense also resolves DNS and checks every returned IP, but
/// that requires hooking `reqwest`'s resolver. Host-literal filtering blocks
/// the common prompt-injection cases (`127.0.0.1`, `169.254.169.254`,
/// `metadata.google.internal`, `::1`, `10.*`, `192.168.*`).
pub fn validate_egress_url(url_str: &str) -> Result<(), String> {
    let parsed = url::Url::parse(url_str).map_err(|e| format!("invalid URL: {e}"))?;

    match parsed.scheme() {
        "http" | "https" => {}
        other => return Err(format!("scheme '{other}' not allowed")),
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| "URL has no host".to_string())?
        .to_ascii_lowercase();

    // Explicit name-level blocks for common metadata and internal hosts.
    const BLOCKED_HOSTS: &[&str] = &[
        "localhost",
        "ip6-localhost",
        "ip6-loopback",
        "metadata.google.internal",
        "metadata",
    ];
    if BLOCKED_HOSTS.contains(&host.as_str()) {
        return Err(format!("host '{host}' is blocked"));
    }
    if host.ends_with(".internal") || host.ends_with(".localhost") {
        return Err(format!("host '{host}' is blocked"));
    }

    // IP-literal checks cover loopback, link-local, private, metadata, etc.
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_blocked_ip(ip) {
            return Err(format!("ip '{ip}' is in a blocked range"));
        }
    }

    Ok(())
}

fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_blocked_v4(v4),
        IpAddr::V6(v6) => is_blocked_v6(v6),
    }
}

fn is_blocked_v4(ip: Ipv4Addr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() || ip.is_link_local()
        || ip.is_private() || ip.is_broadcast() || ip.is_multicast()
        || ip.is_documentation()
    {
        return true;
    }
    let octets = ip.octets();
    // AWS/GCP/Azure metadata
    if ip == Ipv4Addr::new(169, 254, 169, 254) {
        return true;
    }
    // CGNAT shared address space (RFC 6598) — still internal from a caller's view.
    if octets[0] == 100 && (64..=127).contains(&octets[1]) {
        return true;
    }
    false
}

fn is_blocked_v6(ip: Ipv6Addr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() {
        return true;
    }
    let seg = ip.segments();
    // Unique local addresses fc00::/7
    if (seg[0] & 0xfe00) == 0xfc00 {
        return true;
    }
    // Link-local fe80::/10
    if (seg[0] & 0xffc0) == 0xfe80 {
        return true;
    }
    // IPv4-mapped — run the v4 filter on the embedded address.
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_blocked_v4(v4);
    }
    false
}

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
// HttpRequestTool
// ---------------------------------------------------------------------------

/// Make an HTTP request with configurable method, headers, and body.
pub struct HttpRequestTool;

#[async_trait]
impl Tool for HttpRequestTool {
    fn name(&self) -> &str {
        "http_request"
    }

    fn description(&self) -> &str {
        "Make an HTTP request. Supports GET, POST, PUT, PATCH, DELETE with headers and JSON body."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "Full URL to request" },
                "method": { "type": "string", "description": "HTTP method (default: GET)", "enum": ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"] },
                "headers": {
                    "type": "object",
                    "description": "HTTP headers as key-value pairs",
                    "additionalProperties": { "type": "string" }
                },
                "body": { "description": "Request body (string or JSON object)" },
                "timeout_secs": { "type": "integer", "description": "Timeout in seconds (default: 30)" }
            },
            "required": ["url"]
        })
    }

    fn category(&self) -> &str {
        "external_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let url = match input.parameters.get("url").and_then(|v| v.as_str()) {
            Some(u) => u,
            None => return err("Missing required parameter: url"),
        };

        if let Err(reason) = validate_egress_url(url) {
            return err(format!("blocked by SSRF policy: {reason}"));
        }

        let method_str = input
            .parameters
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("GET")
            .to_uppercase();

        let timeout_secs = input
            .parameters
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(30);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .build();

        let client = match client {
            Ok(c) => c,
            Err(e) => return err(format!("Failed to create HTTP client: {}", e)),
        };

        let method = match method_str.as_str() {
            "GET" => reqwest::Method::GET,
            "POST" => reqwest::Method::POST,
            "PUT" => reqwest::Method::PUT,
            "PATCH" => reqwest::Method::PATCH,
            "DELETE" => reqwest::Method::DELETE,
            "HEAD" => reqwest::Method::HEAD,
            _ => return err(format!("Unsupported HTTP method: {}", method_str)),
        };

        let mut req = client.request(method, url);

        // Add headers
        if let Some(headers) = input.parameters.get("headers").and_then(|v| v.as_object()) {
            for (key, val) in headers {
                if let Some(v) = val.as_str() {
                    req = req.header(key.as_str(), v);
                }
            }
        }

        // Add body
        if let Some(body) = input.parameters.get("body") {
            if let Some(s) = body.as_str() {
                req = req.body(s.to_string());
            } else {
                req = req.json(body);
            }
        }

        match req.send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let headers: HashMap<String, String> = resp
                    .headers()
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.as_str().to_string(),
                            v.to_str().unwrap_or("<binary>").to_string(),
                        )
                    })
                    .collect();

                let body_text = resp.text().await.unwrap_or_default();

                // Truncate very large responses
                let truncated = body_text.len() > 100_000;
                let body_out = if truncated {
                    format!(
                        "{}...\n[truncated — {} bytes total]",
                        &body_text[..100_000],
                        body_text.len()
                    )
                } else {
                    body_text
                };

                ok(json!({
                    "status": status,
                    "headers": headers,
                    "body": body_out,
                    "truncated": truncated
                }))
            }
            Err(e) => err(format!("HTTP request failed: {}", e)),
        }
    }
}

// ---------------------------------------------------------------------------
// WebFetchTool
// ---------------------------------------------------------------------------

/// Fetch a URL and extract its text content.
///
/// This is a simplified GET that strips HTML tags and returns plain text,
/// suitable for feeding page content to an LLM.
pub struct WebFetchTool;

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetch a URL and extract text content. Strips HTML tags for cleaner output."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL to fetch" },
                "max_length": { "type": "integer", "description": "Max characters to return (default: 50000)" }
            },
            "required": ["url"]
        })
    }

    fn category(&self) -> &str {
        "external_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let url = match input.parameters.get("url").and_then(|v| v.as_str()) {
            Some(u) => u,
            None => return err("Missing required parameter: url"),
        };

        if let Err(reason) = validate_egress_url(url) {
            return err(format!("blocked by SSRF policy: {reason}"));
        }

        let max_length = input
            .parameters
            .get("max_length")
            .and_then(|v| v.as_u64())
            .unwrap_or(50_000) as usize;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("NexusAgent/1.0")
            .build();

        let client = match client {
            Ok(c) => c,
            Err(e) => return err(format!("Failed to create HTTP client: {}", e)),
        };

        match client.get(url).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                if status >= 400 {
                    return err(format!("HTTP {} for {}", status, url));
                }

                let body = resp.text().await.unwrap_or_default();

                // Strip HTML tags (simple regex-free approach)
                let text = strip_html_tags(&body);

                // Collapse whitespace
                let cleaned = collapse_whitespace(&text);

                let truncated = cleaned.len() > max_length;
                let output = if truncated {
                    cleaned[..max_length].to_string()
                } else {
                    cleaned
                };

                ok(json!({
                    "url": url,
                    "status": status,
                    "content": output,
                    "length": output.len(),
                    "truncated": truncated
                }))
            }
            Err(e) => err(format!("Failed to fetch '{}': {}", url, e)),
        }
    }
}

/// Simple HTML tag stripper (no regex dependency required).
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;

    let chars: Vec<char> = html.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if chars[i] == '<' {
            // Check for script/style open/close
            let remaining: String = chars[i..].iter().take(20).collect();
            let lower = remaining.to_lowercase();

            if lower.starts_with("<script") {
                in_script = true;
            } else if lower.starts_with("</script") {
                in_script = false;
            } else if lower.starts_with("<style") {
                in_style = true;
            } else if lower.starts_with("</style") {
                in_style = false;
            }

            in_tag = true;
            i += 1;
            continue;
        }

        if chars[i] == '>' {
            in_tag = false;
            i += 1;
            continue;
        }

        if !in_tag && !in_script && !in_style {
            result.push(chars[i]);
        }
        i += 1;
    }

    result
}

/// Collapse consecutive whitespace into single spaces and trim.
fn collapse_whitespace(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut prev_ws = false;

    for ch in text.chars() {
        if ch.is_whitespace() {
            if !prev_ws {
                result.push(' ');
            }
            prev_ws = true;
        } else {
            result.push(ch);
            prev_ws = false;
        }
    }

    result.trim().to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_html_tags_basic() {
        let html = "<html><body><h1>Hello</h1><p>World</p></body></html>";
        let text = strip_html_tags(html);
        assert!(text.contains("Hello"));
        assert!(text.contains("World"));
        assert!(!text.contains("<"));
    }

    #[test]
    fn strip_html_tags_removes_script() {
        let html = "<p>before</p><script>alert('x')</script><p>after</p>";
        let text = strip_html_tags(html);
        assert!(text.contains("before"));
        assert!(text.contains("after"));
        assert!(!text.contains("alert"));
    }

    #[test]
    fn collapse_whitespace_works() {
        let text = "  hello   world  \n\n  foo  ";
        let result = collapse_whitespace(text);
        assert_eq!(result, "hello world foo");
    }

    #[test]
    fn http_request_tool_schema_valid() {
        let tool = HttpRequestTool;
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("url")));
    }

    #[test]
    fn web_fetch_tool_schema_valid() {
        let tool = WebFetchTool;
        assert_eq!(tool.name(), "web_fetch");
        assert_eq!(tool.category(), "external_ops");
    }
}
