//! Tool dependency analysis — determines which tool calls can run in parallel.
//!
//! When the LLM returns multiple `tool_use` blocks in a single response, we
//! want to execute as many as possible concurrently. However, some pairs of
//! calls conflict (e.g. two writes to the same file, or two shell commands
//! that could interfere with each other). This module builds an
//! [`ExecutionPlan`] that groups non-conflicting calls into parallel batches.

use serde_json::Value;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// An ordered list of execution groups.
///
/// Each group is a `Vec<usize>` of indices into the original call list.
/// All calls within a single group may run concurrently; groups themselves
/// execute sequentially.
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    pub groups: Vec<Vec<usize>>,
}

// ---------------------------------------------------------------------------
// Checker
// ---------------------------------------------------------------------------

/// Stateless dependency analyser for tool calls.
pub struct ToolDependencyChecker;

impl ToolDependencyChecker {
    /// Build an [`ExecutionPlan`] for the given tool calls.
    ///
    /// Calls are represented as `(tool_name, input_json)` pairs. The planner
    /// uses a greedy algorithm: iterate calls in order and append each to the
    /// current group if it does not conflict with any call already in that
    /// group. Otherwise, start a new group.
    pub fn plan_execution(calls: &[(String, Value)]) -> ExecutionPlan {
        if calls.is_empty() {
            return ExecutionPlan {
                groups: Vec::new(),
            };
        }

        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut current_group: Vec<usize> = vec![0];

        for i in 1..calls.len() {
            let conflicts_with_current = current_group.iter().any(|&j| {
                Self::conflicts(&calls[j].0, &calls[j].1, &calls[i].0, &calls[i].1)
            });

            if conflicts_with_current {
                // Flush current group and start a new one.
                groups.push(std::mem::take(&mut current_group));
                current_group.push(i);
            } else {
                current_group.push(i);
            }
        }

        if !current_group.is_empty() {
            groups.push(current_group);
        }

        ExecutionPlan { groups }
    }

    /// Returns `true` when two calls conflict and must not run concurrently.
    fn conflicts(a_name: &str, a_input: &Value, b_name: &str, b_input: &Value) -> bool {
        let a_write = Self::is_write_op(a_name);
        let b_write = Self::is_write_op(b_name);
        let a_shell = Self::is_shell_op(a_name);
        let b_shell = Self::is_shell_op(b_name);

        // Shell commands conflict with everything — they can read/write any
        // file, modify cwd, change environment, install packages, etc.
        if a_shell || b_shell {
            return true;
        }

        // Two write ops to the same path conflict.
        if a_write && b_write {
            if let (Some(pa), Some(pb)) = (Self::extract_path(a_input), Self::extract_path(b_input))
            {
                if pa == pb {
                    return true;
                }
            }
        }

        // A write and a read to the same path conflict (read-after-write
        // ordering matters).
        if a_write || b_write {
            if let (Some(pa), Some(pb)) = (Self::extract_path(a_input), Self::extract_path(b_input))
            {
                if pa == pb {
                    return true;
                }
            }
        }

        false
    }

    /// Extract the filesystem path from a tool's input JSON.
    ///
    /// Different tools use different key names; we check the common ones.
    fn extract_path(input: &Value) -> Option<String> {
        input
            .get("path")
            .or_else(|| input.get("file"))
            .or_else(|| input.get("file_path"))
            .or_else(|| input.get("directory"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// Whether the tool is a write/mutating filesystem operation.
    fn is_write_op(name: &str) -> bool {
        matches!(
            name,
            "write_file"
                | "edit_file"
                | "delete_file"
                | "npm_install"
                | "npm_run"
                | "build_project"
        )
    }

    /// Whether the tool is a shell/command execution.
    fn is_shell_op(name: &str) -> bool {
        matches!(
            name,
            "shell_command" | "npm_install" | "npm_run" | "build_project" | "run_tests"
        )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_calls_produce_empty_plan() {
        let plan = ToolDependencyChecker::plan_execution(&[]);
        assert!(plan.groups.is_empty());
    }

    #[test]
    fn single_call_produces_single_group() {
        let calls = vec![("read_file".to_string(), json!({"path": "/a.rs"}))];
        let plan = ToolDependencyChecker::plan_execution(&calls);
        assert_eq!(plan.groups.len(), 1);
        assert_eq!(plan.groups[0], vec![0]);
    }

    #[test]
    fn reads_to_different_files_are_parallel() {
        let calls = vec![
            ("read_file".to_string(), json!({"path": "/a.rs"})),
            ("read_file".to_string(), json!({"path": "/b.rs"})),
            ("read_file".to_string(), json!({"path": "/c.rs"})),
        ];
        let plan = ToolDependencyChecker::plan_execution(&calls);
        assert_eq!(plan.groups.len(), 1);
        assert_eq!(plan.groups[0], vec![0, 1, 2]);
    }

    #[test]
    fn writes_to_same_file_are_sequential() {
        let calls = vec![
            ("write_file".to_string(), json!({"path": "/a.rs"})),
            ("write_file".to_string(), json!({"path": "/a.rs"})),
        ];
        let plan = ToolDependencyChecker::plan_execution(&calls);
        assert_eq!(plan.groups.len(), 2);
    }

    #[test]
    fn writes_to_different_files_are_parallel() {
        let calls = vec![
            ("write_file".to_string(), json!({"path": "/a.rs"})),
            ("write_file".to_string(), json!({"path": "/b.rs"})),
        ];
        let plan = ToolDependencyChecker::plan_execution(&calls);
        assert_eq!(plan.groups.len(), 1);
    }

    #[test]
    fn shell_commands_conflict_with_each_other() {
        let calls = vec![
            ("shell_command".to_string(), json!({"command": "ls"})),
            ("shell_command".to_string(), json!({"command": "pwd"})),
        ];
        let plan = ToolDependencyChecker::plan_execution(&calls);
        assert_eq!(plan.groups.len(), 2);
    }

    #[test]
    fn read_and_write_to_same_path_conflict() {
        let calls = vec![
            ("write_file".to_string(), json!({"path": "/a.rs"})),
            ("read_file".to_string(), json!({"path": "/a.rs"})),
        ];
        let plan = ToolDependencyChecker::plan_execution(&calls);
        assert_eq!(plan.groups.len(), 2);
    }

    #[test]
    fn mixed_parallel_and_sequential() {
        let calls = vec![
            ("read_file".to_string(), json!({"path": "/a.rs"})),
            ("read_file".to_string(), json!({"path": "/b.rs"})),
            ("shell_command".to_string(), json!({"command": "cargo build"})),
            ("read_file".to_string(), json!({"path": "/c.rs"})),
        ];
        let plan = ToolDependencyChecker::plan_execution(&calls);
        // [0,1] can be parallel, then [2] alone (shell), then [3] alone (after shell)
        assert_eq!(plan.groups.len(), 3);
        assert_eq!(plan.groups[0], vec![0, 1]);
        assert_eq!(plan.groups[1], vec![2]);
        assert_eq!(plan.groups[2], vec![3]);
    }
}
