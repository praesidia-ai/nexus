//! At-rest encryption for secrets (vault values + LLM API keys).
//!
//! Design:
//! - AES-256-GCM via the `aes-gcm` crate.
//! - Key derivation (v2, current): Argon2id(NEXUS_ENCRYPTION_KEY, fixed salt).
//!   A weak env-var value (e.g. "password123") is far more expensive to
//!   brute-force against a database dump than under a single SHA-256 pass.
//! - Key derivation (v1, legacy): SHA-256(NEXUS_ENCRYPTION_KEY). Decryption
//!   still supports v1 so rows written before the upgrade remain readable;
//!   any rewrite transparently re-encrypts as v2.
//! - Storage format: `"enc:v1:"` or `"enc:v2:"` + base64(nonce || ciphertext).
//!   Values without an `"enc:"` prefix are treated as legacy plaintext.
//!
//! No key rotation yet — add when multi-tenant KMS support lands.

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    AeadCore, Aes256Gcm, Key, Nonce,
};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD, Engine};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

const ENCRYPTED_PREFIX_V1: &str = "enc:v1:";
const ENCRYPTED_PREFIX_V2: &str = "enc:v2:";
const ENCRYPTED_PREFIX_V3: &str = "enc:v3:";
/// Dev-only fallback. Emits a one-time warning to operators.
const DEV_FALLBACK: &[u8] = b"nexus-dev-encryption-key-change-me";

/// v2 fixed salt — global to every Nexus install. New rows are no longer
/// written with v2 when a per-install salt (`NEXUS_ENCRYPTION_SALT`) is
/// available; we keep this constant only to decrypt legacy data.
const ARGON2_SALT_V2: &[u8; 32] = b"nexus-store::crypto::v2::argon2s";

/// Password material (possibly the dev fallback) as bytes.
///
/// Per ADR-001, behaviour is gated on `NEXUS_PRODUCTION` at runtime rather
/// than at build time so the same release binary can run dev and prod:
///
/// - `NEXUS_ENCRYPTION_KEY` set → use it.
/// - Otherwise, if `NEXUS_PRODUCTION=1` → panic (operator opted into strict mode).
/// - Otherwise → use the dev fallback and log a one-time WARN.
///
/// The persistent path (`secrets.toml` via `state::ensure_auto_secrets`)
/// sets `NEXUS_ENCRYPTION_KEY` before this is called on a normal boot, so
/// the dev fallback only fires when the data dir is unwritable AND production
/// mode is off.
fn password_material() -> Vec<u8> {
    if let Ok(s) = std::env::var("NEXUS_ENCRYPTION_KEY") {
        if !s.is_empty() {
            return s.into_bytes();
        }
    }

    let is_production = matches!(
        std::env::var("NEXUS_PRODUCTION").ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("True")
    );
    if is_production {
        panic!(
            "NEXUS_ENCRYPTION_KEY is not set and NEXUS_PRODUCTION=1. \
             Refusing to derive encryption material from the built-in dev fallback \
             in production. Set NEXUS_ENCRYPTION_KEY or ensure <data_dir>/secrets.toml \
             is writable so a persistent key can be generated."
        );
    }

    use std::sync::OnceLock;
    static WARNED: OnceLock<()> = OnceLock::new();
    WARNED.get_or_init(|| {
        tracing::warn!(
            "NEXUS_ENCRYPTION_KEY unset and no persisted key — using built-in dev \
             fallback. Encrypted data will NOT survive a binary upgrade or move \
             between machines. Set NEXUS_ENCRYPTION_KEY or run with a writable \
             data dir before storing real secrets."
        );
    });
    DEV_FALLBACK.to_vec()
}

/// Decode the per-install salt from `NEXUS_ENCRYPTION_SALT` (hex string of
/// `>= 16` bytes). Returns `None` when not configured — callers fall back to
/// v2 (legacy fixed salt) so existing deployments keep booting.
fn salt_v3() -> Option<Vec<u8>> {
    let raw = std::env::var("NEXUS_ENCRYPTION_SALT").ok()?;
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let bytes = hex::decode(raw).ok()?;
    if bytes.len() < 16 {
        return None;
    }
    Some(bytes)
}

/// Whether `NEXUS_ENCRYPTION_SALT` is set to a usable value (>= 16 bytes hex).
pub fn is_per_install_salt_configured() -> bool {
    salt_v3().is_some()
}

/// Argon2id-stretched 32-byte key (v3 — per-install salt).
fn derive_key_v3() -> Option<[u8; 32]> {
    static CACHED: OnceLock<[u8; 32]> = OnceLock::new();
    let salt = salt_v3()?;
    Some(*CACHED.get_or_init(|| {
        let params = Params::new(19_456, 2, 1, Some(32)).expect("argon2 params");
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let pw = password_material();
        let mut out = [0u8; 32];
        argon2
            .hash_password_into(&pw, &salt, &mut out)
            .expect("argon2 must succeed on configured salt + non-empty input");
        out
    }))
}

/// Argon2id-stretched 32-byte key (v2 — legacy global salt). Cached so we do
/// not pay the KDF cost on every encrypt/decrypt — the password material is
/// stable for the lifetime of the process.
fn derive_key_v2() -> [u8; 32] {
    static CACHED: OnceLock<[u8; 32]> = OnceLock::new();
    *CACHED.get_or_init(|| {
        // Argon2id with OWASP 2023 "moderate" params (m=19 MiB, t=2, p=1).
        let params = Params::new(19_456, 2, 1, Some(32)).expect("argon2 params");
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let pw = password_material();
        let mut out = [0u8; 32];
        argon2
            .hash_password_into(&pw, ARGON2_SALT_V2, &mut out)
            .expect("argon2 must succeed on fixed salt + non-empty input");
        out
    })
}

/// Legacy SHA-256 key (v1). Retained only to decrypt existing rows.
fn derive_key_v1() -> [u8; 32] {
    let pw = password_material();
    let mut hasher = Sha256::new();
    hasher.update(&pw);
    let result = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

/// Whether `NEXUS_ENCRYPTION_KEY` is set to a non-empty value.
pub fn is_production_key_configured() -> bool {
    std::env::var("NEXUS_ENCRYPTION_KEY")
        .map(|s| !s.is_empty())
        .unwrap_or(false)
}

/// Encrypt a plaintext string. Writes `"enc:v3:<base64>"` when a per-install
/// salt is configured (the safe path); falls back to `"enc:v2:<base64>"` on
/// installs that have not been migrated yet so they keep working.
pub fn encrypt(plaintext: &str) -> Result<String, String> {
    let (key_bytes, prefix) = match derive_key_v3() {
        Some(k) => (k, ENCRYPTED_PREFIX_V3),
        None => (derive_key_v2(), ENCRYPTED_PREFIX_V2),
    };
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ct = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| format!("encrypt failed: {e}"))?;
    let mut buf = Vec::with_capacity(12 + ct.len());
    buf.extend_from_slice(nonce.as_slice());
    buf.extend_from_slice(&ct);
    Ok(format!("{prefix}{}", STANDARD.encode(buf)))
}

/// Decrypt a stored value. Supports v1 (legacy SHA-256), v2 (legacy fixed
/// Argon2id salt), and v3 (per-install Argon2id salt). Values without an
/// `"enc:"` prefix are returned unchanged (legacy plaintext). Callers
/// should re-write decrypted values through [`encrypt`] so they upgrade to
/// v3 on next write.
pub fn decrypt(stored: &str) -> Result<String, String> {
    let (payload, key_bytes) = if let Some(p) = stored.strip_prefix(ENCRYPTED_PREFIX_V3) {
        let key = derive_key_v3()
            .ok_or_else(|| "enc:v3 row found but NEXUS_ENCRYPTION_SALT is not configured".to_string())?;
        (p, key)
    } else if let Some(p) = stored.strip_prefix(ENCRYPTED_PREFIX_V2) {
        (p, derive_key_v2())
    } else if let Some(p) = stored.strip_prefix(ENCRYPTED_PREFIX_V1) {
        (p, derive_key_v1())
    } else {
        return Ok(stored.to_string());
    };

    let raw = STANDARD
        .decode(payload)
        .map_err(|e| format!("base64 decode failed: {e}"))?;
    if raw.len() < 12 {
        return Err("ciphertext too short".into());
    }
    let (nonce_bytes, ct) = raw.split_at(12);
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(nonce_bytes);
    let pt = cipher
        .decrypt(nonce, ct)
        .map_err(|e| format!("decrypt failed: {e}"))?;
    String::from_utf8(pt).map_err(|e| format!("utf-8 decode failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_with_env_key() {
        // SAFETY: single-threaded test-time env mutation.
        unsafe {
            std::env::set_var("NEXUS_ENCRYPTION_KEY", "unit-test-key-abc123");
        }
        let ct = encrypt("super-secret-value").unwrap();
        assert!(ct.starts_with("enc:v2:"));
        let pt = decrypt(&ct).unwrap();
        assert_eq!(pt, "super-secret-value");
    }

    #[test]
    fn legacy_plaintext_passes_through() {
        let pt = decrypt("legacy-value").unwrap();
        assert_eq!(pt, "legacy-value");
    }

    #[test]
    fn ciphertexts_are_non_deterministic() {
        unsafe {
            std::env::set_var("NEXUS_ENCRYPTION_KEY", "another-test-key");
        }
        let a = encrypt("same-plaintext").unwrap();
        let b = encrypt("same-plaintext").unwrap();
        assert_ne!(a, b, "nonce must randomize ciphertexts");
    }
}
