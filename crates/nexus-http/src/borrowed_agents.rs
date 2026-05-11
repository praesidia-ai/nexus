//! Borrowed Agents — cross-instance agent loan with Ed25519-signed
//! artifacts (NEXUS_MASTER_PLAN §10 / WS-9 federation USP).
//!
//! The primitive: a user on `alice.nexus.dev` browses a peer's
//! public [`BorrowableAgent`] catalogue, calls `POST /federation/
//! borrow`, and for the duration of one task the peer runs the agent
//! under the peer's API keys + policy and streams back artifacts
//! that are Ed25519-signed with the peer's key. Every artifact the
//! borrower keeps carries a verifiable chain of custody.
//!
//! Why this is defensible:
//!
//! - Nobody else has a peer protocol for agents. Cursor, Claude
//!   Code, OpenCode, Aider — all single-instance.
//! - Keeps the lending deployment in control of its own costs,
//!   rate limits, and model choices.
//! - The signed artifact chain makes compliance-conscious buyers
//!   comfortable accepting agent output from an external Nexus.
//!
//! This module keeps the **signing + verification + serialisation**
//! logic. Transport (HTTP fetches between peers) lives in
//! `handlers/borrowed_agents.rs`; that handler is the thin layer on
//! top that speaks JSON over the wire.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::trust::{TrustLogBuilder, TrustCertificate, TrustSigner, verify_certificate};

/// One borrowable-agent entry advertised by a peer. Mirrors the shape
/// a Nexus instance would publish at
/// `GET /federation/borrowable-agents`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BorrowableAgent {
    /// Peer-scoped id — stable across loans so the borrower can
    /// reliably re-borrow "Nova" from `alice.nexus.dev`.
    pub id: String,
    /// Human-readable label shown in the borrower's UI.
    pub name: String,
    /// Conductor-roster slug when the borrowable agent is one of the
    /// 10 named personas (nova/atlas/kai/…). Empty string otherwise
    /// so plain JSON round-trips cleanly.
    #[serde(default)]
    pub conductor: String,
    /// Short capability summary — free-form English.
    pub description: String,
    /// Peer's Nexus version at advertisement time.
    pub nexus_version: String,
    /// Per-call rate cap the lender will enforce (requests/minute).
    pub rate_limit_rpm: u32,
    /// Per-call dollar cap the lender will enforce. 0.0 = unlimited.
    pub max_cost_usd_per_call: f64,
}

/// Payload the borrower POSTs to start a loan. Narrow on purpose —
/// the full task shape lives on Nexus-HTTP's usual `/oneshot` route;
/// the loan envelope just tells the lender **who** is borrowing and
/// which agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoanRequest {
    /// Agent id from the lender's [`BorrowableAgent`] catalogue.
    pub agent_id: String,
    /// Opaque prompt the borrower wants the agent to act on.
    pub prompt: String,
    /// Borrower-side run id for correlation. Artifacts echo it back.
    pub borrower_run_id: String,
    /// Public URL of the borrower's Nexus instance — lets the
    /// lender verify the borrower's own identity via `/.well-known/
    /// nexus-trust.json` if the deployment wants mutual auth.
    pub borrower_url: String,
}

/// One artifact the lender returns to the borrower. Everything
/// meaningful (kind/content/cost/tokens) is hashed into the Merkle
/// log — the borrower can tell if anything between them and the
/// peer lender has been tampered with.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoanArtifact {
    /// Stable kind tag. Defined shapes: `"message"`, `"file"`,
    /// `"tool_call"`, `"tool_result"`, `"summary"`.
    pub kind: String,
    /// Opaque content. For `"file"` artifacts this is the full
    /// file body; for `"message"` it's the LLM's text reply.
    pub content: serde_json::Value,
    /// Lender-side timestamp for ordering + replay.
    pub ts: String,
    /// Tokens + dollars the lender spent producing this artifact.
    /// Recorded so the borrower can see the lender's cost without
    /// trusting the wire alone — the Merkle root covers these.
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub cost_usd: f64,
}

/// Response envelope returned by the lender when a loan completes.
/// The certificate is a [`TrustCertificate`] (same primitive used
/// for Agent TV runs) whose leaves cover every artifact in order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoanReceipt {
    pub borrower_run_id: String,
    pub lender_run_id: String,
    pub agent_id: String,
    pub artifacts: Vec<LoanArtifact>,
    pub certificate: TrustCertificate,
}

/// Build + sign a receipt for a completed loan.
pub fn sign_receipt(
    signer: &TrustSigner,
    borrower_run_id: &str,
    lender_run_id: &str,
    agent_id: &str,
    artifacts: Vec<LoanArtifact>,
) -> LoanReceipt {
    let mut log = TrustLogBuilder::new(lender_run_id);
    // Seed leaf carries the borrower's run id + agent id — any swap
    // of the receipt's envelope invalidates the Merkle root.
    log.append(
        "loan_started",
        chrono::Utc::now().to_rfc3339(),
        &serde_json::json!({
            "borrower_run_id": borrower_run_id,
            "agent_id": agent_id,
        }),
    );
    for (i, art) in artifacts.iter().enumerate() {
        log.append(
            "loan_artifact",
            art.ts.clone(),
            &serde_json::json!({
                "seq": i,
                "kind": art.kind,
                "content_hash": sha256_hex(&art.content),
                "tokens_in": art.tokens_in,
                "tokens_out": art.tokens_out,
                "cost_usd": art.cost_usd,
            }),
        );
    }
    let certificate = log.finalize(signer);
    LoanReceipt {
        borrower_run_id: borrower_run_id.to_string(),
        lender_run_id: lender_run_id.to_string(),
        agent_id: agent_id.to_string(),
        artifacts,
        certificate,
    }
}

/// Verify an incoming receipt **without** trusting the sender.
/// Checks that:
///
/// 1. The certificate's Merkle root is actually derivable from the
///    declared leaves (catches tampering with artifact content).
/// 2. The Ed25519 signature matches the supplied peer public key.
/// 3. Every artifact's content hash still matches the hash stored
///    in the Merkle log (catches swapping one artifact for another
///    while keeping the root nominally intact).
pub fn verify_receipt(receipt: &LoanReceipt, peer_public_key_b64: &str) -> Result<bool, String> {
    if receipt.certificate.leaves.is_empty() {
        return Ok(false);
    }
    // Signature + root internally consistent.
    let sig_ok = verify_certificate(&receipt.certificate, peer_public_key_b64)
        .map_err(|e| format!("verify cert: {e}"))?;
    if !sig_ok {
        return Ok(false);
    }
    // Re-hash every artifact and match its declared payload_hash.
    // Leaves are: [loan_started, artifact_0, artifact_1, …]. Artifacts
    // start at leaf index 1.
    let artifact_leaves = &receipt.certificate.leaves[1..];
    if artifact_leaves.len() != receipt.artifacts.len() {
        return Ok(false);
    }
    for (i, (art, leaf)) in receipt
        .artifacts
        .iter()
        .zip(artifact_leaves.iter())
        .enumerate()
    {
        let expected = serde_json::json!({
            "seq": i,
            "kind": art.kind,
            "content_hash": sha256_hex(&art.content),
            "tokens_in": art.tokens_in,
            "tokens_out": art.tokens_out,
            "cost_usd": art.cost_usd,
        });
        let canon = canonicalize(&expected);
        let hash = hex::encode(Sha256::digest(canon.as_bytes()));
        if hash != leaf.payload_hash {
            return Ok(false);
        }
    }
    Ok(true)
}

fn sha256_hex(v: &serde_json::Value) -> String {
    let canon = canonicalize(v);
    hex::encode(Sha256::digest(canon.as_bytes()))
}

fn canonicalize(v: &serde_json::Value) -> String {
    fn sort(v: &serde_json::Value) -> serde_json::Value {
        match v {
            serde_json::Value::Object(m) => {
                let mut keys: Vec<&String> = m.keys().collect();
                keys.sort();
                let mut out = serde_json::Map::new();
                for k in keys {
                    out.insert(k.clone(), sort(&m[k]));
                }
                serde_json::Value::Object(out)
            }
            serde_json::Value::Array(a) => {
                serde_json::Value::Array(a.iter().map(sort).collect())
            }
            _ => v.clone(),
        }
    }
    serde_json::to_string(&sort(v)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make_artifact(kind: &str, content: serde_json::Value) -> LoanArtifact {
        LoanArtifact {
            kind: kind.into(),
            content,
            ts: "2026-04-23T10:00:00Z".into(),
            tokens_in: 100,
            tokens_out: 200,
            cost_usd: 0.003,
        }
    }

    #[test]
    fn receipt_roundtrip_signs_and_verifies() {
        let dir = tempdir().unwrap();
        let signer = TrustSigner::load_or_generate(dir.path()).unwrap();
        let pk = base64_url_nopad(&signer.verifying.to_bytes());
        let receipt = sign_receipt(
            &signer,
            "bob-run-1",
            "alice-run-7",
            "nova",
            vec![
                make_artifact("message", serde_json::json!({"text": "ok"})),
                make_artifact("file", serde_json::json!({"path": "main.rs", "body": "fn main(){}"})),
            ],
        );
        let ok = verify_receipt(&receipt, &pk).unwrap();
        assert!(ok);
    }

    #[test]
    fn swapping_an_artifact_after_signing_fails_verification() {
        let dir = tempdir().unwrap();
        let signer = TrustSigner::load_or_generate(dir.path()).unwrap();
        let pk = base64_url_nopad(&signer.verifying.to_bytes());
        let mut receipt = sign_receipt(
            &signer,
            "bob-run-1",
            "alice-run-7",
            "nova",
            vec![make_artifact("message", serde_json::json!({"text": "ok"}))],
        );
        // Tamper with the artifact content after signing — root still
        // nominally matches because the leaf hash is unchanged (we
        // don't re-sign) but verify_receipt re-hashes and catches it.
        receipt.artifacts[0].content = serde_json::json!({"text": "EVIL"});
        let ok = verify_receipt(&receipt, &pk).unwrap();
        assert!(!ok);
    }

    #[test]
    fn wrong_pubkey_fails_verification() {
        let a = tempdir().unwrap();
        let b = tempdir().unwrap();
        let alice = TrustSigner::load_or_generate(a.path()).unwrap();
        let bob = TrustSigner::load_or_generate(b.path()).unwrap();
        let receipt = sign_receipt(
            &alice,
            "bob-run-1",
            "alice-run-7",
            "nova",
            vec![make_artifact("message", serde_json::json!({"text": "ok"}))],
        );
        let bob_pk = base64_url_nopad(&bob.verifying.to_bytes());
        let ok = verify_receipt(&receipt, &bob_pk).unwrap();
        assert!(!ok);
    }

    #[test]
    fn empty_loan_rejected() {
        let dir = tempdir().unwrap();
        let signer = TrustSigner::load_or_generate(dir.path()).unwrap();
        let pk = base64_url_nopad(&signer.verifying.to_bytes());
        let receipt = LoanReceipt {
            borrower_run_id: "bob".into(),
            lender_run_id: "alice".into(),
            agent_id: "nova".into(),
            artifacts: vec![],
            certificate: TrustCertificate {
                run_id: "alice".into(),
                alg: "ed25519".into(),
                key_id: "x".into(),
                leaves: vec![],
                merkle_root: "0".into(),
                signature: "0".into(),
            },
        };
        let ok = verify_receipt(&receipt, &pk).unwrap();
        assert!(!ok);
    }

    #[test]
    fn artifact_count_mismatch_fails_verification() {
        let dir = tempdir().unwrap();
        let signer = TrustSigner::load_or_generate(dir.path()).unwrap();
        let pk = base64_url_nopad(&signer.verifying.to_bytes());
        let mut receipt = sign_receipt(
            &signer,
            "bob-run-1",
            "alice-run-7",
            "nova",
            vec![
                make_artifact("message", serde_json::json!({"text": "ok"})),
                make_artifact("message", serde_json::json!({"text": "two"})),
            ],
        );
        // Attacker truncates the artifact list — leaves still claim 2.
        receipt.artifacts.pop();
        let ok = verify_receipt(&receipt, &pk).unwrap();
        assert!(!ok);
    }

    fn base64_url_nopad(bytes: &[u8]) -> String {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
        URL_SAFE_NO_PAD.encode(bytes)
    }
}
