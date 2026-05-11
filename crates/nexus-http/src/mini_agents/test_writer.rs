//! `test.writer` — write one unit test for one function.
//!
//! v1.0 ships a deterministic skeleton generator (no LLM call) that
//! emits a placeholder test matching the project language. A later
//! revision routes through an LLM with the target function's source as
//! context — the trait contract doesn't change.
//!
//! Input schema:
//! ```json
//! {
//!   "language": "rust" | "typescript" | "python",
//!   "target_fn": "parse_config",
//!   "target_path": "src/config.rs",   // for import path inference
//!   "test_path": "tests/config_test.rs" // where to write
//! }
//! ```
//!
//! Output schema:
//! ```json
//! {"test_path": "tests/config_test.rs", "skeleton": "#[test]\nfn parse_config_works() { … }"}
//! ```

use std::path::PathBuf;

use async_trait::async_trait;
use nexus_agents_core::mini::{MiniAgent, MiniError, MiniKind, MiniOutput, Task};

use super::canonical_child;
use super::fs_patcher::FsPatcher;

pub struct TestWriter {
    patcher: FsPatcher,
    root: PathBuf,
}

impl TestWriter {
    pub fn new(root: PathBuf) -> Self {
        Self {
            patcher: FsPatcher::new(root.clone()),
            root,
        }
    }
}

#[async_trait]
impl MiniAgent for TestWriter {
    fn kind(&self) -> MiniKind {
        MiniKind::TestWriter
    }

    async fn run(&self, task: Task) -> Result<MiniOutput, MiniError> {
        let started = std::time::Instant::now();
        let language = task
            .input
            .get("language")
            .and_then(|v| v.as_str())
            .unwrap_or("rust");
        let target_fn = task
            .input
            .get("target_fn")
            .and_then(|v| v.as_str())
            .ok_or_else(|| MiniError::BadInput {
                kind: MiniKind::TestWriter,
                reason: "missing `target_fn`".into(),
            })?;
        let test_path = task
            .input
            .get("test_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| MiniError::BadInput {
                kind: MiniKind::TestWriter,
                reason: "missing `test_path`".into(),
            })?;

        canonical_child(&self.root, test_path).map_err(|e| MiniError::BadInput {
            kind: MiniKind::TestWriter,
            reason: e,
        })?;

        let skeleton = match language {
            "typescript" | "javascript" => format!(
                "import {{ describe, it, expect }} from 'vitest';\n\
                 import {{ {target_fn} }} from '../src';\n\n\
                 describe('{target_fn}', () => {{\n\
                 \tit('works on the happy path', () => {{\n\
                 \t\tconst result = {target_fn}();\n\
                 \t\texpect(result).toBeDefined();\n\
                 \t}});\n\
                 }});\n"
            ),
            "python" => format!(
                "def test_{target_fn}_happy_path():\n\
                 \tfrom src import {target_fn}\n\
                 \tassert {target_fn}() is not None\n"
            ),
            _ => format!(
                "#[cfg(test)]\nmod tests {{\n\
                 \tuse super::*;\n\n\
                 \t#[test]\n\
                 \tfn {target_fn}_works() {{\n\
                 \t\t// Arrange\n\
                 \t\t// Act\n\
                 \t\tlet _got = {target_fn}();\n\
                 \t\t// Assert\n\
                 \t}}\n\
                 }}\n"
            ),
        };

        // Write via the patcher to reuse its atomic + sandbox guarantees.
        let write_task = Task {
            id: format!("{}-write", task.id),
            kind: MiniKind::FsPatcher,
            input: serde_json::json!({
                "path": test_path,
                "mode": "replace",
                "content": skeleton,
            }),
            budget: task.budget.clone(),
            parent_id: Some(task.id.clone()),
        };
        self.patcher.run(write_task).await?;

        Ok(MiniOutput {
            task_id: task.id,
            kind: MiniKind::TestWriter,
            output: serde_json::json!({
                "test_path": test_path,
                "skeleton": skeleton,
            }),
            tokens_used: 0,
            duration: started.elapsed(),
            cost_usd: 0.0,
            // Skeleton tests are placeholders — always ask the
            // conductor to review before promoting to the test suite.
            needs_review: true,
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
            kind: MiniKind::TestWriter,
            input,
            budget: Budget::default(),
            parent_id: None,
        }
    }

    #[tokio::test]
    async fn writes_rust_skeleton_by_default() {
        let dir = tempdir().unwrap();
        let w = TestWriter::new(dir.path().to_path_buf());
        w.run(task(serde_json::json!({
            "target_fn": "parse_config",
            "test_path": "tests/config_test.rs"
        })))
        .await
        .unwrap();
        let got = std::fs::read_to_string(dir.path().join("tests/config_test.rs")).unwrap();
        assert!(got.contains("parse_config_works"));
        assert!(got.contains("#[test]"));
    }

    #[tokio::test]
    async fn supports_typescript() {
        let dir = tempdir().unwrap();
        let w = TestWriter::new(dir.path().to_path_buf());
        let out = w
            .run(task(serde_json::json!({
                "language": "typescript",
                "target_fn": "compute",
                "test_path": "tests/compute.test.ts"
            })))
            .await
            .unwrap();
        assert!(out.needs_review);
        let got = std::fs::read_to_string(dir.path().join("tests/compute.test.ts")).unwrap();
        assert!(got.contains("describe('compute'"));
    }
}
