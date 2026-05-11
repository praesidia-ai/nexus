//! Invariant System — enforce production quality rules across generated code.
//!
//! Scans all files and checks for violations:
//! - Every API route has error handling
//! - Every form has validation
//! - Every async call handles loading + error states
//! - No hardcoded secrets
//! - No console.log in production code
//! - Every page has proper metadata/title
//! - Every image has alt text
//! - No TODO/FIXME/HACK comments
//! - Every fetch has try/catch
//! - Proper TypeScript types (no `any`)

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvariantResult {
    pub total_files: usize,
    pub total_violations: usize,
    pub violations: Vec<Violation>,
    pub score: u32,          // 0-100 production readiness score
    pub auto_fixable: usize, // how many can be auto-fixed
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Violation {
    pub rule: String,
    pub severity: String,    // "error", "warning", "info"
    pub file: String,
    pub line: Option<usize>,
    pub message: String,
    pub fix: Option<String>, // suggested fix (for auto-fix)
    pub auto_fixable: bool,
}

// Rule IDs and severities are used inline in check_invariants().
// Rule documentation is in the module-level doc comment above.

/// Scan a project directory and check all invariants.
pub fn check_invariants(project_dir: &Path) -> InvariantResult {
    let mut violations = Vec::new();

    // Collect source files
    let files = collect_files(project_dir);
    let total_files = files.len();

    // Track project-level state
    let has_env_example = project_dir.join(".env.example").exists();
    let mut env_vars_used: Vec<String> = Vec::new();

    for file_path in &files {
        let rel_path = file_path.strip_prefix(project_dir)
            .unwrap_or(file_path)
            .display().to_string();

        let Ok(content) = std::fs::read_to_string(file_path) else { continue };
        let is_ts = rel_path.ends_with(".ts") || rel_path.ends_with(".tsx");
        let is_api = rel_path.contains("/api/") && rel_path.ends_with("route.ts");
        let is_page = rel_path.ends_with("page.tsx") || rel_path.ends_with("page.ts");

        for (line_num, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            let ln = line_num + 1;

            // ---- Security: hardcoded secrets ----
            check_secrets(trimmed, &rel_path, ln, &mut violations);

            // ---- No `any` type ----
            if is_ts && (trimmed.contains(": any") || trimmed.contains(": any;") || trimmed.contains("<any>") || trimmed.contains("as any")) {
                // Skip type definition files and comments
                if !rel_path.contains(".d.ts") && !trimmed.starts_with("//") && !trimmed.starts_with('*') {
                    violations.push(Violation {
                        rule: "no-any-type".into(), severity: "warning".into(),
                        file: rel_path.clone(), line: Some(ln),
                        message: "Avoid using `any` type — use a specific type instead".into(),
                        fix: None, auto_fixable: false,
                    });
                }
            }

            // ---- No console.log ----
            if trimmed.contains("console.log(") && !trimmed.starts_with("//") {
                violations.push(Violation {
                    rule: "no-console-log".into(), severity: "warning".into(),
                    file: rel_path.clone(), line: Some(ln),
                    message: "Remove console.log before production".into(),
                    fix: Some(format!("Remove line: {}", trimmed)), auto_fixable: true,
                });
            }

            // ---- No TODO/FIXME ----
            if (trimmed.contains("TODO") || trimmed.contains("FIXME") || trimmed.contains("HACK"))
                && (trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*")) {
                violations.push(Violation {
                    rule: "no-todo-comments".into(), severity: "info".into(),
                    file: rel_path.clone(), line: Some(ln),
                    message: format!("Unresolved comment: {}", trimmed.chars().take(80).collect::<String>()),
                    fix: None, auto_fixable: false,
                });
            }

            // ---- No empty catch ----
            if trimmed == "catch {}" || trimmed == "catch(e) {}" || trimmed == "} catch {" {
                violations.push(Violation {
                    rule: "no-empty-catch".into(), severity: "error".into(),
                    file: rel_path.clone(), line: Some(ln),
                    message: "Empty catch block — errors are silently swallowed".into(),
                    fix: Some("Add error handling: catch(e) { console.error(e) }".into()),
                    auto_fixable: true,
                });
            }

            // ---- Track env vars used ----
            if trimmed.contains("process.env.") {
                if let Some(var_start) = trimmed.find("process.env.") {
                    let rest = &trimmed[var_start + 12..];
                    let var_name: String = rest.chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect();
                    if !var_name.is_empty() {
                        env_vars_used.push(var_name);
                    }
                }
            }

            // ---- img without alt ----
            if trimmed.contains("<img") && !trimmed.contains("alt=") && !trimmed.contains("alt =") {
                violations.push(Violation {
                    rule: "img-has-alt".into(), severity: "warning".into(),
                    file: rel_path.clone(), line: Some(ln),
                    message: "Image missing alt text (accessibility)".into(),
                    fix: Some("Add alt=\"Description\" to the img tag".into()),
                    auto_fixable: false,
                });
            }
        }

        // ---- File-level checks ----

        // API routes must have try/catch or error handling
        if is_api {
            let has_error_handling = content.contains("try {") || content.contains("try{")
                || content.contains(".catch(") || content.contains("catch (");
            if !has_error_handling {
                violations.push(Violation {
                    rule: "api-has-error-handling".into(), severity: "error".into(),
                    file: rel_path.clone(), line: None,
                    message: "API route has no error handling (try/catch or .catch)".into(),
                    fix: Some("Wrap the handler body in try/catch and return appropriate error responses".into()),
                    auto_fixable: false,
                });
            }
        }

        // Pages should have metadata
        if is_page && !content.contains("metadata") && !content.contains("<title") && !content.contains("document.title") {
            violations.push(Violation {
                rule: "page-has-metadata".into(), severity: "info".into(),
                file: rel_path.clone(), line: None,
                message: "Page has no metadata/title — affects SEO".into(),
                fix: Some("Export metadata = { title: 'Page Title' } for App Router pages".into()),
                auto_fixable: false,
            });
        }

        // Fetch calls should have error handling
        if content.contains("fetch(") {
            let fetch_count = content.matches("fetch(").count();
            let catch_count = content.matches(".catch(").count() + content.matches("try {").count() + content.matches("try{").count();
            if catch_count < fetch_count && !is_api {
                violations.push(Violation {
                    rule: "fetch-has-try-catch".into(), severity: "error".into(),
                    file: rel_path.clone(), line: None,
                    message: format!("{} fetch calls but only {} error handlers", fetch_count, catch_count),
                    fix: Some("Wrap fetch calls in try/catch or add .catch() handlers".into()),
                    auto_fixable: false,
                });
            }
        }

        // Forms should have validation
        if content.contains("<form") || content.contains("onSubmit") {
            let has_validation = content.contains("required") || content.contains("validate")
                || content.contains("pattern=") || content.contains("minLength")
                || content.contains("zod") || content.contains("yup");
            if !has_validation {
                violations.push(Violation {
                    rule: "form-has-validation".into(), severity: "warning".into(),
                    file: rel_path.clone(), line: None,
                    message: "Form has no input validation".into(),
                    fix: Some("Add required, minLength, or a validation library".into()),
                    auto_fixable: false,
                });
            }
        }

        // Async/useEffect should have loading state
        if content.contains("useEffect") && content.contains("fetch(")
            && !content.contains("loading") && !content.contains("isLoading") && !content.contains("pending") {
                violations.push(Violation {
                    rule: "async-has-loading-state".into(), severity: "warning".into(),
                    file: rel_path.clone(), line: None,
                    message: "Component fetches data but has no loading state".into(),
                    fix: Some("Add a loading state: const [loading, setLoading] = useState(true)".into()),
                    auto_fixable: false,
                });
            }
    }

    // ---- Project-level checks ----

    // Env vars should be documented
    env_vars_used.sort();
    env_vars_used.dedup();
    if !env_vars_used.is_empty() && !has_env_example {
        violations.push(Violation {
            rule: "env-vars-documented".into(), severity: "info".into(),
            file: ".env.example".into(), line: None,
            message: format!("Missing .env.example — {} env vars used: {}", env_vars_used.len(), env_vars_used.join(", ")),
            fix: Some(format!("Create .env.example with: {}", env_vars_used.iter().map(|v| format!("{}=", v)).collect::<Vec<_>>().join("\n"))),
            auto_fixable: true,
        });
    }

    let auto_fixable = violations.iter().filter(|v| v.auto_fixable).count();
    let error_count = violations.iter().filter(|v| v.severity == "error").count();
    let warning_count = violations.iter().filter(|v| v.severity == "warning").count();

    // Score: start at 100, subtract for violations
    let score = (100i32 - (error_count as i32 * 10) - (warning_count as i32 * 3))
        .max(0) as u32;

    InvariantResult {
        total_files,
        total_violations: violations.len(),
        violations,
        score,
        auto_fixable,
    }
}

/// Auto-fix violations that are marked as fixable (console.log removal, etc.)
pub fn auto_fix(project_dir: &Path, result: &InvariantResult) -> usize {
    let mut fixed = 0;

    for v in &result.violations {
        if !v.auto_fixable { continue; }

        match v.rule.as_str() {
            "no-console-log" => {
                if let Some(line_num) = v.line {
                    let path = project_dir.join(&v.file);
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        let lines: Vec<&str> = content.lines().collect();
                        let new_lines: Vec<&str> = lines.iter().enumerate()
                            .filter(|(i, l)| !(*i + 1 == line_num && l.trim().starts_with("console.log(")))
                            .map(|(_, l)| *l)
                            .collect();
                        if new_lines.len() < lines.len() {
                            let _ = std::fs::write(&path, new_lines.join("\n"));
                            fixed += 1;
                        }
                    }
                }
            }
            "env-vars-documented" => {
                if let Some(ref fix) = v.fix {
                    let env_content = fix.strip_prefix("Create .env.example with: ").unwrap_or(fix);
                    let _ = std::fs::write(project_dir.join(".env.example"), env_content);
                    fixed += 1;
                }
            }
            _ => {}
        }
    }

    fixed
}

fn check_secrets(line: &str, file: &str, ln: usize, violations: &mut Vec<Violation>) {
    let patterns = [
        ("sk-", "OpenAI API key"), ("sk-ant-", "Anthropic key"),
        ("ghp_", "GitHub token"), ("AKIA", "AWS key"),
        ("-----BEGIN", "Private key"),
    ];
    for (pat, desc) in &patterns {
        if line.contains(pat) && !line.contains("process.env") && !line.contains("PLACEHOLDER") {
            violations.push(Violation {
                rule: "no-hardcoded-secrets".into(), severity: "error".into(),
                file: file.into(), line: Some(ln),
                message: format!("Possible hardcoded {} detected", desc),
                fix: Some("Move to environment variable".into()),
                auto_fixable: false,
            });
        }
    }
}

fn collect_files(dir: &Path) -> Vec<PathBuf> {
    crate::file_utils::collect_files_by_ext(dir, &["ts", "tsx", "js", "jsx", "css", "json"])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn detects_console_log() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/app.ts"), "const x = 1;\nconsole.log(x);\n").unwrap();
        let result = check_invariants(dir.path());
        assert!(result.violations.iter().any(|v| v.rule == "no-console-log"));
    }

    #[test]
    fn detects_hardcoded_secret() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("config.ts"), "const key = 'sk-abc123def';\n").unwrap();
        let result = check_invariants(dir.path());
        assert!(result.violations.iter().any(|v| v.rule == "no-hardcoded-secrets"));
    }

    #[test]
    fn detects_any_type() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("types.ts"), "function foo(x: any) {}\n").unwrap();
        let result = check_invariants(dir.path());
        assert!(result.violations.iter().any(|v| v.rule == "no-any-type"));
    }

    #[test]
    fn clean_code_gets_high_score() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("index.ts"), "export const x: string = 'hello';\n").unwrap();
        let result = check_invariants(dir.path());
        assert!(result.score >= 90);
    }

    #[test]
    fn auto_fixes_console_log() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("app.ts"), "const x = 1;\nconsole.log(x);\nconst y = 2;\n").unwrap();
        let result = check_invariants(dir.path());
        let fixed = auto_fix(dir.path(), &result);
        assert!(fixed > 0);
        let content = fs::read_to_string(dir.path().join("app.ts")).unwrap();
        assert!(!content.contains("console.log"));
    }
}
