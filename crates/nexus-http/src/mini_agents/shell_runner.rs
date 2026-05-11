//! `shell.runner` — run one allow-listed shell command inside the
//! project root.
//!
//! Input schema:
//! ```json
//! {
//!   "cmd": "cargo",          // must be in ALLOWED_COMMANDS
//!   "args": ["test", "--lib"], // optional
//!   "timeout_secs": 60        // optional, default 60, max 300
//! }
//! ```
//!
//! Output schema:
//! ```json
//! {
//!   "cmd": "cargo",
//!   "exit_code": 0,
//!   "stdout": "…",   // truncated to 8 KB
//!   "stderr": "…",   // truncated to 8 KB
//!   "duration_ms": 1234
//! }
//! ```
//!
//! Security: the command must be in the allow-list. Arguments are
//! passed via `Command::args` (no shell, no `sh -c`) so user-supplied
//! strings can't inject arbitrary commands (the fix pattern from the
//! round-2 deploy-handler hardening).

use std::path::PathBuf;

use async_trait::async_trait;
use nexus_agents_core::mini::{MiniAgent, MiniError, MiniKind, MiniOutput, Task};

const ALLOWED_COMMANDS: &[&str] = &[
    // Build / test runners
    "cargo", "npm", "pnpm", "yarn", "bun", "node", "deno",
    "python", "python3", "pip", "uv", "pytest", "poetry",
    "go", "pytest", "vitest", "jest", "ruby", "bundle",
    // Read-only source inspection
    "git", "grep", "rg", "fd", "ls", "cat", "head", "tail",
    // Lint / format
    "eslint", "prettier", "ruff", "black", "mypy", "tsc",
    "cargo-clippy",
];

pub struct ShellRunner {
    root: PathBuf,
}

impl ShellRunner {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl MiniAgent for ShellRunner {
    fn kind(&self) -> MiniKind {
        MiniKind::ShellRunner
    }

    async fn run(&self, task: Task) -> Result<MiniOutput, MiniError> {
        let started = std::time::Instant::now();
        let cmd = task
            .input
            .get("cmd")
            .and_then(|v| v.as_str())
            .ok_or_else(|| MiniError::BadInput {
                kind: MiniKind::ShellRunner,
                reason: "missing `cmd`".into(),
            })?;

        if !ALLOWED_COMMANDS.contains(&cmd) {
            return Err(MiniError::BadInput {
                kind: MiniKind::ShellRunner,
                reason: format!(
                    "command `{cmd}` not allow-listed — see shell_runner::ALLOWED_COMMANDS"
                ),
            });
        }

        let args: Vec<String> = task
            .input
            .get("args")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let timeout_secs = task
            .input
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(60)
            .min(300);

        let mut proc = tokio::process::Command::new(cmd);
        proc.args(&args)
            .current_dir(&self.root)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let output_fut = async {
            proc.output()
                .await
                .map_err(|e| MiniError::Provider(format!("spawn {cmd}: {e}")))
        };
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            output_fut,
        )
        .await
        .map_err(|_| MiniError::BudgetExceeded {
            dimension: "wall_clock",
        })??;

        let stdout = truncate_utf8(&String::from_utf8_lossy(&output.stdout), 8_192);
        let stderr = truncate_utf8(&String::from_utf8_lossy(&output.stderr), 8_192);
        let exit_code = output.status.code().unwrap_or(-1);
        let duration_ms = started.elapsed().as_millis() as u64;

        // Non-zero exit is a signal, not a mini-agent failure — the
        // conductor may legitimately ask `shell.runner` to run a test
        // and observe failure. Mark it `needs_review` so the conductor
        // knows to look.
        Ok(MiniOutput {
            task_id: task.id,
            kind: MiniKind::ShellRunner,
            output: serde_json::json!({
                "cmd": cmd,
                "args": args,
                "exit_code": exit_code,
                "stdout": stdout,
                "stderr": stderr,
                "duration_ms": duration_ms,
            }),
            tokens_used: 0,
            duration: started.elapsed(),
            cost_usd: 0.0,
            needs_review: exit_code != 0,
        })
    }
}

fn truncate_utf8(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars).collect();
    out.push_str("…[truncated]");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_agents_core::mini::Budget;
    use tempfile::tempdir;

    fn task(input: serde_json::Value) -> Task {
        Task {
            id: "t".into(),
            kind: MiniKind::ShellRunner,
            input,
            budget: Budget::default(),
            parent_id: None,
        }
    }

    #[tokio::test]
    async fn rejects_commands_not_in_allowlist() {
        let dir = tempdir().unwrap();
        let r = ShellRunner::new(dir.path().to_path_buf());
        let err = r
            .run(task(serde_json::json!({"cmd": "rm", "args": ["-rf", "/"]})))
            .await
            .unwrap_err();
        assert!(matches!(err, MiniError::BadInput { .. }));
    }

    #[tokio::test]
    async fn runs_ls_successfully() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("marker.txt"), "hi").unwrap();
        let r = ShellRunner::new(dir.path().to_path_buf());
        let out = r.run(task(serde_json::json!({"cmd": "ls"}))).await.unwrap();
        let exit = out.output.get("exit_code").and_then(|v| v.as_i64()).unwrap();
        assert_eq!(exit, 0);
        let stdout = out.output.get("stdout").and_then(|v| v.as_str()).unwrap();
        assert!(stdout.contains("marker.txt"));
        assert!(!out.needs_review);
    }

    #[tokio::test]
    async fn nonzero_exit_flags_needs_review() {
        let dir = tempdir().unwrap();
        let r = ShellRunner::new(dir.path().to_path_buf());
        let out = r
            .run(task(serde_json::json!({
                "cmd": "cat",
                "args": ["does-not-exist.txt"]
            })))
            .await
            .unwrap();
        let exit = out.output.get("exit_code").and_then(|v| v.as_i64()).unwrap();
        assert_ne!(exit, 0);
        assert!(out.needs_review);
    }
}
