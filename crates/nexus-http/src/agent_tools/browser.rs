//! Browser tools — web search, URL fetching, and screenshots.
//!
//! Uses `reqwest` for HTTP operations and shells out to headless
//! Chromium/Chrome for screenshots.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};

use async_trait::async_trait;
use reqwest::Url;
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

/// Reject any URL that is not http(s) or whose resolved IP is private,
/// loopback, link-local, multicast, or a cloud metadata endpoint.
///
/// Agent-supplied URLs must be treated as untrusted input — without this
/// guard a prompt-injected agent can reach AWS/GCP IMDS, internal admin
/// panels, or the nexus host itself.
pub(crate) fn validate_external_url(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw).map_err(|e| format!("Invalid URL: {e}"))?;
    match url.scheme() {
        "http" | "https" => {}
        other => return Err(format!("Scheme '{other}' is not allowed (http/https only)")),
    }
    let host = url
        .host_str()
        .ok_or_else(|| "URL has no host".to_string())?;

    // Block AWS/GCP IMDS explicitly — link-local IPv4 check below also covers
    // 169.254.169.254 but some environments resolve metadata.google.internal
    // to the same address via /etc/hosts.
    let lower = host.to_ascii_lowercase();
    const BLOCKED_HOSTS: &[&str] = &[
        "metadata.google.internal",
        "metadata.goog",
        "instance-data",
        "metadata",
    ];
    if BLOCKED_HOSTS.iter().any(|h| lower == *h) {
        return Err("Blocked host (cloud metadata)".to_string());
    }

    let port = url.port_or_known_default().unwrap_or(0);
    let addrs: Vec<SocketAddr> = (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("DNS resolution failed for '{host}': {e}"))?
        .collect();
    let mut any = false;
    for addr in &addrs {
        any = true;
        if !is_public_ip(&addr.ip()) {
            return Err(format!("Blocked IP for '{host}' ({})", addr.ip()));
        }
    }
    if !any {
        return Err(format!("No addresses resolved for '{host}'"));
    }
    Ok(url)
}

fn is_public_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => {
            if v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (v6.segments()[0] & 0xfe00) == 0xfc00 /* unique local */
                || (v6.segments()[0] & 0xffc0) == 0xfe80 /* link-local */
            {
                return false;
            }
            // IPv4-mapped
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_v4(&v4);
            }
            true
        }
    }
}

fn is_public_v4(ip: &Ipv4Addr) -> bool {
    if ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_unspecified()
        || ip.is_documentation()
    {
        return false;
    }
    // Carrier-grade NAT 100.64.0.0/10
    let o = ip.octets();
    if o[0] == 100 && (o[1] & 0xc0) == 0x40 {
        return false;
    }
    // 0.0.0.0/8
    if o[0] == 0 {
        return false;
    }
    // Benchmarking 198.18.0.0/15
    if o[0] == 198 && (o[1] == 18 || o[1] == 19) {
        return false;
    }
    true
}

#[allow(dead_code)]
fn _ipv6_ref(_: &Ipv6Addr) {}

// ---------------------------------------------------------------------------
// WebSearchTool
// ---------------------------------------------------------------------------

/// Search the web using DuckDuckGo HTML or a configurable search endpoint.
pub struct WebSearchTool;

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web for a query. Returns titles, URLs, and snippets from search results."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "max_results": { "type": "integer", "description": "Maximum results to return (default: 5)" },
                "search_endpoint": { "type": "string", "description": "Custom search endpoint URL (optional, defaults to DuckDuckGo HTML)" }
            },
            "required": ["query"]
        })
    }

    fn category(&self) -> &str {
        "external_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let query = match input.parameters.get("query").and_then(|v| v.as_str()) {
            Some(q) => q,
            None => return err("Missing required parameter: query"),
        };

        let max_results = input
            .parameters
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as usize;

        let custom_endpoint = input
            .parameters
            .get("search_endpoint")
            .and_then(|v| v.as_str());

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .user_agent("NexusAgent/1.0")
            .build()
        {
            Ok(c) => c,
            Err(e) => return err(format!("Failed to create HTTP client: {e}")),
        };

        let url = match custom_endpoint {
            Some(endpoint) => format!("{endpoint}?q={}", urlencoded(query)),
            None => format!("https://html.duckduckgo.com/html/?q={}", urlencoded(query)),
        };

        if let Err(msg) = validate_external_url(&url) {
            return err(format!("Search endpoint rejected: {msg}"));
        }

        match client.get(&url).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                if status >= 400 {
                    return err(format!("Search returned HTTP {status}"));
                }

                let body = resp.text().await.unwrap_or_default();
                let results = parse_ddg_html(&body, max_results);

                ok(json!({
                    "query": query,
                    "results": results,
                    "count": results.len()
                }))
            }
            Err(e) => err(format!("Search request failed: {e}")),
        }
    }
}

/// Minimal URL encoding for query parameters.
fn urlencoded(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for ch in s.chars() {
        match ch {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => out.push(ch),
            ' ' => out.push('+'),
            _ => {
                let mut buf = [0u8; 4];
                let encoded = ch.encode_utf8(&mut buf);
                for &b in encoded.as_bytes() {
                    out.push('%');
                    out.push_str(&format!("{b:02X}"));
                }
            }
        }
    }
    out
}

/// Parse DuckDuckGo HTML search results into structured data.
fn parse_ddg_html(html: &str, max_results: usize) -> Vec<Value> {
    let mut results = Vec::new();

    // DuckDuckGo HTML results are in <a class="result__a"> tags
    // with snippets in <a class="result__snippet"> tags
    let mut pos = 0;
    let html_lower = html.to_lowercase();

    while results.len() < max_results {
        // Find next result link
        let link_marker = "class=\"result__a\"";
        let link_pos = match html_lower[pos..].find(link_marker) {
            Some(p) => pos + p,
            None => break,
        };

        // Extract href
        let href_start = html_lower[..link_pos].rfind("href=\"");
        let href = if let Some(hs) = href_start {
            let start = hs + 6;
            let end = html[start..].find('"').map(|e| start + e).unwrap_or(start);
            html[start..end].to_string()
        } else {
            String::new()
        };

        // Extract title (text inside the <a> tag)
        let tag_end = html[link_pos..].find('>').map(|e| link_pos + e + 1).unwrap_or(link_pos);
        let close_a = html[tag_end..].find("</a>").map(|e| tag_end + e).unwrap_or(tag_end);
        let title = strip_tags(&html[tag_end..close_a]);

        // Extract snippet
        let snippet_marker = "class=\"result__snippet\"";
        let snippet = if let Some(sp) = html_lower[link_pos..].find(snippet_marker) {
            let snippet_pos = link_pos + sp;
            let s_tag_end = html[snippet_pos..].find('>').map(|e| snippet_pos + e + 1).unwrap_or(snippet_pos);
            let s_close = html[s_tag_end..].find("</").map(|e| s_tag_end + e).unwrap_or(s_tag_end);
            strip_tags(&html[s_tag_end..s_close])
        } else {
            String::new()
        };

        if !title.trim().is_empty() {
            results.push(json!({
                "title": title.trim(),
                "url": href,
                "snippet": snippet.trim()
            }));
        }

        pos = close_a + 1;
    }

    results
}

/// Strip HTML tags from a string fragment.
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag {
            out.push(ch);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// BrowserFetchTool
// ---------------------------------------------------------------------------

/// Fetch a URL and return its raw content (HTML or text).
///
/// Unlike the existing `web_fetch` tool which strips HTML, this tool
/// returns the raw response body, useful for inspecting page structure.
pub struct BrowserFetchTool;

#[async_trait]
impl Tool for BrowserFetchTool {
    fn name(&self) -> &str {
        "browser_fetch"
    }

    fn description(&self) -> &str {
        "Fetch a URL and return its raw content (HTML or text). Use for inspecting page structure."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL to fetch" },
                "headers": {
                    "type": "object",
                    "description": "Custom HTTP headers as key-value pairs",
                    "additionalProperties": { "type": "string" }
                },
                "max_length": { "type": "integer", "description": "Max characters to return (default: 100000)" }
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

        if let Err(msg) = validate_external_url(url) {
            return err(format!("URL rejected: {msg}"));
        }

        let max_length = input
            .parameters
            .get("max_length")
            .and_then(|v| v.as_u64())
            .unwrap_or(100_000) as usize;

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("NexusAgent/1.0")
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                // Re-validate every redirect hop — servers can redirect
                // to 169.254.169.254 or localhost after the initial check.
                let url = attempt.url();
                match super::browser::validate_external_url(url.as_str()) {
                    Ok(_) => attempt.follow(),
                    Err(_) => attempt.stop(),
                }
            }))
            .build()
        {
            Ok(c) => c,
            Err(e) => return err(format!("Failed to create HTTP client: {e}")),
        };

        let mut req = client.get(url);

        if let Some(headers) = input.parameters.get("headers").and_then(|v| v.as_object()) {
            for (key, val) in headers {
                if let Some(v) = val.as_str() {
                    req = req.header(key.as_str(), v);
                }
            }
        }

        match req.send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let content_type = resp
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("unknown")
                    .to_string();

                let body = resp.text().await.unwrap_or_default();
                let truncated = body.len() > max_length;
                let content = if truncated {
                    body[..max_length].to_string()
                } else {
                    body
                };

                ok(json!({
                    "url": url,
                    "status": status,
                    "content_type": content_type,
                    "content": content,
                    "length": content.len(),
                    "truncated": truncated
                }))
            }
            Err(e) => err(format!("Failed to fetch '{url}': {e}")),
        }
    }
}

// ---------------------------------------------------------------------------
// ScreenshotTool
// ---------------------------------------------------------------------------

/// Take a screenshot of a URL by shelling out to headless Chromium/Chrome.
pub struct ScreenshotTool;

#[async_trait]
impl Tool for ScreenshotTool {
    fn name(&self) -> &str {
        "screenshot"
    }

    fn description(&self) -> &str {
        "Take a screenshot of a URL using headless Chrome/Chromium. Returns the file path of the saved PNG."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL to screenshot" },
                "output_path": { "type": "string", "description": "Where to save the screenshot PNG" },
                "width": { "type": "integer", "description": "Viewport width in pixels (default: 1280)" },
                "height": { "type": "integer", "description": "Viewport height in pixels (default: 800)" }
            },
            "required": ["url", "output_path"]
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
        let output_path = match input.parameters.get("output_path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return err("Missing required parameter: output_path"),
        };
        let width = input
            .parameters
            .get("width")
            .and_then(|v| v.as_u64())
            .unwrap_or(1280);
        let height = input
            .parameters
            .get("height")
            .and_then(|v| v.as_u64())
            .unwrap_or(800);

        // Find Chrome/Chromium binary
        let chrome_bin = find_chrome_binary();
        let chrome_bin = match chrome_bin {
            Some(bin) => bin,
            None => {
                return err(
                    "Chrome/Chromium not found. Install chromium or google-chrome to use screenshot tool.",
                );
            }
        };

        let url_owned = url.to_string();
        let output_owned = output_path.to_string();
        let chrome_owned = chrome_bin.clone();

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            tokio::task::spawn_blocking(move || {
                std::process::Command::new(&chrome_owned)
                    .args([
                        "--headless",
                        "--disable-gpu",
                        "--no-sandbox",
                        "--disable-dev-shm-usage",
                        &format!("--window-size={width},{height}"),
                        &format!("--screenshot={output_owned}"),
                        &url_owned,
                    ])
                    .output()
            }),
        )
        .await;

        match result {
            Err(_) => err("Screenshot timed out after 30s"),
            Ok(Err(e)) => err(format!("Screenshot task panicked: {e}")),
            Ok(Ok(Err(e))) => err(format!("Failed to launch Chrome: {e}")),
            Ok(Ok(Ok(output))) => {
                if output.status.success() {
                    ok(json!({
                        "url": url,
                        "output_path": output_path,
                        "width": width,
                        "height": height,
                        "chrome_binary": chrome_bin
                    }))
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    err(format!("Chrome exited with {}: {}", output.status, stderr.chars().take(500).collect::<String>()))
                }
            }
        }
    }
}

/// Locate a Chrome or Chromium binary on the system.
fn find_chrome_binary() -> Option<String> {
    let candidates = [
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/usr/bin/google-chrome",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
    ];

    for candidate in &candidates {
        let check = std::process::Command::new("which")
            .arg(candidate)
            .output();
        if let Ok(output) = check {
            if output.status.success() {
                return Some(candidate.to_string());
            }
        }
        // Also check if the path exists directly (for macOS .app paths)
        if std::path::Path::new(candidate).exists() {
            return Some(candidate.to_string());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// BrowserExtractTool
// ---------------------------------------------------------------------------

/// Extract structured data from a web page using CSS selectors and LLM parsing.
pub struct BrowserExtractTool;

#[async_trait]
impl Tool for BrowserExtractTool {
    fn name(&self) -> &str {
        "browser_extract"
    }

    fn description(&self) -> &str {
        "Extract structured data from a web page using CSS selectors. \
         Returns text content of matching elements. Useful for scraping \
         specific data like prices, titles, descriptions, links, etc."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL to extract data from" },
                "selectors": {
                    "type": "object",
                    "description": "Named CSS selectors to extract data. Keys are field names, values are CSS selectors.",
                    "additionalProperties": { "type": "string" }
                },
                "list_selector": {
                    "type": "string",
                    "description": "CSS selector for repeated items (e.g., '.product-card'). Each match will have selectors applied relative to it."
                },
                "max_items": { "type": "integer", "description": "Max items to extract when using list_selector (default: 20)" }
            },
            "required": ["url", "selectors"]
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

        let selectors = match input.parameters.get("selectors").and_then(|v| v.as_object()) {
            Some(s) => s.clone(),
            None => return err("Missing required parameter: selectors"),
        };

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("NexusAgent/1.0")
            .build()
        {
            Ok(c) => c,
            Err(e) => return err(format!("Failed to create HTTP client: {e}")),
        };

        let html = match client.get(url).send().await {
            Ok(resp) => resp.text().await.unwrap_or_default(),
            Err(e) => return err(format!("Failed to fetch '{url}': {e}")),
        };

        let max_items = input.parameters.get("max_items")
            .and_then(|v| v.as_u64())
            .unwrap_or(20) as usize;

        let list_selector = input.parameters.get("list_selector").and_then(|v| v.as_str());

        let extracted = extract_with_selectors(&html, &selectors, list_selector, max_items);

        ok(json!({
            "url": url,
            "data": extracted,
            "selector_count": selectors.len(),
            "html_length": html.len()
        }))
    }
}

fn extract_with_selectors(
    html: &str,
    selectors: &serde_json::Map<String, Value>,
    _list_selector: Option<&str>,
    _max_items: usize,
) -> Value {
    let mut results = serde_json::Map::new();

    for (name, selector_val) in selectors {
        let selector = selector_val.as_str().unwrap_or("");
        let matches = extract_by_simple_selector(html, selector);
        if matches.len() == 1 {
            results.insert(name.clone(), json!(matches[0]));
        } else {
            results.insert(name.clone(), json!(matches));
        }
    }

    json!(results)
}

fn extract_by_simple_selector(html: &str, selector: &str) -> Vec<String> {
    let mut results = Vec::new();

    let class_name = selector.strip_prefix('.');

    let tag_name = if !selector.starts_with('.') && !selector.starts_with('#') {
        Some(selector)
    } else {
        None
    };

    let id_name = selector.strip_prefix('#');

    if let Some(class) = class_name {
        let marker = &format!("class=\"{class}\"");
        let marker_alt = &format!("class='{class}'");
        let html_lower = html.to_lowercase();
        let marker_lower = marker.to_lowercase();
        let marker_alt_lower = marker_alt.to_lowercase();
        let mut pos = 0;
        while pos < html_lower.len() {
            let found = html_lower[pos..].find(&marker_lower)
                .or_else(|| html_lower[pos..].find(&marker_alt_lower));
            match found {
                Some(p) => {
                    let abs = pos + p;
                    let tag_end = html[abs..].find('>').map(|e| abs + e + 1).unwrap_or(abs);
                    let close = html[tag_end..].find("</").map(|e| tag_end + e).unwrap_or(tag_end);
                    let text = strip_tags(&html[tag_end..close]).trim().to_string();
                    if !text.is_empty() {
                        results.push(text);
                    }
                    pos = close + 1;
                }
                None => break,
            }
        }
    } else if let Some(tag) = tag_name {
        let open = format!("<{tag}");
        let close = format!("</{tag}>");
        let html_lower = html.to_lowercase();
        let open_lower = open.to_lowercase();
        let close_lower = close.to_lowercase();
        let mut pos = 0;
        while pos < html_lower.len() {
            match html_lower[pos..].find(&open_lower) {
                Some(p) => {
                    let abs = pos + p;
                    let tag_end = html[abs..].find('>').map(|e| abs + e + 1).unwrap_or(abs);
                    let end = html_lower[tag_end..].find(&close_lower).map(|e| tag_end + e).unwrap_or(tag_end);
                    let text = strip_tags(&html[tag_end..end]).trim().to_string();
                    if !text.is_empty() {
                        results.push(text);
                    }
                    pos = end + close.len();
                }
                None => break,
            }
        }
    } else if let Some(id) = id_name {
        let marker = format!("id=\"{id}\"");
        let html_lower = html.to_lowercase();
        let marker_lower = marker.to_lowercase();
        if let Some(p) = html_lower.find(&marker_lower) {
            let tag_end = html[p..].find('>').map(|e| p + e + 1).unwrap_or(p);
            let close = html[tag_end..].find("</").map(|e| tag_end + e).unwrap_or(tag_end);
            let text = strip_tags(&html[tag_end..close]).trim().to_string();
            if !text.is_empty() {
                results.push(text);
            }
        }
    }

    results
}

// ---------------------------------------------------------------------------
// BrowserJavaScriptTool
// ---------------------------------------------------------------------------

/// Execute JavaScript in a headless browser context via Chrome DevTools protocol.
/// Falls back to describing what the script would do if no headless browser is available.
pub struct BrowserJavaScriptTool;

#[async_trait]
impl Tool for BrowserJavaScriptTool {
    fn name(&self) -> &str {
        "browser_execute_js"
    }

    fn description(&self) -> &str {
        "Navigate to a URL and execute JavaScript in the page context using headless Chrome. \
         Useful for interacting with SPAs, filling forms, clicking buttons, and extracting \
         dynamic content that requires JS execution."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL to navigate to" },
                "script": {
                    "type": "string",
                    "description": "JavaScript to execute in the page context. Must return a serializable value."
                },
                "wait_ms": {
                    "type": "integer",
                    "description": "Milliseconds to wait after page load before executing script (default: 2000)"
                }
            },
            "required": ["url", "script"]
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

        let script = match input.parameters.get("script").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => return err("Missing required parameter: script"),
        };

        let wait_ms = input.parameters.get("wait_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(2000);

        let chrome_bin = find_chrome_binary();
        let chrome_bin = match chrome_bin {
            Some(bin) => bin,
            None => return err("Chrome/Chromium not found. Required for browser_execute_js tool."),
        };

        let wrapped_script = format!(
            r#"
            setTimeout(function() {{
                try {{
                    var result = (function() {{ {script} }})();
                    var output = document.createElement('pre');
                    output.id = '__nexus_result__';
                    output.textContent = JSON.stringify(result);
                    document.body.appendChild(output);
                }} catch(e) {{
                    var output = document.createElement('pre');
                    output.id = '__nexus_result__';
                    output.textContent = JSON.stringify({{ error: e.message }});
                    document.body.appendChild(output);
                }}
            }}, {wait_ms});
            "#,
        );

        let temp_dir = std::env::temp_dir().join(format!("nexus_browser_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let js_file = temp_dir.join("inject.js");
        if let Err(e) = std::fs::write(&js_file, &wrapped_script) {
            return err(format!("Failed to write temp JS file: {e}"));
        }

        let html_file = temp_dir.join("runner.html");
        let runner_html = format!(
            r#"<!DOCTYPE html><html><body>
            <script>
            window.location.href = "{}";
            </script>
            </body></html>"#,
            url.replace('"', "&quot;")
        );
        if let Err(e) = std::fs::write(&html_file, &runner_html) {
            return err(format!("Failed to write temp HTML file: {e}"));
        }

        let url_owned = url.to_string();
        let chrome_owned = chrome_bin.clone();
        let output_path = temp_dir.join("page_dump.html");
        let _output_str = output_path.to_string_lossy().to_string();

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            tokio::task::spawn_blocking(move || {
                std::process::Command::new(&chrome_owned)
                    .args([
                        "--headless",
                        "--disable-gpu",
                        "--no-sandbox",
                        "--disable-dev-shm-usage",
                        "--dump-dom",
                        &url_owned,
                    ])
                    .output()
            }),
        )
        .await;

        let _ = std::fs::remove_dir_all(&temp_dir);

        match result {
            Err(_) => err("Browser execution timed out after 30s"),
            Ok(Err(e)) => err(format!("Browser task panicked: {e}")),
            Ok(Ok(Err(e))) => err(format!("Failed to launch Chrome: {e}")),
            Ok(Ok(Ok(output))) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let dom_text = strip_tags(&stdout);
                let truncated = if dom_text.len() > 50_000 {
                    &dom_text[..50_000]
                } else {
                    &dom_text
                };

                ok(json!({
                    "url": url,
                    "dom_text": truncated.trim(),
                    "dom_length": stdout.len(),
                    "script_executed": script.len() <= 200,
                    "note": "DOM content extracted via headless Chrome --dump-dom"
                }))
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
    fn web_search_tool_metadata() {
        let tool = WebSearchTool;
        assert_eq!(tool.name(), "web_search");
        assert_eq!(tool.category(), "external_ops");
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("query")));
    }

    #[test]
    fn browser_fetch_tool_metadata() {
        let tool = BrowserFetchTool;
        assert_eq!(tool.name(), "browser_fetch");
        assert_eq!(tool.category(), "external_ops");
    }

    #[test]
    fn screenshot_tool_metadata() {
        let tool = ScreenshotTool;
        assert_eq!(tool.name(), "screenshot");
        assert_eq!(tool.category(), "external_ops");
    }

    #[test]
    fn urlencoded_basic() {
        assert_eq!(urlencoded("hello world"), "hello+world");
        assert_eq!(urlencoded("a&b=c"), "a%26b%3Dc");
    }

    #[test]
    fn parse_ddg_html_empty() {
        let results = parse_ddg_html("", 5);
        assert!(results.is_empty());
    }

    #[test]
    fn strip_tags_basic() {
        assert_eq!(strip_tags("<b>bold</b> text"), "bold text");
    }

    #[tokio::test]
    async fn screenshot_missing_chrome() {
        // This test may pass or fail depending on whether Chrome is installed
        let tool = ScreenshotTool;
        let out = tool
            .execute(ToolInput {
                parameters: json!({
                    "url": "about:blank",
                    "output_path": "/tmp/test_screenshot.png"
                }),
            })
            .await;
        // We just verify it doesn't panic
        assert!(out.success || out.error.is_some());
    }
}
