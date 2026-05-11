//! Tool safety layer — validates tool inputs before execution.
//!
//! The [`ToolSafetyLayer`] checks file paths for traversal attacks,
//! shell commands for dangerous patterns, and enforces execution time limits.

use serde_json::Value;
use std::path::{Path, PathBuf};

/// Result of a safety check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SafetyResult {
    /// The tool call is safe to proceed.
    Allow,
    /// The tool call was blocked with the given reason.
    Deny(String),
}

/// Pre-execution safety validator for all agent tools.
///
/// Validates that:
/// - File paths stay within the allowed project directory
/// - Shell commands do not match dangerous patterns
/// - Execution timeouts are enforced
#[derive(Clone)]
pub struct ToolSafetyLayer {
    /// Directories the agent is allowed to access.
    allowed_paths: Vec<PathBuf>,
    /// Shell command patterns that are always blocked.
    blocked_commands: Vec<String>,
    /// Maximum wall-clock time for any tool execution (milliseconds).
    pub max_execution_ms: u64,
}

impl ToolSafetyLayer {
    /// Create a new safety layer scoped to a project directory.
    ///
    /// By default blocks destructive system commands and limits
    /// execution to 120 seconds.
    pub fn new(project_dir: &Path) -> Self {
        Self {
            allowed_paths: vec![project_dir.to_path_buf()],
            blocked_commands: vec![
                "rm -rf /".to_string(),
                "rm -rf ~".to_string(),
                "rm -rf /*".to_string(),
                "rm -rf .".to_string(),
                "rm -rf *".to_string(),
                "sudo".to_string(),
                "su -".to_string(),
                "su root".to_string(),
                "mkfs".to_string(),
                "dd if=".to_string(),
                "shutdown".to_string(),
                "reboot".to_string(),
                "poweroff".to_string(),
                "halt".to_string(),
                "init 0".to_string(),
                "init 6".to_string(),
                "format".to_string(),
                ":(){:|:&};:".to_string(),
                "chmod -R 777 /".to_string(),
                "chmod 777 /".to_string(),
                "chown -R".to_string(),
                "curl | sh".to_string(),
                "curl | bash".to_string(),
                "wget | sh".to_string(),
                "wget | bash".to_string(),
                "curl|sh".to_string(),
                "curl|bash".to_string(),
                "wget|sh".to_string(),
                "wget|bash".to_string(),
                "pip install".to_string(),
                "> /dev/sd".to_string(),
                "> /dev/null".to_string(),
                "mv /* ".to_string(),
                "mv / ".to_string(),
                "cp /dev/zero".to_string(),
                "kill -9 1".to_string(),
                "kill -9 -1".to_string(),
                "killall".to_string(),
                "pkill -9".to_string(),
                "nohup".to_string(),
                "crontab".to_string(),
                "at ".to_string(),
                "nc -l".to_string(),
                "netcat".to_string(),
                "ncat".to_string(),
                "python -c".to_string(),
                "python3 -c".to_string(),
                "perl -e".to_string(),
                "ruby -e".to_string(),
                "eval ".to_string(),
                "base64 -d".to_string(),
                "base64 --decode".to_string(),
                "xargs rm".to_string(),
                "find / -delete".to_string(),
                "find / -exec".to_string(),
                "iptables".to_string(),
                "ufw".to_string(),
                "systemctl".to_string(),
                "service ".to_string(),
                "mount ".to_string(),
                "umount ".to_string(),
                "fdisk".to_string(),
                "parted".to_string(),
                "passwd".to_string(),
                "useradd".to_string(),
                "userdel".to_string(),
                "groupadd".to_string(),
                "visudo".to_string(),
            ],
            max_execution_ms: 120_000,
        }
    }

    /// Add an additional allowed path.
    pub fn allow_path(&mut self, path: PathBuf) {
        self.allowed_paths.push(path);
    }

    /// Add a blocked command pattern.
    pub fn block_command(&mut self, pattern: String) {
        self.blocked_commands.push(pattern);
    }

    /// Check whether a tool invocation is safe.
    ///
    /// Returns [`SafetyResult::Allow`] if the call may proceed, or
    /// [`SafetyResult::Deny`] with an explanation string.
    pub fn check(&self, tool_name: &str, input: &Value) -> SafetyResult {
        match tool_name {
            "read_file" | "write_file" | "edit_file" | "file_info" => {
                self.check_file_path(input)
            }
            "list_directory" | "search_files" => self.check_file_path(input),
            "shell_command" | "npm_install" | "npm_run" | "run_tests" | "build_project" => {
                self.check_command(tool_name, input)
            }
            "http_request" | "web_fetch" => self.check_url(input),
            _ => SafetyResult::Allow,
        }
    }

    /// Validate that a file path in the input stays within allowed directories.
    fn check_file_path(&self, input: &Value) -> SafetyResult {
        let path_str = match input.get("path").and_then(|p| p.as_str()) {
            Some(p) => p,
            None => return SafetyResult::Allow, // no path param — let the tool handle it
        };

        // Block obvious traversal
        if path_str.contains("..") {
            return SafetyResult::Deny("Path traversal (..) is not allowed".to_string());
        }

        // Block absolute paths unless within allowed dirs
        if Path::new(path_str).is_absolute() {
            let abs = PathBuf::from(path_str);
            let within_allowed = self.allowed_paths.iter().any(|allowed| abs.starts_with(allowed));
            if !within_allowed {
                return SafetyResult::Deny(format!(
                    "Absolute path '{}' is outside allowed directories",
                    path_str
                ));
            }
        }

        // Block writing into sensitive directories
        let blocked_prefixes = [
            "node_modules",
            ".git",
            ".next",
            "target",
            "__pycache__",
        ];
        for prefix in &blocked_prefixes {
            if path_str.starts_with(prefix) {
                return SafetyResult::Deny(format!(
                    "Writing to '{}' directory is blocked",
                    prefix
                ));
            }
        }

        SafetyResult::Allow
    }

    /// Validate that a shell command does not match any blocked patterns.
    fn check_command(&self, _tool_name: &str, input: &Value) -> SafetyResult {
        let command = match input.get("command").and_then(|c| c.as_str()) {
            Some(c) => c,
            None => {
                // For npm/build tools, check the "script" or "packages" field instead
                return SafetyResult::Allow;
            }
        };

        let lower = command.to_lowercase();
        for blocked in &self.blocked_commands {
            if lower.contains(&blocked.to_lowercase()) {
                return SafetyResult::Deny(format!(
                    "Blocked command pattern detected: '{}'",
                    blocked
                ));
            }
        }

        SafetyResult::Allow
    }

    /// Validate URLs — block internal/private network access.
    fn check_url(&self, input: &Value) -> SafetyResult {
        let url = match input.get("url").and_then(|u| u.as_str()) {
            Some(u) => u,
            None => return SafetyResult::Allow,
        };

        let lower = url.to_lowercase();

        // Block obvious internal network targets
        let blocked_hosts = [
            "localhost",
            "127.0.0.1",
            "0.0.0.0",
            "[::1]",
            "169.254.",
            "10.",
            "192.168.",
            "172.16.",
            "172.17.",
            "172.18.",
            "172.19.",
            "172.20.",
            "172.21.",
            "172.22.",
            "172.23.",
            "172.24.",
            "172.25.",
            "172.26.",
            "172.27.",
            "172.28.",
            "172.29.",
            "172.30.",
            "172.31.",
        ];

        // Extract host portion (rough heuristic)
        let after_scheme = lower
            .strip_prefix("http://")
            .or_else(|| lower.strip_prefix("https://"))
            .unwrap_or(&lower);

        for blocked in &blocked_hosts {
            if after_scheme.starts_with(blocked) {
                return SafetyResult::Deny(format!(
                    "Access to internal/private network host '{}' is blocked",
                    blocked
                ));
            }
        }

        SafetyResult::Allow
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    fn layer() -> ToolSafetyLayer {
        ToolSafetyLayer::new(&PathBuf::from("/tmp/test-project"))
    }

    #[test]
    fn allows_normal_file_path() {
        let l = layer();
        let input = json!({ "path": "src/main.rs" });
        assert_eq!(l.check("read_file", &input), SafetyResult::Allow);
    }

    #[test]
    fn blocks_path_traversal() {
        let l = layer();
        let input = json!({ "path": "../../etc/passwd" });
        assert!(matches!(l.check("read_file", &input), SafetyResult::Deny(_)));
    }

    #[test]
    fn blocks_absolute_path_outside_project() {
        let l = layer();
        let input = json!({ "path": "/etc/passwd" });
        assert!(matches!(l.check("read_file", &input), SafetyResult::Deny(_)));
    }

    #[test]
    fn allows_absolute_path_inside_project() {
        let l = layer();
        let input = json!({ "path": "/tmp/test-project/src/main.rs" });
        assert_eq!(l.check("read_file", &input), SafetyResult::Allow);
    }

    #[test]
    fn blocks_writing_to_node_modules() {
        let l = layer();
        let input = json!({ "path": "node_modules/foo/bar.js" });
        assert!(matches!(l.check("write_file", &input), SafetyResult::Deny(_)));
    }

    #[test]
    fn blocks_dangerous_shell_command() {
        let l = layer();
        let input = json!({ "command": "rm -rf /" });
        assert!(matches!(l.check("shell_command", &input), SafetyResult::Deny(_)));
    }

    #[test]
    fn blocks_sudo_command() {
        let l = layer();
        let input = json!({ "command": "sudo apt-get install something" });
        assert!(matches!(l.check("shell_command", &input), SafetyResult::Deny(_)));
    }

    #[test]
    fn allows_safe_command() {
        let l = layer();
        let input = json!({ "command": "cargo build" });
        assert_eq!(l.check("shell_command", &input), SafetyResult::Allow);
    }

    #[test]
    fn blocks_internal_url() {
        let l = layer();
        let input = json!({ "url": "http://localhost:8020/api" });
        assert!(matches!(l.check("http_request", &input), SafetyResult::Deny(_)));
    }

    #[test]
    fn allows_external_url() {
        let l = layer();
        let input = json!({ "url": "https://api.github.com/repos" });
        assert_eq!(l.check("http_request", &input), SafetyResult::Allow);
    }

    #[test]
    fn blocks_fork_bomb() {
        let l = layer();
        let input = json!({ "command": ":(){:|:&};:" });
        assert!(matches!(l.check("shell_command", &input), SafetyResult::Deny(_)));
    }
}
