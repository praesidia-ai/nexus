//! Deterministic Execution Core — agents propose, core validates and executes.
//!
//! Every file modification goes through this pipeline:
//! 1. Agent submits a Proposal (what it wants to do)
//! 2. Core validates the proposal (safety, correctness, conflicts)
//! 3. Core executes atomically (all-or-nothing)
//! 4. Core returns a Receipt (what happened, what changed)

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Proposal: what the agent wants to do
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub id: String,
    pub intent: String,
    pub operations: Vec<Operation>,
    pub constraints: Vec<String>,
    pub rollback_plan: Vec<Operation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Operation {
    #[serde(rename = "file_write")]
    FileWrite { path: String, content: String },
    #[serde(rename = "file_edit")]
    FileEdit {
        path: String,
        old_string: String,
        new_string: String,
    },
    #[serde(rename = "file_delete")]
    FileDelete { path: String },
    #[serde(rename = "bash")]
    Bash {
        command: String,
        timeout_secs: Option<u64>,
    },
}

// ---------------------------------------------------------------------------
// Validation: is this proposal safe?
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
    pub risk_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub code: String,
    pub message: String,
    pub operation_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationWarning {
    pub code: String,
    pub message: String,
    pub operation_index: usize,
}

// ---------------------------------------------------------------------------
// Receipt: what actually happened
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionReceipt {
    pub proposal_id: String,
    pub status: String,
    pub operations_applied: usize,
    pub operations_failed: usize,
    pub files_changed: Vec<String>,
    pub backup_dir: Option<String>,
    pub errors: Vec<String>,
    pub duration_ms: u64,
}

// ---------------------------------------------------------------------------
// Core Engine
// ---------------------------------------------------------------------------

pub struct ExecutionCore {
    project_dir: PathBuf,
    security: SecurityConfig,
}

#[derive(Debug, Clone)]
pub struct SecurityConfig {
    /// Allowed file extensions for writing.
    pub allowed_extensions: Vec<String>,
    /// Blocked paths (never write/delete).
    pub blocked_paths: Vec<String>,
    /// Blocked bash commands.
    pub blocked_commands: Vec<String>,
    /// Max file size in bytes.
    pub max_file_size: usize,
    /// Max operations per proposal.
    pub max_operations: usize,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            allowed_extensions: vec![
                "ts", "tsx", "js", "jsx", "json", "css", "scss", "html", "md", "rs", "toml", "py",
                "go", "java", "sql", "prisma", "graphql", "yaml", "yml", "env", "txt", "sh",
                "dockerfile",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            blocked_paths: vec![
                ".git",
                ".env",
                "node_modules",
                ".next",
                "target",
                "__pycache__",
                ".nexus/brain.json",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            blocked_commands: vec![
                "rm -rf /",
                "rm -rf ~",
                "rm -rf .",
                ":(){:|:&};:",
                "mkfs",
                "dd if=",
                "wget",
                "curl.*|.*sh",
                "chmod 777",
                "> /dev/sda",
                "shutdown",
                "reboot",
                "kill -9 1",
                "sudo",
                "su -",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            max_file_size: 1_000_000,
            max_operations: 50,
        }
    }
}

impl ExecutionCore {
    pub fn new(project_dir: PathBuf) -> Self {
        Self {
            project_dir,
            security: SecurityConfig::default(),
        }
    }

    pub fn with_security(mut self, config: SecurityConfig) -> Self {
        self.security = config;
        self
    }

    /// Validate a proposal without executing it.
    pub fn validate(&self, proposal: &Proposal) -> ValidationResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Check operation count
        if proposal.operations.len() > self.security.max_operations {
            errors.push(ValidationError {
                code: "TOO_MANY_OPS".into(),
                message: format!(
                    "Proposal has {} operations (max: {})",
                    proposal.operations.len(),
                    self.security.max_operations
                ),
                operation_index: 0,
            });
        }

        for (i, op) in proposal.operations.iter().enumerate() {
            match op {
                Operation::FileWrite { path, content } => {
                    self.validate_path(path, i, &mut errors, &mut warnings);
                    self.validate_extension(path, i, &mut errors);
                    if content.len() > self.security.max_file_size {
                        errors.push(ValidationError {
                            code: "FILE_TOO_LARGE".into(),
                            message: format!(
                                "{}: {} bytes exceeds max {}",
                                path,
                                content.len(),
                                self.security.max_file_size
                            ),
                            operation_index: i,
                        });
                    }
                    self.check_secrets(content, path, i, &mut warnings);
                }
                Operation::FileEdit {
                    path,
                    old_string,
                    new_string,
                } => {
                    self.validate_path(path, i, &mut errors, &mut warnings);
                    // Verify old_string exists and is unique
                    let full_path = self.project_dir.join(path);
                    if let Ok(content) = std::fs::read_to_string(&full_path) {
                        let count = content.matches(old_string.as_str()).count();
                        if count == 0 {
                            errors.push(ValidationError {
                                code: "OLD_STRING_NOT_FOUND".into(),
                                message: format!("{}: old_string not found in file", path),
                                operation_index: i,
                            });
                        } else if count > 1 {
                            errors.push(ValidationError {
                                code: "OLD_STRING_AMBIGUOUS".into(),
                                message: format!(
                                    "{}: old_string found {} times (must be unique)",
                                    path, count
                                ),
                                operation_index: i,
                            });
                        }
                    } else {
                        errors.push(ValidationError {
                            code: "FILE_NOT_FOUND".into(),
                            message: format!("{}: file does not exist for editing", path),
                            operation_index: i,
                        });
                    }
                    self.check_secrets(new_string, path, i, &mut warnings);
                }
                Operation::FileDelete { path } => {
                    self.validate_path(path, i, &mut errors, &mut warnings);
                    let full_path = self.project_dir.join(path);
                    if !full_path.exists() {
                        warnings.push(ValidationWarning {
                            code: "DELETE_NONEXISTENT".into(),
                            message: format!("{}: file doesn't exist", path),
                            operation_index: i,
                        });
                    }
                }
                Operation::Bash { command, .. } => {
                    self.validate_command(command, i, &mut errors, &mut warnings);
                }
            }
        }

        let risk = if !errors.is_empty() {
            "critical"
        } else if warnings.len() > 5 {
            "high"
        } else if warnings.len() > 2 {
            "medium"
        } else if !warnings.is_empty() {
            "low"
        } else {
            "safe"
        };

        ValidationResult {
            valid: errors.is_empty(),
            errors,
            warnings,
            risk_level: risk.to_string(),
        }
    }

    /// Execute a validated proposal. Creates backups before modifying files.
    pub async fn execute(&self, proposal: &Proposal) -> ExecutionReceipt {
        let start = std::time::Instant::now();
        let mut files_changed = Vec::new();
        let mut errors = Vec::new();
        let mut applied = 0usize;
        let mut failed = 0usize;

        // Create backup directory
        let backup_dir = self
            .project_dir
            .join(".nexus")
            .join("backups")
            .join(&proposal.id);
        let _ = std::fs::create_dir_all(&backup_dir);

        for op in &proposal.operations {
            match self.execute_operation(op, &backup_dir).await {
                Ok(changed_file) => {
                    applied += 1;
                    if let Some(f) = changed_file {
                        files_changed.push(f);
                    }
                }
                Err(e) => {
                    failed += 1;
                    errors.push(e);
                    // Don't continue after a failure — rollback
                    break;
                }
            }
        }

        let status = if failed > 0 && applied > 0 {
            // Partial failure — attempt rollback
            self.rollback(&backup_dir, &files_changed);
            "rolled_back"
        } else if failed > 0 {
            "rejected"
        } else {
            "applied"
        };

        ExecutionReceipt {
            proposal_id: proposal.id.clone(),
            status: status.to_string(),
            operations_applied: applied,
            operations_failed: failed,
            files_changed,
            backup_dir: Some(backup_dir.display().to_string()),
            errors,
            duration_ms: start.elapsed().as_millis() as u64,
        }
    }

    async fn execute_operation(
        &self,
        op: &Operation,
        backup_dir: &Path,
    ) -> Result<Option<String>, String> {
        match op {
            Operation::FileWrite { path, content } => {
                let full_path = self.project_dir.join(path);

                // Backup existing file
                if full_path.exists() {
                    let backup_path = backup_dir.join(path);
                    if let Some(parent) = backup_path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let _ = std::fs::copy(&full_path, &backup_path);
                }

                // Create parent dirs
                if let Some(parent) = full_path.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("mkdir: {}", e))?;
                }

                std::fs::write(&full_path, content)
                    .map_err(|e| format!("write {}: {}", path, e))?;
                info!(path = %path, bytes = content.len(), "File written");
                Ok(Some(path.clone()))
            }
            Operation::FileEdit {
                path,
                old_string,
                new_string,
            } => {
                let full_path = self.project_dir.join(path);
                let content = std::fs::read_to_string(&full_path)
                    .map_err(|e| format!("read {}: {}", path, e))?;

                // Backup
                let backup_path = backup_dir.join(path);
                if let Some(parent) = backup_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(&backup_path, &content);

                // Apply edit
                let new_content = content.replacen(old_string, new_string, 1);
                std::fs::write(&full_path, &new_content)
                    .map_err(|e| format!("write {}: {}", path, e))?;
                info!(path = %path, "File edited");
                Ok(Some(path.clone()))
            }
            Operation::FileDelete { path } => {
                let full_path = self.project_dir.join(path);
                if full_path.exists() {
                    // Backup
                    let backup_path = backup_dir.join(path);
                    if let Some(parent) = backup_path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let _ = std::fs::copy(&full_path, &backup_path);

                    std::fs::remove_file(&full_path)
                        .map_err(|e| format!("delete {}: {}", path, e))?;
                    info!(path = %path, "File deleted");
                }
                Ok(Some(path.clone()))
            }
            Operation::Bash {
                command,
                timeout_secs,
            } => {
                let timeout = std::time::Duration::from_secs(timeout_secs.unwrap_or(120));
                let dir = self.project_dir.clone();
                let cmd = command.clone();

                let result = tokio::time::timeout(
                    timeout,
                    tokio::task::spawn_blocking(move || {
                        std::process::Command::new("sh")
                            .arg("-c")
                            .arg(&cmd)
                            .current_dir(&dir)
                            .output()
                    }),
                )
                .await
                .map_err(|_| "Command timed out".to_string())?
                .map_err(|e| format!("Task panicked: {}", e))?
                .map_err(|e| format!("Command failed: {}", e))?;

                if !result.status.success() {
                    let stderr = String::from_utf8_lossy(&result.stderr);
                    return Err(format!(
                        "Command exited with {}: {}",
                        result.status.code().unwrap_or(-1),
                        stderr
                    ));
                }
                Ok(None)
            }
        }
    }

    fn rollback(&self, backup_dir: &Path, files_changed: &[String]) {
        for file in files_changed {
            let backup_path = backup_dir.join(file);
            let target_path = self.project_dir.join(file);
            if backup_path.exists() {
                let _ = std::fs::copy(&backup_path, &target_path);
                warn!(path = %file, "Rolled back");
            }
        }
    }

    // ---- Validation helpers ----

    fn validate_path(
        &self,
        path: &str,
        idx: usize,
        errors: &mut Vec<ValidationError>,
        warnings: &mut Vec<ValidationWarning>,
    ) {
        let _ = warnings; // used below for future expansion
        // Path traversal check
        if path.contains("..") {
            errors.push(ValidationError {
                code: "PATH_TRAVERSAL".into(),
                message: format!(
                    "{}: path contains '..' — directory traversal blocked",
                    path
                ),
                operation_index: idx,
            });
        }
        // Absolute path check
        if path.starts_with('/') {
            errors.push(ValidationError {
                code: "ABSOLUTE_PATH".into(),
                message: format!("{}: absolute paths not allowed", path),
                operation_index: idx,
            });
        }
        // Blocked paths
        for blocked in &self.security.blocked_paths {
            if path.starts_with(blocked) || path.contains(&format!("/{}/", blocked)) {
                errors.push(ValidationError {
                    code: "BLOCKED_PATH".into(),
                    message: format!("{}: writing to '{}' is blocked", path, blocked),
                    operation_index: idx,
                });
            }
        }
    }

    fn validate_extension(&self, path: &str, idx: usize, errors: &mut Vec<ValidationError>) {
        let ext = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if !ext.is_empty()
            && !self
                .security
                .allowed_extensions
                .iter()
                .any(|a| a == &ext)
        {
            errors.push(ValidationError {
                code: "BLOCKED_EXTENSION".into(),
                message: format!("{}: extension '.{}' not in allowlist", path, ext),
                operation_index: idx,
            });
        }
    }

    fn validate_command(
        &self,
        command: &str,
        idx: usize,
        errors: &mut Vec<ValidationError>,
        warnings: &mut Vec<ValidationWarning>,
    ) {
        let lower = command.to_lowercase();
        for blocked in &self.security.blocked_commands {
            if lower.contains(&blocked.to_lowercase()) {
                errors.push(ValidationError {
                    code: "BLOCKED_COMMAND".into(),
                    message: format!("Command contains blocked pattern: '{}'", blocked),
                    operation_index: idx,
                });
            }
        }
        // Warn on potentially dangerous commands
        if lower.contains("rm ") && !lower.contains("rm -rf node_modules") {
            warnings.push(ValidationWarning {
                code: "DESTRUCTIVE_COMMAND".into(),
                message: "Command uses 'rm' — verify this is intentional".to_string(),
                operation_index: idx,
            });
        }
        if lower.contains("git push") || lower.contains("git reset") {
            warnings.push(ValidationWarning {
                code: "GIT_MUTATION".into(),
                message: "Command modifies git history — verify this is intentional".into(),
                operation_index: idx,
            });
        }
    }

    fn check_secrets(
        &self,
        content: &str,
        path: &str,
        idx: usize,
        warnings: &mut Vec<ValidationWarning>,
    ) {
        let patterns = [
            ("sk-", "OpenAI API key"),
            ("sk-ant-", "Anthropic API key"),
            ("ghp_", "GitHub token"),
            ("AKIA", "AWS access key"),
            ("-----BEGIN", "Private key"),
            ("password=", "Hardcoded password"),
            ("secret=", "Hardcoded secret"),
        ];
        for (pattern, desc) in &patterns {
            if content.contains(pattern) {
                warnings.push(ValidationWarning {
                    code: "POTENTIAL_SECRET".into(),
                    message: format!("{}: possible {} detected ('{}')", path, desc, pattern),
                    operation_index: idx,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_core(dir: &Path) -> ExecutionCore {
        ExecutionCore::new(dir.to_path_buf())
    }

    #[test]
    fn rejects_path_traversal() {
        let dir = std::env::temp_dir().join("exec_core_test_traversal");
        let _ = std::fs::create_dir_all(&dir);
        let core = test_core(&dir);
        let proposal = Proposal {
            id: "t1".into(),
            intent: "test".into(),
            operations: vec![Operation::FileWrite {
                path: "../escape.txt".into(),
                content: "bad".into(),
            }],
            constraints: vec![],
            rollback_plan: vec![],
        };
        let result = core.validate(&proposal);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.code == "PATH_TRAVERSAL"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_blocked_path() {
        let dir = std::env::temp_dir().join("exec_core_test_blocked");
        let _ = std::fs::create_dir_all(&dir);
        let core = test_core(&dir);
        let proposal = Proposal {
            id: "t2".into(),
            intent: "test".into(),
            operations: vec![Operation::FileWrite {
                path: ".git/config".into(),
                content: "bad".into(),
            }],
            constraints: vec![],
            rollback_plan: vec![],
        };
        let result = core.validate(&proposal);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.code == "BLOCKED_PATH"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_blocked_command() {
        let dir = std::env::temp_dir().join("exec_core_test_cmd");
        let _ = std::fs::create_dir_all(&dir);
        let core = test_core(&dir);
        let proposal = Proposal {
            id: "t3".into(),
            intent: "test".into(),
            operations: vec![Operation::Bash {
                command: "sudo rm -rf /".into(),
                timeout_secs: None,
            }],
            constraints: vec![],
            rollback_plan: vec![],
        };
        let result = core.validate(&proposal);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.code == "BLOCKED_COMMAND"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn accepts_safe_proposal() {
        let dir = std::env::temp_dir().join("exec_core_test_safe");
        let _ = std::fs::create_dir_all(&dir);
        let core = test_core(&dir);
        let proposal = Proposal {
            id: "t4".into(),
            intent: "test".into(),
            operations: vec![Operation::FileWrite {
                path: "src/main.ts".into(),
                content: "console.log('hello');".into(),
            }],
            constraints: vec![],
            rollback_plan: vec![],
        };
        let result = core.validate(&proposal);
        assert!(result.valid);
        assert_eq!(result.risk_level, "safe");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn warns_on_secrets() {
        let dir = std::env::temp_dir().join("exec_core_test_secrets");
        let _ = std::fs::create_dir_all(&dir);
        let core = test_core(&dir);
        let proposal = Proposal {
            id: "t5".into(),
            intent: "test".into(),
            operations: vec![Operation::FileWrite {
                path: "config.ts".into(),
                content: "const key = 'sk-abc123';".into(),
            }],
            constraints: vec![],
            rollback_plan: vec![],
        };
        let result = core.validate(&proposal);
        assert!(result.valid); // warnings don't block
        assert!(!result.warnings.is_empty());
        assert!(result.warnings.iter().any(|w| w.code == "POTENTIAL_SECRET"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn executes_and_creates_backup() {
        let dir = std::env::temp_dir().join("exec_core_test_exec");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);

        // Create an existing file to back up
        let _ = std::fs::create_dir_all(dir.join("src"));
        std::fs::write(dir.join("src/existing.ts"), "old content").unwrap();

        let core = test_core(&dir);
        let proposal = Proposal {
            id: "exec1".into(),
            intent: "test write".into(),
            operations: vec![Operation::FileWrite {
                path: "src/existing.ts".into(),
                content: "new content".into(),
            }],
            constraints: vec![],
            rollback_plan: vec![],
        };

        let receipt = core.execute(&proposal).await;
        assert_eq!(receipt.status, "applied");
        assert_eq!(receipt.operations_applied, 1);
        assert_eq!(receipt.operations_failed, 0);

        // Verify file was written
        let written = std::fs::read_to_string(dir.join("src/existing.ts")).unwrap();
        assert_eq!(written, "new content");

        // Verify backup was created
        let backup = std::fs::read_to_string(
            dir.join(".nexus/backups/exec1/src/existing.ts"),
        )
        .unwrap();
        assert_eq!(backup, "old content");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_blocked_extension() {
        let dir = std::env::temp_dir().join("exec_core_test_ext");
        let _ = std::fs::create_dir_all(&dir);
        let core = test_core(&dir);
        let proposal = Proposal {
            id: "t6".into(),
            intent: "test".into(),
            operations: vec![Operation::FileWrite {
                path: "malware.exe".into(),
                content: "bad".into(),
            }],
            constraints: vec![],
            rollback_plan: vec![],
        };
        let result = core.validate(&proposal);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.code == "BLOCKED_EXTENSION"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_too_many_operations() {
        let dir = std::env::temp_dir().join("exec_core_test_ops");
        let _ = std::fs::create_dir_all(&dir);
        let core = test_core(&dir);
        let ops: Vec<Operation> = (0..51)
            .map(|i| Operation::FileWrite {
                path: format!("file{}.ts", i),
                content: "x".into(),
            })
            .collect();
        let proposal = Proposal {
            id: "t7".into(),
            intent: "test".into(),
            operations: ops,
            constraints: vec![],
            rollback_plan: vec![],
        };
        let result = core.validate(&proposal);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.code == "TOO_MANY_OPS"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
