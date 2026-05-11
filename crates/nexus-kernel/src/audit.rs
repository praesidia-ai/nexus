//! Cryptographic audit trail for agent actions.
//!
//! Provides:
//! - **Ed25519 signing**: Every audit entry is signed with the node's key pair.
//! - **Merkle audit log**: Each entry hashes the previous entry's hash into its
//!   own hash, forming a tamper-evident hash chain (simplified Merkle log).
//! - **Content-addressable storage**: Entries are identified by their SHA-256 hash.
//! - **Disk persistence**: `AuditKeypair::load_or_create` and `AuditLog::open`
//!   persist the signing key (32-byte seed) and entries (JSON-Lines) under the
//!   data directory so the chain survives restarts.
//!
//! # Usage
//!
//! ```rust,ignore
//! let mut log = AuditLog::new();
//! let keypair = AuditKeypair::generate();
//! log.append("agent-1", "tool_call", json!({"tool": "web_search"}), &keypair);
//! assert!(log.verify_chain());
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::{Mutex, RwLock};
use tracing::{error, info, warn};

// ---------------------------------------------------------------------------
// Key pair management
// ---------------------------------------------------------------------------

/// Ed25519 key pair for signing audit entries.
pub struct AuditKeypair {
    signing_key: SigningKey,
}

impl AuditKeypair {
    /// Generate a fresh Ed25519 key pair (no persistence — tests only).
    pub fn generate() -> Self {
        Self {
            signing_key: SigningKey::generate(&mut OsRng),
        }
    }

    /// Load the signing key from `path`, or generate and persist a fresh one
    /// if the file does not exist.
    ///
    /// The on-disk format is the raw 32-byte secret seed. On Unix the file is
    /// created with mode 0600 so only the running user can read it; on other
    /// platforms permissions follow the umask.
    ///
    /// SECURITY: regenerating the keypair invalidates every prior signature.
    /// This loader exists specifically so the chain remains verifiable across
    /// restarts.
    pub async fn load_or_create(path: &Path) -> std::io::Result<Self> {
        match tokio::fs::read(path).await {
            Ok(bytes) => {
                let arr: [u8; 32] = bytes.as_slice().try_into().map_err(|_| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!(
                            "audit key file {} has wrong length: expected 32 bytes, got {}",
                            path.display(),
                            bytes.len()
                        ),
                    )
                })?;
                info!(path = %path.display(), "Loaded existing audit signing key");
                Ok(Self {
                    signing_key: SigningKey::from_bytes(&arr),
                })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let signing_key = SigningKey::generate(&mut OsRng);
                if let Some(parent) = path.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }
                tokio::fs::write(path, signing_key.to_bytes()).await?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let perms = std::fs::Permissions::from_mode(0o600);
                    if let Err(perm_err) = tokio::fs::set_permissions(path, perms).await {
                        warn!(
                            path = %path.display(),
                            error = %perm_err,
                            "Failed to set 0600 on audit key file — key is readable to other local users",
                        );
                    }
                }
                info!(path = %path.display(), "Generated and persisted fresh audit signing key");
                Ok(Self { signing_key })
            }
            Err(e) => Err(e),
        }
    }

    /// The public (verifying) key — share this to allow signature verification.
    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    /// Hex-encoded public key.
    pub fn public_key_hex(&self) -> String {
        hex::encode(self.verifying_key().to_bytes())
    }
}

// ---------------------------------------------------------------------------
// Audit entry
// ---------------------------------------------------------------------------

/// A single tamper-evident audit entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// SHA-256 content hash of this entry (includes prev_hash).
    pub hash: String,
    /// Hash of the previous entry (or all-zeros for the genesis entry).
    pub prev_hash: String,
    /// Timestamp of the action.
    pub timestamp: DateTime<Utc>,
    /// Agent that performed the action.
    pub agent_id: String,
    /// Action type (e.g., "tool_call", "llm_request", "file_write").
    pub action: String,
    /// Structured payload describing the action.
    pub payload: serde_json::Value,
    /// Ed25519 signature of `prev_hash || timestamp || agent_id || action || payload_json`.
    pub signature: String,
    /// Hex-encoded public key of the signer.
    pub signer_pubkey: String,
}

impl AuditEntry {
    /// Compute the canonical bytes to sign for this entry (excluding hash/signature fields).
    fn signable_bytes(
        prev_hash: &str,
        timestamp: &DateTime<Utc>,
        agent_id: &str,
        action: &str,
        payload: &serde_json::Value,
    ) -> Vec<u8> {
        let payload_json = payload.to_string();
        format!(
            "{}|{}|{}|{}|{}",
            prev_hash,
            timestamp.timestamp_nanos_opt().unwrap_or(0),
            agent_id,
            action,
            payload_json
        )
        .into_bytes()
    }

    /// Compute the SHA-256 content hash of this entry.
    fn compute_hash(signable: &[u8], signature: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(signable);
        hasher.update(signature.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Create and sign a new audit entry.
    pub fn new(
        prev_hash: &str,
        agent_id: impl Into<String>,
        action: impl Into<String>,
        payload: serde_json::Value,
        keypair: &AuditKeypair,
    ) -> Self {
        let timestamp = Utc::now();
        let agent_id = agent_id.into();
        let action = action.into();

        let signable = Self::signable_bytes(prev_hash, &timestamp, &agent_id, &action, &payload);

        let signature = keypair.signing_key.sign(&signable);
        let signature_hex = hex::encode(signature.to_bytes());

        let hash = Self::compute_hash(&signable, &signature_hex);

        Self {
            hash,
            prev_hash: prev_hash.to_string(),
            timestamp,
            agent_id,
            action,
            payload,
            signature: signature_hex,
            signer_pubkey: keypair.public_key_hex(),
        }
    }

    /// Verify the signature on this entry.
    pub fn verify_signature(&self) -> bool {
        use ed25519_dalek::Verifier;

        let Ok(pubkey_bytes) = hex::decode(&self.signer_pubkey) else {
            return false;
        };
        let Ok(pubkey_bytes_arr): Result<[u8; 32], _> = pubkey_bytes.try_into() else {
            return false;
        };
        let Ok(verifying_key) = VerifyingKey::from_bytes(&pubkey_bytes_arr) else {
            return false;
        };

        let signable = Self::signable_bytes(
            &self.prev_hash,
            &self.timestamp,
            &self.agent_id,
            &self.action,
            &self.payload,
        );

        let Ok(sig_bytes) = hex::decode(&self.signature) else {
            return false;
        };
        let Ok(sig_bytes_arr): Result<[u8; 64], _> = sig_bytes.try_into() else {
            return false;
        };
        let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes_arr);

        verifying_key.verify(&signable, &sig).is_ok()
    }
}

// ---------------------------------------------------------------------------
// AuditLog — the Merkle chain
// ---------------------------------------------------------------------------

const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// An append-only, Merkle-chained audit log.
///
/// In-memory `Vec` is the source of truth at runtime; an optional JSON-Lines
/// file mirrors every append for durability across restarts. Disk write
/// failures are logged but do NOT propagate, so a transient I/O error cannot
/// corrupt the in-memory chain or surface as an HTTP 500 to a caller.
pub struct AuditLog {
    entries: Arc<RwLock<Vec<AuditEntry>>>,
    /// Optional append-only JSONL writer. `None` for tests that use `new()`.
    writer: Option<Arc<Mutex<File>>>,
    /// Backing file path (for diagnostics).
    path: Option<PathBuf>,
}

impl AuditLog {
    /// In-memory only audit log (tests / ephemeral use).
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(Vec::new())),
            writer: None,
            path: None,
        }
    }

    /// Open or create a persistent audit log backed by a JSON-Lines file.
    ///
    /// On startup any existing entries are read into memory and `verify_chain`
    /// is run; if verification fails a critical warning is logged but the
    /// process continues with whatever loaded so operators can inspect the
    /// damage rather than face a hard boot failure.
    pub async fn open(path: &Path) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let mut loaded: Vec<AuditEntry> = Vec::new();
        match tokio::fs::read_to_string(path).await {
            Ok(contents) => {
                for (idx, line) in contents.lines().enumerate() {
                    if line.trim().is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<AuditEntry>(line) {
                        Ok(entry) => loaded.push(entry),
                        Err(e) => {
                            error!(
                                path = %path.display(),
                                line_no = idx + 1,
                                error = %e,
                                "Skipping malformed audit log line",
                            );
                        }
                    }
                }
                info!(
                    path = %path.display(),
                    entries = loaded.len(),
                    "Loaded audit log from disk",
                );
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                info!(path = %path.display(), "No existing audit log — starting fresh");
            }
            Err(e) => return Err(e),
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            if let Err(perm_err) = tokio::fs::set_permissions(path, perms).await {
                warn!(
                    path = %path.display(),
                    error = %perm_err,
                    "Failed to set 0600 on audit log file",
                );
            }
        }

        let log = Self {
            entries: Arc::new(RwLock::new(loaded)),
            writer: Some(Arc::new(Mutex::new(file))),
            path: Some(path.to_path_buf()),
        };

        if !log.verify_chain().await {
            error!(
                path = %path.display(),
                "Loaded audit chain failed verification — chain may be tampered with or truncated",
            );
        }

        Ok(log)
    }

    /// Append a new signed audit entry.
    ///
    /// The in-memory chain is updated under the entries write lock, then the
    /// serialized line is appended to the backing file (best effort — disk
    /// failures are logged, never propagated).
    pub async fn append(
        &self,
        agent_id: impl Into<String>,
        action: impl Into<String>,
        payload: serde_json::Value,
        keypair: &AuditKeypair,
    ) -> String {
        let mut entries = self.entries.write().await;
        let prev_hash = entries
            .last()
            .map(|e| e.hash.as_str())
            .unwrap_or(GENESIS_HASH);
        let entry = AuditEntry::new(prev_hash, agent_id, action, payload, keypair);
        let hash = entry.hash.clone();

        if let Some(writer) = &self.writer {
            match serde_json::to_string(&entry) {
                Ok(mut line) => {
                    line.push('\n');
                    let mut file = writer.lock().await;
                    if let Err(e) = file.write_all(line.as_bytes()).await {
                        error!(
                            entry_hash = %entry.hash,
                            error = %e,
                            path = ?self.path,
                            "Failed to persist audit entry — chain on disk is now divergent from RAM",
                        );
                    } else if let Err(e) = file.flush().await {
                        error!(
                            entry_hash = %entry.hash,
                            error = %e,
                            "Failed to flush audit log",
                        );
                    }
                }
                Err(e) => {
                    error!(error = %e, "Failed to serialize audit entry for disk");
                }
            }
        }

        entries.push(entry);
        hash
    }

    /// List recent entries (most recent first).
    pub async fn recent(&self, limit: usize) -> Vec<AuditEntry> {
        let entries = self.entries.read().await;
        entries.iter().rev().take(limit).cloned().collect()
    }

    /// Total number of entries in the log.
    pub async fn len(&self) -> usize {
        self.entries.read().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.entries.read().await.is_empty()
    }

    /// Verify the integrity of the entire chain.
    ///
    /// Returns `true` if all signatures are valid and all prev_hash values
    /// correctly link to the previous entry's hash.
    pub async fn verify_chain(&self) -> bool {
        let entries = self.entries.read().await;
        let mut prev = GENESIS_HASH;
        for entry in entries.iter() {
            if entry.prev_hash != prev {
                warn!(
                    entry_hash = %entry.hash,
                    expected_prev = %prev,
                    actual_prev = %entry.prev_hash,
                    "Audit chain broken: prev_hash mismatch"
                );
                return false;
            }
            if !entry.verify_signature() {
                warn!(entry_hash = %entry.hash, "Audit chain broken: invalid signature");
                return false;
            }
            prev = &entry.hash;

            // Note: We're storing `prev` as a &str into the entry slice,
            // which is owned by the lock guard. This is safe within the guard.
            // We reassign `prev` to point into the current entry's hash.
        }
        true
    }
}

impl Default for AuditLog {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Plugin manifest signing
// ---------------------------------------------------------------------------

/// A signed plugin manifest — prevents supply-chain attacks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedManifest {
    /// JSON-serialized plugin manifest.
    pub manifest_json: String,
    /// SHA-256 hash of `manifest_json`.
    pub content_hash: String,
    /// Ed25519 signature of `content_hash`.
    pub signature: String,
    /// Hex-encoded public key of the signer.
    pub signer_pubkey: String,
}

impl SignedManifest {
    /// Sign a plugin manifest.
    pub fn sign(manifest_json: impl Into<String>, keypair: &AuditKeypair) -> Self {
        let manifest_json = manifest_json.into();

        let mut hasher = Sha256::new();
        hasher.update(manifest_json.as_bytes());
        let content_hash = hex::encode(hasher.finalize());

        let signature = keypair.signing_key.sign(content_hash.as_bytes());
        let signature_hex = hex::encode(signature.to_bytes());

        Self {
            manifest_json,
            content_hash,
            signature: signature_hex,
            signer_pubkey: keypair.public_key_hex(),
        }
    }

    /// Verify the manifest signature.
    pub fn verify(&self) -> bool {
        use ed25519_dalek::Verifier;

        // Verify content hash
        let mut hasher = Sha256::new();
        hasher.update(self.manifest_json.as_bytes());
        let expected_hash = hex::encode(hasher.finalize());
        if expected_hash != self.content_hash {
            return false;
        }

        // Verify signature
        let Ok(pubkey_bytes) = hex::decode(&self.signer_pubkey) else {
            return false;
        };
        let Ok(pubkey_bytes_arr): Result<[u8; 32], _> = pubkey_bytes.try_into() else {
            return false;
        };
        let Ok(verifying_key) = VerifyingKey::from_bytes(&pubkey_bytes_arr) else {
            return false;
        };

        let Ok(sig_bytes) = hex::decode(&self.signature) else {
            return false;
        };
        let Ok(sig_bytes_arr): Result<[u8; 64], _> = sig_bytes.try_into() else {
            return false;
        };
        let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes_arr);

        verifying_key
            .verify(self.content_hash.as_bytes(), &sig)
            .is_ok()
    }
}

// ---------------------------------------------------------------------------
// Capability token — signed capability grant for spawned agents
// ---------------------------------------------------------------------------

/// A signed capability token issued at agent spawn time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityToken {
    pub agent_id: String,
    pub capabilities_json: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub signature: String,
    pub issuer_pubkey: String,
}

impl CapabilityToken {
    /// Issue a new capability token for an agent.
    pub fn issue(
        agent_id: impl Into<String>,
        capabilities: &serde_json::Value,
        ttl_seconds: i64,
        keypair: &AuditKeypair,
    ) -> Self {
        let agent_id = agent_id.into();
        let issued_at = Utc::now();
        let expires_at = issued_at + chrono::Duration::seconds(ttl_seconds);
        let capabilities_json = capabilities.to_string();

        let signable = format!(
            "{}|{}|{}|{}",
            agent_id,
            capabilities_json,
            issued_at.timestamp(),
            expires_at.timestamp()
        );

        let signature = keypair.signing_key.sign(signable.as_bytes());

        Self {
            agent_id,
            capabilities_json,
            issued_at,
            expires_at,
            signature: hex::encode(signature.to_bytes()),
            issuer_pubkey: keypair.public_key_hex(),
        }
    }

    /// Check whether this token is currently valid (not expired, signature OK).
    pub fn is_valid(&self) -> bool {
        use ed25519_dalek::Verifier;

        if Utc::now() > self.expires_at {
            return false;
        }

        let Ok(pubkey_bytes) = hex::decode(&self.issuer_pubkey) else {
            return false;
        };
        let Ok(pubkey_bytes_arr): Result<[u8; 32], _> = pubkey_bytes.try_into() else {
            return false;
        };
        let Ok(verifying_key) = VerifyingKey::from_bytes(&pubkey_bytes_arr) else {
            return false;
        };

        let signable = format!(
            "{}|{}|{}|{}",
            self.agent_id,
            self.capabilities_json,
            self.issued_at.timestamp(),
            self.expires_at.timestamp()
        );

        let Ok(sig_bytes) = hex::decode(&self.signature) else {
            return false;
        };
        let Ok(sig_bytes_arr): Result<[u8; 64], _> = sig_bytes.try_into() else {
            return false;
        };
        let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes_arr);

        verifying_key.verify(signable.as_bytes(), &sig).is_ok()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keypair_generation() {
        let kp = AuditKeypair::generate();
        assert!(!kp.public_key_hex().is_empty());
    }

    #[test]
    fn entry_sign_verify() {
        let kp = AuditKeypair::generate();
        let entry = AuditEntry::new(
            GENESIS_HASH,
            "agent-1",
            "tool_call",
            serde_json::json!({ "tool": "web_search" }),
            &kp,
        );
        assert!(entry.verify_signature());
    }

    #[tokio::test]
    async fn audit_log_chain_verify() {
        let log = AuditLog::new();
        let kp = AuditKeypair::generate();

        log.append("agent-1", "tool_call", serde_json::json!({}), &kp)
            .await;
        log.append("agent-1", "llm_request", serde_json::json!({}), &kp)
            .await;
        log.append("agent-2", "file_write", serde_json::json!({}), &kp)
            .await;

        assert_eq!(log.len().await, 3);
        assert!(log.verify_chain().await);
    }

    #[test]
    fn capability_token_valid() {
        let kp = AuditKeypair::generate();
        let token = CapabilityToken::issue(
            "agent-1",
            &serde_json::json!(["ReadFiles", "CallLlm"]),
            3600,
            &kp,
        );
        assert!(token.is_valid());
    }

    #[test]
    fn capability_token_expired() {
        let kp = AuditKeypair::generate();
        let token = CapabilityToken::issue(
            "agent-1",
            &serde_json::json!(["ReadFiles"]),
            -1, // Already expired
            &kp,
        );
        assert!(!token.is_valid());
    }

    #[test]
    fn signed_manifest_verify() {
        let kp = AuditKeypair::generate();
        let signed = SignedManifest::sign(r#"{"name":"my-plugin","version":"1.0.0"}"#, &kp);
        assert!(signed.verify());
    }

    #[tokio::test]
    async fn keypair_roundtrip_persists_secret() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("audit.key");

        let first = AuditKeypair::load_or_create(&path).await.expect("create");
        let pub1 = first.public_key_hex();

        // Re-load: must produce the same public key.
        let second = AuditKeypair::load_or_create(&path).await.expect("load");
        assert_eq!(pub1, second.public_key_hex());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = std::fs::metadata(&path).unwrap();
            assert_eq!(meta.permissions().mode() & 0o777, 0o600);
        }
    }

    #[tokio::test]
    async fn audit_log_persists_and_verifies_after_reopen() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("audit_chain.jsonl");
        let keypair = AuditKeypair::generate();

        // First boot — write three entries.
        {
            let log = AuditLog::open(&path).await.expect("open fresh");
            log.append(
                "agent-1",
                "tool_call",
                serde_json::json!({"tool": "ls"}),
                &keypair,
            )
            .await;
            log.append(
                "agent-1",
                "llm_request",
                serde_json::json!({"model": "test"}),
                &keypair,
            )
            .await;
            log.append(
                "agent-2",
                "file_write",
                serde_json::json!({"path": "/tmp/x"}),
                &keypair,
            )
            .await;
            assert_eq!(log.len().await, 3);
            assert!(log.verify_chain().await);
        }

        // Second boot — reopen the same file.
        let log = AuditLog::open(&path).await.expect("reopen");
        assert_eq!(log.len().await, 3, "entries must survive restart");
        assert!(
            log.verify_chain().await,
            "chain must still verify after reload"
        );

        // And we can keep appending — chain stays linked.
        log.append("agent-3", "policy_change", serde_json::json!({}), &keypair)
            .await;
        assert_eq!(log.len().await, 4);
        assert!(log.verify_chain().await);
    }

    #[tokio::test]
    async fn audit_log_open_skips_malformed_lines() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("audit_chain.jsonl");

        // Write one valid entry, one garbage line, one valid entry.
        let kp = AuditKeypair::generate();
        {
            let log = AuditLog::open(&path).await.expect("open");
            log.append("a", "x", serde_json::json!({}), &kp).await;
        }
        // Inject garbage between valid entries.
        {
            use tokio::io::AsyncWriteExt;
            let mut f = tokio::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .await
                .unwrap();
            f.write_all(b"not-json\n").await.unwrap();
            f.flush().await.unwrap();
        }
        // Append another valid entry through a fresh handle.
        {
            let log = AuditLog::open(&path).await.expect("reopen");
            // The garbage line was skipped; only one entry is in memory.
            assert_eq!(log.len().await, 1);
            log.append("b", "y", serde_json::json!({}), &kp).await;
            assert_eq!(log.len().await, 2);
        }
    }
}
