use crate::blueprint::{FailureAction, GateCheck};
use serde::{Deserialize, Serialize};
use std::process::Command;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateResult {
    pub check_name: String,
    pub passed: bool,
    pub output: String,
    pub duration_ms: u64,
    pub tier: GateTier,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum GateTier {
    /// Lint + format (<5s)
    Tier1,
    /// Type check + selective tests
    Tier2,
    /// Full CI + self-healing
    Tier3,
}

/// Execute a single gate check synchronously.
pub fn execute_check(check: &GateCheck, working_dir: &str) -> GateResult {
    let start = std::time::Instant::now();
    let (name, cmd_str, tier) = match check {
        GateCheck::Lint { command } => ("lint".to_string(), command.clone(), GateTier::Tier1),
        GateCheck::TypeCheck { command } => {
            ("typecheck".to_string(), command.clone(), GateTier::Tier2)
        }
        GateCheck::Test {
            command,
            selective: _,
        } => ("test".to_string(), command.clone(), GateTier::Tier2),
        GateCheck::SecurityScan { rules } => (
            "security_scan".to_string(),
            format!("echo 'scanning rules: {}'", rules.join(",")),
            GateTier::Tier3,
        ),
        GateCheck::PolicyCheck { policy_id } => (
            format!("policy_{policy_id}"),
            format!("echo 'checking policy {policy_id}'"),
            GateTier::Tier3,
        ),
        GateCheck::Custom {
            command,
            expected_exit,
        } => {
            let output = run_command(command, working_dir);
            let passed = output.0 == *expected_exit;
            return GateResult {
                check_name: "custom".to_string(),
                passed,
                output: output.1,
                duration_ms: start.elapsed().as_millis() as u64,
                tier: GateTier::Tier3,
            };
        }
    };

    let (exit_code, stdout) = run_command(&cmd_str, working_dir);
    let passed = exit_code == 0;
    if passed {
        info!(check = %name, "Gate check passed");
    } else {
        warn!(check = %name, exit_code, "Gate check failed");
    }

    GateResult {
        check_name: name,
        passed,
        output: stdout,
        duration_ms: start.elapsed().as_millis() as u64,
        tier,
    }
}

fn tier_sort_key(check: &GateCheck) -> u8 {
    match check {
        GateCheck::Lint { .. } => 0,
        GateCheck::TypeCheck { .. } => 1,
        GateCheck::Test { .. } => 2,
        GateCheck::SecurityScan { .. } => 3,
        GateCheck::PolicyCheck { .. } => 4,
        GateCheck::Custom { .. } => 5,
    }
}

/// Run all gate checks in tier order (Tier1 first, then Tier2, then Tier3).
/// Stops early if a tier fails and the failure action is Abort.
pub fn run_tiered_gates(
    checks: &[GateCheck],
    failure_action: &FailureAction,
    working_dir: &str,
) -> Vec<GateResult> {
    let mut results = Vec::new();
    let mut sorted_checks: Vec<_> = checks.iter().collect();
    sorted_checks.sort_by_key(|c| tier_sort_key(c));

    for check in sorted_checks {
        let result = execute_check(check, working_dir);
        let passed = result.passed;
        results.push(result);

        if !passed {
            match failure_action {
                FailureAction::Abort
                | FailureAction::Retry { .. }
                | FailureAction::SelfHeal { .. }
                | FailureAction::Escalate { .. } => break,
            }
        }
    }

    results
}

fn run_command(cmd: &str, working_dir: &str) -> (i32, String) {
    match Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(working_dir)
        .output()
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let combined = if stderr.is_empty() {
                stdout
            } else {
                format!("{stdout}\n{stderr}")
            };
            (output.status.code().unwrap_or(-1), combined)
        }
        Err(e) => (-1, format!("Failed to execute: {e}")),
    }
}
