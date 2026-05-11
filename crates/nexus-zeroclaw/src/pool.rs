//! `AgentPool` — a live pool of ZeroClaw agent instances.
//!
//! The pool creates agents lazily (on first use) and keeps them alive for the
//! lifetime of a workflow run so conversation history is preserved between
//! steps.  Each agent is protected by a `Mutex` because `Agent::turn` takes
//! `&mut self`.
//!
//! When `persistence_dir` is set in the `RosterConfig`, the pool will
//! automatically restore conversation history from a previous run and persist
//! state after each interaction.
//!
//! Typical usage:
//! ```rust,ignore
//! let pool = AgentPool::new(config);
//! pool.warm_up().await; // pre-load recently active agents
//! let reply = pool.send(AgentName::Nova, "Scaffold a REST API in Rust").await?;
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::{info, instrument, warn};

use crate::agent::NexusAgent;
use crate::config::RosterConfig;
use crate::message::{AgentMessage, AgentReply};
use crate::persistence::{AgentPersistence, AgentStats, InteractionTimer};
use crate::roster::AgentName;

// ---------------------------------------------------------------------------
// AgentPool
// ---------------------------------------------------------------------------

/// A live pool of `NexusAgent` instances, created on demand per workflow run.
///
/// Optionally backed by SQLite persistence so agent state survives restarts.
pub struct AgentPool {
    config: RosterConfig,
    agents: Mutex<HashMap<AgentName, Arc<Mutex<NexusAgent>>>>,
    /// Per-agent interaction counters (in-memory, synced to persistence).
    interactions: Mutex<HashMap<AgentName, u64>>,
    /// Optional persistence layer, initialised when `config.persistence_dir` is `Some`.
    persistence: Option<Mutex<AgentPersistence>>,
}

impl AgentPool {
    /// Create an empty pool backed by the given roster config.
    ///
    /// If `config.persistence_dir` is set, opens the SQLite database.  If
    /// opening fails the pool falls back to in-memory mode and logs a warning.
    pub fn new(config: RosterConfig) -> Self {
        let persistence = config.persistence_dir.as_ref().and_then(|dir| {
            match AgentPersistence::open(dir) {
                Ok(p) => Some(Mutex::new(p)),
                Err(e) => {
                    warn!(
                        path = %dir.display(),
                        err = %e,
                        "Failed to open persistence DB — running in-memory"
                    );
                    None
                }
            }
        });

        Self {
            config,
            agents: Mutex::new(HashMap::new()),
            interactions: Mutex::new(HashMap::new()),
            persistence,
        }
    }

    /// Get-or-create the agent for `name`.  The agent is initialised on the
    /// first call and reused on subsequent calls within the same pool.
    ///
    /// If persistence is available, the agent's conversation history is
    /// restored from the database before being returned.
    pub async fn get_or_init(&self, name: AgentName) -> anyhow::Result<Arc<Mutex<NexusAgent>>> {
        let mut map = self.agents.lock().await;
        if let Some(agent) = map.get(&name) {
            return Ok(agent.clone());
        }

        info!(agent = %name, "Initialising agent");
        let mut agent = NexusAgent::from_roster_config(name, &self.config).await?;

        // Restore persisted state if available.
        if let Some(ref persistence) = self.persistence {
            let p = persistence.lock().await;
            match p.load_state(name) {
                Ok(Some(state)) => {
                    agent.seed_history(&state.conversation_history);
                    // Restore interaction counter.
                    self.interactions
                        .lock()
                        .await
                        .insert(name, state.total_interactions);
                    info!(
                        agent = %name,
                        interactions = state.total_interactions,
                        history_len = state.conversation_history.len(),
                        "Restored persisted state"
                    );
                }
                Ok(None) => {}
                Err(e) => {
                    warn!(agent = %name, err = %e, "Failed to load persisted state");
                }
            }
        }

        let handle = Arc::new(Mutex::new(agent));
        map.insert(name, handle.clone());
        Ok(handle)
    }

    /// Send a message to a named agent and return its reply.
    ///
    /// After the turn completes the agent's state and stats are persisted.
    #[instrument(skip(self, message), fields(to = %message.to))]
    pub async fn send(&self, message: AgentMessage) -> anyhow::Result<AgentReply> {
        let to = message.to;
        let agent_handle = self.get_or_init(to).await?;
        let prompt = message.build_prompt();

        let timer = InteractionTimer::start();
        let mut agent = agent_handle.lock().await;
        let result = agent.turn(prompt).await;
        let elapsed = timer.elapsed_ms();

        let success = result.is_ok();
        let content = match result {
            Ok(text) => text,
            Err(e) => {
                self.persist_interaction(to, &agent, false, elapsed, 0).await;
                return Err(e);
            }
        };

        self.persist_interaction(to, &agent, success, elapsed, 0).await;

        Ok(AgentReply {
            from: to,
            content,
            artefacts: vec![],
        })
    }

    /// Convenience: send a plain string task to a named agent.
    pub async fn task(
        &self,
        to: AgentName,
        content: impl Into<String>,
    ) -> anyhow::Result<String> {
        let msg = AgentMessage::task(to, content);
        let reply = self.send(msg).await?;
        Ok(reply.content)
    }

    /// Reset (clear history) for every initialised agent in the pool.
    pub async fn reset_all(&self) {
        let map = self.agents.lock().await;
        for agent_handle in map.values() {
            agent_handle.lock().await.clear_history();
        }
    }

    /// Returns the names of agents that have been initialised.
    pub async fn active_agents(&self) -> Vec<AgentName> {
        self.agents.lock().await.keys().copied().collect()
    }

    /// Pre-load agents that were recently active (within the last 24 hours).
    ///
    /// This restores their conversation context so they can continue
    /// seamlessly after a server restart.  No-op if persistence is not
    /// configured.
    pub async fn warm_up(&self) {
        let recent = match &self.persistence {
            Some(p) => {
                let p = p.lock().await;
                match p.recently_active_agents(24) {
                    Ok(names) => names,
                    Err(e) => {
                        warn!(err = %e, "warm_up: failed to query recently active agents");
                        return;
                    }
                }
            }
            None => return,
        };

        if recent.is_empty() {
            info!("warm_up: no recently active agents to pre-load");
            return;
        }

        info!(count = recent.len(), "warm_up: pre-loading recently active agents");

        for name_str in &recent {
            if let Ok(name) = name_str.parse::<AgentName>() {
                if let Err(e) = self.get_or_init(name).await {
                    warn!(agent = %name, err = %e, "warm_up: failed to init agent");
                }
            }
        }
    }

    /// Query persisted statistics for a named agent.
    ///
    /// Returns `AgentStats::default()` when persistence is disabled or the
    /// agent has no recorded stats.
    pub async fn get_stats(&self, agent_name: AgentName) -> AgentStats {
        match &self.persistence {
            Some(p) => {
                let p = p.lock().await;
                p.get_stats(agent_name).unwrap_or_default()
            }
            None => AgentStats::default(),
        }
    }

    // -- internal helpers --------------------------------------------------

    /// Persist the agent's state and record the interaction outcome.
    async fn persist_interaction(
        &self,
        name: AgentName,
        agent: &NexusAgent,
        success: bool,
        response_time_ms: f64,
        tokens_used: u64,
    ) {
        // Bump in-memory interaction counter.
        let total = {
            let mut counts = self.interactions.lock().await;
            let counter = counts.entry(name).or_insert(0);
            *counter += 1;
            *counter
        };

        if let Some(ref persistence) = self.persistence {
            let p = persistence.lock().await;

            if let Err(e) = p.save_state(name, agent.history(), total) {
                warn!(agent = %name, err = %e, "Failed to persist agent state");
            }

            if let Err(e) = p.record_interaction(name, success, response_time_ms, tokens_used) {
                warn!(agent = %name, err = %e, "Failed to persist interaction stats");
            }
        }
    }
}
