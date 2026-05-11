---
name: add-agent
description: Add a new coding agent or super agent to nexus-http. Use when creating a new specialized agent role in the coding pipeline or a new background optimization super agent.
---

# Adding a New Agent to nexus-http

There are two agent types. Choose one:

| Type | Purpose | Location |
|------|---------|----------|
| **Coding Agent** | Participates in the coding pipeline (architect, coder, debugger, etc.) | `coding_agents/` |
| **Super Agent** | Background optimizer/analyzer (cache, latency, LLM cost, etc.) | `super_agents/` |

---

## A. Adding a Coding Agent

### 1. Add the AgentRole variant

Open `crates/nexus-http/src/coding_agents/types.rs` and add your role to `AgentRole`:

```rust
pub enum AgentRole {
    Architect,
    Coder,
    // ... existing ...
    MyNewRole,   // <-- add here
}
```

Also add `as_str` and display mappings in the same impl block:
```rust
Self::MyNewRole => "my_new_role",
```

### 2. Create the agent file

`crates/nexus-http/src/coding_agents/my_new_role.rs`

```rust
//! MyNewRole coding agent — <one-line description>.

use async_trait::async_trait;

use crate::coding_agents::{
    traits::{AgentOutput, CodingAgent, CodingAgentContext},
    types::{AgentDecision, AgentError, AgentPhase, AgentRole, FileChange},
};

pub struct MyNewRoleAgent;

#[async_trait]
impl CodingAgent for MyNewRoleAgent {
    fn role(&self) -> AgentRole {
        AgentRole::MyNewRole
    }

    fn name(&self) -> &str {
        "My New Role Agent"
    }

    fn system_prompt(&self, ctx: &CodingAgentContext) -> String {
        format!(
            "You are a specialized {} agent working on: {}\n\
             Your job is to ...\n\
             Brain context: {}\n\
             Code graph: {}",
            self.name(),
            ctx.workspace.task.description,
            ctx.brain_context,
            ctx.code_graph_context,
        )
    }

    fn max_iterations(&self) -> u32 {
        10
    }

    fn tools_allowed(&self) -> Vec<&str> {
        vec!["file_read", "file_write", "bash", "grep"]
    }

    async fn execute(&self, ctx: &CodingAgentContext) -> anyhow::Result<AgentOutput> {
        // Stream progress event
        ctx.event_tx
            .send(crate::coding_agents::types::CodingEvent::Progress {
                agent: self.role(),
                phase: AgentPhase::Implement,
                message: "Starting my new role work".into(),
                progress: 0.0,
            })
            .await
            .ok();

        // Do LLM call + file operations here
        // ...

        Ok(AgentOutput {
            agent: self.role(),
            summary: "Completed my new role".into(),
            files_changed: vec![],
            decisions: vec![],
            errors: vec![],
            should_continue: true,
            next_phase: Some(AgentPhase::Review),
            iterations_used: 1,
        })
    }
}
```

### 3. Register in coding_agents/mod.rs

```rust
pub mod my_new_role;
pub use my_new_role::MyNewRoleAgent;
```

### 4. Wire into the pipeline

In `crates/nexus-http/src/coding_agents/engine.rs`, add your agent to the wave execution sequence where appropriate.

---

## B. Adding a Super Agent

### 1. Add the SuperAgentKind variant

Open `crates/nexus-http/src/super_agents/types.rs` and add to `SuperAgentKind`:

```rust
pub enum SuperAgentKind {
    // ... existing ...
    MyOptimizer,  // <-- add here
}
```

Implement `default_priority`, `conflict_groups`, and display in the impl block.

### 2. Create the agent file

`crates/nexus-http/src/super_agents/my_optimizer.rs`

```rust
//! MyOptimizer super agent — <one-line description>.

use std::sync::Arc;

use async_trait::async_trait;

use super::{
    traits::{
        AnalysisContext, AnalysisReport, OptimizationContext, OptimizationResult,
        SuperAgent, ValidationContext, ValidationOutcome,
    },
    types::{ConflictGroup, Finding, FindingSeverity, SuperAgentKind, TriggerMode},
};
use crate::state::AppState;

pub struct MyOptimizerAgent;

#[async_trait]
impl SuperAgent for MyOptimizerAgent {
    fn name(&self) -> &str {
        "My Optimizer"
    }

    fn kind(&self) -> SuperAgentKind {
        SuperAgentKind::MyOptimizer
    }

    fn trigger(&self) -> TriggerMode {
        TriggerMode::Scheduled { interval_secs: 300 }  // every 5 minutes
    }

    async fn analyze(&self, ctx: &AnalysisContext) -> anyhow::Result<AnalysisReport> {
        let mut findings = vec![];

        // Collect metrics from the bus
        // let metric = ctx.metrics.get("some_metric").await;

        // Produce findings
        findings.push(Finding {
            id: "my_finding".into(),
            severity: FindingSeverity::Info,
            title: "Example finding".into(),
            description: "Description of what was found".into(),
            actionable: true,
            rollback_key: Some("my_optimizer_v1".into()),
        });

        Ok(AnalysisReport {
            agent: self.kind(),
            findings,
            snapshot: ctx.snapshot.clone(),
        })
    }

    async fn optimize(
        &self,
        ctx: &OptimizationContext,
        report: &AnalysisReport,
    ) -> anyhow::Result<OptimizationResult> {
        if ctx.dry_run {
            return Ok(OptimizationResult::skipped("dry_run"));
        }

        // Apply optimizations
        // MUST be idempotent

        Ok(OptimizationResult::applied("my_optimizer_v1", "Applied optimization"))
    }

    async fn validate(&self, ctx: &ValidationContext) -> anyhow::Result<ValidationOutcome> {
        // Verify the system is still healthy
        Ok(ValidationOutcome::healthy())
    }

    async fn rollback(&self, _app: &Arc<AppState>, rollback_key: &str) -> anyhow::Result<()> {
        tracing::info!(rollback_key, "Rolling back MyOptimizer");
        // Undo the optimization
        Ok(())
    }
}
```

### 3. Register in super_agents/mod.rs

```rust
pub mod my_optimizer;
pub use my_optimizer::MyOptimizerAgent;
```

### 4. Register in the orchestrator

In `crates/nexus-http/src/super_agents/orchestrator.rs`, add your agent to the `build_agents` function:

```rust
agents.push(Arc::new(MyOptimizerAgent));
```

---

## Common rules for both agent types

- **Always stream events** using the provided channel — never silently block
- **Never hold the SQLite lock across `.await`** — acquire, use, release
- **LLM calls** go through `crate::llm_client` — use the shared `http_client` from AppState
- **Errors** should be returned as `anyhow::Result` — the orchestrator handles failures
- **Idempotency** — super agent `optimize` MUST be safe to call multiple times
- All new agents need a corresponding test in `crates/nexus-http/tests/`
