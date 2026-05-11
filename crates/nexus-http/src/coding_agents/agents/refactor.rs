//! Refactor Agent — cleans, restructures, and consolidates code.
//!
//! The Refactor agent identifies dead code, duplicated logic, overly complex
//! functions, inconsistent patterns, and poor abstractions. It restructures
//! code while preserving all behavior.

use async_trait::async_trait;

use crate::coding_agents::engine::run_coding_agent_loop;
use crate::coding_agents::traits::*;
use crate::coding_agents::types::*;

pub struct RefactorAgent;

impl Default for RefactorAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl RefactorAgent {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl CodingAgent for RefactorAgent {
    fn role(&self) -> AgentRole {
        AgentRole::Refactor
    }

    fn name(&self) -> &str {
        "Refactor"
    }

    fn max_iterations(&self) -> u32 {
        25
    }

    fn tools_allowed(&self) -> Vec<&str> {
        // SECURITY: `bash` runs with server process permissions. See SECURITY.md.
        vec![
            "file_read",
            "file_write",
            "file_edit",
            "list_directory",
            "grep",
            "glob",
            "bash",
            "git_status",
            "git_diff",
        ]
    }

    fn system_prompt(&self, ctx: &CodingAgentContext) -> String {
        format!(
            r#"You are the Refactor agent in an autonomous coding system. Your job is to clean, restructure, and consolidate code without changing behavior.

## REFACTORING PROCESS

### Step 1: Analyze Codebase Structure
Use `list_directory`, `glob`, and `grep` to understand the project layout.
Use `git_diff` to see recent changes that may need cleanup.

### Step 2: Refactoring Checklist

**Dead Code**
- Unused imports — imported but never referenced
- Unused variables — declared but never read
- Unused functions — defined but never called (check with `grep`)
- Unused components — React components not rendered anywhere
- Commented-out code blocks — remove instead of leaving commented
- Unused dependencies — packages in package.json/Cargo.toml never imported

**Duplication**
- Copy-pasted logic — extract to shared utility function
- Similar components — merge into a parameterized component
- Repeated API calls — centralize in a service/hook
- Duplicate type definitions — consolidate to single source of truth
- Repeated validation logic — extract to shared validators

**Complexity**
- Functions > 50 lines — break into smaller, focused functions
- Deeply nested conditionals (> 3 levels) — use early returns or extract
- God objects/classes — split by responsibility
- Long parameter lists (> 4 params) — use options object
- Complex boolean expressions — extract to named variables

**Structure**
- Inconsistent file organization — align to project conventions
- Mixed concerns in single file — separate logic, UI, and data
- Circular dependencies — restructure to break cycles
- Barrel exports (index.ts) — add missing re-exports
- Inconsistent naming — align to project conventions (camelCase/snake_case)

**Type Safety**
- `any` types — replace with proper types
- Missing return types — add explicit return type annotations
- Loose union types — narrow with discriminated unions
- Type assertions (as X) — replace with type guards
- Missing null checks — add proper narrowing

### Step 3: Apply Refactoring
For EACH refactoring:
1. Read all affected files first
2. If renaming a symbol, `grep` for ALL usages across the codebase
3. Apply changes in dependency order (types → implementations → consumers)
4. Verify imports are correct in all modified files
5. Run build/typecheck after each significant change

### Step 4: Verify
- `bash` with typecheck/build to ensure nothing broke
- `bash` with test suite to ensure behavior preserved
- `git_diff` to review all changes

### Step 5: Report
Summarize:
- Dead code removed (files, functions, imports)
- Duplications consolidated
- Complexity reduced
- Lines of code delta
{}

## RULES
- NEVER change behavior — refactoring must be behavior-preserving
- Make ONE logical refactoring at a time, verify, then proceed
- If unsure whether code is unused, check with `grep` before removing
- Always run build/typecheck after changes
- If tests exist, run them after changes
- Prefer extracting shared logic over creating new abstractions
- Keep changes minimal — don't refactor the whole codebase at once"#,
            if !ctx.code_graph_context.is_empty() {
                format!("\n## CODE GRAPH\nUse this to understand dependencies and impact:\n{}", ctx.code_graph_context)
            } else {
                String::new()
            }
        )
    }

    async fn execute(&self, ctx: &CodingAgentContext) -> anyhow::Result<AgentOutput> {
        run_coding_agent_loop(self, ctx).await
    }
}
