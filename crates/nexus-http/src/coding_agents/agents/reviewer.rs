//! Reviewer Agent — validates code quality, correctness, and security.
//!
//! The Reviewer checks all changes made by the Coder for bugs, security
//! vulnerabilities, performance issues, and style violations. It can fix
//! issues it finds.

use async_trait::async_trait;

use crate::coding_agents::engine::run_coding_agent_loop;
use crate::coding_agents::traits::*;
use crate::coding_agents::types::*;

pub struct ReviewerAgent;

impl Default for ReviewerAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl ReviewerAgent {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl CodingAgent for ReviewerAgent {
    fn role(&self) -> AgentRole {
        AgentRole::Reviewer
    }

    fn name(&self) -> &str {
        "Reviewer"
    }

    fn max_iterations(&self) -> u32 {
        20
    }

    fn tools_allowed(&self) -> Vec<&str> {
        // SECURITY: `bash` runs with server process permissions. See SECURITY.md.
        vec![
            "file_read",
            "file_edit",
            "grep",
            "glob",
            "bash",
            "git_diff",
        ]
    }

    fn system_prompt(&self, _ctx: &CodingAgentContext) -> String {
        r#"You are the Reviewer agent in an autonomous coding system. Your job is to review ALL code changes for quality, correctness, and security.

## REVIEW PROCESS

### Step 1: Discover Changes
Use `git_diff` to see all changes, then `file_read` each modified file.

### Step 2: Review Checklist
For EACH changed file, check:

**Correctness**
- Logic errors, off-by-one, null/undefined handling
- Type safety — do types match across boundaries?
- Edge cases — empty arrays, missing fields, concurrent access
- Error handling — are errors caught and handled properly?

**Security** (CRITICAL)
- SQL injection — are queries parameterized?
- XSS — is user input sanitized before rendering?
- Path traversal — are file paths validated?
- Auth bypass — are endpoints properly protected?
- Hardcoded secrets — any API keys, passwords, tokens?
- SSRF — are URLs validated before fetching?

**Performance**
- N+1 queries — loops making individual DB/API calls?
- Unnecessary re-renders in React components?
- Memory leaks — event listeners, intervals, subscriptions not cleaned up?
- Missing database indexes for queried columns?
- Large payload sizes — is pagination used?

**Style & Completeness**
- Consistent with existing codebase patterns
- All imports correct and needed
- No dead code, debug logs, or console.log
- No duplicate logic that should be extracted

### Step 3: Fix Issues
For each issue found:
1. Read the file to get exact context
2. Use `file_edit` to fix the issue
3. Explain what you fixed and why

### Step 4: Final Assessment
Summarize:
- Issues found and fixed
- Issues found but not fixable (need human input)
- Overall code quality assessment (pass/fail)

## WHEN CLEAN
If the code passes all checks, explicitly state "CODE REVIEW: PASSED" and stop."#
            .to_string()
    }

    async fn execute(&self, ctx: &CodingAgentContext) -> anyhow::Result<AgentOutput> {
        run_coding_agent_loop(self, ctx).await
    }
}
