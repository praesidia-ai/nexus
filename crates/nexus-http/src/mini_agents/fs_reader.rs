//! `fs.reader` — read + summarise one file to ≤ 400 tokens.
//!
//! Input schema:
//! ```json
//! {
//!   "path": "src/lib.rs",  // required, relative to project root
//!   "max_chars": 4000       // optional, default 4000
//! }
//! ```
//!
//! Output schema:
//! ```json
//! {
//!   "path": "src/lib.rs",
//!   "size_bytes": 12345,
//!   "line_count": 412,
//!   "head": "…first N chars of content…",
//!   "tail": "…last ~500 chars for context (if truncated)…",
//!   "truncated": true
//! }
//! ```
//!
//! Deterministic — no LLM call. The "summary" here is a structural
//! read: head + tail + metadata. The caller (usually a conductor) may
//! pass this to an LLM if it wants a natural-language summary.

use std::path::PathBuf;

use async_trait::async_trait;
use nexus_agents_core::mini::{MiniAgent, MiniError, MiniKind, MiniOutput, Task};

use super::canonical_child;

pub struct FsReader {
    root: PathBuf,
}

impl FsReader {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl MiniAgent for FsReader {
    fn kind(&self) -> MiniKind {
        MiniKind::FsReader
    }

    async fn run(&self, task: Task) -> Result<MiniOutput, MiniError> {
        let started = std::time::Instant::now();
        let path = task
            .input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| MiniError::BadInput {
                kind: MiniKind::FsReader,
                reason: "missing `path`".into(),
            })?;
        let max_chars = task
            .input
            .get("max_chars")
            .and_then(|v| v.as_u64())
            .unwrap_or(4000)
            .min(32_000) as usize;

        let target = canonical_child(&self.root, path).map_err(|e| MiniError::BadInput {
            kind: MiniKind::FsReader,
            reason: e,
        })?;

        let body = tokio::fs::read_to_string(&target)
            .await
            .map_err(|e| MiniError::Provider(format!("read {path}: {e}")))?;

        let size_bytes = body.len();
        let line_count = body.lines().count();
        let chars_total = body.chars().count();
        let truncated = chars_total > max_chars;
        let (head, tail) = if truncated {
            let head_take = max_chars.saturating_sub(500);
            let head: String = body.chars().take(head_take).collect();
            let tail_chars = body.chars().count().saturating_sub(500);
            let tail: String = body.chars().skip(tail_chars).collect();
            (head, Some(tail))
        } else {
            (body.clone(), None)
        };

        let out = serde_json::json!({
            "path": path,
            "size_bytes": size_bytes,
            "line_count": line_count,
            "head": head,
            "tail": tail,
            "truncated": truncated,
        });

        Ok(MiniOutput {
            task_id: task.id,
            kind: MiniKind::FsReader,
            output: out,
            tokens_used: 0,
            duration: started.elapsed(),
            cost_usd: 0.0,
            needs_review: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_agents_core::mini::Budget;
    use tempfile::tempdir;

    fn task(input: serde_json::Value) -> Task {
        Task {
            id: "t".into(),
            kind: MiniKind::FsReader,
            input,
            budget: Budget::default(),
            parent_id: None,
        }
    }

    #[tokio::test]
    async fn reads_small_file_untruncated() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("a.txt");
        std::fs::write(&p, "hello world\n").unwrap();
        let r = FsReader::new(dir.path().to_path_buf());
        let out = r.run(task(serde_json::json!({"path": "a.txt"}))).await.unwrap();
        assert_eq!(
            out.output.get("truncated").and_then(|v| v.as_bool()),
            Some(false)
        );
        assert_eq!(
            out.output.get("line_count").and_then(|v| v.as_u64()),
            Some(1)
        );
    }

    #[tokio::test]
    async fn truncates_large_file() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("big.txt");
        std::fs::write(&p, "x".repeat(5000)).unwrap();
        let r = FsReader::new(dir.path().to_path_buf());
        let out = r
            .run(task(serde_json::json!({"path": "big.txt", "max_chars": 500})))
            .await
            .unwrap();
        assert_eq!(
            out.output.get("truncated").and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    #[tokio::test]
    async fn rejects_paths_outside_root() {
        let dir = tempdir().unwrap();
        let r = FsReader::new(dir.path().to_path_buf());
        let err = r
            .run(task(serde_json::json!({"path": "../evil"})))
            .await
            .unwrap_err();
        assert!(matches!(err, MiniError::BadInput { .. }));
    }
}
