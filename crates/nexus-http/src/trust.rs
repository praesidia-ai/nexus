//! Trust Certificate v1 (NEXUS_MASTER_PLAN §7).
//!
//! Every swarm / oneshot / wave run ends with a certificate:
//!
//! - An ordered **Merkle log** of the run's events (leaf hash = SHA-256
//!   over the canonicalised event payload)
//! - A single **Merkle root** over the log
//! - An **Ed25519 signature** of the root by this deployment's key
//!
//! The certificate is self-contained — any third party can verify it
//! against the deployment's public key published at
//! `/.well-known/nexus-trust.json`, offline, without talking to the
//! originating Nexus again. Storing the `merkle_root` + `signature`
//! on the `agent_tv_runs` row means `GET /tv/:runId` always includes
//! the certificate; a `GET /trust` page can verify any cert from any
//! deployment given a public key.
//!
//! Storage layout: the Ed25519 keypair lives at
//! `<data_dir>/trust/ed25519.key` (32 bytes, `0o600` on Unix). The
//! matching public-key JWK lives at
//! `<data_dir>/trust/ed25519.pub.json` for the `/.well-known` handler
//! to serve directly. The keypair is generated at first boot and
//! never rotated automatically — operators wanting rotation swap the
//! file and restart.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ed25519_dalek::{
    Signature, Signer, SigningKey, Verifier, VerifyingKey, SECRET_KEY_LENGTH,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Public identity of this Nexus instance, served as JSON at
/// `/.well-known/nexus-trust.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustIdentity {
    /// Algorithm tag — always `ed25519` in v1.
    pub alg: String,
    /// Public key, base64url-no-pad (32 bytes).
    pub public_key: String,
    /// Opaque key id. Today we use the hex of the public key's first
    /// 8 bytes; a future rotation scheme would mint a fresh id.
    pub key_id: String,
    /// Self-reported instance label — purely informational.
    #[serde(default)]
    pub instance: Option<String>,
}

/// A single entry in the Merkle log. Intentionally narrow — the
/// `payload_hash` is the only thing the root depends on, so
/// `event_type` / `seq` / `ts` drift can't invalidate historical
/// certificates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustLeaf {
    pub seq: u64,
    pub event_type: String,
    pub ts: String,
    /// SHA-256 over the canonicalised event payload, hex-encoded.
    pub payload_hash: String,
}

/// The shareable certificate attached to a run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustCertificate {
    pub run_id: String,
    pub alg: String,
    pub key_id: String,
    pub leaves: Vec<TrustLeaf>,
    /// Hex-encoded SHA-256 Merkle root over the log.
    pub merkle_root: String,
    /// Base64url-no-pad signature of the root bytes.
    pub signature: String,
}

/// A builder that accumulates events, deterministically hashes them
/// into a Merkle log on finalisation, and signs the root.
pub struct TrustLogBuilder {
    run_id: String,
    leaves: Vec<TrustLeaf>,
}

impl TrustLogBuilder {
    pub fn new(run_id: impl Into<String>) -> Self {
        Self {
            run_id: run_id.into(),
            leaves: Vec::new(),
        }
    }

    /// Append one event. `payload` is canonicalised (JSON with sorted
    /// keys) before hashing so the Merkle root is reproducible.
    pub fn append(&mut self, event_type: impl Into<String>, ts: impl Into<String>, payload: &serde_json::Value) {
        let canon = canonicalize_json(payload);
        let payload_hash = hex::encode(Sha256::digest(canon.as_bytes()));
        let seq = self.leaves.len() as u64;
        self.leaves.push(TrustLeaf {
            seq,
            event_type: event_type.into(),
            ts: ts.into(),
            payload_hash,
        });
    }

    /// Seal the log — produce a signed certificate.
    pub fn finalize(self, signer: &TrustSigner) -> TrustCertificate {
        let merkle_root = merkle_root_hex(&self.leaves);
        let sig = signer.signing.sign(merkle_root.as_bytes());
        TrustCertificate {
            run_id: self.run_id,
            alg: "ed25519".to_string(),
            key_id: signer.key_id.clone(),
            leaves: self.leaves,
            merkle_root,
            signature: base64_url_nopad(&sig.to_bytes()),
        }
    }
}

/// Persistent identity used to sign certificates.
pub struct TrustSigner {
    signing: SigningKey,
    pub verifying: VerifyingKey,
    pub key_id: String,
}

impl TrustSigner {
    /// Load the signer from `data_dir/trust/ed25519.key`, generating a
    /// fresh keypair on first boot. The keypair file is chmod'd to
    /// `0o600` on Unix (matches the `secrets.toml` pattern).
    pub fn load_or_generate(data_dir: &Path) -> Result<Self> {
        let trust_dir = data_dir.join("trust");
        std::fs::create_dir_all(&trust_dir)
            .with_context(|| format!("create {}", trust_dir.display()))?;
        let key_path = trust_dir.join("ed25519.key");
        let pub_path = trust_dir.join("ed25519.pub.json");

        let signing = if key_path.exists() {
            let raw = std::fs::read(&key_path)
                .with_context(|| format!("read {}", key_path.display()))?;
            if raw.len() != SECRET_KEY_LENGTH {
                return Err(anyhow::anyhow!(
                    "bad key length at {}: expected {} got {}",
                    key_path.display(),
                    SECRET_KEY_LENGTH,
                    raw.len()
                ));
            }
            let mut bytes = [0u8; SECRET_KEY_LENGTH];
            bytes.copy_from_slice(&raw);
            SigningKey::from_bytes(&bytes)
        } else {
            let mut rng = rand::rngs::OsRng;
            let sk = SigningKey::generate(&mut rng);
            std::fs::write(&key_path, sk.to_bytes())
                .with_context(|| format!("write {}", key_path.display()))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ =
                    std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600));
            }
            sk
        };

        let verifying = signing.verifying_key();
        let pk_bytes = verifying.to_bytes();
        let key_id = hex::encode(&pk_bytes[..8]);

        // Always rewrite the advertised public-key JSON — cheap, and
        // guarantees the file tracks the active key.
        let identity = TrustIdentity {
            alg: "ed25519".to_string(),
            public_key: base64_url_nopad(&pk_bytes),
            key_id: key_id.clone(),
            instance: std::env::var("NEXUS_INSTANCE_LABEL").ok(),
        };
        std::fs::write(&pub_path, serde_json::to_vec_pretty(&identity)?)
            .with_context(|| format!("write {}", pub_path.display()))?;

        Ok(Self {
            signing,
            verifying,
            key_id,
        })
    }

    pub fn identity(&self) -> TrustIdentity {
        TrustIdentity {
            alg: "ed25519".to_string(),
            public_key: base64_url_nopad(&self.verifying.to_bytes()),
            key_id: self.key_id.clone(),
            instance: std::env::var("NEXUS_INSTANCE_LABEL").ok(),
        }
    }

    pub fn public_key_path(data_dir: &Path) -> PathBuf {
        data_dir.join("trust").join("ed25519.pub.json")
    }
}

/// Verify a certificate against a caller-supplied public key.
/// Intended for the public `/trust` verifier UI — accepts any
/// deployment's public key, not just ours.
pub fn verify_certificate(cert: &TrustCertificate, public_key_b64: &str) -> Result<bool> {
    if cert.alg != "ed25519" {
        return Err(anyhow::anyhow!("unsupported alg: {}", cert.alg));
    }
    let pk_bytes = base64_url_decode(public_key_b64)?;
    if pk_bytes.len() != 32 {
        return Err(anyhow::anyhow!("public key must be 32 bytes"));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&pk_bytes);
    let vk = VerifyingKey::from_bytes(&arr).context("decode verifying key")?;

    let sig_bytes = base64_url_decode(&cert.signature)?;
    if sig_bytes.len() != 64 {
        return Err(anyhow::anyhow!("signature must be 64 bytes"));
    }
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sig_bytes);
    let sig = Signature::from_bytes(&sig_arr);

    // Re-compute the Merkle root from the leaves — this catches any
    // tampering with the leaf list after signing.
    let recomputed_root = merkle_root_hex(&cert.leaves);
    if recomputed_root != cert.merkle_root {
        return Ok(false);
    }

    Ok(vk.verify(cert.merkle_root.as_bytes(), &sig).is_ok())
}

// ---------------------------------------------------------------------------
// Hashing + encoding helpers
// ---------------------------------------------------------------------------

fn merkle_root_hex(leaves: &[TrustLeaf]) -> String {
    if leaves.is_empty() {
        return hex::encode(Sha256::digest(b""));
    }
    let mut layer: Vec<[u8; 32]> = leaves
        .iter()
        .map(|l| {
            let mut h = Sha256::new();
            h.update(l.seq.to_be_bytes());
            h.update(l.event_type.as_bytes());
            h.update(l.ts.as_bytes());
            h.update(l.payload_hash.as_bytes());
            let out = h.finalize();
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&out);
            arr
        })
        .collect();

    // Classic binary Merkle — promote the last leaf if the layer is
    // odd-sized (matches RFC 6962 §2.1 with duplication semantics).
    while layer.len() > 1 {
        let mut next: Vec<[u8; 32]> = Vec::with_capacity(layer.len().div_ceil(2));
        for pair in layer.chunks(2) {
            let mut h = Sha256::new();
            h.update(pair[0]);
            h.update(pair.get(1).unwrap_or(&pair[0]));
            let out = h.finalize();
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&out);
            next.push(arr);
        }
        layer = next;
    }
    hex::encode(layer[0])
}

fn canonicalize_json(v: &serde_json::Value) -> String {
    // RFC 8785-ish: recursively sort object keys so hash is
    // reproducible across producers. Arrays are order-sensitive.
    fn sort(v: &serde_json::Value) -> serde_json::Value {
        match v {
            serde_json::Value::Object(map) => {
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                let mut out = serde_json::Map::new();
                for k in keys {
                    out.insert(k.clone(), sort(&map[k]));
                }
                serde_json::Value::Object(out)
            }
            serde_json::Value::Array(items) => {
                serde_json::Value::Array(items.iter().map(sort).collect())
            }
            _ => v.clone(),
        }
    }
    serde_json::to_string(&sort(v)).unwrap_or_default()
}

fn base64_url_nopad(bytes: &[u8]) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    URL_SAFE_NO_PAD.encode(bytes)
}

fn base64_url_decode(s: &str) -> Result<Vec<u8>> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    URL_SAFE_NO_PAD
        .decode(s.as_bytes())
        .context("base64url decode")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn signer_generates_and_reloads_same_key() {
        let dir = tempdir().unwrap();
        let s1 = TrustSigner::load_or_generate(dir.path()).unwrap();
        let s2 = TrustSigner::load_or_generate(dir.path()).unwrap();
        assert_eq!(s1.key_id, s2.key_id);
        assert_eq!(s1.verifying.to_bytes(), s2.verifying.to_bytes());
    }

    #[test]
    fn roundtrip_sign_and_verify() {
        let dir = tempdir().unwrap();
        let signer = TrustSigner::load_or_generate(dir.path()).unwrap();
        let mut b = TrustLogBuilder::new("r-1");
        b.append("swarm_started", "2026-04-22T10:00:00Z", &serde_json::json!({"n":1}));
        b.append("mini_complete", "2026-04-22T10:00:01Z", &serde_json::json!({"k":"fs.reader"}));
        b.append("complete", "2026-04-22T10:00:02Z", &serde_json::json!({"status":"completed"}));
        let cert = b.finalize(&signer);
        let ok = verify_certificate(&cert, &base64_url_nopad(&signer.verifying.to_bytes())).unwrap();
        assert!(ok);
    }

    #[test]
    fn tampering_with_leaf_fails_verification() {
        let dir = tempdir().unwrap();
        let signer = TrustSigner::load_or_generate(dir.path()).unwrap();
        let mut b = TrustLogBuilder::new("r-1");
        b.append("one", "t1", &serde_json::json!({"a":1}));
        b.append("two", "t2", &serde_json::json!({"b":2}));
        let mut cert = b.finalize(&signer);
        // Mutate a leaf after signing — root recomputation catches it.
        cert.leaves[0].payload_hash = "00".repeat(32);
        let ok = verify_certificate(&cert, &base64_url_nopad(&signer.verifying.to_bytes())).unwrap();
        assert!(!ok);
    }

    #[test]
    fn tampering_with_signature_fails_verification() {
        let dir = tempdir().unwrap();
        let signer = TrustSigner::load_or_generate(dir.path()).unwrap();
        let mut b = TrustLogBuilder::new("r-1");
        b.append("one", "t1", &serde_json::json!({"a":1}));
        let mut cert = b.finalize(&signer);
        // Flip a bit in the signature.
        let mut sig_bytes = base64_url_decode(&cert.signature).unwrap();
        sig_bytes[0] ^= 0x01;
        cert.signature = base64_url_nopad(&sig_bytes);
        let ok = verify_certificate(&cert, &base64_url_nopad(&signer.verifying.to_bytes())).unwrap();
        assert!(!ok);
    }

    #[test]
    fn canonicalize_is_key_order_stable() {
        let a = serde_json::json!({"b": 1, "a": 2});
        let b = serde_json::json!({"a": 2, "b": 1});
        assert_eq!(canonicalize_json(&a), canonicalize_json(&b));
    }

    #[test]
    fn empty_log_has_deterministic_root() {
        let leaves: Vec<TrustLeaf> = Vec::new();
        assert_eq!(merkle_root_hex(&leaves), hex::encode(Sha256::digest(b"")));
    }
}
