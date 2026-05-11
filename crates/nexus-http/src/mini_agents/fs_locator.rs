//! `fs.locator` — given a spec, return a ranked list of candidate file paths.
//!
//! Input schema:
//! ```json
//! {
//!   "glob": "**/*.rs",      // optional
//!   "contains": "impl Foo", // optional literal substring
//!   "limit": 20             // optional, default 20, max 200
//! }
//! ```
//!
//! Output schema:
//! ```json
//! {"paths": ["src/lib.rs", "src/module.rs"]}
//! ```
//!
//! Deterministic — does not touch an LLM. Walks the project root with
//! `walkdir`, applies the glob (if any), and grep-matches the literal
//! `contains` substring. Paths are returned sorted by (shorter path
//! first, then lexicographic).

use std::path::PathBuf;

use async_trait::async_trait;
use nexus_agents_core::mini::{MiniAgent, MiniError, MiniKind, MiniOutput, Task};
use walkdir::WalkDir;

pub struct FsLocator {
    root: PathBuf,
}

impl FsLocator {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl MiniAgent for FsLocator {
    fn kind(&self) -> MiniKind {
        MiniKind::FsLocator
    }

    async fn run(&self, task: Task) -> Result<MiniOutput, MiniError> {
        let started = std::time::Instant::now();

        let glob_pattern = task
            .input
            .get("glob")
            .and_then(|v| v.as_str())
            .unwrap_or("**/*");
        let contains = task.input.get("contains").and_then(|v| v.as_str());
        let limit = task
            .input
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(20)
            .min(200) as usize;

        let matcher = globset::Glob::new(glob_pattern)
            .map_err(|e| MiniError::BadInput {
                kind: MiniKind::FsLocator,
                reason: format!("bad glob: {e}"),
            })?
            .compile_matcher();

        let root = self.root.clone();
        let contains_owned = contains.map(|s| s.to_string());
        let paths: Vec<String> = tokio::task::spawn_blocking(move || -> Vec<String> {
            let mut out: Vec<(usize, String)> = Vec::new();
            for entry in WalkDir::new(&root)
                .into_iter()
                .filter_entry(|e| {
                    // Skip common noise directories up-front.
                    let name = e.file_name().to_string_lossy();
                    !(matches!(
                        name.as_ref(),
                        ".git" | "node_modules" | "target" | ".next" | "dist" | "build"
                    ))
                })
                .filter_map(|r| r.ok())
            {
                if !entry.file_type().is_file() {
                    continue;
                }
                let Ok(rel) = entry.path().strip_prefix(&root) else {
                    continue;
                };
                let rel_str = rel.to_string_lossy().to_string();
                if !matcher.is_match(&rel_str) {
                    continue;
                }
                if let Some(needle) = contains_owned.as_deref() {
                    match std::fs::read_to_string(entry.path()) {
                        Ok(body) => {
                            if !body.contains(needle) {
                                continue;
                            }
                        }
                        Err(_) => continue,
                    }
                }
                out.push((rel_str.len(), rel_str));
            }
            out.sort();
            out.into_iter().take(limit).map(|(_, p)| p).collect()
        })
        .await
        .map_err(|e| MiniError::Internal(format!("locator join: {e}")))?;

        Ok(MiniOutput {
            task_id: task.id,
            kind: MiniKind::FsLocator,
            output: serde_json::json!({"paths": paths}),
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
            kind: MiniKind::FsLocator,
            input,
            budget: Budget::default(),
            parent_id: None,
        }
    }

    #[tokio::test]
    async fn locates_matching_files_by_glob() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "").unwrap();
        std::fs::write(dir.path().join("b.rs"), "").unwrap();
        std::fs::write(dir.path().join("c.md"), "").unwrap();

        let loc = FsLocator::new(dir.path().to_path_buf());
        let out = loc
            .run(task(serde_json::json!({"glob": "**/*.rs"})))
            .await
            .unwrap();
        let paths = out.output.get("paths").unwrap().as_array().unwrap();
        assert_eq!(paths.len(), 2);
    }

    #[tokio::test]
    async fn filters_by_contains() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("needle.rs"), "NEEDLE").unwrap();
        std::fs::write(dir.path().join("other.rs"), "nope").unwrap();

        let loc = FsLocator::new(dir.path().to_path_buf());
        let out = loc
            .run(task(serde_json::json!({"glob": "**/*.rs", "contains": "NEEDLE"})))
            .await
            .unwrap();
        let paths = out.output.get("paths").unwrap().as_array().unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], "needle.rs");
    }
}
