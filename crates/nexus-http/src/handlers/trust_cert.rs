//! HTTP surface for the Trust Certificate system.
//!
//! - `GET /.well-known/nexus-trust.json` → this deployment's public
//!   Ed25519 identity (the `/trust` verifier across any Nexus instance
//!   starts here)
//! - `GET /trust/cert/:run_id` → signed certificate for a single run
//! - `POST /trust/verify` → verify an arbitrary certificate against a
//!   caller-supplied public key (powers the `/trust` verifier page)

use std::sync::Arc;

use axum::{extract::{Path, State}, Json};
use serde::Deserialize;
use serde_json::json;

use crate::{
    error::{ApiError, ApiResult},
    state::AppState,
    trust::{verify_certificate, TrustCertificate, TrustLogBuilder, TrustSigner},
};

/// `GET /.well-known/nexus-trust.json` — public key of this instance.
/// Listed in `PUBLIC_PATHS` so anyone can fetch it.
#[tracing::instrument(skip(app))]
pub async fn well_known_trust(
    State(app): State<Arc<AppState>>,
) -> ApiResult<Json<serde_json::Value>> {
    let signer = TrustSigner::load_or_generate(&app.data_dir)
        .map_err(|e| ApiError::Internal(format!("load signer: {e}")))?;
    Ok(Json(serde_json::to_value(signer.identity()).unwrap_or(json!({}))))
}

/// `GET /trust/cert/:run_id` — reconstruct the trust cert for a
/// completed run from its `agent_tv_events` log and sign it with the
/// current instance key. The cert is self-contained; the caller can
/// verify it offline using the public key at `/.well-known/
/// nexus-trust.json`.
#[tracing::instrument(skip(app))]
pub async fn get_cert(
    State(app): State<Arc<AppState>>,
    Path(run_id): Path<String>,
) -> ApiResult<Json<TrustCertificate>> {
    // Runs must exist + be at least unlisted to be verifiable. Private
    // runs don't hand out certs to anonymous callers.
    let visibility: String = {
        let db = app.db.lock().await;
        db.query_row(
            "SELECT visibility FROM agent_tv_runs WHERE id = ?1",
            [&run_id],
            |row| row.get(0),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                ApiError::NotFound(format!("run {run_id} not found"))
            }
            other => ApiError::Internal(format!("load run: {other}")),
        })?
    };
    if visibility == "private" {
        return Err(ApiError::NotFound(format!("run {run_id} not found")));
    }

    // Read + canonicalise every event into a TrustLogBuilder.
    let mut builder = TrustLogBuilder::new(&run_id);
    {
        let db = app.db.lock().await;
        let mut stmt = db
            .prepare(
                "SELECT event_type, payload, ts
                 FROM agent_tv_events
                 WHERE run_id = ?1 ORDER BY seq ASC",
            )
            .map_err(|e| ApiError::Internal(format!("prepare: {e}")))?;
        let rows = stmt
            .query_map([&run_id], |row| {
                let et: String = row.get(0)?;
                let payload_str: String = row.get(1)?;
                let ts: String = row.get(2)?;
                Ok((et, payload_str, ts))
            })
            .map_err(|e| ApiError::Internal(format!("query: {e}")))?;
        for r in rows.flatten() {
            let payload: serde_json::Value =
                serde_json::from_str(&r.1).unwrap_or(serde_json::Value::Null);
            builder.append(r.0, r.2, &payload);
        }
    }

    let signer = TrustSigner::load_or_generate(&app.data_dir)
        .map_err(|e| ApiError::Internal(format!("load signer: {e}")))?;
    let cert = builder.finalize(&signer);

    // Denormalise onto the agent_tv_runs row so the replay endpoint
    // can surface merkle_root + signature inline without recomputing.
    {
        let db = app.db.lock().await;
        let _ = db.execute(
            "UPDATE agent_tv_runs SET merkle_root = ?1, signature = ?2 WHERE id = ?3",
            rusqlite::params![&cert.merkle_root, &cert.signature, &run_id],
        );
    }

    Ok(Json(cert))
}

#[derive(Debug, Deserialize)]
pub struct VerifyRequest {
    pub certificate: TrustCertificate,
    /// Base64url-no-pad public key to verify against. Any deployment's
    /// key is accepted; callers typically fetch it from the issuing
    /// instance's `/.well-known/nexus-trust.json`.
    pub public_key: String,
}

/// `POST /trust/verify` — verify a certificate against a supplied
/// public key. Returns `{"valid": true|false, ...}`.
#[tracing::instrument(skip(body))]
pub async fn verify(
    Json(body): Json<VerifyRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    match verify_certificate(&body.certificate, &body.public_key) {
        Ok(ok) => Ok(Json(json!({
            "valid": ok,
            "run_id": body.certificate.run_id,
            "alg": body.certificate.alg,
            "key_id": body.certificate.key_id,
            "merkle_root": body.certificate.merkle_root,
            "leaf_count": body.certificate.leaves.len(),
        }))),
        Err(e) => Err(ApiError::BadRequest(format!("verification failed: {e}"))),
    }
}
