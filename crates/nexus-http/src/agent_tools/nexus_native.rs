//! Nexus-native tools — taste scoring and runtime management.
//!
//! These tools expose Nexus-specific capabilities that are not generic
//! developer tools but rather part of the Nexus platform itself.

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::agents::tools::registry::{Tool, ToolInput, ToolOutput};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn ok(result: Value) -> ToolOutput {
    ToolOutput {
        result,
        success: true,
        error: None,
    }
}

fn err(msg: impl Into<String>) -> ToolOutput {
    ToolOutput {
        result: json!({}),
        success: false,
        error: Some(msg.into()),
    }
}

// ---------------------------------------------------------------------------
// ScoreTasteTool
// ---------------------------------------------------------------------------

/// Run the Nexus taste scoring heuristic on a project's UI files.
///
/// Taste scoring analyzes HTML/CSS/JSX files for visual quality signals
/// such as spacing consistency, color palette, font hierarchy, and
/// responsive design patterns.
pub struct ScoreTasteTool;

#[async_trait]
impl Tool for ScoreTasteTool {
    fn name(&self) -> &str {
        "score_taste"
    }

    fn description(&self) -> &str {
        "Run taste scoring on project files. Analyzes UI quality signals: spacing, color, typography, responsiveness."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Directory or file to score (default: '.')" },
                "include": { "type": "string", "description": "Glob pattern for files to analyze (default: '*.{tsx,jsx,html,css}')" },
                "threshold": { "type": "number", "description": "Minimum acceptable score 0-100 (default: 70)" }
            },
            "required": []
        })
    }

    fn category(&self) -> &str {
        "project_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let path = input
            .parameters
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");
        let include = input
            .parameters
            .get("include")
            .and_then(|v| v.as_str())
            .unwrap_or("*.tsx");
        let threshold = input
            .parameters
            .get("threshold")
            .and_then(|v| v.as_f64())
            .unwrap_or(70.0);

        // Collect files to analyze
        let dir = std::path::Path::new(path);
        if !dir.exists() {
            return err(format!("Path '{}' does not exist", path));
        }

        let files = collect_files(dir, include);
        if files.is_empty() {
            return ok(json!({
                "score": 100.0,
                "pass": true,
                "files_analyzed": 0,
                "message": "No matching files to score"
            }));
        }

        let mut total_score = 0.0;
        let mut file_scores = Vec::new();
        let mut issues = Vec::new();

        for file_path in &files {
            let content = match std::fs::read_to_string(file_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let score = score_file_taste(&content);
            let file_issues = detect_taste_issues(&content);

            file_scores.push(json!({
                "file": file_path.display().to_string(),
                "score": score
            }));
            total_score += score;

            for issue in file_issues {
                issues.push(json!({
                    "file": file_path.display().to_string(),
                    "issue": issue
                }));
            }
        }

        let avg_score = if files.is_empty() {
            100.0
        } else {
            total_score / files.len() as f64
        };
        let pass = avg_score >= threshold;

        ok(json!({
            "score": (avg_score * 10.0).round() / 10.0,
            "pass": pass,
            "threshold": threshold,
            "files_analyzed": files.len(),
            "file_scores": file_scores,
            "issues": issues
        }))
    }
}

/// Collect files matching a simple extension-based glob.
fn collect_files(dir: &std::path::Path, include: &str) -> Vec<std::path::PathBuf> {
    let extensions: Vec<&str> = include
        .trim_start_matches("*.")
        .trim_start_matches("{")
        .trim_end_matches("}")
        .split(',')
        .collect();

    let mut result = Vec::new();
    walk_for_files(dir, &extensions, &mut result, 0, 5);
    result
}

fn walk_for_files(
    dir: &std::path::Path,
    extensions: &[&str],
    result: &mut Vec<std::path::PathBuf>,
    depth: usize,
    max_depth: usize,
) {
    if depth > max_depth {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    let skip = ["node_modules", ".git", ".next", "target", "__pycache__"];

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if skip.contains(&name.as_str()) || name.starts_with('.') {
            continue;
        }

        let path = entry.path();
        if path.is_dir() {
            walk_for_files(&path, extensions, result, depth + 1, max_depth);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if extensions.iter().any(|e| e.trim() == ext) {
                result.push(path);
            }
        }
    }
}

/// Heuristic taste score for a single file (0-100).
fn score_file_taste(content: &str) -> f64 {
    let mut score = 80.0; // start with a baseline

    let lines: Vec<&str> = content.lines().collect();

    // Penalize very long lines (readability)
    let long_lines = lines.iter().filter(|l| l.len() > 120).count();
    score -= (long_lines as f64 * 0.5).min(10.0);

    // Penalize inline styles in JSX/HTML
    let inline_styles = content.matches("style={{").count() + content.matches("style=\"").count();
    score -= (inline_styles as f64 * 2.0).min(15.0);

    // Reward Tailwind usage (modern CSS approach)
    let tailwind_classes = content.matches("className=").count();
    if tailwind_classes > 0 {
        score += 5.0;
    }

    // Penalize magic numbers in CSS values
    let magic_px = content.matches("px").count();
    if magic_px > 20 {
        score -= 5.0;
    }

    // Penalize !important usage
    let importants = content.matches("!important").count();
    score -= (importants as f64 * 3.0).min(10.0);

    // Penalize deeply nested JSX (complexity)
    let max_indent = lines
        .iter()
        .map(|l| l.len() - l.trim_start().len())
        .max()
        .unwrap_or(0);
    if max_indent > 40 {
        score -= 5.0;
    }

    // Reward semantic HTML elements
    let semantic = ["<header", "<nav", "<main", "<section", "<article", "<footer", "<aside"];
    let semantic_count = semantic
        .iter()
        .filter(|s| content.contains(*s))
        .count();
    score += (semantic_count as f64 * 2.0).min(10.0);

    // Reward responsive patterns
    if content.contains("@media") || content.contains("md:") || content.contains("lg:") {
        score += 5.0;
    }

    score.clamp(0.0, 100.0)
}

/// Detect specific taste issues in a file.
fn detect_taste_issues(content: &str) -> Vec<String> {
    let mut issues = Vec::new();

    if content.matches("!important").count() > 0 {
        issues.push("Uses !important — consider refactoring CSS specificity".to_string());
    }
    if content.matches("style={{").count() > 3 {
        issues.push("Excessive inline styles — extract to CSS/Tailwind classes".to_string());
    }
    let long_lines = content.lines().filter(|l| l.len() > 120).count();
    if long_lines > 5 {
        issues.push(format!(
            "{} lines exceed 120 characters — consider breaking up",
            long_lines
        ));
    }
    if content.contains("color: #") && content.matches("color: #").count() > 5 {
        issues.push("Many hardcoded colors — consider using CSS variables or theme".to_string());
    }

    issues
}

// ---------------------------------------------------------------------------
// ManageRuntimeTool
// ---------------------------------------------------------------------------

/// Manage the app runtime — start, stop, restart, or check status.
///
/// This tool controls the development server process (e.g. `npm run dev`,
/// `cargo run`) that is spawned for live preview.
pub struct ManageRuntimeTool;

#[async_trait]
impl Tool for ManageRuntimeTool {
    fn name(&self) -> &str {
        "manage_runtime"
    }

    fn description(&self) -> &str {
        "Manage the app runtime: start, stop, restart, or check status of the dev server."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "Action to perform",
                    "enum": ["start", "stop", "restart", "status"]
                },
                "command": { "type": "string", "description": "Start command (e.g. 'npm run dev'). Only needed for 'start'." },
                "port": { "type": "integer", "description": "Port the server runs on (for health check, default: 3000)" },
                "cwd": { "type": "string", "description": "Working directory (optional)" }
            },
            "required": ["action"]
        })
    }

    fn category(&self) -> &str {
        "project_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let action = match input.parameters.get("action").and_then(|v| v.as_str()) {
            Some(a) => a,
            None => return err("Missing required parameter: action"),
        };

        // Validate port strictly as u16 to prevent argv injection via
        // non-numeric JSON (serde already enforces u64, but we require 1..=65535).
        let port_raw = input
            .parameters
            .get("port")
            .and_then(|v| v.as_u64())
            .unwrap_or(3000);
        if port_raw == 0 || port_raw > 65_535 {
            return err(format!("Invalid port: {port_raw}"));
        }
        let port: u16 = port_raw as u16;

        // Agent-supplied start commands go through `sh -c` because dev server
        // commands like "npm run dev -- --port 3000" need shell parsing.
        // Size-cap the string and reject NULs so it can't be abused to
        // smuggle absurd payloads.
        fn validate_command(cmd: &str) -> Result<(), String> {
            if cmd.is_empty() {
                return Err("command is empty".into());
            }
            if cmd.len() > 8192 {
                return Err("command too long".into());
            }
            if cmd.contains('\0') {
                return Err("command contains NUL".into());
            }
            Ok(())
        }

        fn kill_port(port: u16) {
            // Resolve PIDs via lsof without shell interpolation, then kill(1)
            // each PID as a separate argv element.
            let lsof = std::process::Command::new("lsof")
                .args(["-ti", &format!(":{port}")])
                .output();
            if let Ok(out) = lsof {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let pids: Vec<String> = stdout
                    .split_whitespace()
                    .filter(|s| s.chars().all(|c| c.is_ascii_digit()))
                    .map(|s| s.to_string())
                    .collect();
                if !pids.is_empty() {
                    let _ = std::process::Command::new("kill")
                        .arg("-TERM")
                        .args(&pids)
                        .output();
                }
            }
        }

        match action {
            "status" => {
                // Check if the port is in use (simple TCP connect check)
                let addr = std::net::SocketAddr::from(([127u8, 0, 0, 1], port));
                let is_running = std::net::TcpStream::connect_timeout(
                    &addr,
                    std::time::Duration::from_secs(2),
                )
                .is_ok();

                ok(json!({
                    "action": "status",
                    "port": port,
                    "running": is_running
                }))
            }
            "start" => {
                let command = match input.parameters.get("command").and_then(|v| v.as_str()) {
                    Some(c) => c.to_string(),
                    None => return err("'command' is required for 'start' action"),
                };
                if let Err(e) = validate_command(&command) {
                    return err(e);
                }
                let cwd = input
                    .parameters
                    .get("cwd")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                // Spawn the process in the background
                let mut proc = std::process::Command::new("sh");
                proc.arg("-c").arg(&command);
                if let Some(ref dir) = cwd {
                    proc.current_dir(dir);
                }
                proc.stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null());

                match proc.spawn() {
                    Ok(child) => ok(json!({
                        "action": "start",
                        "pid": child.id(),
                        "command": command,
                        "port": port
                    })),
                    Err(e) => err(format!("Failed to start process: {}", e)),
                }
            }
            "stop" => {
                kill_port(port);
                ok(json!({
                    "action": "stop",
                    "port": port,
                    "message": "Stop signal sent"
                }))
            }
            "restart" => {
                kill_port(port);

                // Wait briefly for port to free up
                std::thread::sleep(std::time::Duration::from_millis(500));

                let command = match input.parameters.get("command").and_then(|v| v.as_str()) {
                    Some(c) => c.to_string(),
                    None => {
                        return ok(json!({
                            "action": "restart",
                            "port": port,
                            "message": "Stopped. Provide 'command' to auto-restart."
                        }))
                    }
                };
                if let Err(e) = validate_command(&command) {
                    return err(e);
                }

                let cwd = input
                    .parameters
                    .get("cwd")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let mut proc = std::process::Command::new("sh");
                proc.arg("-c").arg(&command);
                if let Some(ref dir) = cwd {
                    proc.current_dir(dir);
                }
                proc.stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null());

                match proc.spawn() {
                    Ok(child) => ok(json!({
                        "action": "restart",
                        "pid": child.id(),
                        "command": command,
                        "port": port
                    })),
                    Err(e) => err(format!("Failed to restart: {}", e)),
                }
            }
            _ => err(format!("Unknown action: '{}'. Use start, stop, restart, or status.", action)),
        }
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

    #[test]
    fn taste_score_clean_file() {
        let content = r#"
            <main className="flex flex-col md:gap-4">
                <header className="p-4">
                    <h1 className="text-2xl font-bold">Title</h1>
                </header>
                <section className="grid lg:grid-cols-2 gap-4">
                    <article className="p-4 rounded-lg">Content</article>
                </section>
            </main>
        "#;
        let score = score_file_taste(content);
        assert!(score >= 70.0, "Expected >= 70, got {}", score);
    }

    #[test]
    fn taste_score_penalizes_inline_styles() {
        let content = r#"
            <div style={{ color: 'red' }}>
                <div style={{ margin: '10px' }}>
                    <span style={{ fontSize: '14px' }}>
                        <p style={{ padding: '5px' }}>Bad</p>
                    </span>
                </div>
            </div>
        "#;
        let clean = "<main className=\"flex\"><p>Good</p></main>";
        let score_bad = score_file_taste(content);
        let score_good = score_file_taste(clean);
        assert!(
            score_good > score_bad,
            "Clean ({}) should score higher than inline-styled ({})",
            score_good,
            score_bad
        );
    }

    #[test]
    fn detect_issues_finds_problems() {
        let content = "color: #ff0000; color: #00ff00; color: #0000ff; color: #fff; color: #000; color: #abc; !important";
        let issues = detect_taste_issues(content);
        assert!(!issues.is_empty());
    }

    #[tokio::test]
    async fn manage_runtime_status() {
        let tool = ManageRuntimeTool;
        // Use a port that's almost certainly not in use
        let out = tool
            .execute(input(json!({ "action": "status", "port": 59999 })))
            .await;

        assert!(out.success);
        assert_eq!(out.result["running"], false);
    }

    #[tokio::test]
    async fn manage_runtime_unknown_action() {
        let tool = ManageRuntimeTool;
        let out = tool
            .execute(input(json!({ "action": "explode" })))
            .await;

        assert!(!out.success);
    }
}
