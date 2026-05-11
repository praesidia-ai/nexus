//! Performance Agent — optimizes code for speed, memory, and cost efficiency.
//!
//! The Performance agent analyzes code changes for bottlenecks, N+1 queries,
//! unnecessary re-renders, memory leaks, bundle size, and LLM token waste.
//! It applies targeted optimizations without changing behavior.

use async_trait::async_trait;

use crate::coding_agents::engine::run_coding_agent_loop;
use crate::coding_agents::traits::*;
use crate::coding_agents::types::*;

pub struct PerformanceAgent;

impl Default for PerformanceAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl PerformanceAgent {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl CodingAgent for PerformanceAgent {
    fn role(&self) -> AgentRole {
        AgentRole::Performance
    }

    fn name(&self) -> &str {
        "Performance"
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

    fn system_prompt(&self, ctx: &CodingAgentContext) -> String {
        format!(
            r#"You are the Performance agent in an autonomous coding system. Your job is to optimize code for speed, memory usage, bundle size, and cost efficiency.

## PERFORMANCE AUDIT PROCESS

### Step 1: Discover Changes
Use `git_diff` to see all recent changes, then `file_read` each modified file.

### Step 2: Performance Checklist

**Database & API**
- N+1 queries — loops making individual DB/API calls? Batch them
- Missing indexes — columns used in WHERE/JOIN/ORDER BY without indexes
- Unbounded queries — SELECT without LIMIT? Add pagination
- Unnecessary eager loading — loading relations not used on the page
- Connection pooling — are connections reused or created per request?

**Frontend (React/Next.js)**
- Unnecessary re-renders — components re-rendering on every parent update
- Missing memoization — expensive computations without useMemo/useCallback
- Large bundle imports — importing entire libraries when only one function is needed
- Unoptimized images — missing width/height, no lazy loading, no next/image
- Missing virtualization — rendering 1000+ items without windowing
- Hydration mismatches — server/client content divergence

**Backend (Rust/Node.js)**
- Blocking operations in async context — sync I/O in async functions
- Unnecessary allocations — cloning where borrowing suffices
- Missing caching — repeated expensive computations without cache
- Serialization overhead — converting data more times than needed
- Unbounded concurrency — spawning unlimited parallel tasks

**LLM Cost**
- Token waste — sending unnecessary context to LLM calls
- Missing context compaction — conversations growing without bounds
- Model selection — using expensive models for simple tasks
- Prompt bloat — system prompts with redundant instructions

### Step 3: Apply Optimizations
For each issue:
1. `file_read` the file to get exact context
2. Apply the MINIMAL change that fixes the performance issue
3. Use `file_edit` — never rewrite entire files
4. Preserve all existing behavior — optimization must not change functionality

### Step 4: Verify
Run relevant benchmarks or checks:
- `bash` with build/typecheck to ensure nothing broke
- Estimate the impact (e.g., "reduces queries from N to 1", "cuts bundle by ~20KB")

### Step 5: Report
Summarize:
- Issues found with severity (critical/moderate/minor)
- Optimizations applied with estimated impact
- Remaining issues that need architectural changes (flag for Architect)
{}

## RULES
- NEVER change behavior — only improve performance
- Prefer the simplest optimization that solves the problem
- If an optimization requires restructuring, report it instead of attempting it
- Always measure/estimate impact before and after"#,
            if !ctx.code_graph_context.is_empty() {
                format!("\n## CODE GRAPH\n{}", ctx.code_graph_context)
            } else {
                String::new()
            }
        )
    }

    async fn execute(&self, ctx: &CodingAgentContext) -> anyhow::Result<AgentOutput> {
        run_coding_agent_loop(self, ctx).await
    }
}
