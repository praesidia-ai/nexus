//! Federation Manager — Layer 9 of the Nexus Agent OS.
//!
//! Enables multiple Nexus instances to form a federated network, sharing
//! capabilities like remote agents, knowledge bases, and task delegation
//! across organizational boundaries with configurable trust levels.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

/// Errors from federation operations.
#[derive(Debug, thiserror::Error)]
pub enum FederationError {
    #[error("peer {peer_id} not found")]
    PeerNotFound { peer_id: String },

    #[error("already connected to peer at {peer_url}")]
    AlreadyConnected { peer_url: String },

    #[error("connection to {peer_url} failed: {reason}")]
    ConnectionFailed { peer_url: String, reason: String },

    #[error("peer {peer_id} trust level insufficient for {operation}")]
    InsufficientTrust {
        peer_id: String,
        operation: String,
    },

    #[error("http error: {0}")]
    HttpError(String),
}

/// The trust level granted to a federated peer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FederationTrust {
    /// Full trust — all capabilities shared.
    Full,
    /// Only remote agent invocation is allowed.
    AgentsOnly,
    /// Read-only access to shared knowledge.
    ReadOnly,
}

/// A capability that can be shared with federated peers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SharedCapability {
    /// Allow remote agent invocation.
    RemoteAgents,
    /// Share knowledge base entries.
    SharedKnowledge,
    /// Delegate tasks to this peer.
    TaskDelegation,
    /// Expose local tools to remote peers.
    RemoteTools,
}

/// A link to a federated peer instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationLink {
    /// Unique identifier for this peer.
    pub peer_id: String,
    /// The base URL of the peer's API.
    pub peer_url: String,
    /// Trust level for this peer.
    pub trust_level: FederationTrust,
    /// Capabilities shared with this peer.
    pub shared_capabilities: Vec<SharedCapability>,
    /// When the link was established.
    pub connected_at: DateTime<Utc>,
    /// When the last heartbeat was received.
    pub last_heartbeat: DateTime<Utc>,
}

/// Manages federation links with other Nexus instances.
pub struct FederationManager {
    /// This instance's federation identifier.
    local_id: String,
    /// Connected peers keyed by peer ID.
    peers: RwLock<HashMap<String, FederationLink>>,
    /// HTTP client for peer communication.
    client: reqwest::Client,
}

impl std::fmt::Debug for FederationManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FederationManager")
            .field("local_id", &self.local_id)
            .field("peers", &"<RwLock>")
            .finish()
    }
}

impl FederationManager {
    /// Create a new federation manager with the given local instance ID.
    pub fn new(local_id: String) -> Self {
        Self {
            local_id,
            peers: RwLock::new(HashMap::new()),
            client: reqwest::Client::new(),
        }
    }

    /// Connect to a federated peer. Returns the assigned peer ID.
    pub async fn connect(
        &self,
        peer_url: &str,
        trust: FederationTrust,
    ) -> Result<String, FederationError> {
        // Check for duplicate connections.
        {
            let peers = self.peers.read().await;
            if peers.values().any(|p| p.peer_url == peer_url) {
                return Err(FederationError::AlreadyConnected {
                    peer_url: peer_url.to_string(),
                });
            }
        }

        // Attempt a handshake with the remote peer.
        let handshake_url = format!("{}/federation/handshake", peer_url.trim_end_matches('/'));
        let resp = self
            .client
            .post(&handshake_url)
            .json(&serde_json::json!({
                "from_id": self.local_id,
                "trust": trust,
            }))
            .send()
            .await
            .map_err(|e| FederationError::ConnectionFailed {
                peer_url: peer_url.to_string(),
                reason: e.to_string(),
            })?;

        if !resp.status().is_success() {
            return Err(FederationError::ConnectionFailed {
                peer_url: peer_url.to_string(),
                reason: format!("handshake returned HTTP {}", resp.status()),
            });
        }

        let peer_id = Uuid::new_v4().to_string();
        let now = Utc::now();

        let capabilities = match &trust {
            FederationTrust::Full => vec![
                SharedCapability::RemoteAgents,
                SharedCapability::SharedKnowledge,
                SharedCapability::TaskDelegation,
                SharedCapability::RemoteTools,
            ],
            FederationTrust::AgentsOnly => vec![SharedCapability::RemoteAgents],
            FederationTrust::ReadOnly => vec![SharedCapability::SharedKnowledge],
        };

        let link = FederationLink {
            peer_id: peer_id.clone(),
            peer_url: peer_url.to_string(),
            trust_level: trust,
            shared_capabilities: capabilities,
            connected_at: now,
            last_heartbeat: now,
        };

        self.peers.write().await.insert(peer_id.clone(), link);
        Ok(peer_id)
    }

    /// Disconnect from a federated peer.
    pub async fn disconnect(&self, peer_id: &str) -> Result<(), FederationError> {
        let mut peers = self.peers.write().await;
        if peers.remove(peer_id).is_none() {
            return Err(FederationError::PeerNotFound {
                peer_id: peer_id.to_string(),
            });
        }
        Ok(())
    }

    /// List all connected peers.
    pub async fn list_peers(&self) -> Vec<FederationLink> {
        self.peers.read().await.values().cloned().collect()
    }

    /// Update the heartbeat timestamp for a peer. Returns an error if the peer is unknown.
    pub async fn heartbeat(&self, peer_id: &str) -> Result<(), FederationError> {
        let mut peers = self.peers.write().await;
        let link = peers
            .get_mut(peer_id)
            .ok_or_else(|| FederationError::PeerNotFound {
                peer_id: peer_id.to_string(),
            })?;
        link.last_heartbeat = Utc::now();
        Ok(())
    }

    /// Delegate a task to a federated peer. The peer must have `TaskDelegation` capability.
    pub async fn delegate_task(
        &self,
        peer_id: &str,
        task_payload: serde_json::Value,
    ) -> Result<serde_json::Value, FederationError> {
        let peers = self.peers.read().await;
        let link = peers
            .get(peer_id)
            .ok_or_else(|| FederationError::PeerNotFound {
                peer_id: peer_id.to_string(),
            })?;

        if !link
            .shared_capabilities
            .contains(&SharedCapability::TaskDelegation)
        {
            return Err(FederationError::InsufficientTrust {
                peer_id: peer_id.to_string(),
                operation: "task_delegation".to_string(),
            });
        }

        let url = format!(
            "{}/federation/tasks",
            link.peer_url.trim_end_matches('/')
        );

        let resp = self
            .client
            .post(&url)
            .json(&serde_json::json!({
                "from_id": self.local_id,
                "task": task_payload,
            }))
            .send()
            .await
            .map_err(|e| FederationError::HttpError(e.to_string()))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| FederationError::HttpError(e.to_string()))?;

        Ok(body)
    }

    /// Search for knowledge across all peers that share the `SharedKnowledge` capability.
    pub async fn federated_search(
        &self,
        query: &str,
    ) -> Vec<(String, Result<serde_json::Value, FederationError>)> {
        let peers = self.peers.read().await;
        let knowledge_peers: Vec<_> = peers
            .values()
            .filter(|p| {
                p.shared_capabilities
                    .contains(&SharedCapability::SharedKnowledge)
            })
            .cloned()
            .collect();
        drop(peers);

        let mut results = Vec::new();

        for peer in knowledge_peers {
            let url = format!(
                "{}/federation/search?q={}",
                peer.peer_url.trim_end_matches('/'),
                urlencoding::encode(query),
            );

            let result = match self.client.get(&url).send().await {
                Ok(resp) => match resp.json::<serde_json::Value>().await {
                    Ok(body) => Ok(body),
                    Err(e) => Err(FederationError::HttpError(e.to_string())),
                },
                Err(e) => Err(FederationError::HttpError(e.to_string())),
            };

            results.push((peer.peer_id.clone(), result));
        }

        results
    }

    /// Get this instance's local federation ID.
    pub fn local_id(&self) -> &str {
        &self.local_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_list_peers_empty() {
        let mgr = FederationManager::new("local-1".to_string());
        assert!(mgr.list_peers().await.is_empty());
    }

    #[tokio::test]
    async fn test_disconnect_unknown_peer() {
        let mgr = FederationManager::new("local-1".to_string());
        let result = mgr.disconnect("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_heartbeat_unknown_peer() {
        let mgr = FederationManager::new("local-1".to_string());
        let result = mgr.heartbeat("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_delegate_task_unknown_peer() {
        let mgr = FederationManager::new("local-1".to_string());
        let result = mgr
            .delegate_task("nonexistent", serde_json::json!({"task": "test"}))
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn test_federation_trust_serialization() {
        let trust = FederationTrust::AgentsOnly;
        let json = serde_json::to_string(&trust).unwrap();
        assert_eq!(json, "\"agents_only\"");
        let deserialized: FederationTrust = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, FederationTrust::AgentsOnly);
    }

    #[test]
    fn test_shared_capability_serialization() {
        let cap = SharedCapability::TaskDelegation;
        let json = serde_json::to_string(&cap).unwrap();
        assert_eq!(json, "\"task_delegation\"");
    }
}
