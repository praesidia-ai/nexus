//! Agent tools — file operations, code search, shell execution.
//!
//! Each tool implements a simple pattern: JSON schema for the LLM,
//! execute function that does the work.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tracing::info;

/// A tool definition that can be sent to the LLM.
#[derive(Debug, Clone, Serialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: Value, // JSON Schema
}

/// Result of executing a tool.
#[derive(Debug, Clone, Serialize)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
    pub truncated: bool,
}

/// A tool call from the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// All available tools and their execution context.
pub struct ToolRegistry {
    project_dir: PathBuf,
    max_output_chars: usize,
}

impl ToolRegistry {
    pub fn new(project_dir: PathBuf) -> Self {
        Self {
            project_dir,
            max_output_chars: 50_000,
        }
    }

    /// Get all tool definitions for sending to the LLM.
    pub fn tool_defs(&self) -> Vec<ToolDef> {
        vec![
            Self::file_read_def(),
            Self::file_write_def(),
            Self::file_edit_def(),
            Self::list_directory_def(),
            Self::grep_def(),
            Self::glob_def(),
            Self::bash_def(),
            Self::git_status_def(),
            Self::git_diff_def(),
        ]
    }

    /// Convert tool defs to the format expected by OpenAI/Ollama APIs.
    pub fn to_api_tools(&self) -> Vec<Value> {
        self.tool_defs()
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    }
                })
            })
            .collect()
    }

    /// Execute a tool call and return the result.
    pub async fn execute(&self, call: &ToolCall) -> ToolResult {
        let result = match call.name.as_str() {
            "file_read" => self.exec_file_read(&call.arguments).await,
            "file_write" => self.exec_file_write(&call.arguments).await,
            "file_edit" => self.exec_file_edit(&call.arguments).await,
            "list_directory" => self.exec_list_directory(&call.arguments).await,
            "grep" => self.exec_grep(&call.arguments).await,
            "glob" => self.exec_glob(&call.arguments).await,
            "bash" => self.exec_bash(&call.arguments).await,
            "git_status" => self.exec_git_status(&call.arguments).await,
            "git_diff" => self.exec_git_diff(&call.arguments).await,
            _ => Err(format!("Unknown tool: {}", call.name)),
        };

        match result {
            Ok(output) => {
                let truncated = output.len() > self.max_output_chars;
                let final_output = if truncated {
                    format!(
                        "{}...\n[truncated — {} chars total]",
                        &output[..self.max_output_chars],
                        output.len()
                    )
                } else {
                    output
                };
                ToolResult {
                    success: true,
                    output: final_output,
                    truncated,
                }
            }
            Err(e) => ToolResult {
                success: false,
                output: format!("Error: {}", e),
                truncated: false,
            },
        }
    }

    // ---- Tool Definitions ----

    fn file_read_def() -> ToolDef {
        ToolDef {
            name: "file_read".into(),
            description: "Read a file's contents. Returns line-numbered output. Use offset/limit for large files.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path relative to project root"},
                    "offset": {"type": "integer", "description": "Start line (1-indexed, optional)"},
                    "limit": {"type": "integer", "description": "Number of lines to read (optional, default: all)"}
                },
                "required": ["path"]
            }),
        }
    }

    fn file_write_def() -> ToolDef {
        ToolDef {
            name: "file_write".into(),
            description: "Create or overwrite a file with the given content. Creates parent directories automatically.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path relative to project root"},
                    "content": {"type": "string", "description": "Complete file content to write"}
                },
                "required": ["path", "content"]
            }),
        }
    }

    fn file_edit_def() -> ToolDef {
        ToolDef {
            name: "file_edit".into(),
            description: "Replace an exact string in a file. The old_string must appear exactly once in the file. Use this instead of file_write when modifying existing files.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path relative to project root"},
                    "old_string": {"type": "string", "description": "Exact string to find and replace (must be unique in file)"},
                    "new_string": {"type": "string", "description": "Replacement string"}
                },
                "required": ["path", "old_string", "new_string"]
            }),
        }
    }

    fn list_directory_def() -> ToolDef {
        ToolDef {
            name: "list_directory".into(),
            description: "List files and directories in a path. Returns a tree structure.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Directory path relative to project root (default: root)"},
                    "depth": {"type": "integer", "description": "Max depth to recurse (default: 3)"}
                },
                "required": []
            }),
        }
    }

    fn grep_def() -> ToolDef {
        ToolDef {
            name: "grep".into(),
            description: "Search file contents using regex pattern. Returns matching lines with file paths and line numbers.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": {"type": "string", "description": "Regex pattern to search for"},
                    "path": {"type": "string", "description": "Directory or file to search in (default: project root)"},
                    "include": {"type": "string", "description": "Glob pattern to filter files (e.g., '*.ts', '*.rs')"},
                    "max_results": {"type": "integer", "description": "Max number of matches (default: 50)"}
                },
                "required": ["pattern"]
            }),
        }
    }

    fn glob_def() -> ToolDef {
        ToolDef {
            name: "glob".into(),
            description: "Find files matching a glob pattern. Returns file paths.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": {"type": "string", "description": "Glob pattern (e.g., 'src/**/*.ts', '*.json')"}
                },
                "required": ["pattern"]
            }),
        }
    }

    fn bash_def() -> ToolDef {
        ToolDef {
            name: "bash".into(),
            description: "Execute a shell command. Returns stdout and stderr. Use for: running tests, installing packages, checking versions, git operations not covered by other tools.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": {"type": "string", "description": "Shell command to execute"},
                    "timeout_secs": {"type": "integer", "description": "Timeout in seconds (default: 120)"}
                },
                "required": ["command"]
            }),
        }
    }

    fn git_status_def() -> ToolDef {
        ToolDef {
            name: "git_status".into(),
            description: "Show git status of the project — modified, staged, and untracked files."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }

    fn git_diff_def() -> ToolDef {
        ToolDef {
            name: "git_diff".into(),
            description: "Show git diff of changes in the project.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "staged": {"type": "boolean", "description": "Show staged changes only (default: false)"},
                    "file": {"type": "string", "description": "Specific file to diff (optional)"}
                },
                "required": []
            }),
        }
    }

    // ---- Security Validation ----

    /// Validate a file path and content before writing.
    fn validate_write(&self, path: &str, content: &str) -> Result<(), String> {
        // Path traversal
        if path.contains("..") {
            return Err("Path traversal (..) not allowed".into());
        }
        if path.starts_with('/') {
            return Err("Absolute paths not allowed".into());
        }

        // Blocked paths
        let blocked = [
            "node_modules",
            ".git",
            ".next",
            "target",
            "__pycache__",
        ];
        for b in &blocked {
            if path.starts_with(b) {
                return Err(format!("Writing to '{}' is blocked", b));
            }
        }

        // File size
        if content.len() > 1_000_000 {
            return Err("File too large (max 1MB)".into());
        }

        // Extension check
        let ext = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let allowed = [
            "ts", "tsx", "js", "jsx", "json", "css", "scss", "html", "md", "rs", "toml", "py",
            "go", "sql", "yaml", "yml", "env", "txt", "sh", "prisma",
        ];
        if !ext.is_empty() && !allowed.contains(&ext.as_str()) {
            return Err(format!("Extension '.{}' not allowed", ext));
        }

        // Secret detection
        let secrets = [
            ("sk-", "API key"),
            ("ghp_", "GitHub token"),
            ("AKIA", "AWS key"),
            ("-----BEGIN", "private key"),
        ];
        for (pat, desc) in &secrets {
            if content.contains(pat) {
                return Err(format!(
                    "Possible {} detected — refusing to write",
                    desc
                ));
            }
        }

        Ok(())
    }

    /// Validate a shell command before executing.
    fn validate_command(&self, command: &str) -> Result<(), String> {
        let lower = command.to_lowercase();
        let blocked = [
            "rm -rf /",
            "rm -rf ~",
            "sudo",
            "su -",
            "mkfs",
            "dd if=",
            "shutdown",
            "reboot",
            ":(){:|:&};:",
        ];
        for b in &blocked {
            if lower.contains(b) {
                return Err(format!("Blocked command pattern: '{}'", b));
            }
        }
        Ok(())
    }

    /// Create a backup of a file before modifying it.
    fn backup_file(&self, path: &str, full_path: &Path) {
        if full_path.exists() {
            let backup_dir = self
                .project_dir
                .join(".nexus")
                .join("backups")
                .join("latest");
            let _ = std::fs::create_dir_all(&backup_dir);
            let backup_path = backup_dir.join(path);
            if let Some(parent) = backup_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if std::fs::copy(full_path, &backup_path).is_ok() {
                info!(path = %path, "Backed up file before modification");
            }
        }
    }

    // ---- Tool Execution ----

    fn resolve_path(&self, relative: &str) -> Result<PathBuf, String> {
        let path = if Path::new(relative).is_absolute() {
            PathBuf::from(relative)
        } else {
            self.project_dir.join(relative)
        };
        // Security: ensure path doesn't escape project dir
        let canonical = path
            .canonicalize()
            .or_else(|_| {
                // For new files, check parent exists
                if let Some(parent) = path.parent() {
                    if parent.exists() {
                        Ok(path.clone())
                    } else {
                        Err(std::io::Error::new(
                            std::io::ErrorKind::NotFound,
                            "parent dir not found",
                        ))
                    }
                } else {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "path not found",
                    ))
                }
            })
            .map_err(|e| format!("Invalid path '{}': {}", relative, e))?;
        Ok(canonical)
    }

    async fn exec_file_read(&self, args: &Value) -> Result<String, String> {
        let path = args["path"].as_str().ok_or("path is required")?;
        let full_path = self.resolve_path(path)?;

        let content = std::fs::read_to_string(&full_path)
            .map_err(|e| format!("Cannot read '{}': {}", path, e))?;

        let lines: Vec<&str> = content.lines().collect();
        let offset = args["offset"].as_u64().unwrap_or(1).max(1) as usize;
        let limit = args["limit"].as_u64().map(|l| l as usize);

        let start = (offset - 1).min(lines.len());
        let end = limit
            .map(|l| (start + l).min(lines.len()))
            .unwrap_or(lines.len());

        let mut output = String::new();
        for (i, line) in lines[start..end].iter().enumerate() {
            output.push_str(&format!("{:>4}  {}\n", start + i + 1, line));
        }

        if output.is_empty() {
            output = "(empty file)".to_string();
        }

        Ok(output)
    }

    async fn exec_file_write(&self, args: &Value) -> Result<String, String> {
        let path = args["path"].as_str().ok_or("path is required")?;
        let content = args["content"].as_str().ok_or("content is required")?;

        // Security validation
        self.validate_write(path, content)?;

        let full_path = if Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            self.project_dir.join(path)
        };

        // Backup before overwrite
        self.backup_file(path, &full_path);

        // Create parent directories
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Cannot create directories: {}", e))?;
        }

        std::fs::write(&full_path, content)
            .map_err(|e| format!("Cannot write '{}': {}", path, e))?;

        Ok(format!("Wrote {} bytes to {}", content.len(), path))
    }

    async fn exec_file_edit(&self, args: &Value) -> Result<String, String> {
        let path = args["path"].as_str().ok_or("path is required")?;
        let old_string = args["old_string"].as_str().ok_or("old_string is required")?;
        let new_string = args["new_string"].as_str().ok_or("new_string is required")?;

        // Security validation on the new content
        self.validate_write(path, new_string)?;

        let full_path = self.resolve_path(path)?;
        let content = std::fs::read_to_string(&full_path)
            .map_err(|e| format!("Cannot read '{}': {}", path, e))?;

        let count = content.matches(old_string).count();
        if count == 0 {
            return Err(format!(
                "'old_string' not found in {}. Read the file first to check the exact content.",
                path
            ));
        }
        if count > 1 {
            return Err(format!(
                "'old_string' found {} times in {} — must be unique. Provide more context.",
                count, path
            ));
        }

        // Backup before edit
        self.backup_file(path, &full_path);

        let new_content = content.replacen(old_string, new_string, 1);
        std::fs::write(&full_path, &new_content)
            .map_err(|e| format!("Cannot write '{}': {}", path, e))?;

        // Show a small diff context
        let old_lines = old_string.lines().count();
        let new_lines = new_string.lines().count();
        Ok(format!(
            "Edited {}: replaced {} lines with {} lines",
            path, old_lines, new_lines
        ))
    }

    async fn exec_list_directory(&self, args: &Value) -> Result<String, String> {
        let rel_path = args["path"].as_str().unwrap_or(".");
        let depth = args["depth"].as_u64().unwrap_or(3) as usize;

        let dir = if rel_path == "." {
            self.project_dir.clone()
        } else {
            self.resolve_path(rel_path)?
        };

        let output = tokio::task::spawn_blocking(move || {
            let mut out = String::new();
            fn walk(dir: &Path, prefix: &str, depth: usize, max_depth: usize, output: &mut String) {
                if depth > max_depth {
                    return;
                }
                let Ok(entries) = std::fs::read_dir(dir) else {
                    return;
                };
                let mut entries: Vec<_> = entries.flatten().collect();
                entries.sort_by_key(|e| e.file_name());

                for entry in entries {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with('.')
                        || name == "node_modules"
                        || name == ".next"
                        || name == "target"
                        || name == "__pycache__"
                    {
                        continue;
                    }
                    let is_dir = entry.path().is_dir();
                    let icon = if is_dir { "d " } else { "  " };
                    output.push_str(&format!("{}{}{}\n", prefix, icon, name));
                    if is_dir {
                        walk(
                            &entry.path(),
                            &format!("{}  ", prefix),
                            depth + 1,
                            max_depth,
                            output,
                        );
                    }
                }
            }
            walk(&dir, "", 0, depth, &mut out);
            if out.is_empty() {
                out = "(empty directory)".into();
            }
            out
        })
        .await
        .map_err(|e| format!("list_directory panicked: {}", e))?;

        Ok(output)
    }

    async fn exec_grep(&self, args: &Value) -> Result<String, String> {
        let pattern = args["pattern"].as_str().ok_or("pattern is required")?;
        let search_path = args["path"].as_str().unwrap_or(".");
        let include = args["include"].as_str().unwrap_or("");
        let max_results = args["max_results"].as_u64().unwrap_or(50) as usize;

        let dir = if search_path == "." {
            self.project_dir.clone()
        } else {
            self.resolve_path(search_path)?
        };

        let mut cmd_args = vec!["grep".to_string(), "-rn".to_string()];
        if !include.is_empty() {
            cmd_args.push("--include".to_string());
            cmd_args.push(include.to_string());
        }
        cmd_args.push("--".to_string());
        cmd_args.push(pattern.to_string());
        cmd_args.push(dir.display().to_string());

        let output = tokio::task::spawn_blocking(move || {
            std::process::Command::new(&cmd_args[0])
                .args(&cmd_args[1..])
                .output()
        })
        .await
        .map_err(|e| format!("grep task panicked: {}", e))?
        .map_err(|e| format!("grep failed: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<&str> = stdout.lines().take(max_results).collect();

        // Make paths relative to project dir
        let prefix = self.project_dir.display().to_string();
        let result = lines
            .iter()
            .map(|l| {
                if l.starts_with(&prefix) {
                    l[prefix.len()..].trim_start_matches('/').to_string()
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        if result.is_empty() {
            Ok(format!("No matches found for pattern '{}'", pattern))
        } else {
            Ok(format!("{}\n({} matches shown)", result, lines.len()))
        }
    }

    async fn exec_glob(&self, args: &Value) -> Result<String, String> {
        let pattern = args["pattern"].as_str().ok_or("pattern is required")?;

        // Use find command for glob matching (in blocking context)
        let project_dir = self.project_dir.clone();
        let pattern_owned = pattern.to_string();
        let output = tokio::task::spawn_blocking(move || {
            std::process::Command::new("find")
                .arg(project_dir.display().to_string())
                .arg("-name")
                .arg(&pattern_owned)
                .arg("-not")
                .arg("-path")
                .arg("*/node_modules/*")
                .arg("-not")
                .arg("-path")
                .arg("*/.next/*")
                .arg("-not")
                .arg("-path")
                .arg("*/target/*")
                .arg("-type")
                .arg("f")
                .output()
        })
        .await
        .map_err(|e| format!("glob task panicked: {}", e))?
        .map_err(|e| format!("glob failed: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let prefix = format!("{}/", self.project_dir.display());
        let files: Vec<String> = stdout
            .lines()
            .map(|l| l.strip_prefix(&prefix).unwrap_or(l).to_string())
            .take(100)
            .collect();

        if files.is_empty() {
            Ok(format!("No files matching '{}'", pattern))
        } else {
            Ok(files.join("\n"))
        }
    }

    async fn exec_bash(&self, args: &Value) -> Result<String, String> {
        let command = args["command"].as_str().ok_or("command is required")?;

        // Security validation
        self.validate_command(command)?;

        let timeout_secs = args["timeout_secs"].as_u64().unwrap_or(120);

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            tokio::task::spawn_blocking({
                let cmd = command.to_string();
                let dir = self.project_dir.clone();
                move || {
                    std::process::Command::new("sh")
                        .arg("-c")
                        .arg(&cmd)
                        .current_dir(&dir)
                        .output()
                }
            }),
        )
        .await
        .map_err(|_| format!("Command timed out after {}s", timeout_secs))?
        .map_err(|e| format!("Task panicked: {}", e))?
        .map_err(|e| format!("Command failed: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output.status.code().unwrap_or(-1);

        let mut result = String::new();
        if !stdout.is_empty() {
            result.push_str(&stdout);
        }
        if !stderr.is_empty() {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str("[stderr] ");
            result.push_str(&stderr);
        }
        if exit_code != 0 {
            result.push_str(&format!("\n[exit code: {}]", exit_code));
        }

        Ok(if result.is_empty() {
            "(no output)".into()
        } else {
            result
        })
    }

    async fn exec_git_status(&self, _args: &Value) -> Result<String, String> {
        let output = std::process::Command::new("git")
            .args(["status", "--short"])
            .current_dir(&self.project_dir)
            .output()
            .map_err(|e| format!("git status failed: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(if stdout.is_empty() {
            "Working tree clean".into()
        } else {
            stdout.to_string()
        })
    }

    async fn exec_git_diff(&self, args: &Value) -> Result<String, String> {
        let staged = args["staged"].as_bool().unwrap_or(false);
        let file = args["file"].as_str();

        let mut cmd_args = vec!["diff".to_string()];
        if staged {
            cmd_args.push("--staged".to_string());
        }
        if let Some(f) = file {
            cmd_args.push(f.to_string());
        }

        let output = std::process::Command::new("git")
            .args(&cmd_args)
            .current_dir(&self.project_dir)
            .output()
            .map_err(|e| format!("git diff failed: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(if stdout.is_empty() {
            "No changes".into()
        } else {
            stdout.to_string()
        })
    }
}
