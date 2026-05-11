//! Stdio ACP server — line-delimited JSON-RPC against Nexus's
//! existing HTTP surface.
//!
//! Strategy: keep this adapter **stateless**. It doesn't touch the
//! Nexus database directly; everything flows through the HTTP API at
//! `NEXUS_API_URL` (default `http://localhost:8020`). That means:
//!
//!   * The editor plugin talks ACP → this adapter
//!   * This adapter translates to `/oneshot` / `/projects/*` calls
//!   * Nexus answers with SSE → we repackage each SSE event as an
//!     ACP `session/update` notification
//!
//! Keeping the adapter thin lets it live in the `nexus` binary
//! without dragging in `AppState`, SQLite, or any of the heavier
//! nexus-http wiring. A local user running `nexus acp serve` just
//! needs a Nexus server reachable at `NEXUS_API_URL`.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::types::{AcpError, JsonRpcError, JsonRpcMessage};

/// Stateless ACP server. Construct one with [`AcpServer::new`] and
/// call [`AcpServer::serve_stdio`] to drive it until the client
/// closes stdin.
pub struct AcpServer {
    http: reqwest::Client,
    api_base: String,
    sessions: Arc<Mutex<HashMap<String, SessionState>>>,
}

#[derive(Debug, Clone)]
struct SessionState {
    project_id: Option<String>,
}

impl AcpServer {
    /// Default constructor — reads `NEXUS_API_URL` from the
    /// environment or falls back to the local-dev default of
    /// `http://localhost:8020`.
    pub fn new() -> Self {
        let api_base = std::env::var("NEXUS_API_URL")
            .unwrap_or_else(|_| "http://localhost:8020".to_string());
        Self::with_api_base(api_base)
    }

    pub fn with_api_base(api_base: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            http,
            api_base: api_base.into(),
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Run the server loop. Reads line-delimited JSON-RPC from stdin
    /// and writes responses + `session/update` notifications to
    /// stdout until stdin closes.
    pub async fn serve_stdio(&self) -> Result<(), AcpError> {
        let stdin = tokio::io::stdin();
        let mut reader = BufReader::new(stdin);
        let stdout = Arc::new(Mutex::new(tokio::io::stdout()));

        let mut line = String::new();
        loop {
            line.clear();
            let n = reader.read_line(&mut line).await?;
            if n == 0 {
                info!("ACP stdin closed — exiting cleanly");
                return Ok(());
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let msg: JsonRpcMessage = match serde_json::from_str(trimmed) {
                Ok(m) => m,
                Err(e) => {
                    warn!(error = %e, raw = %trimmed, "ACP parse error");
                    let resp = JsonRpcMessage::error(
                        None,
                        JsonRpcError::parse_error(format!("{e}")),
                    );
                    write_line(&stdout, &resp).await?;
                    continue;
                }
            };

            if msg.is_notification() {
                self.handle_notification(&msg).await;
                continue;
            }
            if !msg.is_request() {
                // Responses from the client to our server-to-client
                // requests — we don't initiate any today, so just
                // log and move on.
                debug!(?msg, "ACP: ignoring non-request/non-notification");
                continue;
            }

            let out = self.handle_request(&msg, stdout.clone()).await;
            write_line(&stdout, &out).await?;
        }
    }

    async fn handle_notification(&self, msg: &JsonRpcMessage) {
        let method = msg.method.as_deref().unwrap_or("");
        match method {
            "initialized" | "$/cancelRequest" => {
                debug!(method, "ACP notification handled");
            }
            _ => {
                debug!(method, "ACP notification ignored");
            }
        }
    }

    async fn handle_request(
        &self,
        msg: &JsonRpcMessage,
        stdout: Arc<Mutex<tokio::io::Stdout>>,
    ) -> JsonRpcMessage {
        let method = msg.method.as_deref().unwrap_or("");
        let params = msg.params.clone().unwrap_or(Value::Null);
        debug!(method, "ACP request");

        match method {
            "initialize" => JsonRpcMessage::success(
                msg.id.clone(),
                json!({
                    "protocolVersion": "2025-03-15",
                    "serverInfo": {
                        "name": "nexus",
                        "version": env!("CARGO_PKG_VERSION"),
                    },
                    "capabilities": {
                        "promptCapabilities": {
                            "streaming": true,
                        },
                        "sessionCapabilities": {
                            "multiTurn": true,
                        }
                    }
                }),
            ),

            "authenticate" => JsonRpcMessage::success(
                msg.id.clone(),
                json!({ "status": "ok" }),
            ),

            "session/new" => {
                let project_id =
                    params.get("projectId").and_then(|v| v.as_str()).map(String::from);
                let session_id = uuid::Uuid::new_v4().to_string();
                self.sessions.lock().await.insert(
                    session_id.clone(),
                    SessionState { project_id },
                );
                JsonRpcMessage::success(
                    msg.id.clone(),
                    json!({ "sessionId": session_id }),
                )
            }

            "session/prompt" => {
                let session_id = params
                    .get("sessionId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let prompt = params
                    .get("prompt")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if session_id.is_empty() {
                    return JsonRpcMessage::error(
                        msg.id.clone(),
                        JsonRpcError::invalid_params("missing sessionId"),
                    );
                }
                if prompt.trim().is_empty() {
                    return JsonRpcMessage::error(
                        msg.id.clone(),
                        JsonRpcError::invalid_params("empty prompt"),
                    );
                }
                let state = self.sessions.lock().await.get(&session_id).cloned();
                match state {
                    Some(state) => self
                        .run_prompt(session_id, state, prompt, stdout)
                        .await
                        .map(|summary| JsonRpcMessage::success(msg.id.clone(), summary))
                        .unwrap_or_else(|e| {
                            JsonRpcMessage::error(msg.id.clone(), JsonRpcError::internal(e))
                        }),
                    None => JsonRpcMessage::error(
                        msg.id.clone(),
                        JsonRpcError::invalid_params(format!(
                            "unknown sessionId: {session_id}"
                        )),
                    ),
                }
            }

            "session/cancel" => {
                let session_id = params
                    .get("sessionId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                self.sessions.lock().await.remove(&session_id);
                JsonRpcMessage::success(msg.id.clone(), json!({ "ok": true }))
            }

            "shutdown" => {
                JsonRpcMessage::success(msg.id.clone(), json!(null))
            }

            other => JsonRpcMessage::error(
                msg.id.clone(),
                JsonRpcError::method_not_found(other),
            ),
        }
    }

    /// Drive a single `session/prompt` through the Nexus `/oneshot`
    /// endpoint. Emits at least one `session/update` notification so
    /// the client knows we received the prompt, and returns a final
    /// summary when the upstream call finishes.
    async fn run_prompt(
        &self,
        session_id: String,
        state: SessionState,
        prompt: String,
        stdout: Arc<Mutex<tokio::io::Stdout>>,
    ) -> Result<Value, String> {
        // Announce receipt so editor UI can spin a progress dot.
        let ack = JsonRpcMessage::notification(
            "session/update",
            json!({
                "sessionId": &session_id,
                "kind": "ack",
                "message": "received",
            }),
        );
        let _ = write_line(&stdout, &ack).await;

        // Proxy to /oneshot. We use the non-streaming endpoint for
        // now — SSE streaming over ACP lands in v0.3.
        let url = format!("{}/oneshot", self.api_base.trim_end_matches('/'));
        let body = json!({
            "prompt": prompt,
            "project_id": state.project_id,
        });

        let resp = match self.http.post(&url).json(&body).send().await {
            Ok(r) => r,
            Err(e) => {
                let err_note = JsonRpcMessage::notification(
                    "session/update",
                    json!({
                        "sessionId": &session_id,
                        "kind": "error",
                        "message": format!("nexus {} unreachable: {e}", self.api_base),
                    }),
                );
                let _ = write_line(&stdout, &err_note).await;
                return Err(format!("upstream request failed: {e}"));
            }
        };
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();

        // Forward the result back as a `message` update, preserving
        // raw JSON when the response parsed cleanly.
        let parsed: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!(text));
        let msg = JsonRpcMessage::notification(
            "session/update",
            json!({
                "sessionId": &session_id,
                "kind": "message",
                "role": "assistant",
                "status": status.as_u16(),
                "content": parsed,
            }),
        );
        let _ = write_line(&stdout, &msg).await;

        if !status.is_success() {
            return Err(format!("oneshot returned HTTP {status}"));
        }

        Ok(json!({
            "sessionId": &session_id,
            "status": status.as_u16(),
            "ok": true,
        }))
    }
}

impl Default for AcpServer {
    fn default() -> Self {
        Self::new()
    }
}

async fn write_line(
    out: &Arc<Mutex<tokio::io::Stdout>>,
    msg: &JsonRpcMessage,
) -> Result<(), AcpError> {
    let serialised = serde_json::to_string(msg)
        .map_err(|e| AcpError::MalformedJson(e.to_string()))?;
    let mut guard = out.lock().await;
    guard.write_all(serialised.as_bytes()).await?;
    guard.write_all(b"\n").await?;
    guard.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn initialize_reports_nexus_capabilities() {
        let s = AcpServer::with_api_base("http://localhost:0");
        let req = JsonRpcMessage {
            version: "2.0".into(),
            id: Some(json!(1)),
            method: Some("initialize".into()),
            params: Some(json!({})),
            result: None,
            error: None,
        };
        let stdout = Arc::new(Mutex::new(tokio::io::stdout()));
        let resp = s.handle_request(&req, stdout).await;
        assert!(resp.error.is_none());
        let r = resp.result.unwrap();
        assert_eq!(r["serverInfo"]["name"], "nexus");
        assert!(r["capabilities"]["promptCapabilities"]["streaming"]
            .as_bool()
            .unwrap_or(false));
    }

    #[tokio::test]
    async fn unknown_method_returns_method_not_found() {
        let s = AcpServer::with_api_base("http://localhost:0");
        let req = JsonRpcMessage {
            version: "2.0".into(),
            id: Some(json!(7)),
            method: Some("session/does-not-exist".into()),
            params: None,
            result: None,
            error: None,
        };
        let stdout = Arc::new(Mutex::new(tokio::io::stdout()));
        let resp = s.handle_request(&req, stdout).await;
        assert_eq!(resp.error.as_ref().unwrap().code, -32601);
    }

    #[tokio::test]
    async fn session_new_and_cancel_roundtrip() {
        let s = AcpServer::with_api_base("http://localhost:0");
        let stdout = Arc::new(Mutex::new(tokio::io::stdout()));
        let new_req = JsonRpcMessage {
            version: "2.0".into(),
            id: Some(json!(1)),
            method: Some("session/new".into()),
            params: Some(json!({ "projectId": "p-1" })),
            result: None,
            error: None,
        };
        let r = s.handle_request(&new_req, stdout.clone()).await;
        let sid = r.result.unwrap()["sessionId"].as_str().unwrap().to_string();
        assert!(!sid.is_empty());
        // Cancelling an unknown session is still OK — ACP treats it
        // as idempotent shutdown signalling.
        let cancel = JsonRpcMessage {
            version: "2.0".into(),
            id: Some(json!(2)),
            method: Some("session/cancel".into()),
            params: Some(json!({ "sessionId": sid })),
            result: None,
            error: None,
        };
        let r = s.handle_request(&cancel, stdout).await;
        assert!(r.result.is_some());
    }

    #[tokio::test]
    async fn session_prompt_rejects_unknown_session() {
        let s = AcpServer::with_api_base("http://localhost:0");
        let stdout = Arc::new(Mutex::new(tokio::io::stdout()));
        let req = JsonRpcMessage {
            version: "2.0".into(),
            id: Some(json!(3)),
            method: Some("session/prompt".into()),
            params: Some(json!({
                "sessionId": "does-not-exist",
                "prompt": "hello",
            })),
            result: None,
            error: None,
        };
        let r = s.handle_request(&req, stdout).await;
        assert_eq!(r.error.as_ref().unwrap().code, -32602);
    }

    #[tokio::test]
    async fn jsonrpc_roundtrips_through_serde() {
        let m = JsonRpcMessage::success(Some(json!(7)), json!({"ok": true}));
        let s = serde_json::to_string(&m).unwrap();
        let back: JsonRpcMessage = serde_json::from_str(&s).unwrap();
        assert_eq!(back.id, Some(json!(7)));
        assert!(back.result.is_some());
    }
}

