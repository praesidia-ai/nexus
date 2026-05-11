//! Verification System — validates build, lint, types, and tests after changes.
//!
//! Each verification runs the appropriate command for the detected tech stack
//! and parses the output to determine pass/fail status.

use std::path::Path;
use std::time::Instant;

use tracing::{info, warn};

use crate::project_brain::TechStack;

use super::types::*;

/// Run all applicable verification checks for a project.
pub async fn run_verification_suite(
    project_dir: &Path,
    stack: &TechStack,
) -> Vec<VerificationResult> {
    let mut results = Vec::new();

    let checks = determine_checks(project_dir, stack);

    for (check_type, command) in checks {
        let result = run_verification_check(project_dir, check_type, &command).await;
        results.push(result);
    }

    results
}

/// Run a single verification check and return the result.
pub async fn run_verification_check(
    project_dir: &Path,
    check_type: VerificationType,
    command: &str,
) -> VerificationResult {
    let start = Instant::now();

    let dir = project_dir.to_path_buf();
    let cmd = command.to_string();

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(180),
        tokio::task::spawn_blocking(move || {
            std::process::Command::new("sh")
                .arg("-c")
                .arg(&cmd)
                .current_dir(&dir)
                .output()
        }),
    )
    .await;

    let duration_ms = start.elapsed().as_millis() as u64;
    let timestamp = chrono::Utc::now().to_rfc3339();

    match output {
        Ok(Ok(Ok(output))) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let passed = output.status.success();

            let (errors, warnings) = parse_verification_output(&stdout, &stderr, check_type);

            let combined = if stderr.is_empty() {
                stdout.clone()
            } else {
                format!("{}\n{}", stdout, stderr)
            };

            let output_trimmed = if combined.chars().count() > 5000 {
                let mut out: String = combined.chars().take(5000).collect();
                out.push_str("...[truncated]");
                out
            } else {
                combined
            };

            info!(
                check = ?check_type,
                passed = passed,
                errors = errors.len(),
                duration_ms = duration_ms,
                "Verification check completed"
            );

            VerificationResult {
                check_type,
                passed,
                output: output_trimmed,
                errors,
                warnings,
                duration_ms,
                timestamp,
            }
        }
        Ok(Ok(Err(e))) => VerificationResult {
            check_type,
            passed: false,
            output: format!("Command failed to execute: {}", e),
            errors: vec![e.to_string()],
            warnings: Vec::new(),
            duration_ms,
            timestamp,
        },
        Ok(Err(e)) => VerificationResult {
            check_type,
            passed: false,
            output: format!("Task panicked: {}", e),
            errors: vec![e.to_string()],
            warnings: Vec::new(),
            duration_ms,
            timestamp,
        },
        Err(_) => VerificationResult {
            check_type,
            passed: false,
            output: "Command timed out after 180s".to_string(),
            errors: vec!["Timeout".to_string()],
            warnings: Vec::new(),
            duration_ms,
            timestamp,
        },
    }
}

/// Determine which checks to run based on the tech stack and available files.
fn determine_checks(project_dir: &Path, stack: &TechStack) -> Vec<(VerificationType, String)> {
    let mut checks = Vec::new();

    match stack.framework.as_str() {
        "Next.js" | "React" | "Vue" | "Node.js" | "Express" | "Fastify" => {
            if project_dir.join("node_modules").exists() {
                if stack.has_typescript {
                    checks.push((
                        VerificationType::TypeCheck,
                        "npx tsc --noEmit 2>&1".to_string(),
                    ));
                }

                let has_eslint_config = project_dir.join("eslint.config.js").exists()
                    || project_dir.join("eslint.config.mjs").exists()
                    || project_dir.join(".eslintrc.json").exists()
                    || project_dir.join(".eslintrc.js").exists();
                let lint_cmd = if has_eslint_config {
                    "npx eslint . --max-warnings 0 2>&1"
                } else {
                    ""
                };
                if !lint_cmd.is_empty() {
                    checks.push((VerificationType::Lint, lint_cmd.to_string()));
                }

                checks.push((
                    VerificationType::Build,
                    "npm run build 2>&1 || npx next build 2>&1".to_string(),
                ));

                let test_cmd = match stack.test_framework.as_str() {
                    "Vitest" => "npx vitest run 2>&1",
                    "Jest" => "npx jest --passWithNoTests 2>&1",
                    "Mocha" => "npx mocha 2>&1",
                    "Playwright" => "npx playwright test 2>&1",
                    _ => "",
                };
                if !test_cmd.is_empty() {
                    checks.push((VerificationType::UnitTest, test_cmd.to_string()));
                }
            } else {
                checks.push((
                    VerificationType::Build,
                    "npm install && npm run build 2>&1".to_string(),
                ));
            }
        }
        "Cargo" => {
            checks.push((
                VerificationType::Build,
                "cargo check 2>&1".to_string(),
            ));
            checks.push((
                VerificationType::Lint,
                "cargo clippy -- -D warnings 2>&1".to_string(),
            ));
            checks.push((
                VerificationType::UnitTest,
                "cargo test 2>&1".to_string(),
            ));
        }
        "Django" | "Flask" | "FastAPI" | "Python" => {
            if project_dir.join("pyproject.toml").exists()
                || project_dir.join("requirements.txt").exists()
            {
                checks.push((
                    VerificationType::Lint,
                    "python -m flake8 . 2>&1 || true".to_string(),
                ));

                let test_cmd = if project_dir.join("pytest.ini").exists()
                    || project_dir.join("pyproject.toml").exists()
                {
                    "python -m pytest -x 2>&1"
                } else {
                    "python -m pytest 2>&1 || python -m unittest discover 2>&1"
                };
                checks.push((VerificationType::UnitTest, test_cmd.to_string()));
            }
        }
        "Go Modules" => {
            checks.push((
                VerificationType::Build,
                "go build ./... 2>&1".to_string(),
            ));
            checks.push((
                VerificationType::Lint,
                "go vet ./... 2>&1".to_string(),
            ));
            checks.push((
                VerificationType::UnitTest,
                "go test ./... 2>&1".to_string(),
            ));
        }
        _ => {
            warn!(
                framework = %stack.framework,
                "Unknown framework — skipping verification"
            );
        }
    }

    checks
}

/// Parse command output to extract structured errors and warnings.
fn parse_verification_output(
    stdout: &str,
    stderr: &str,
    check_type: VerificationType,
) -> (Vec<String>, Vec<String>) {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let combined = format!("{}\n{}", stdout, stderr);

    for line in combined.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let lower = trimmed.to_lowercase();

        match check_type {
            VerificationType::TypeCheck => {
                if trimmed.contains("error TS") || trimmed.contains(": error") {
                    errors.push(trimmed.to_string());
                } else if lower.contains("warning") {
                    warnings.push(trimmed.to_string());
                }
            }
            VerificationType::Lint => {
                if lower.contains("error") && !lower.contains("0 errors") {
                    errors.push(trimmed.to_string());
                } else if lower.contains("warning") && !lower.contains("0 warnings") {
                    warnings.push(trimmed.to_string());
                }
            }
            VerificationType::Build => {
                if lower.contains("error") || lower.contains("failed") {
                    errors.push(trimmed.to_string());
                } else if lower.contains("warning") || lower.contains("warn") {
                    warnings.push(trimmed.to_string());
                }
            }
            VerificationType::UnitTest => {
                if lower.contains("fail") || lower.contains("error") {
                    errors.push(trimmed.to_string());
                }
            }
            VerificationType::IntegrationTest => {
                if lower.contains("fail") || lower.contains("error") {
                    errors.push(trimmed.to_string());
                }
            }
            VerificationType::SecurityScan => {
                if lower.contains("vulnerability") || lower.contains("critical") {
                    errors.push(trimmed.to_string());
                } else if lower.contains("warning") || lower.contains("moderate") {
                    warnings.push(trimmed.to_string());
                }
            }
        }
    }

    if errors.len() > 20 {
        errors.truncate(20);
        errors.push("...[more errors truncated]".to_string());
    }
    if warnings.len() > 10 {
        warnings.truncate(10);
        warnings.push("...[more warnings truncated]".to_string());
    }

    (errors, warnings)
}
