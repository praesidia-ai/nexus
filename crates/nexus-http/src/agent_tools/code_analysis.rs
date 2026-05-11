//! Code analysis tools — security scanning, complexity metrics, linting, dependency auditing.
//!
//! These tools provide static analysis capabilities without requiring
//! external services. They operate on local files and directories.

use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::Path;

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
// SecurityScanTool
// ---------------------------------------------------------------------------

/// Scan files for common security issues.
pub struct SecurityScanTool;

#[async_trait]
impl Tool for SecurityScanTool {
    fn name(&self) -> &str {
        "security_scan"
    }

    fn description(&self) -> &str {
        "Scan source files for common security issues: hardcoded secrets, SQL injection patterns, XSS vectors, insecure functions."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "File or directory path to scan" },
                "patterns": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Additional regex patterns to search for (optional)"
                }
            },
            "required": ["path"]
        })
    }

    fn category(&self) -> &str {
        "data_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let path = match input.parameters.get("path").and_then(|v| v.as_str()) {
            Some(p) => p.to_string(),
            None => return err("Missing required parameter: path"),
        };

        let path_clone = path.clone();
        let result = tokio::task::spawn_blocking(move || scan_security(&path_clone)).await;

        match result {
            Ok(output) => output,
            Err(e) => err(format!("Task panicked: {e}")),
        }
    }
}

/// Security patterns to search for.
const SECURITY_PATTERNS: &[(&str, &str, &str)] = &[
    // (pattern_substring, description, severity)
    ("password", "Possible hardcoded password", "high"),
    ("secret", "Possible hardcoded secret", "high"),
    ("api_key", "Possible hardcoded API key", "high"),
    ("apikey", "Possible hardcoded API key", "high"),
    ("api-key", "Possible hardcoded API key", "high"),
    ("private_key", "Possible hardcoded private key", "critical"),
    ("BEGIN RSA PRIVATE KEY", "RSA private key found", "critical"),
    ("BEGIN OPENSSH PRIVATE KEY", "SSH private key found", "critical"),
    ("AKIA", "Possible AWS access key", "critical"),
    ("eval(", "Use of eval() — potential code injection", "medium"),
    ("innerHTML", "Use of innerHTML — potential XSS", "medium"),
    ("document.write", "Use of document.write — potential XSS", "medium"),
    ("dangerouslySetInnerHTML", "React dangerouslySetInnerHTML — review for XSS", "medium"),
    ("exec(", "Use of exec — potential command injection", "high"),
    ("os.system(", "Use of os.system — potential command injection", "high"),
    ("subprocess.call(", "subprocess with shell — potential injection", "medium"),
    ("SELECT.*FROM.*WHERE.*'", "Possible SQL injection (string interpolation)", "high"),
    ("--disable-web-security", "Disabled web security", "high"),
    ("cors: true", "CORS enabled — review access control", "low"),
    ("TODO: security", "Security TODO marker", "low"),
    ("FIXME: security", "Security FIXME marker", "low"),
];

fn scan_security(path: &str) -> ToolOutput {
    let p = Path::new(path);
    if !p.exists() {
        return err(format!("Path does not exist: {path}"));
    }

    let mut findings = Vec::new();
    let mut files_scanned = 0;

    if p.is_file() {
        scan_file(p, &mut findings);
        files_scanned = 1;
    } else if p.is_dir() {
        scan_directory(p, &mut findings, &mut files_scanned, 0);
    }

    let severity_counts = {
        let critical = findings.iter().filter(|f| f["severity"] == "critical").count();
        let high = findings.iter().filter(|f| f["severity"] == "high").count();
        let medium = findings.iter().filter(|f| f["severity"] == "medium").count();
        let low = findings.iter().filter(|f| f["severity"] == "low").count();
        json!({ "critical": critical, "high": high, "medium": medium, "low": low })
    };

    ok(json!({
        "path": path,
        "files_scanned": files_scanned,
        "findings": findings,
        "finding_count": findings.len(),
        "severity_counts": severity_counts
    }))
}

fn scan_directory(dir: &Path, findings: &mut Vec<Value>, files_scanned: &mut usize, depth: usize) {
    if depth > 10 {
        return;
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Skip hidden dirs and common non-source directories
        if name.starts_with('.') || name == "node_modules" || name == "target" || name == "vendor" || name == "dist" || name == "build" {
            continue;
        }

        if path.is_dir() {
            scan_directory(&path, findings, files_scanned, depth + 1);
        } else if is_source_file(&path) {
            scan_file(&path, findings);
            *files_scanned += 1;
        }
    }
}

fn is_source_file(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    matches!(
        ext,
        "rs" | "js" | "ts" | "tsx" | "jsx" | "py" | "rb" | "go" | "java"
        | "php" | "c" | "cpp" | "h" | "hpp" | "cs" | "sh" | "bash"
        | "yml" | "yaml" | "toml" | "json" | "env" | "cfg" | "ini" | "conf"
    )
}

fn scan_file(path: &Path, findings: &mut Vec<Value>) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let path_str = path.display().to_string();

    for (line_num, line) in content.lines().enumerate() {
        let line_lower = line.to_lowercase();
        for &(pattern, description, severity) in SECURITY_PATTERNS {
            let pattern_lower = pattern.to_lowercase();
            if line_lower.contains(&pattern_lower) {
                // Skip comments that are just mentioning the concept
                let trimmed = line.trim();
                if trimmed.starts_with("//") && !trimmed.contains('=') && !trimmed.contains(':') {
                    continue;
                }
                findings.push(json!({
                    "file": path_str,
                    "line": line_num + 1,
                    "pattern": pattern,
                    "description": description,
                    "severity": severity,
                    "context": line.trim().chars().take(200).collect::<String>()
                }));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ComplexityAnalysisTool
// ---------------------------------------------------------------------------

/// Analyze code complexity metrics for files.
pub struct ComplexityAnalysisTool;

#[async_trait]
impl Tool for ComplexityAnalysisTool {
    fn name(&self) -> &str {
        "complexity_analysis"
    }

    fn description(&self) -> &str {
        "Analyze code complexity: lines of code, function count, max nesting depth, and other metrics per file."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "File or directory path to analyze" }
            },
            "required": ["path"]
        })
    }

    fn category(&self) -> &str {
        "data_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let path = match input.parameters.get("path").and_then(|v| v.as_str()) {
            Some(p) => p.to_string(),
            None => return err("Missing required parameter: path"),
        };

        let path_clone = path.clone();
        let result = tokio::task::spawn_blocking(move || analyze_complexity(&path_clone)).await;

        match result {
            Ok(output) => output,
            Err(e) => err(format!("Task panicked: {e}")),
        }
    }
}

fn analyze_complexity(path: &str) -> ToolOutput {
    let p = Path::new(path);
    if !p.exists() {
        return err(format!("Path does not exist: {path}"));
    }

    let mut file_metrics = Vec::new();

    if p.is_file() {
        if let Some(m) = analyze_file(p) {
            file_metrics.push(m);
        }
    } else if p.is_dir() {
        collect_file_metrics(p, &mut file_metrics, 0);
    }

    let total_lines: usize = file_metrics.iter().filter_map(|m| m["total_lines"].as_u64()).map(|n| n as usize).sum();
    let total_code: usize = file_metrics.iter().filter_map(|m| m["code_lines"].as_u64()).map(|n| n as usize).sum();
    let total_functions: usize = file_metrics.iter().filter_map(|m| m["function_count"].as_u64()).map(|n| n as usize).sum();
    let max_depth: usize = file_metrics.iter().filter_map(|m| m["max_nesting_depth"].as_u64()).map(|n| n as usize).max().unwrap_or(0);

    ok(json!({
        "path": path,
        "files": file_metrics,
        "file_count": file_metrics.len(),
        "summary": {
            "total_lines": total_lines,
            "total_code_lines": total_code,
            "total_functions": total_functions,
            "max_nesting_depth": max_depth
        }
    }))
}

fn collect_file_metrics(dir: &Path, metrics: &mut Vec<Value>, depth: usize) {
    if depth > 10 {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('.') || name == "node_modules" || name == "target" || name == "vendor" {
            continue;
        }
        if path.is_dir() {
            collect_file_metrics(&path, metrics, depth + 1);
        } else if is_source_file(&path) {
            if let Some(m) = analyze_file(&path) {
                metrics.push(m);
            }
        }
    }
}

fn analyze_file(path: &Path) -> Option<Value> {
    let content = std::fs::read_to_string(path).ok()?;
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    let mut code_lines = 0;
    let mut blank_lines = 0;
    let mut comment_lines = 0;
    let mut function_count = 0;
    let mut max_depth: usize = 0;
    let mut current_depth: usize = 0;

    for line in &lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            blank_lines += 1;
        } else if trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with("/*") || trimmed.starts_with('*') {
            comment_lines += 1;
        } else {
            code_lines += 1;
        }

        // Count functions (heuristic: fn/function/def/func keywords)
        if trimmed.contains("fn ") || trimmed.contains("function ") || trimmed.contains("def ") || trimmed.contains("func ") {
            function_count += 1;
        }

        // Track nesting depth via braces
        for ch in trimmed.chars() {
            if ch == '{' {
                current_depth += 1;
                if current_depth > max_depth {
                    max_depth = current_depth;
                }
            } else if ch == '}' {
                current_depth = current_depth.saturating_sub(1);
            }
        }
    }

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("unknown");

    Some(json!({
        "file": path.display().to_string(),
        "language": ext,
        "total_lines": total_lines,
        "code_lines": code_lines,
        "blank_lines": blank_lines,
        "comment_lines": comment_lines,
        "function_count": function_count,
        "max_nesting_depth": max_depth
    }))
}

// ---------------------------------------------------------------------------
// LintFixTool
// ---------------------------------------------------------------------------

/// Run the project's linter and optionally fix issues.
pub struct LintFixTool;

#[async_trait]
impl Tool for LintFixTool {
    fn name(&self) -> &str {
        "lint_fix"
    }

    fn description(&self) -> &str {
        "Run the appropriate linter for a project (eslint, clippy, pylint, etc.) and optionally auto-fix issues."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Project directory to lint" },
                "fix": { "type": "boolean", "description": "Auto-fix issues if supported (default: false)" },
                "language": { "type": "string", "description": "Override language detection (js, ts, rust, python)" }
            },
            "required": ["path"]
        })
    }

    fn category(&self) -> &str {
        "build_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let path = match input.parameters.get("path").and_then(|v| v.as_str()) {
            Some(p) => p.to_string(),
            None => return err("Missing required parameter: path"),
        };
        let fix = input
            .parameters
            .get("fix")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let language_override = input
            .parameters
            .get("language")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let path_clone = path.clone();
        let result = tokio::task::spawn_blocking(move || {
            run_linter(&path_clone, fix, language_override.as_deref())
        })
        .await;

        match result {
            Ok(output) => output,
            Err(e) => err(format!("Task panicked: {e}")),
        }
    }
}

fn detect_language(path: &str) -> &'static str {
    let p = Path::new(path);
    if p.join("Cargo.toml").exists() {
        return "rust";
    }
    if p.join("package.json").exists() {
        // Check if TypeScript
        if p.join("tsconfig.json").exists() {
            return "typescript";
        }
        return "javascript";
    }
    if p.join("requirements.txt").exists() || p.join("pyproject.toml").exists() || p.join("setup.py").exists() {
        return "python";
    }
    if p.join("go.mod").exists() {
        return "go";
    }
    "unknown"
}

fn run_linter(path: &str, fix: bool, language_override: Option<&str>) -> ToolOutput {
    let p = Path::new(path);
    if !p.exists() {
        return err(format!("Path does not exist: {path}"));
    }

    let lang = language_override.unwrap_or_else(|| detect_language(path));

    let (cmd, args): (&str, Vec<&str>) = match lang {
        "rust" => ("cargo", if fix { vec!["clippy", "--fix", "--allow-dirty"] } else { vec!["clippy"] }),
        "javascript" | "js" => {
            if fix {
                ("npx", vec!["eslint", ".", "--fix"])
            } else {
                ("npx", vec!["eslint", "."])
            }
        }
        "typescript" | "ts" => {
            if fix {
                ("npx", vec!["eslint", ".", "--fix"])
            } else {
                ("npx", vec!["eslint", "."])
            }
        }
        "python" | "py" => {
            if fix {
                ("python3", vec!["-m", "black", "."])
            } else {
                ("python3", vec!["-m", "pylint", "."])
            }
        }
        "go" => {
            if fix {
                ("gofmt", vec!["-w", "."])
            } else {
                ("go", vec!["vet", "./..."])
            }
        }
        _ => return err(format!("Cannot detect project language at {path}. Specify 'language' parameter.")),
    };

    let output = std::process::Command::new(cmd)
        .args(&args)
        .current_dir(path)
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let combined = format!("{stdout}{stderr}");
            let truncated = combined.len() > 50_000;
            let output_text = if truncated {
                combined[..50_000].to_string()
            } else {
                combined
            };

            ok(json!({
                "language": lang,
                "command": format!("{cmd} {}", args.join(" ")),
                "exit_code": out.status.code().unwrap_or(-1),
                "success": out.status.success(),
                "output": output_text,
                "truncated": truncated,
                "fix_applied": fix && out.status.success()
            }))
        }
        Err(e) => err(format!("Failed to run linter: {e}")),
    }
}

// ---------------------------------------------------------------------------
// DependencyAuditTool
// ---------------------------------------------------------------------------

/// Check for outdated or vulnerable dependencies.
pub struct DependencyAuditTool;

#[async_trait]
impl Tool for DependencyAuditTool {
    fn name(&self) -> &str {
        "dependency_audit"
    }

    fn description(&self) -> &str {
        "Audit project dependencies for outdated or vulnerable packages. Supports npm, cargo, and pip."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Project directory to audit" },
                "language": { "type": "string", "description": "Override language detection (js, rust, python)" }
            },
            "required": ["path"]
        })
    }

    fn category(&self) -> &str {
        "build_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let path = match input.parameters.get("path").and_then(|v| v.as_str()) {
            Some(p) => p.to_string(),
            None => return err("Missing required parameter: path"),
        };
        let language_override = input
            .parameters
            .get("language")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let path_clone = path.clone();
        let result = tokio::task::spawn_blocking(move || {
            run_audit(&path_clone, language_override.as_deref())
        })
        .await;

        match result {
            Ok(output) => output,
            Err(e) => err(format!("Task panicked: {e}")),
        }
    }
}

fn run_audit(path: &str, language_override: Option<&str>) -> ToolOutput {
    let p = Path::new(path);
    if !p.exists() {
        return err(format!("Path does not exist: {path}"));
    }

    let lang = language_override.unwrap_or_else(|| detect_language(path));

    let (cmd, args): (&str, Vec<&str>) = match lang {
        "rust" => ("cargo", vec!["audit"]),
        "javascript" | "typescript" | "js" | "ts" => ("npm", vec!["audit", "--json"]),
        "python" | "py" => ("pip", vec!["audit", "--format", "json"]),
        _ => return err(format!("Cannot detect project language at {path}. Specify 'language' parameter.")),
    };

    let output = std::process::Command::new(cmd)
        .args(&args)
        .current_dir(path)
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let combined = format!("{stdout}{stderr}");
            let truncated = combined.len() > 50_000;
            let output_text = if truncated {
                combined[..50_000].to_string()
            } else {
                combined
            };

            // Try to parse as JSON if possible
            let parsed: Option<Value> = serde_json::from_str(&stdout).ok();

            ok(json!({
                "language": lang,
                "command": format!("{cmd} {}", args.join(" ")),
                "exit_code": out.status.code().unwrap_or(-1),
                "has_vulnerabilities": !out.status.success(),
                "output": output_text,
                "parsed": parsed,
                "truncated": truncated
            }))
        }
        Err(e) => err(format!("Failed to run audit (is '{cmd}' installed?): {e}")),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn security_scan_tool_metadata() {
        let tool = SecurityScanTool;
        assert_eq!(tool.name(), "security_scan");
        assert_eq!(tool.category(), "data_ops");
    }

    #[test]
    fn complexity_analysis_tool_metadata() {
        let tool = ComplexityAnalysisTool;
        assert_eq!(tool.name(), "complexity_analysis");
        assert_eq!(tool.category(), "data_ops");
    }

    #[test]
    fn lint_fix_tool_metadata() {
        let tool = LintFixTool;
        assert_eq!(tool.name(), "lint_fix");
        assert_eq!(tool.category(), "build_ops");
    }

    #[test]
    fn dependency_audit_tool_metadata() {
        let tool = DependencyAuditTool;
        assert_eq!(tool.name(), "dependency_audit");
        assert_eq!(tool.category(), "build_ops");
    }

    #[test]
    fn detect_language_unknown() {
        assert_eq!(detect_language("/nonexistent/path"), "unknown");
    }

    #[test]
    fn is_source_file_checks() {
        assert!(is_source_file(Path::new("foo.rs")));
        assert!(is_source_file(Path::new("bar.ts")));
        assert!(!is_source_file(Path::new("image.png")));
    }

    #[tokio::test]
    async fn security_scan_nonexistent_path() {
        let tool = SecurityScanTool;
        let out = tool
            .execute(ToolInput {
                parameters: json!({ "path": "/nonexistent/path/abc" }),
            })
            .await;
        assert!(!out.success);
    }
}
