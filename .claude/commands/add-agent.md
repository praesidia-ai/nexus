Add a new coding agent or super agent to nexus-http.

Read `.claude/skills/add-agent/SKILL.md` first, then determine the agent type:

**Coding Agent** (participates in the coding pipeline — architect, coder, reviewer, etc.):
1. Add variant to `AgentRole` enum in `coding_agents/types.rs`
2. Create `crates/nexus-http/src/coding_agents/<role>.rs` implementing `CodingAgent` trait
3. Register in `coding_agents/mod.rs`
4. Wire into the wave pipeline in `coding_agents/engine.rs`

**Super Agent** (background optimizer — cache, latency, cost, etc.):
1. Add variant to `SuperAgentKind` enum in `super_agents/types.rs`
2. Create `crates/nexus-http/src/super_agents/<name>.rs` implementing `SuperAgent` trait
3. Register in `super_agents/mod.rs`
4. Register in `super_agents/orchestrator.rs` `build_agents` function

Both agent types:
5. Run `cargo build -p nexus-http` and fix errors
6. Run `cargo clippy -p nexus-http -- -D warnings`
7. Add a test for the core logic

Key invariants:
- Never hold the SQLite mutex lock across `.await` in agent code
- Always stream progress events via the provided channel
- Super agent `optimize` MUST be idempotent (safe to call multiple times)
