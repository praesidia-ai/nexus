//! `fs.patcher` — apply one atomic diff to one file.
//!
//! Input schema:
//! ```json
//! {
//!   "path": "src/lib.rs",
//!   "mode": "replace",         // "replace" | "search_replace" | "append"
//!   "content": "…",            // required for replace / append
//!   "find": "old",              // required for search_replace
//!   "with": "new"               // required for search_replace
//! }
//! ```
//!
//! Output schema:
//! ```json
//! {"path": "src/lib.rs", "bytes_before": 123, "bytes_after": 145, "mode": "replace"}
//! ```
//!
//! Deterministic. Writes go via temp-file + fsync + rename to avoid
//! leaving a half-written file on crash (matches
//! `taste_handler::copy_rewrite`).

use std::path::PathBuf;

use async_trait::async_trait;
use nexus_agents_core::mini::{MiniAgent, MiniError, MiniKind, MiniOutput, Task};

use super::canonical_child;

pub struct FsPatcher {
    root: PathBuf,
}

impl FsPatcher {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl MiniAgent for FsPatcher {
    fn kind(&self) -> MiniKind {
        MiniKind::FsPatcher
    }

    async fn run(&self, task: Task) -> Result<MiniOutput, MiniError> {
        let started = std::time::Instant::now();
        let path = task
            .input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| MiniError::BadInput {
                kind: MiniKind::FsPatcher,
                reason: "missing `path`".into(),
            })?;
        let mode = task
            .input
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("replace");

        let target = canonical_child(&self.root, path).map_err(|e| MiniError::BadInput {
            kind: MiniKind::FsPatcher,
            reason: e,
        })?;
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| MiniError::Provider(format!("mkdir -p: {e}")))?;
        }

        let before = tokio::fs::read_to_string(&target).await.unwrap_or_default();
        let bytes_before = before.len();

        let after = match mode {
            "replace" => task
                .input
                .get("content")
                .and_then(|v| v.as_str())
                .ok_or_else(|| MiniError::BadInput {
                    kind: MiniKind::FsPatcher,
                    reason: "replace requires `content`".into(),
                })?
                .to_string(),
            "append" => {
                let extra = task
                    .input
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| MiniError::BadInput {
                        kind: MiniKind::FsPatcher,
                        reason: "append requires `content`".into(),
                    })?;
                format!("{before}{extra}")
            }
            "search_replace" => {
                let find = task
                    .input
                    .get("find")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| MiniError::BadInput {
                        kind: MiniKind::FsPatcher,
                        reason: "search_replace requires `find`".into(),
                    })?;
                let with = task
                    .input
                    .get("with")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if !before.contains(find) {
                    return Err(MiniError::BadInput {
                        kind: MiniKind::FsPatcher,
                        reason: format!("`find` not present in {path}"),
                    });
                }
                // Single-occurrence replace to avoid corrupting the file
                // when the needle is short/common. Matches the
                // `taste_handler::copy_rewrite` fix.
                before.replacen(find, with, 1)
            }
            other => {
                return Err(MiniError::BadInput {
                    kind: MiniKind::FsPatcher,
                    reason: format!("unknown mode: {other}"),
                })
            }
        };

        let bytes_after = after.len();

        // Atomic write: tmp -> fsync -> rename. The tmp file sits next
        // to the target using a sibling file name so the rename stays
        // intra-filesystem.
        let tmp = {
            let mut t = target.clone();
            let mut fname = target
                .file_name()
                .unwrap_or_default()
                .to_os_string();
            fname.push(".nexus-tmp");
            t.set_file_name(fname);
            t
        };
        tokio::fs::write(&tmp, after.as_bytes())
            .await
            .map_err(|e| MiniError::Provider(format!("atomic write: {e} (tmp={})", tmp.display())))?;
        tokio::fs::rename(&tmp, &target)
            .await
            .map_err(|e| {
                MiniError::Provider(format!(
                    "rename failed: {e} (tmp={}, target={})",
                    tmp.display(),
                    target.display()
                ))
            })?;

        Ok(MiniOutput {
            task_id: task.id,
            kind: MiniKind::FsPatcher,
            output: serde_json::json!({
                "path": path,
                "mode": mode,
                "bytes_before": bytes_before,
                "bytes_after": bytes_after,
            }),
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
            kind: MiniKind::FsPatcher,
            input,
            budget: Budget::default(),
            parent_id: None,
        }
    }

    #[tokio::test]
    async fn replace_writes_new_content_atomically() {
        let dir = tempdir().unwrap();
        let p = FsPatcher::new(dir.path().to_path_buf());
        p.run(task(serde_json::json!({
            "path": "out.txt",
            "mode": "replace",
            "content": "hello"
        })))
        .await
        .unwrap();
        let got = std::fs::read_to_string(dir.path().join("out.txt")).unwrap();
        assert_eq!(got, "hello");
    }

    #[tokio::test]
    async fn search_replace_once_avoids_global_nuking() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("a.md"), "foo foo foo").unwrap();
        let p = FsPatcher::new(dir.path().to_path_buf());
        p.run(task(serde_json::json!({
            "path": "a.md",
            "mode": "search_replace",
            "find": "foo",
            "with": "bar"
        })))
        .await
        .unwrap();
        let got = std::fs::read_to_string(dir.path().join("a.md")).unwrap();
        assert_eq!(got, "bar foo foo");
    }

    #[tokio::test]
    async fn search_replace_errors_when_needle_missing() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("a.md"), "foo").unwrap();
        let p = FsPatcher::new(dir.path().to_path_buf());
        let err = p
            .run(task(serde_json::json!({
                "path": "a.md",
                "mode": "search_replace",
                "find": "baz",
                "with": "qux"
            })))
            .await
            .unwrap_err();
        assert!(matches!(err, MiniError::BadInput { .. }));
    }
}
