//! SQLite-backed persistence for ZeroClaw agent state.
//!
//! When a `persistence_dir` is configured on the `AgentPool`, agent
//! conversation history and performance statistics are saved to a SQLite
//! database so they survive server restarts.  If no path is configured the
//! pool operates purely in-memory as before.

use std::path::{Path, PathBuf};
use std::time::Instant;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::agent::ChatMessage;
use crate::roster::AgentName;

// ---------------------------------------------------------------------------
// AgentStats
// ---------------------------------------------------------------------------

/// Cumulative performance metrics for a single agent.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AgentStats {
    pub total_tasks: u64,
    pub successful_tasks: u64,
    pub failed_tasks: u64,
    pub total_tokens_used: u64,
    pub avg_response_time_ms: f64,
}

// ---------------------------------------------------------------------------
// Persisted state row
// ---------------------------------------------------------------------------

/// The full persisted state for one agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    pub agent_name: String,
    pub conversation_history: Vec<ChatMessage>,
    pub last_active: DateTime<Utc>,
    pub total_interactions: u64,
    pub stats: AgentStats,
}

// ---------------------------------------------------------------------------
// AgentPersistence
// ---------------------------------------------------------------------------

/// SQLite-backed store for agent conversation history and statistics.
///
/// Thread-safety: each method opens a short-lived connection or reuses the
/// single stored one behind a mutex-free model (SQLite serialises writes
/// internally).  We keep one `Connection` and rely on the caller (`AgentPool`)
/// to serialise access via its own `Mutex`.
pub struct AgentPersistence {
    db_path: PathBuf,
    conn: Connection,
}

impl AgentPersistence {
    /// Open (or create) the persistence database at `dir/zeroclaw_state.db`.
    pub fn open(dir: &Path) -> anyhow::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let db_path = dir.join("zeroclaw_state.db");
        let conn = Connection::open(&db_path)?;

        // Enable WAL for better concurrent-read performance.
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS agent_state (
                agent_name           TEXT PRIMARY KEY,
                conversation_history TEXT NOT NULL DEFAULT '[]',
                last_active          TEXT NOT NULL,
                total_interactions   INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS agent_stats (
                agent_name         TEXT PRIMARY KEY,
                total_tasks        INTEGER NOT NULL DEFAULT 0,
                successful_tasks   INTEGER NOT NULL DEFAULT 0,
                failed_tasks       INTEGER NOT NULL DEFAULT 0,
                total_tokens_used  INTEGER NOT NULL DEFAULT 0,
                avg_response_time_ms REAL NOT NULL DEFAULT 0.0
            );"
        )?;

        info!(path = %db_path.display(), "Agent persistence database ready");

        Ok(Self { db_path, conn })
    }

    /// Path to the underlying SQLite file.
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    // -- state CRUD --------------------------------------------------------

    /// Persist the current state of an agent.
    pub fn save_state(
        &self,
        agent_name: AgentName,
        history: &[ChatMessage],
        total_interactions: u64,
    ) -> anyhow::Result<()> {
        let name = agent_name.display_name().to_lowercase();
        let history_json = serde_json::to_string(history)?;
        let now = Utc::now().to_rfc3339();

        self.conn.execute(
            "INSERT INTO agent_state (agent_name, conversation_history, last_active, total_interactions)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(agent_name) DO UPDATE SET
                conversation_history = excluded.conversation_history,
                last_active          = excluded.last_active,
                total_interactions   = excluded.total_interactions",
            params![name, history_json, now, total_interactions as i64],
        )?;

        debug!(agent = %agent_name, interactions = total_interactions, "State persisted");
        Ok(())
    }

    /// Load persisted state for an agent (returns `None` if no record exists).
    pub fn load_state(&self, agent_name: AgentName) -> anyhow::Result<Option<AgentState>> {
        let name = agent_name.display_name().to_lowercase();

        let mut stmt = self.conn.prepare(
            "SELECT conversation_history, last_active, total_interactions
             FROM agent_state WHERE agent_name = ?1"
        )?;

        let mut rows = stmt.query(params![name])?;

        let row = match rows.next()? {
            Some(r) => r,
            None => return Ok(None),
        };

        let history_json: String = row.get(0)?;
        let last_active_str: String = row.get(1)?;
        let total_interactions: i64 = row.get(2)?;

        let conversation_history: Vec<ChatMessage> =
            serde_json::from_str(&history_json).unwrap_or_default();
        let last_active = DateTime::parse_from_rfc3339(&last_active_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        let stats = self.load_stats(agent_name)?;

        Ok(Some(AgentState {
            agent_name: name,
            conversation_history,
            last_active,
            total_interactions: total_interactions as u64,
            stats,
        }))
    }

    /// Return the names of agents that were active within the last `hours` hours.
    pub fn recently_active_agents(&self, hours: i64) -> anyhow::Result<Vec<String>> {
        let cutoff = (Utc::now() - chrono::Duration::hours(hours)).to_rfc3339();

        let mut stmt = self.conn.prepare(
            "SELECT agent_name FROM agent_state WHERE last_active >= ?1"
        )?;

        let names: Vec<String> = stmt
            .query_map(params![cutoff], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(names)
    }

    // -- stats CRUD --------------------------------------------------------

    /// Persist updated stats for an agent.
    pub fn save_stats(&self, agent_name: AgentName, stats: &AgentStats) -> anyhow::Result<()> {
        let name = agent_name.display_name().to_lowercase();

        self.conn.execute(
            "INSERT INTO agent_stats (agent_name, total_tasks, successful_tasks, failed_tasks, total_tokens_used, avg_response_time_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(agent_name) DO UPDATE SET
                total_tasks        = excluded.total_tasks,
                successful_tasks   = excluded.successful_tasks,
                failed_tasks       = excluded.failed_tasks,
                total_tokens_used  = excluded.total_tokens_used,
                avg_response_time_ms = excluded.avg_response_time_ms",
            params![
                name,
                stats.total_tasks as i64,
                stats.successful_tasks as i64,
                stats.failed_tasks as i64,
                stats.total_tokens_used as i64,
                stats.avg_response_time_ms,
            ],
        )?;

        debug!(agent = %agent_name, "Stats persisted");
        Ok(())
    }

    /// Load stats for an agent.
    pub fn load_stats(&self, agent_name: AgentName) -> anyhow::Result<AgentStats> {
        let name = agent_name.display_name().to_lowercase();

        let mut stmt = self.conn.prepare(
            "SELECT total_tasks, successful_tasks, failed_tasks, total_tokens_used, avg_response_time_ms
             FROM agent_stats WHERE agent_name = ?1"
        )?;

        let mut rows = stmt.query(params![name])?;

        match rows.next()? {
            Some(row) => {
                let total_tasks: i64 = row.get(0)?;
                let successful_tasks: i64 = row.get(1)?;
                let failed_tasks: i64 = row.get(2)?;
                let total_tokens_used: i64 = row.get(3)?;
                let avg_response_time_ms: f64 = row.get(4)?;
                Ok(AgentStats {
                    total_tasks: total_tasks as u64,
                    successful_tasks: successful_tasks as u64,
                    failed_tasks: failed_tasks as u64,
                    total_tokens_used: total_tokens_used as u64,
                    avg_response_time_ms,
                })
            }
            None => Ok(AgentStats::default()),
        }
    }

    /// Query stats for any agent by name string.
    pub fn get_stats(&self, agent_name: AgentName) -> anyhow::Result<AgentStats> {
        self.load_stats(agent_name)
    }

    // -- helpers -----------------------------------------------------------

    /// Record the outcome of a single agent interaction, updating both the
    /// running stats and the cumulative averages.
    pub fn record_interaction(
        &self,
        agent_name: AgentName,
        success: bool,
        response_time_ms: f64,
        tokens_used: u64,
    ) -> anyhow::Result<()> {
        let mut stats = self.load_stats(agent_name)?;

        stats.total_tasks += 1;
        if success {
            stats.successful_tasks += 1;
        } else {
            stats.failed_tasks += 1;
        }
        stats.total_tokens_used += tokens_used;

        // Incremental average: new_avg = old_avg + (value - old_avg) / n
        let n = stats.total_tasks as f64;
        stats.avg_response_time_ms += (response_time_ms - stats.avg_response_time_ms) / n;

        self.save_stats(agent_name, &stats)?;
        Ok(())
    }
}

/// A helper that measures elapsed time since creation, used to record
/// response latency after an agent turn.
pub struct InteractionTimer {
    start: Instant,
}

impl InteractionTimer {
    pub fn start() -> Self {
        Self { start: Instant::now() }
    }

    pub fn elapsed_ms(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.0
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_persistence() -> (TempDir, AgentPersistence) {
        let dir = TempDir::new().expect("create tempdir");
        let p = AgentPersistence::open(dir.path()).expect("open persistence");
        (dir, p)
    }

    #[test]
    fn test_init_creates_database() {
        let (_dir, p) = temp_persistence();
        assert!(p.db_path().exists());
    }

    #[test]
    fn test_save_load_roundtrip() {
        let (_dir, p) = temp_persistence();

        let history = vec![
            ChatMessage::user("Hello"),
            ChatMessage::assistant("Hi there!"),
        ];

        p.save_state(AgentName::Nova, &history, 5).unwrap();

        let state = p.load_state(AgentName::Nova).unwrap().expect("state exists");
        assert_eq!(state.agent_name, "nova");
        assert_eq!(state.conversation_history.len(), 2);
        assert_eq!(state.conversation_history[0].role, "user");
        assert_eq!(state.conversation_history[0].content, "Hello");
        assert_eq!(state.conversation_history[1].content, "Hi there!");
        assert_eq!(state.total_interactions, 5);
    }

    #[test]
    fn test_stats_tracking() {
        let (_dir, p) = temp_persistence();

        p.record_interaction(AgentName::Nova, true, 150.0, 100).unwrap();
        p.record_interaction(AgentName::Nova, true, 250.0, 200).unwrap();
        p.record_interaction(AgentName::Nova, false, 50.0, 50).unwrap();

        let stats = p.get_stats(AgentName::Nova).unwrap();
        assert_eq!(stats.total_tasks, 3);
        assert_eq!(stats.successful_tasks, 2);
        assert_eq!(stats.failed_tasks, 1);
        assert_eq!(stats.total_tokens_used, 350);
        // avg = 150 + (250-150)/2 + (50 - 200)/3 = 200 + (-50) = 150
        assert!((stats.avg_response_time_ms - 150.0).abs() < 1.0);
    }

    #[test]
    fn test_warm_up_recently_active() {
        let (_dir, p) = temp_persistence();

        let history = vec![ChatMessage::user("test")];
        p.save_state(AgentName::Nova, &history, 1).unwrap();
        p.save_state(AgentName::Atlas, &history, 2).unwrap();

        let active = p.recently_active_agents(24).unwrap();
        assert_eq!(active.len(), 2);
        assert!(active.contains(&"nova".to_string()));
        assert!(active.contains(&"atlas".to_string()));

        // Agent not saved should not appear
        let state = p.load_state(AgentName::Kai).unwrap();
        assert!(state.is_none());
    }

    #[test]
    fn test_graceful_degradation_missing_state() {
        let (_dir, p) = temp_persistence();

        // Loading non-existent agent returns None, not an error.
        let state = p.load_state(AgentName::Mia).unwrap();
        assert!(state.is_none());

        // Stats for non-existent agent returns defaults, not an error.
        let stats = p.get_stats(AgentName::Mia).unwrap();
        assert_eq!(stats, AgentStats::default());
    }

    #[test]
    fn test_save_overwrites_previous() {
        let (_dir, p) = temp_persistence();

        let h1 = vec![ChatMessage::user("first")];
        p.save_state(AgentName::Nova, &h1, 1).unwrap();

        let h2 = vec![
            ChatMessage::user("first"),
            ChatMessage::assistant("reply"),
            ChatMessage::user("second"),
        ];
        p.save_state(AgentName::Nova, &h2, 2).unwrap();

        let state = p.load_state(AgentName::Nova).unwrap().expect("exists");
        assert_eq!(state.conversation_history.len(), 3);
        assert_eq!(state.total_interactions, 2);
    }
}
