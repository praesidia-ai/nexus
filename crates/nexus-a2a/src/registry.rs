//! Agent Card registry — stores discovered remote agents and the local agent card.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::error::A2aError;
use crate::types::AgentCard;

/// Manages the set of known A2A peers (by their Agent Card URL) and the
/// local server's own Agent Card.
#[derive(Clone)]
pub struct AgentCardRegistry {
    inner: Arc<RwLock<RegistryInner>>,
}

struct RegistryInner {
    local_card: Option<AgentCard>,
    remote_cards: HashMap<String, AgentCard>,
}

impl AgentCardRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(RegistryInner {
                local_card: None,
                remote_cards: HashMap::new(),
            })),
        }
    }

    /// Set the local agent card (published at `/.well-known/agent.json`).
    pub async fn set_local(&self, card: AgentCard) {
        let mut inner = self.inner.write().await;
        info!(name = %card.name, url = %card.url, "Local A2A agent card set");
        inner.local_card = Some(card);
    }

    /// Get the local agent card.
    pub async fn local_card(&self) -> Option<AgentCard> {
        self.inner.read().await.local_card.clone()
    }

    /// Register a remote agent card (from discovery or manual registration).
    pub async fn register_remote(&self, card: AgentCard) {
        let mut inner = self.inner.write().await;
        info!(name = %card.name, url = %card.url, "Registered remote A2A agent");
        inner.remote_cards.insert(card.url.clone(), card);
    }

    /// Remove a remote agent by URL.
    pub async fn remove_remote(&self, url: &str) -> Result<(), A2aError> {
        let mut inner = self.inner.write().await;
        if inner.remote_cards.remove(url).is_none() {
            return Err(A2aError::TaskNotFound(format!("No remote agent at {url}")));
        }
        Ok(())
    }

    /// List all remote agents.
    pub async fn list_remote(&self) -> Vec<AgentCard> {
        self.inner
            .read()
            .await
            .remote_cards
            .values()
            .cloned()
            .collect()
    }

    /// Get a remote agent card by URL.
    pub async fn get_remote(&self, url: &str) -> Option<AgentCard> {
        self.inner.read().await.remote_cards.get(url).cloned()
    }

    /// Fetch and register a remote agent card by fetching `{base_url}/.well-known/agent.json`.
    pub async fn discover(&self, base_url: &str) -> Result<AgentCard, A2aError> {
        let client = reqwest::Client::new();
        let url = format!(
            "{}/{}",
            base_url.trim_end_matches('/'),
            ".well-known/agent.json"
        );
        let card: AgentCard = client
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        self.register_remote(card.clone()).await;
        Ok(card)
    }
}

impl Default for AgentCardRegistry {
    fn default() -> Self {
        Self::new()
    }
}
