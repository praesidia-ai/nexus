use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use rusqlite::Connection;
use tokio::sync::{Mutex, OwnedMutexGuard, OwnedSemaphorePermit, Semaphore};

use crate::db::run_migrations;
use crate::error::Result;

const DEFAULT_READERS: usize = 4;

const WRITER_PRAGMAS: &str = "\
    PRAGMA journal_mode=WAL;\
    PRAGMA synchronous=NORMAL;\
    PRAGMA busy_timeout=5000;\
    PRAGMA cache_size=-64000;\
    PRAGMA mmap_size=268435456;\
    PRAGMA foreign_keys=ON;\
";

const READER_PRAGMAS: &str = "\
    PRAGMA query_only=ON;\
    PRAGMA busy_timeout=5000;\
    PRAGMA cache_size=-64000;\
    PRAGMA mmap_size=268435456;\
    PRAGMA foreign_keys=ON;\
";

pub struct ConnectionPool {
    writer: Arc<Mutex<Connection>>,
    readers: Vec<Arc<Mutex<Connection>>>,
    reader_sem: Arc<Semaphore>,
    next_reader: AtomicUsize,
}

/// RAII guard that holds both a semaphore permit and a connection lock.
/// Dropping it releases the reader back to the pool.
pub struct ReaderGuard {
    _permit: OwnedSemaphorePermit,
    guard: OwnedMutexGuard<Connection>,
}

impl std::ops::Deref for ReaderGuard {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        &self.guard
    }
}

impl ConnectionPool {
    /// Open a pool with 1 writer + `reader_count` reader connections.
    /// Migrations run once on the writer before readers are opened.
    pub fn new(path: &Path, reader_count: Option<usize>) -> Result<Self> {
        let count = reader_count.unwrap_or(DEFAULT_READERS).max(1);

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| crate::error::StoreError::Msg(format!("create_dir_all: {e}")))?;
        }

        let writer_conn = Connection::open(path)?;
        writer_conn.execute_batch(WRITER_PRAGMAS)?;
        run_migrations(&writer_conn)?;

        let mut readers = Vec::with_capacity(count);
        for _ in 0..count {
            let rc = Connection::open(path)?;
            rc.execute_batch(READER_PRAGMAS)?;
            readers.push(Arc::new(Mutex::new(rc)));
        }

        Ok(Self {
            writer: Arc::new(Mutex::new(writer_conn)),
            readers,
            reader_sem: Arc::new(Semaphore::new(count)),
            next_reader: AtomicUsize::new(0),
        })
    }

    pub async fn acquire_writer(&self) -> OwnedMutexGuard<Connection> {
        Arc::clone(&self.writer).lock_owned().await
    }

    /// Acquire a reader connection via round-robin.
    /// The returned [`ReaderGuard`] holds a semaphore permit; dropping it
    /// releases the slot back to the pool.
    pub async fn acquire_reader(&self) -> ReaderGuard {
        let permit = Arc::clone(&self.reader_sem)
            .acquire_owned()
            .await
            .expect("semaphore closed");

        let idx = self.next_reader.fetch_add(1, Ordering::Relaxed) % self.readers.len();
        let guard = Arc::clone(&self.readers[idx]).lock_owned().await;

        ReaderGuard {
            _permit: permit,
            guard,
        }
    }

    pub fn reader_count(&self) -> usize {
        self.readers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn pool_opens_and_migrates() {
        let dir = tempdir().unwrap();
        let pool = ConnectionPool::new(&dir.path().join("test.db"), Some(2)).unwrap();

        assert_eq!(pool.reader_count(), 2);

        let writer = pool.acquire_writer().await;
        let v: i64 = writer
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(v >= 1);
    }

    #[tokio::test]
    async fn reader_is_query_only() {
        let dir = tempdir().unwrap();
        let pool = ConnectionPool::new(&dir.path().join("ro.db"), Some(1)).unwrap();

        let reader = pool.acquire_reader().await;
        let res = reader.execute("CREATE TABLE _nope (id TEXT)", []);
        assert!(res.is_err(), "reader should reject writes");
    }

    #[tokio::test]
    async fn concurrent_readers() {
        let dir = tempdir().unwrap();
        let pool = Arc::new(ConnectionPool::new(&dir.path().join("conc.db"), Some(4)).unwrap());

        let mut handles = Vec::new();
        for _ in 0..8 {
            let p = Arc::clone(&pool);
            handles.push(tokio::spawn(async move {
                let r = p.acquire_reader().await;
                let _: i64 = r
                    .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                        row.get(0)
                    })
                    .unwrap();
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
    }
}
