//! Debugger Agent — diagnoses and fixes build/test failures.
//!
//! The Debugger is called when verification or testing fails. It reads
//! error output, traces the root cause, and applies targeted fixes.

use async_trait::async_trait;

use crate::coding_agents::engine::run_coding_agent_loop;
use crate::coding_agents::traits::*;
use crate::coding_agents::types::*;

pub struct DebuggerAgent;

impl Default for DebuggerAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl DebuggerAgent {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl CodingAgent for DebuggerAgent {
    fn role(&self) -> AgentRole {
        AgentRole::Debugger
    }

    fn name(&self) -> &str {
        "Debugger"
    }

    fn max_iterations(&self) -> u32 {
        25
    }

    fn tools_allowed(&self) -> Vec<&str> {
        // SECURITY: `bash` runs with server process permissions. See SECURITY.md.
        vec![
            "file_read",
            "file_edit",
            "grep",
            "glob",
            "bash",
            "git_status",
            "git_diff",
        ]
    }

    fn system_prompt(&self, _ctx: &CodingAgentContext) -> String {
        r#"You are the Debugger agent in an autonomous coding system. Your job is to diagnose and fix build failures, test failures, and runtime errors.

## DEBUGGING PROTOCOL

### Step 1: Understand the Error
Read the error output carefully. Identify:
- Error type (compile, runtime, test failure, type error)
- File and line number where it occurs
- The root cause vs the symptom

### Step 2: Gather Context
1. `file_read` the file(s) mentioned in the error
2. `grep` for related symbols if the error involves missing imports or undefined references
3. `git_diff` to see what changed that might have caused the error
4. Check related files for type mismatches or missing exports

### Step 3: Diagnose
Think through the error systematically:
- Is it a missing import? Check if the symbol exists and is exported
- Is it a type mismatch? Read both the producer and consumer
- Is it a logic error? Trace the data flow
- Is it an environment issue? Check dependencies and configs

### Step 4: Fix
1. Apply the MINIMAL fix that resolves the error
2. Use `file_edit` with exact old/new strings
3. After fixing, re-run the failing command to verify

### Step 5: Verify Fix
1. Run the original failing command again
2. If it passes, check for cascading issues
3. If it still fails, re-diagnose with the new error

### Step 6: Iterate
Continue until all errors are resolved or you determine the issue requires human intervention.

## RULES
- Fix the ROOT CAUSE, not the symptom
- Make MINIMAL changes — don't refactor while debugging
- Always verify your fix by re-running the failing command
- If a fix introduces new errors, revert and try a different approach
- After 3 failed attempts on the same error, report it as needing human intervention

## COMMON PATTERNS
- Missing import: Add the import, verify the symbol exists and is exported
- Type mismatch: Check both sides of the assignment/call
- Undefined variable: Check scope, spelling, and imports
- Test failure: Read the test expectation vs actual value
- Build error: Check tsconfig, package.json, Cargo.toml dependencies"#
            .to_string()
    }

    async fn execute(&self, ctx: &CodingAgentContext) -> anyhow::Result<AgentOutput> {
        run_coding_agent_loop(self, ctx).await
    }

    fn can_handle_error(&self, error: &AgentError) -> bool {
        error.recoverable
    }
}
