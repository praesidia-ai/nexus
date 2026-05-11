//! Code execution tools — shell commands, npm scripts, test runners, builds.
//!
//! Every command is executed via `tokio::process::Command` with configurable
//! timeouts and captured stdout/stderr/exit-code.

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::agents::tools::registry::{Tool, ToolInput, ToolOutput};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a successful [`ToolOutput`].
#[allow(dead_code)]
fn ok(result: Value) -> ToolOutput {
    ToolOutput {
        result,
        success: true,
        error: None,
    }
}

/// Build a failed [`ToolOutput`].
fn err(msg: impl Into<String>) -> ToolOutput {
    ToolOutput {
        result: json!({}),
        success: false,
        error: Some(msg.into()),
    }
}

/// Run a shell command with timeout, returning structured output.
async fn run_command(
    command: &str,
    cwd: Option<&str>,
    timeout_secs: u64,
) -> ToolOutput {
    let cmd = command.to_string();
    let dir = cwd.map(|d| d.to_string());

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        tokio::task::spawn_blocking(move || {
            let mut proc = std::process::Command::new("sh");
            proc.arg("-c").arg(&cmd);
            if let Some(ref d) = dir {
                proc.current_dir(d);
            }
            proc.output()
        }),
    )
    .await;

    match result {
        Err(_) => err(format!("Command timed out after {}s", timeout_secs)),
        Ok(Err(e)) => err(format!("Task panicked: {}", e)),
        Ok(Ok(Err(e))) => err(format!("Command failed to start: {}", e)),
        Ok(Ok(Ok(output))) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let exit_code = output.status.code().unwrap_or(-1);

            let success = exit_code == 0;
            ToolOutput {
                result: json!({
                    "stdout": stdout,
                    "stderr": stderr,
                    "exit_code": exit_code
                }),
                success,
                error: if success {
                    None
                } else {
                    Some(format!("Process exited with code {}", exit_code))
                },
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ShellCommandTool
// ---------------------------------------------------------------------------

/// Run an arbitrary shell command within the project directory.
///
/// Captures stdout, stderr, and exit code. Supports configurable timeouts.
pub struct ShellCommandTool;

#[async_trait]
impl Tool for ShellCommandTool {
    fn name(&self) -> &str {
        "shell_command"
    }

    fn description(&self) -> &str {
        "Execute a shell command. Returns stdout, stderr, and exit code. Use for git, tests, builds, etc."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "Shell command to execute" },
                "cwd": { "type": "string", "description": "Working directory (optional)" },
                "timeout_secs": { "type": "integer", "description": "Timeout in seconds (default: 120)" }
            },
            "required": ["command"]
        })
    }

    fn category(&self) -> &str {
        "build_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let command = match input.parameters.get("command").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => return err("Missing required parameter: command"),
        };
        let cwd = input.parameters.get("cwd").and_then(|v| v.as_str());
        let timeout = input
            .parameters
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(120);

        run_command(command, cwd, timeout).await
    }
}

// ---------------------------------------------------------------------------
// NpmInstallTool
// ---------------------------------------------------------------------------

/// Install npm packages, optionally specifying package names.
///
/// With no `packages` parameter, runs `npm install`.
/// With a list, runs `npm install <pkg1> <pkg2> ...`.
pub struct NpmInstallTool;

#[async_trait]
impl Tool for NpmInstallTool {
    fn name(&self) -> &str {
        "npm_install"
    }

    fn description(&self) -> &str {
        "Install npm packages. Without packages list, runs 'npm install'. With list, installs specific packages."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "packages": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Package names to install (optional — omit for plain npm install)"
                },
                "dev": { "type": "boolean", "description": "Install as dev dependencies (default: false)" },
                "cwd": { "type": "string", "description": "Working directory (optional)" }
            },
            "required": []
        })
    }

    fn category(&self) -> &str {
        "build_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let mut cmd = String::from("npm install");

        if let Some(dev) = input.parameters.get("dev").and_then(|v| v.as_bool()) {
            if dev {
                cmd.push_str(" --save-dev");
            }
        }

        if let Some(pkgs) = input.parameters.get("packages").and_then(|v| v.as_array()) {
            for pkg in pkgs {
                if let Some(name) = pkg.as_str() {
                    cmd.push(' ');
                    cmd.push_str(name);
                }
            }
        }

        let cwd = input.parameters.get("cwd").and_then(|v| v.as_str());
        run_command(&cmd, cwd, 300).await
    }
}

// ---------------------------------------------------------------------------
// NpmRunTool
// ---------------------------------------------------------------------------

/// Run an npm script defined in `package.json`.
pub struct NpmRunTool;

#[async_trait]
impl Tool for NpmRunTool {
    fn name(&self) -> &str {
        "npm_run"
    }

    fn description(&self) -> &str {
        "Run an npm script (e.g. 'dev', 'build', 'lint'). Equivalent to 'npm run <script>'."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "script": { "type": "string", "description": "Script name from package.json" },
                "cwd": { "type": "string", "description": "Working directory (optional)" },
                "timeout_secs": { "type": "integer", "description": "Timeout in seconds (default: 120)" }
            },
            "required": ["script"]
        })
    }

    fn category(&self) -> &str {
        "build_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let script = match input.parameters.get("script").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => return err("Missing required parameter: script"),
        };
        let cwd = input.parameters.get("cwd").and_then(|v| v.as_str());
        let timeout = input
            .parameters
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(120);

        let cmd = format!("npm run {}", script);
        run_command(&cmd, cwd, timeout).await
    }
}

// ---------------------------------------------------------------------------
// RunTestsTool
// ---------------------------------------------------------------------------

/// Run project tests, auto-detecting the test framework.
///
/// Detection order: `cargo test` if `Cargo.toml` exists, `npm test` if
/// `package.json` exists, otherwise reports "no test framework detected".
pub struct RunTestsTool;

#[async_trait]
impl Tool for RunTestsTool {
    fn name(&self) -> &str {
        "run_tests"
    }

    fn description(&self) -> &str {
        "Run project tests. Auto-detects framework (cargo test, npm test, pytest, etc.)."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "filter": { "type": "string", "description": "Test name filter (optional)" },
                "cwd": { "type": "string", "description": "Working directory (optional)" },
                "timeout_secs": { "type": "integer", "description": "Timeout in seconds (default: 300)" }
            },
            "required": []
        })
    }

    fn category(&self) -> &str {
        "build_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let cwd = input
            .parameters
            .get("cwd")
            .and_then(|v| v.as_str())
            .unwrap_or(".");
        let filter = input.parameters.get("filter").and_then(|v| v.as_str());
        let timeout = input
            .parameters
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(300);

        let dir = std::path::Path::new(cwd);

        // Detect test framework
        let cmd = if dir.join("Cargo.toml").exists() {
            match filter {
                Some(f) => format!("cargo test {} -- --nocapture", f),
                None => "cargo test -- --nocapture".to_string(),
            }
        } else if dir.join("package.json").exists() {
            match filter {
                Some(f) => format!("npm test -- --grep '{}'", f),
                None => "npm test".to_string(),
            }
        } else if dir.join("pytest.ini").exists()
            || dir.join("setup.py").exists()
            || dir.join("pyproject.toml").exists()
        {
            match filter {
                Some(f) => format!("python -m pytest -k '{}'", f),
                None => "python -m pytest".to_string(),
            }
        } else {
            return err("No test framework detected. Provide a command via shell_command instead.");
        };

        run_command(&cmd, Some(cwd), timeout).await
    }
}

// ---------------------------------------------------------------------------
// BuildProjectTool
// ---------------------------------------------------------------------------

/// Build the project, auto-detecting the build system.
pub struct BuildProjectTool;

#[async_trait]
impl Tool for BuildProjectTool {
    fn name(&self) -> &str {
        "build_project"
    }

    fn description(&self) -> &str {
        "Build the project. Auto-detects build system (cargo build, npm run build, make, etc.)."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "release": { "type": "boolean", "description": "Build in release/production mode (default: false)" },
                "cwd": { "type": "string", "description": "Working directory (optional)" },
                "timeout_secs": { "type": "integer", "description": "Timeout in seconds (default: 300)" }
            },
            "required": []
        })
    }

    fn category(&self) -> &str {
        "build_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let cwd = input
            .parameters
            .get("cwd")
            .and_then(|v| v.as_str())
            .unwrap_or(".");
        let release = input
            .parameters
            .get("release")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let timeout = input
            .parameters
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(300);

        let dir = std::path::Path::new(cwd);

        let cmd = if dir.join("Cargo.toml").exists() {
            if release {
                "cargo build --release"
            } else {
                "cargo build"
            }
            .to_string()
        } else if dir.join("package.json").exists() {
            "npm run build".to_string()
        } else if dir.join("Makefile").exists() {
            "make".to_string()
        } else if dir.join("CMakeLists.txt").exists() {
            "cmake --build .".to_string()
        } else {
            return err("No build system detected. Use shell_command directly.");
        };

        run_command(&cmd, Some(cwd), timeout).await
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn input(params: Value) -> ToolInput {
        ToolInput { parameters: params }
    }

    #[tokio::test]
    async fn shell_command_executes_and_captures_output() {
        let tool = ShellCommandTool;
        let out = tool
            .execute(input(json!({ "command": "echo hello world" })))
            .await;

        assert!(out.success, "Expected success: {:?}", out.error);
        let stdout = out.result["stdout"].as_str().unwrap();
        assert!(stdout.contains("hello world"));
        assert_eq!(out.result["exit_code"], 0);
    }

    #[tokio::test]
    async fn shell_command_captures_stderr_and_exit_code() {
        let tool = ShellCommandTool;
        let out = tool
            .execute(input(json!({ "command": "echo err >&2; exit 42" })))
            .await;

        assert!(!out.success);
        assert_eq!(out.result["exit_code"], 42);
        let stderr = out.result["stderr"].as_str().unwrap();
        assert!(stderr.contains("err"));
    }

    #[tokio::test]
    async fn shell_command_timeout() {
        let tool = ShellCommandTool;
        let out = tool
            .execute(input(json!({
                "command": "sleep 10",
                "timeout_secs": 1
            })))
            .await;

        assert!(!out.success);
        assert!(out.error.unwrap().contains("timed out"));
    }

    #[tokio::test]
    async fn npm_run_tool_missing_script() {
        let tool = NpmRunTool;
        let out = tool.execute(input(json!({}))).await;
        assert!(!out.success);
    }

    #[tokio::test]
    async fn build_project_no_build_system() {
        let dir = tempfile::tempdir().unwrap();
        let tool = BuildProjectTool;
        let out = tool
            .execute(input(json!({ "cwd": dir.path().to_str().unwrap() })))
            .await;

        assert!(!out.success);
        assert!(out.error.unwrap().contains("No build system"));
    }
}
