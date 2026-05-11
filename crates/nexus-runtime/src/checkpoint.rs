use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: String,
    pub process_id: String,
    pub step_index: u32,
    pub state: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

pub struct CheckpointStore {
    db: Connection,
}

impl CheckpointStore {
    pub fn new(db_path: &std::path::Path) -> Result<Self, rusqlite::Error> {
        let db = Connection::open(db_path)?;
        Self::init_schema(&db)?;
        Ok(Self { db })
    }

    pub fn in_memory() -> Result<Self, rusqlite::Error> {
        let db = Connection::open_in_memory()?;
        Self::init_schema(&db)?;
        Ok(Self { db })
    }

    fn init_schema(db: &Connection) -> Result<(), rusqlite::Error> {
        db.execute_batch(
            "
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;
            CREATE TABLE IF NOT EXISTS checkpoints (
                id TEXT PRIMARY KEY,
                process_id TEXT NOT NULL,
                step_index INTEGER NOT NULL,
                state TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_cp_process ON checkpoints(process_id, step_index);
            ",
        )
    }

    pub fn save(&self, checkpoint: &Checkpoint) -> Result<(), rusqlite::Error> {
        self.db.execute(
            "INSERT OR REPLACE INTO checkpoints (id, process_id, step_index, state, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                checkpoint.id,
                checkpoint.process_id,
                checkpoint.step_index,
                serde_json::to_string(&checkpoint.state).unwrap_or_default(),
                checkpoint.created_at.to_rfc3339(),
            ],
        )?;
        info!(
            process_id = %checkpoint.process_id,
            step = checkpoint.step_index,
            "Checkpoint saved"
        );
        Ok(())
    }

    pub fn latest(&self, process_id: &str) -> Option<Checkpoint> {
        let mut stmt = self
            .db
            .prepare(
                "SELECT id, process_id, step_index, state, created_at \
                 FROM checkpoints WHERE process_id = ?1 \
                 ORDER BY step_index DESC LIMIT 1",
            )
            .ok()?;

        stmt.query_row(rusqlite::params![process_id], |row| {
            Self::row_to_checkpoint(row)
        })
        .ok()
    }

    pub fn list(&self, process_id: &str) -> Vec<Checkpoint> {
        let mut results = Vec::new();
        if let Ok(mut stmt) = self.db.prepare(
            "SELECT id, process_id, step_index, state, created_at \
             FROM checkpoints WHERE process_id = ?1 \
             ORDER BY step_index ASC",
        ) {
            let _ = stmt
                .query_map(rusqlite::params![process_id], |row| {
                    Self::row_to_checkpoint(row)
                })
                .map(|rows| {
                    for cp in rows.flatten() {
                        results.push(cp);
                    }
                });
        }
        results
    }

    pub fn delete_for_process(&self, process_id: &str) -> Result<(), rusqlite::Error> {
        self.db.execute(
            "DELETE FROM checkpoints WHERE process_id = ?1",
            rusqlite::params![process_id],
        )?;
        Ok(())
    }

    fn row_to_checkpoint(row: &rusqlite::Row<'_>) -> Result<Checkpoint, rusqlite::Error> {
        let state_str: String = row.get(3)?;
        let created_str: String = row.get(4)?;
        Ok(Checkpoint {
            id: row.get(0)?,
            process_id: row.get(1)?,
            step_index: row.get::<_, i64>(2)? as u32,
            state: serde_json::from_str(&state_str).unwrap_or(serde_json::Value::Null),
            created_at: chrono::DateTime::parse_from_rfc3339(&created_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_checkpoint(process_id: &str, step: u32) -> Checkpoint {
        Checkpoint {
            id: uuid::Uuid::new_v4().to_string(),
            process_id: process_id.to_string(),
            step_index: step,
            state: serde_json::json!({"step": step, "data": "test"}),
            created_at: Utc::now(),
        }
    }

    #[test]
    fn test_save_and_latest() {
        let store = CheckpointStore::in_memory().unwrap();
        let cp1 = make_checkpoint("proc-1", 0);
        let cp2 = make_checkpoint("proc-1", 1);
        let cp3 = make_checkpoint("proc-1", 2);

        store.save(&cp1).unwrap();
        store.save(&cp2).unwrap();
        store.save(&cp3).unwrap();

        let latest = store.latest("proc-1").unwrap();
        assert_eq!(latest.step_index, 2);
        assert_eq!(latest.process_id, "proc-1");
    }

    #[test]
    fn test_latest_returns_none_for_unknown() {
        let store = CheckpointStore::in_memory().unwrap();
        assert!(store.latest("nonexistent").is_none());
    }

    #[test]
    fn test_list_ordered_by_step() {
        let store = CheckpointStore::in_memory().unwrap();
        store.save(&make_checkpoint("proc-1", 2)).unwrap();
        store.save(&make_checkpoint("proc-1", 0)).unwrap();
        store.save(&make_checkpoint("proc-1", 1)).unwrap();

        let list = store.list("proc-1");
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].step_index, 0);
        assert_eq!(list[1].step_index, 1);
        assert_eq!(list[2].step_index, 2);
    }

    #[test]
    fn test_list_isolates_processes() {
        let store = CheckpointStore::in_memory().unwrap();
        store.save(&make_checkpoint("proc-a", 0)).unwrap();
        store.save(&make_checkpoint("proc-a", 1)).unwrap();
        store.save(&make_checkpoint("proc-b", 0)).unwrap();

        assert_eq!(store.list("proc-a").len(), 2);
        assert_eq!(store.list("proc-b").len(), 1);
    }

    #[test]
    fn test_delete_for_process() {
        let store = CheckpointStore::in_memory().unwrap();
        store.save(&make_checkpoint("proc-1", 0)).unwrap();
        store.save(&make_checkpoint("proc-1", 1)).unwrap();
        store.save(&make_checkpoint("proc-2", 0)).unwrap();

        store.delete_for_process("proc-1").unwrap();
        assert!(store.latest("proc-1").is_none());
        assert!(store.latest("proc-2").is_some());
    }

    #[test]
    fn test_save_overwrites_same_id() {
        let store = CheckpointStore::in_memory().unwrap();
        let mut cp = make_checkpoint("proc-1", 0);
        let id = cp.id.clone();
        cp.state = serde_json::json!({"version": 1});
        store.save(&cp).unwrap();

        cp.id = id;
        cp.state = serde_json::json!({"version": 2});
        store.save(&cp).unwrap();

        let list = store.list("proc-1");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].state["version"], 2);
    }

    #[test]
    fn test_checkpoint_state_roundtrip() {
        let store = CheckpointStore::in_memory().unwrap();
        let state = serde_json::json!({
            "messages": [{"role": "user", "content": "hello"}],
            "step": 5,
            "nested": {"a": [1, 2, 3]}
        });
        let mut cp = make_checkpoint("proc-1", 0);
        cp.state = state.clone();
        store.save(&cp).unwrap();

        let loaded = store.latest("proc-1").unwrap();
        assert_eq!(loaded.state, state);
    }
}
