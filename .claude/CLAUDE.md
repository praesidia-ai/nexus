# NEXUS — Claude Code Project Intelligence

Behavioral guidelines + current architecture reference for the nexus-rust workspace.
Bias toward caution over speed. For trivial tasks, use judgment.

---

## Behavioral Rules

### 1. Think Before Coding
- State assumptions explicitly. If uncertain, ask.
- If multiple interpretations exist, present them — don't pick silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what's confusing. Ask.

### 2. Simplicity First
- No features beyond what was asked.
- No abstractions for single-use code, no error handling for impossible scenarios.
- Would a senior engineer say this is overcomplicated? If yes, simplify.

### 3. Surgical Changes
- Touch only what you must. Match existing style.
- Don't refactor things that aren't broken; don't "improve" adjacent code.
- Remove imports/symbols YOUR changes orphaned — leave pre-existing dead code alone.
- Every changed line should trace directly to the user's request.

### 4. Goal-Driven Execution
- Transform tasks into verifiable goals (write the failing test first, then make it pass).
- For multi-step work, state a brief plan with verify steps before touching code.

---

## Architecture Overview

- **Backend:** Rust + Axum 0.7, Tokio async runtime, single binary `nexus-server`.
- **Database:** SQLite via rusqlite, behind `Arc<tokio::sync::Mutex<Connection>>`.
- **Frontend:** Next.js 14+ App Router in `web/` (Tailwind + shadcn/ui).
- **Communication:** SSE-heavy real-time progress; broadcast channels per project.
- **Project state:** under `~/.nexus/projects/<id>` (override with `NEXUS_DATA_DIR`).
- **Philosophy:** *Deterministic intelligence first, LLM generation second.*

## Workspace Crates (33 total)

Listed in `Cargo.toml` workspace members. Roles below:

| Crate | Role |
|-------|------|
| `nexus-http` | Main Axum server — handlers, engines, agents, plugins (118+ .rs files, 86 handlers) |
| `nexus-store` | SQLite persistence — migrations, projects, sessions, knowledge |
| `nexus-core` | Core IR, app state tracking, prompt templates |
| `nexus-kernel` | Process scheduler, event bus, reactive agent manager, audit log |
| `nexus-a2a` | A2A protocol — agent cards, federation, task dispatch |
| `nexus-acp` | Agent control protocol primitives |
| `nexus-agents-core` | Shared agent types and traits |
| `nexus-mcp` | MCP server registry + client integration |
| `nexus-memory` | Long-term memory, embeddings, retrieval |
| `nexus-context` | Context assembly, windowing, compression |
| `nexus-codegen` | Code generation primitives |
| `nexus-runtime` | Generated app runtime adapters |
| `nexus-pipeline` | Pipeline DAG primitives |
| `nexus-workflow` | Workflow definitions, runner |
| `nexus-graph` | Code graph + dependency analysis |
| `nexus-providers` | LLM provider adapters (OpenAI / Anthropic / Ollama) |
| `nexus-plugins-sdk` | Plugin SDK + manifest types |
| `nexus-integrations` / `-ext` | External integrations (Stripe, Slack, etc.) |
| `nexus-eval` | Eval harness, suite runner, regression tracking |
| `nexus-bench` | Benchmarks |
| `nexus-sandbox` | Sandboxed execution for generated code |
| `nexus-browser` | Headless browser automation |
| `nexus-forge` | Marketplace publishing + signing |
| `nexus-pkg` | Package format + distribution |
| `nexus-learn` | Learning loops, decision improvement |
| `nexus-intelligence` | Intelligence layer primitives |
| `nexus-quality-core` | Quality gates, taste scoring core |
| `nexus-zeroclaw` | ZeroClaw agent runtime, pooling, roster |
| `nexus-sdk-client` | Client SDK |
| `praesidia` | Policy framework, governance hooks |
| `cli` | Command-line interface |
| `agent-demo` | Demo / reference agent |

## Core Execution Planes
1. **Oneshot plane** (`/oneshot` → `handlers/oneshot.rs`) — canonical intelligence-first generation path.
2. **Execution pipeline** (`/pipeline/run` → `execution_pipeline.rs`) — older structured pipeline; converging on oneshot.
3. **Wave pipeline** (`coding_agents/wave_orchestrator.rs`) — parallel coding agent execution.
4. **Team orchestrator** (`team_orchestrator.rs`) — multi-agent business teams with HITL injection.

## Key Subsystems (`nexus-http/src/`)

### Intelligence & Decisions
- `intent_engine.rs` — deterministic keyword/rule heuristics (NO LLM calls)
- `decision_engine.rs` + `decision_learning.rs` — architecture selection with learning
- `predictive_intent.rs` + `predictive_preprocess.rs` — speculative prefetching
- `anticipation_engine.rs` — anticipates next user actions
- `causal_learning.rs` — causal inference on outcomes
- `nexus_brain.rs` + `nexus_intelligence.rs` — top-level reasoning surface
- `explain_engine.rs` (+ v2) — decision explainability

### Generation & Quality
- `product_engine.rs` — product brief, personas, monetization, copy
- `taste_engine.rs` + `taste_redesign.rs` + `taste_gate.rs` — UI quality scoring + auto-redesign
- `mutation_engine.rs` — incremental file edits with rollback
- `outcome_guarantee.rs` — multi-cycle auto-repair loop
- `invariant_enforcer.rs` + `invariants.rs` — guarantees on generated output
- `quality_gate.rs` — pre-deploy quality checks
- `smoke_test.rs` + `runtime_tester.rs` — generated-app verification
- `production_sim.rs` — simulated production traffic / chaos
- `variant_engine.rs` — A/B variant generation

### Agents
- `coding_agents/` — architect, coder, debugger, devops, performance, product, refactor, reviewer, tester, ux + `wave_orchestrator.rs`, `engine.rs`, `swarm.rs`, `verification.rs`, `zeroclaw_bridge.rs`
- `super_agents/agents/` — agent_efficiency, build_runtime, cache_optimizer, concurrency_optimizer, context_compressor, database_optimizer, latency_optimizer, llm_cost_optimizer, pipeline_bottleneck, sse_optimizer (+ `orchestrator.rs`, `metrics_bus.rs`)
- `agents/` — definition, planner, factories, runtime, self_correction, team_templates, tools/
- `mini_agents/` — lightweight scoped agents
- `agent_orchestrator.rs`, `agent_routing.rs`, `agent_loop.rs`, `agent_designer.rs`
- `team_orchestrator.rs` + `team_prompting.rs` + `team_templates.rs` — multi-agent teams
- `business_teams.rs` + `app_agent.rs` — App-as-Agent + business team bindings
- `borrowed_agents.rs` — cross-project agent sharing
- `background_executor.rs` + `system_scheduler.rs` — async scheduling
- `swarm.rs` (handlers + coding_agents) — swarm coordination

### Runtime & Process
- `app_runner.rs` (handler) — process lifecycle, portal publishing, deploy hooks
- `live_build_handler.rs` + `live_update.rs` + `live_update_handler.rs` — streaming build progress
- `runtime_feedback.rs` + `runtime_observer.rs` — runtime telemetry
- `self_healing.rs` — auto-recovery
- `adaptive_runtime.rs` + `adaptive_control.rs` — adaptive control loops

### Plugins & Extensibility
- `plugin_system.rs` — unified registry (+ legacy registry for backward compat)
- `plugin_hooks.rs` — hook points called from oneshot AND pipeline
- `plugin_installer.rs` + `marketplace.rs` (handler) — install + browse
- `mcp_handler.rs` + `nexus-mcp` crate — MCP server connections

### Security, Governance, Trust
- `security/` — `auth.rs`, `api_keys.rs`, `tenant.rs`, `project_access.rs`, `rate_limit.rs`, `audit.rs`, `url_guard.rs`
- `governance.rs` + `governance_handler.rs` — policy engine, kill switch
- `trust.rs` + `trust_handler.rs` + `trust_cert.rs` — trust certificates
- `forge_reputation.rs` — marketplace publisher reputation
- `hitl.rs` + `hitl_handler.rs` — human-in-the-loop approvals
- `audit_trail_handler.rs` — cryptographic audit log (Ed25519 + Merkle chain)
- `input_limits.rs` + `log_redact.rs` — input validation, log scrubbing
- `rate_limiter.rs` — concurrency slots + per-IP limits

### LLM & Cost
- `llm_client.rs` + `model_router.rs` + `llm_model_defaults.rs` — multi-provider dispatch
- `anthropic_cache.rs` — Anthropic prompt cache integration
- `cache.rs` — LLM response cache
- `cost_intelligence.rs` — token + cost tracking with optimization
- `prompt_evolution.rs` — automatic prompt refinement
- `personality.rs` — system prompt persona configuration

### Knowledge & Memory
- `learning_memory.rs` + `memory_unification.rs` — unified memory layer
- `code_graph.rs` + `nexus-graph` crate — code graph + analysis
- `evolution/` — `optimizer.rs`, `pattern_learner.rs`, `tracker.rs`
- `skill_dna.rs` + `skill_runtime.rs` + `agent_skills.rs` (handler) — agent skill packs
- `feedback_engine.rs` + `user_learning.rs` + `user_pattern_detector.rs` + `user_sim.rs`
- `claude_md.rs` + `claude_md_injector.rs` — per-project CLAUDE.md injection
- `global_intelligence.rs` + `collective_intelligence.rs` + `intelligence_amplifier.rs`
- `continuous_improve.rs` + `self_improvement.rs` + `self_improvement_engine.rs`

### Observability
- `telemetry.rs` + `observability.rs` (handler) + `http_metrics.rs`
- `health_checks.rs` + `graceful_shutdown.rs`
- `agent_tv_sink.rs` + `agent_tv.rs` (handler) — live agent TV stream
- `thinking_stream.rs` — streaming chain-of-thought
- `generation_event.rs` + `generation_event_tests.rs`
- `webhooks.rs` + `webhook_handler.rs`

### Multimodal & Design
- `multimodal.rs` + `multimodal_handler.rs` — image/audio/video inputs
- `design_system/` — `themes/`, `blocks/`, `animations/`, `css_themes.rs`
- `agent_versioning.rs` — versioned agent definitions

## Handler Endpoints (`handlers/` — 86 files)

Full set, grouped by area:

- **Generation:** `oneshot`, `chat`, `codegen`, `coding`, `coding_agents_handler`, `llm_codegen`, `intent_handler`, `intent_to_app`, `mutation_handler`, `planner`, `wave_pipeline_handler`
- **Execution & Runtime:** `execution`, `app_runner`, `live_build_handler`, `live_update_handler`, `app_agent_handler`, `sandbox_handler`, `smoke_test_handler`, `production_sim_handler`, `swarm`, `unified_agents`
- **Agents & Teams:** `agents`, `agent_run`, `agent_skills`, `agent_tv`, `background_agents`, `borrowed_agents`, `business_handler`, `teams_handler`, `orchestrator_handler`, `super_agents_handler`, `living_system_handler`
- **Quality & Repair:** `taste_handler`, `guarantee_handler`, `eval_handler`, `invariants_handler`, `enforcement`, `self_improvement_handler`
- **Knowledge & Memory:** `knowledge`, `memory_handler`, `global_memory`, `code_graph`, `brain_handler`, `intelligence_handler`, `intelligence_layer_handler`, `learning_handler`, `user_learning_handler`, `user_sim_handler`
- **Plugins & Marketplace:** `plugins`, `plugin_handler`, `marketplace`, `templates_handler`, `forge`, `mcp_handler`, `integrations_handler`
- **Security & Governance:** `security_handler`, `governance_handler`, `trust_handler`, `trust_cert`, `hitl_handler`, `audit_trail_handler`, `a2a_handler`, `webhook_handler`
- **Observability & Cost:** `metrics`, `observability`, `cost_handler`, `credits_handler`, `speed_handler`
- **Decisions & Explain:** `decision_handler`, `decision_learning_handler`, `explain`, `kernel_handler`
- **Project & Settings:** `projects`, `settings`, `preferences_handler`, `vault`, `tables`, `portal`, `multimodal_handler`, `evolution_handler`, `evolution_phase_handler`, `product_engine_handler`, `workflows`, `workflow_designer_handler`, `collaboration`

Handler discovery: `crates/nexus-http/src/handlers/mod.rs` registers modules; `server.rs` wires routes (~400+ route bindings).

## Frontend (`web/`)

Next.js App Router. Key project-scoped views under `web/src/app/[projectId]/`:

`agent-tv`, `agents`, `approvals`, `audit`, `build`, `business`, `data`, `deploy`, `federation`, `files`, `governance`, `integrations`, `knowledge`, `learning`, `memory`, `observability`, `portal`, `processes`, `quality`, `teams`, `vault`, `workflows`

Top-level: `admin`, `agents`, `bench`, `marketplace`, `security`, `settings`, `trust`.

- **State:** React hooks; unified workspace via `lib/use-unified-workspace.ts`
- **API client:** `lib/api.ts`
- **UI:** Tailwind CSS 4 + shadcn/ui pattern (button, card, dialog, input, etc.)
- **SSE consumers:** EventSource clients for live-build, agent-tv, thinking streams

## AppState (`nexus-http/src/state.rs`)

Required fields when accessing app state:
- `db` — `Arc<tokio::sync::Mutex<rusqlite::Connection>>`
- `data_dir` — `~/.nexus` root
- `started_at` — server start timestamp
- `openai_api_key`, `anthropic_api_key`, `model` — LLM config (configured via Settings UI, not env at startup)
- `http_client` — shared reqwest client
- `metrics_bus`, `orchestrator` — super agents
- `legacy_plugin_registry` + `plugin_registry` — plugin systems
- `build_event_bus` — per-project SSE broadcast
- `llm_cache`, `rate_limiter`, `cost_tracker`, `predictor`
- `mcp_registry`, `eval_results`, `webhook_service`
- `scheduler`, `event_bus`, `reactive_manager` — kernel
- `team_run_registry` — in-flight team orchestrators (keyed by run_id)
- `a2a_registry`, `a2a_server`, `federation` — A2A protocol
- `audit_log`, `audit_keypair` — Ed25519-signed Merkle chain
- `policy_engine` — governance
- `app_agent_registry` — App-as-Agent bindings

## Critical Invariants

1. **SQLite lock:** never hold `db.lock().await` across an `.await` point — clone what you need and drop the guard.
2. **Intent engine:** must remain deterministic — NO LLM calls inside `intent_engine.rs`.
3. **SSE streams:** every stream MUST emit a terminal `complete` or `error` event.
4. **Plugin hooks:** must be called in BOTH oneshot AND pipeline paths.
5. **Generated files:** must pass the invariant enforcer before being committed to project state.
6. **Audit log:** all governed actions go through `audit_log` (signed); never bypass.
7. **Tenant isolation:** every project-scoped query must filter by `project_id` via `project_access.rs` guard.
8. **Rate limit:** every public LLM-bearing endpoint must run through `rate_limiter`.

## Code Conventions

- Error handling in handlers: use `crate::error::ApiError` / `ApiResult` (typed HTTP statuses).
- Library code: use `anyhow` for propagation, `thiserror` for typed error enums.
- All endpoints return structured JSON with consistent error envelopes.
- All SSE events follow a typed event schema.
- Migrations are versioned and backward-compatible (added in `nexus-store/src/migrations/`).
- Tests live in `tests/` co-located with the module.
- TypeScript strict mode — no `any`, no `@ts-ignore` without comment.
- Match existing style; don't reformat adjacent code.

## Build & Test

```bash
cargo build --workspace                     # full backend build
cargo build -p nexus-http                   # main server only
cargo test --workspace --no-fail-fast       # all tests
cargo clippy --workspace -- -D warnings     # lint (CI gate)
cargo fmt --check                           # format check

cd web && npm run build                     # frontend build
cd web && npm run dev                       # frontend dev server
```

Run `/check-quality` before committing — it enforces the full gate.

## Environment Variables

- `NEXUS_DATA_DIR` — data root (default `~/.nexus`)
- `NEXUS_PORT` — listen port (default `8080`)
- `NEXUS_MODEL` — default LLM model (default `gpt-4o`)
- `OPENAI_API_KEY` / `ANTHROPIC_API_KEY` — **prefer Settings UI over env**; the user configures keys in the frontend Settings page (per project memory)

## Operational Notes

- **After every change pass:** restart backend AND frontend via tmux so running processes match latest code.
- **Marketplace pages** (`nexus.praesidia.ai/marketplace`) are strategic surface — never recommend removing as stubs; prioritize wiring + SEO.
- **API keys policy:** never set `OPENAI_API_KEY` / `ANTHROPIC_API_KEY` at backend startup; they belong in the Settings UI.

---

## Slash Commands

| Command | Purpose |
|---------|---------|
| `/build` | Full workspace build, group errors by crate |
| `/check-quality` | Full pre-commit gate (clippy, tests, fmt, smells, invariants) |
| `/test` | Run test suite and summarize |
| `/fix-errors` | Systematically fix Rust compile/clippy errors |
| `/debug-performance` | Diagnose perf problems in nexus-rust |
| `/add-handler` | Scaffold a new Axum handler (uses `add-handler` skill) |
| `/add-engine` | Scaffold deterministic-first + LLM-fallback engine |
| `/add-agent` | Scaffold a coding agent or super agent |
| `/add-plugin` | Scaffold plugin / hook |
| `/add-migration` | Scaffold a new SQLite migration |
| `/llm-call` | Add or fix an LLM call |
| `/frontend` | Build/modify a UI feature in `web/` |

## Skills (`.claude/skills/`)

Loaded via the Skill tool when the task matches:
`add-agent`, `add-engine`, `add-handler`, `add-migration`, `add-plugin`,
`debug-performance`, `frontend`, `llm-calls`, `rust-patterns`, `sse-streaming`.

When writing new Rust in any nexus crate, consult `rust-patterns`. When making LLM calls, consult `llm-calls`. When adding SSE endpoints, consult `sse-streaming`.
