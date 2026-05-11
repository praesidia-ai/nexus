//! Borrowed Agents HTTP surface — three endpoints that make the
//! signing primitives in [`crate::borrowed_agents`] reachable over
//! the wire.
//!
//! * `GET  /federation/borrowable-agents`
//!   Public catalogue of agents this deployment will rent out.
//!
//! * `POST /federation/borrow`
//!   Accept a [`LoanRequest`], run the agent locally, return a
//!   signed [`LoanReceipt`]. This is the endpoint peer instances
//!   call when they want to borrow Nova (etc.) from us.
//!
//! * `POST /federation/verify-receipt`
//!   Borrower-side convenience: verify an incoming receipt against
//!   a supplied peer public key. Equivalent to downloading the
//!   peer's `/.well-known/nexus-trust.json` and running the
//!   verifier in-process; exposed as a handler so browser clients
//!   + SDKs can do it without implementing Ed25519 themselves.

use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;

use crate::{
    borrowed_agents::{
        sign_receipt, verify_receipt, BorrowableAgent, LoanArtifact, LoanReceipt, LoanRequest,
    },
    error::{ApiError, ApiResult},
    handlers::oneshot::{run_oneshot_pipeline, OneShotEvent},
    state::AppState,
    trust::TrustSigner,
};

// ---------------------------------------------------------------------------
// GET /federation/borrowable-agents
// ---------------------------------------------------------------------------

/// Returns this deployment's borrowable-agent catalogue. For v1 the
/// ten conductor personas are advertised as borrowable with a shared
/// rate cap and $0 cost gate (operator policy overrides the default
/// via Settings once that surface lands).
pub async fn list_borrowable_agents(
    State(_app): State<Arc<AppState>>,
) -> ApiResult<Json<serde_json::Value>> {
    let roster = [
        ("nova", "Nova", "Full-stack coder"),
        ("atlas", "Atlas", "Cloud + infra"),
        ("kai", "Kai", "Research"),
        ("luna", "Luna", "Writing"),
        ("orion", "Orion", "Security review"),
        ("sage", "Sage", "Data"),
        ("ivy", "Ivy", "Marketing"),
        ("rex", "Rex", "DevOps"),
        ("leo", "Leo", "Product"),
        ("mia", "Mia", "Support"),
    ];
    let version = env!("CARGO_PKG_VERSION").to_string();
    let agents: Vec<BorrowableAgent> = roster
        .iter()
        .map(|(slug, name, desc)| BorrowableAgent {
            id: (*slug).to_string(),
            name: (*name).to_string(),
            conductor: (*slug).to_string(),
            description: (*desc).to_string(),
            nexus_version: version.clone(),
            rate_limit_rpm: 30,
            max_cost_usd_per_call: 0.0,
        })
        .collect();
    Ok(Json(json!({ "agents": agents })))
}

// ---------------------------------------------------------------------------
// POST /federation/borrow
// ---------------------------------------------------------------------------

/// Accept a loan request. Drives the lender's own
/// `run_oneshot_pipeline` end-to-end, captures every `OneShotEvent`
/// as a structured [`LoanArtifact`], and returns a signed
/// [`LoanReceipt`] whose Merkle log covers the actual agent output
/// (not a synthetic stand-in).
///
/// Loans are bounded by a configurable cap `NEXUS_LOAN_MAX_EVENTS`
/// (default 512) — a borrower can't exhaust the lender by asking
/// for a prompt that produces millions of events.
pub async fn borrow(
    State(app): State<Arc<AppState>>,
    Json(body): Json<LoanRequest>,
) -> ApiResult<Json<LoanReceipt>> {
    if body.agent_id.trim().is_empty() {
        return Err(ApiError::BadRequest("agent_id must not be empty".into()));
    }
    if body.prompt.trim().is_empty() {
        return Err(ApiError::BadRequest("prompt must not be empty".into()));
    }
    if body.borrower_run_id.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "borrower_run_id must not be empty".into(),
        ));
    }

    let signer = TrustSigner::load_or_generate(&app.data_dir)
        .map_err(|e| ApiError::Internal(format!("load signer: {e}")))?;

    let lender_run_id = uuid::Uuid::new_v4().to_string();

    // Drive the lender's oneshot pipeline locally. We re-use the
    // exact same machinery that backs `POST /oneshot` — no HTTP hop,
    // no second auth dance.
    let max_events: usize = std::env::var("NEXUS_LOAN_MAX_EVENTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(512);

    let (tx, mut rx) = mpsc::channel::<OneShotEvent>(256);
    let app_clone = app.clone();
    let prompt = body.prompt.clone();
    tokio::spawn(async move {
        // `auto_redesign=false, taste_threshold=70` mirror the
        // conservative defaults used by `oneshot_sync`. Loans
        // intentionally don't trigger a redesign pass — borrowers
        // asked for the agent's output, not a lender-driven polish.
        run_oneshot_pipeline(app_clone, prompt, false, 70, tx, None).await;
    });

    let mut artifacts: Vec<LoanArtifact> = Vec::new();
    let started = std::time::Instant::now();
    let mut saw_terminal = false;

    while let Some(event) = rx.recv().await {
        if artifacts.len() >= max_events {
            // Record a synthetic "truncated" marker so the borrower
            // knows the loan was capped — still inside the Merkle
            // log, still signed, still verifiable.
            artifacts.push(LoanArtifact {
                kind: "truncated".into(),
                content: json!({
                    "reason": "max_events_exceeded",
                    "cap": max_events,
                }),
                ts: chrono::Utc::now().to_rfc3339(),
                tokens_in: 0,
                tokens_out: 0,
                cost_usd: 0.0,
            });
            break;
        }
        let is_terminal = matches!(
            &event,
            OneShotEvent::Complete { .. } | OneShotEvent::Error { fatal: true, .. }
        );
        artifacts.push(event_to_artifact(&event));
        if is_terminal {
            saw_terminal = true;
            break;
        }
    }

    if !saw_terminal {
        // The pipeline closed its channel without a terminal event.
        // Record a closing artifact so the receipt is complete.
        artifacts.push(LoanArtifact {
            kind: "summary".into(),
            content: json!({
                "status": "channel_closed_without_terminal",
                "duration_ms": started.elapsed().as_millis() as u64,
            }),
            ts: chrono::Utc::now().to_rfc3339(),
            tokens_in: 0,
            tokens_out: 0,
            cost_usd: 0.0,
        });
    }

    let receipt = sign_receipt(
        &signer,
        &body.borrower_run_id,
        &lender_run_id,
        &body.agent_id,
        artifacts,
    );
    Ok(Json(receipt))
}

/// Translate one `OneShotEvent` into the lender's [`LoanArtifact`]
/// wire shape. Mini-agent-emitted events use richer kinds so the
/// borrower can filter (`kind == "file"` etc.) without parsing the
/// full event payload.
fn event_to_artifact(event: &OneShotEvent) -> LoanArtifact {
    let ts = chrono::Utc::now().to_rfc3339();
    let (kind, content) = match event {
        OneShotEvent::FileWritten { path, lines } => (
            "file",
            json!({ "path": path, "lines": lines }),
        ),
        OneShotEvent::Complete {
            project_id,
            project_name,
            taste_score,
            files_count,
            duration_ms,
            app_url,
        } => (
            "summary",
            json!({
                "project_id": project_id,
                "project_name": project_name,
                "taste_score": taste_score,
                "files_count": files_count,
                "duration_ms": duration_ms,
                "app_url": app_url,
            }),
        ),
        OneShotEvent::Error { .. } => {
            ("error", serde_json::to_value(event).unwrap_or(json!({})))
        }
        _ => {
            // Everything else stays wrapped so the borrower has the
            // full event if they want it, but the kind stays the
            // stable `"message"` tag loan consumers check for.
            (
                "message",
                serde_json::to_value(event).unwrap_or(json!({})),
            )
        }
    };
    LoanArtifact {
        kind: kind.to_string(),
        content,
        ts,
        tokens_in: 0,
        tokens_out: 0,
        cost_usd: 0.0,
    }
}

// ---------------------------------------------------------------------------
// POST /federation/verify-receipt
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct VerifyRequest {
    pub receipt: LoanReceipt,
    /// Base64url-no-pad Ed25519 public key of the issuing peer.
    pub peer_public_key: String,
}

pub async fn verify(Json(body): Json<VerifyRequest>) -> ApiResult<Json<serde_json::Value>> {
    match verify_receipt(&body.receipt, &body.peer_public_key) {
        Ok(ok) => Ok(Json(json!({
            "valid": ok,
            "agent_id": body.receipt.agent_id,
            "lender_run_id": body.receipt.lender_run_id,
            "borrower_run_id": body.receipt.borrower_run_id,
            "artifact_count": body.receipt.artifacts.len(),
            "merkle_root": body.receipt.certificate.merkle_root,
        }))),
        Err(e) => Err(ApiError::BadRequest(format!("verify failed: {e}"))),
    }
}
