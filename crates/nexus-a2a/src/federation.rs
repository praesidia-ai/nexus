use crate::{
    client::A2aClient,
    registry::AgentCardRegistry,
    types::{AgentCard, Message, Task},
    A2aError,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// A remote Nexus peer instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusPeer {
    pub id: String,
    pub url: String,
    pub name: String,
    /// The Agent Card fetched from `/.well-known/agent.json`
    pub card: Option<AgentCard>,
    pub healthy: bool,
    pub last_seen: Option<chrono::DateTime<chrono::Utc>>,
}

/// Federation manager — maintains peer registry and routes tasks.
pub struct FederationManager {
    peers: RwLock<HashMap<String, NexusPeer>>,
    registry: AgentCardRegistry,
    http: reqwest::Client,
}

impl FederationManager {
    pub fn new(registry: AgentCardRegistry) -> Self {
        Self {
            peers: RwLock::new(HashMap::new()),
            registry,
            http: reqwest::Client::new(),
        }
    }

    /// Register a remote Nexus peer by URL.
    /// Fetches its Agent Card and stores it.
    pub async fn register_peer(&self, url: &str) -> Result<NexusPeer, A2aError> {
        let card_url = format!("{}/.well-known/agent.json", url.trim_end_matches('/'));
        let card: AgentCard = self
            .http
            .get(&card_url)
            .send()
            .await
            .map_err(A2aError::Network)?
            .json()
            .await
            .map_err(A2aError::Network)?;

        let peer = NexusPeer {
            id: card.name.clone(),
            url: url.to_owned(),
            name: card.name.clone(),
            card: Some(card.clone()),
            healthy: true,
            last_seen: Some(chrono::Utc::now()),
        };

        // Also register in the global card registry
        self.registry.register_remote(card).await;

        let mut peers = self.peers.write().await;
        peers.insert(peer.id.clone(), peer.clone());

        info!("Federated peer registered: {} ({})", peer.name, peer.url);
        Ok(peer)
    }

    /// Remove a peer from the federation.
    pub async fn remove_peer(&self, id: &str) -> bool {
        let mut peers = self.peers.write().await;
        if peers.remove(id).is_some() {
            let _ = self.registry.remove_remote(id).await;
            info!("Federated peer removed: {id}");
            true
        } else {
            false
        }
    }

    /// List all registered peers.
    pub async fn list_peers(&self) -> Vec<NexusPeer> {
        self.peers.read().await.values().cloned().collect()
    }

    /// Delegate a task to the best-matching remote Nexus peer.
    ///
    /// Selection strategy:
    /// 1. Filter peers that declare the required skill via their Agent Card.
    /// 2. Among candidates, pick the one with the highest `last_seen` (recently healthy).
    /// 3. Forward the task via `tasks/send`.
    pub async fn delegate(
        &self,
        message: &Message,
        required_skill: Option<&str>,
    ) -> Result<Task, A2aError> {
        let peers = self.peers.read().await;

        let candidates: Vec<&NexusPeer> = peers
            .values()
            .filter(|p| p.healthy)
            .filter(|p| {
                if let Some(skill) = required_skill {
                    p.card
                        .as_ref()
                        .map(|c| c.skills.iter().any(|s| s.id == skill || s.name.contains(skill)))
                        .unwrap_or(false)
                } else {
                    true
                }
            })
            .collect();

        if candidates.is_empty() {
            return Err(A2aError::TaskNotFound(
                "no healthy federated peer available for delegation".to_owned(),
            ));
        }

        // Pick most recently seen peer
        let peer = candidates
            .into_iter()
            .max_by_key(|p| p.last_seen)
            .unwrap();

        info!("Delegating task to peer '{}' at {}", peer.name, peer.url);

        let client = A2aClient::new(&peer.url);
        let text = message
            .parts
            .iter()
            .filter_map(|p| {
                if let crate::types::Part::Text { text } = p {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        let task_id = uuid::Uuid::new_v4().to_string();
        client.send_text(task_id, text).await
    }

    /// Health-check all peers (ping `/.well-known/agent.json`).
    pub async fn health_check_all(&self) {
        let peer_list: Vec<NexusPeer> = {
            let peers = self.peers.read().await;
            peers.values().cloned().collect()
        };

        for peer in peer_list {
            let url = format!("{}/.well-known/agent.json", peer.url.trim_end_matches('/'));
            let healthy = self.http.get(&url).send().await.map(|r| r.status().is_success()).unwrap_or(false);
            let mut peers = self.peers.write().await;
            if let Some(p) = peers.get_mut(&peer.id) {
                p.healthy = healthy;
                if healthy {
                    p.last_seen = Some(chrono::Utc::now());
                } else {
                    warn!("Peer '{}' health check failed", peer.name);
                }
            }
        }
    }
}
