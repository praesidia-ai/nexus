//! Advisory per-project generation lock.
//!
//! Prevents two concurrent generation requests from writing to the same
//! project's `generated/` directory simultaneously, which would produce
//! a corrupted mix of files from both runs.
//!
//! The lock is stored in the `active_generations` SQLite table.
//! Stale locks older than `STALE_LOCK_TIMEOUT_SECS` are ignored and overwritten
//! so a crashed server cannot permanently block a project.

use rusqlite::Connection;

use crate::error::Result;

const STALE_LOCK_TIMEOUT_SECS: u64 = 600;

pub struct GenerationLockService<'a> {
    conn: &'a Connection,
}

impl<'a> GenerationLockService<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Try to acquire the generation lock for `project_id`.
    ///
    /// Returns `Ok(true)` if the lock was acquired, `Ok(false)` if another
    /// active (non-stale) generation is already running for this project.
    pub fn try_acquire(&self, project_id: &str, description: &str) -> Result<bool> {
        // Clean up stale locks first (older than STALE_LOCK_TIMEOUT_SECS).
        self.conn.execute(
            "DELETE FROM active_generations
             WHERE project_id = ?1
               AND (julianday('now') - julianday(started_at)) * 86400 > ?2",
            rusqlite::params![project_id, STALE_LOCK_TIMEOUT_SECS as f64],
        )?;

        // Attempt INSERT — fails if a fresh lock already exists.
        let inserted = self.conn.execute(
            "INSERT OR IGNORE INTO active_generations (project_id, description)
             VALUES (?1, ?2)",
            rusqlite::params![project_id, description],
        )?;

        Ok(inserted == 1)
    }

    /// Release the generation lock for `project_id`.
    pub fn release(&self, project_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM active_generations WHERE project_id = ?1",
            rusqlite::params![project_id],
        )?;
        Ok(())
    }

    /// Returns true if a non-stale lock exists for `project_id`.
    pub fn is_locked(&self, project_id: &str) -> Result<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM active_generations
             WHERE project_id = ?1
               AND (julianday('now') - julianday(started_at)) * 86400 <= ?2",
            rusqlite::params![project_id, STALE_LOCK_TIMEOUT_SECS as f64],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Release all stale locks — called on server startup.
    pub fn cleanup_stale(conn: &Connection) -> Result<usize> {
        let removed = conn.execute(
            "DELETE FROM active_generations
             WHERE (julianday('now') - julianday(started_at)) * 86400 > ?1",
            rusqlite::params![STALE_LOCK_TIMEOUT_SECS as f64],
        )?;
        Ok(removed)
    }
}
