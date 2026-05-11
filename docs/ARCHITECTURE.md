# Architecture

A 5-minute mental model of how Nexus works, plus a tour of the 33 crates.

## Mental model

```
                   ┌────────────────────────────────────────────┐
                   │  Conversation                              │
                   │  "Build me a yoga booking SaaS."           │
                   └────────────────────────────────────────────┘
                                       │
                                       ▼
        ┌──────────────────────────────────────────────────────────────┐
        │  Intent Engine    deterministic heuristics, no LLM call      │
        └──────────────────────────────────────────────────────────────┘
                                       │
                                       ▼
        ┌──────────────────────────────────────────────────────────────┐
        │  Decision Engine  picks stack / agents / workflow            │
        │  (LLM only when heuristics can't classify)                   │
        └──────────────────────────────────────────────────────────────┘
                                       │
                                       ▼
   ┌───────────────────────────────────────────────────────────────────────┐
   │  Wave Orchestrator                                                    │
   │   wave 1 ─ Leo (PM)  Kai (research)                                   │
   │   wave 2 ─ Nova (code)  Sage (data)  Ivy (copy)  Atlas (deploy)       │
   │   wave 3 ─ Orion (sec)  Rex (DevOps) Mia (support) Luna (docs)        │
   └───────────────────────────────────────────────────────────────────────┘
        │            │             │
   bus + sink   plugin hooks   Agent TV (SSE)
        │            │             │
        ▼            ▼             ▼
   SQLite       WASI sandbox   Frontend stream
```

Two principles run through the code:

1. **Deterministic first, LLM second.** Every decision-bearing engine starts
   with rules / heuristics / lookups. The LLM is the fallback, not the
   default. Cheaper, faster, explainable.
2. **The event log is the source of truth.** Team events, cost records,
   audit entries, plugin traps — all written to SQLite append-only tables.
   In-memory state is a derivation. Crash → resume from the log.

## Workspace map (33 crates)

The repo is a Cargo workspace. Each crate has a single responsibility:

### Backbone
- **`nexus-http`** — Axum server. ~250 routes, 86 handler files. The bulk
  of the application logic. SSE streams, REST endpoints, WebSocket for live
  build progress.
- **`nexus-store`** — SQLite persistence. Migrations (1–9 today), services
  per domain (`projects`, `team_events`, `cost_records`, `vault`, …).
- **`nexus-core`** — Single embed surface for library consumers: agent pool
  + workflow engine + project store wired together.
- **`cli`** — `nexus` binary. Project init, MCP server, plugin install,
  workflow run.

### Agents & runtime
- **`nexus-zeroclaw`** — The named agent runtime. Nova, Atlas, Kai, Luna,
  Orion, Sage, Ivy, Rex, Leo, Mia. Multi-provider tool calling.
- **`nexus-agents-core`** — Agent traits and team types shared between
  workspace crates.
- **`nexus-pipeline`** — Deterministic agent execution pipeline with
  blueprints and feedback gates.
- **`nexus-runtime`** — Parallel execution runtime, process isolation,
  durable checkpoints.

### LLM layer
- **`nexus-providers`** — `LlmProvider` trait + concrete impls for OpenAI,
  Anthropic, and Ollama. ADR-003.
- **`nexus-mcp`** — Model Context Protocol client + server.
- **`nexus-acp`** — Agent Client Protocol adapter for Zed / JetBrains.

### Memory & knowledge
- **`nexus-memory`** — Episodic + semantic memory, embedding storage.
- **`nexus-context`** — Three-layer context engine with adaptive
  compression for long-running runs.
- **`nexus-graph`** — Embedded knowledge graph, contradiction detection,
  temporal edges.
- **`nexus-learn`** — Outcome tracking, pattern extraction, eval-gated
  skill promotion.

### Quality & evals
- **`nexus-quality-core`** — Taste scoring traits.
- **`nexus-eval`** — Eval harness, suite runner, LLM-as-judge.
- **`nexus-bench`** — Criterion benchmarks for hot paths.

### Codegen & sandbox
- **`nexus-codegen`** — Multi-agent code generation engine.
- **`nexus-workflow`** — Persistent, resumable workflow engine.
- **`nexus-sandbox`** — Wasmtime + Docker sandboxes for generated code.
- **`nexus-browser`** — Playwright/CDP browser automation for E2E tests.

### Marketplace & plugins
- **`nexus-plugins-sdk`** — Plugin SDK + manifest types.
- **`nexus-forge`** — Marketplace publishing + signing.
- **`nexus-pkg`** — Agent package manager.
- **`nexus-integrations` / `nexus-integrations-ext`** — First- and
  third-party integrations (GitHub, Slack, Stripe, Jira, …).

### Federation & policy
- **`nexus-a2a`** — Agent-to-Agent protocol for cross-instance federation.
- **`nexus-kernel`** — Process scheduler, event bus, reactive agents.
- **`nexus-intelligence`** — Intent / decision / product intelligence
  primitives.
- **`praesidia`** — Policy engine + cloud client (publish, register).
- **`nexus-sdk-client`** — Rust SDK for embedding Nexus in other apps.

### Demo
- **`agent-demo`** — Reference workflow that exercises a full app build.

## Hot paths

### A oneshot generation request

```
POST /oneshot                 (handlers/oneshot.rs)
  │
  ▼
intent_engine::classify       deterministic; ~5ms
  │
  ▼
decision_engine::pick         heuristics + optional LLM
  │
  ▼
wave_orchestrator::run        spawns N agents per wave via tokio
  │       │      │
  ▼       ▼      ▼
 bus    sink   metrics      persist + SSE + Prometheus
  │
  ▼
taste_engine::score           guarantee + redesign loop if < threshold
  │
  ▼
SSE: { type: "done", … }      terminal event, MUST always fire
```

### A team run resumes after a crash

```
1. AppState::init spawns reconcile_orphans_on_boot
2. team_run_state rows with status=running, last_event_at_ms < now-60s
   → flipped to status=paused, pause_reason='server_restart'
3. Operator/user calls POST /teams/runs/:id/resume
4. Orchestrator replays team_events to rebuild in-memory hot index
5. New events resume from last_seq + 1
```

## Critical invariants

1. **Never hold `db.lock().await` across an `.await`.** Clone what you need,
   drop the guard, then await. Violated → SQLite contention deadlock.
2. **Intent engine is deterministic.** No LLM calls inside `intent_engine.rs`.
3. **Every SSE stream emits a terminal event** (`done` or `error`).
4. **Plugin hooks fire in BOTH oneshot and pipeline paths.**
5. **All generated files pass invariant enforcer before commit.**
6. **All governed actions go through `audit_log` (Ed25519 signed).**
7. **Every project-scoped query filters by `project_id`** via the
   `project_access` guard.
8. **Every public LLM endpoint runs through `rate_limiter`.**
9. **Every LLM call goes through the `LlmCallEnvelope`** — runs the budget
   brake, persists to `cost_records`, emits Prometheus.
10. **`unwrap()` and `panic!` are warned by clippy** — outside `#[cfg(test)]`.

## Architecture Decision Records

Concrete contract decisions live in `docs/adr/`:

- **ADR-001** — first-run secrets contract (`secrets.toml`, `NEXUS_PRODUCTION`)
- **ADR-002** — durable team-event schema
- **ADR-003** — `LlmProvider` trait surface
- **ADR-004** — plugin sandbox ABI (WASI)
- **ADR-005** — cost-record write path

New cross-cutting contracts get an ADR before code. Format: Context →
Decision → Consequences → Alternatives considered.

## Frontend

`web/` is a Next.js 16 App Router app. Key client architecture:

- **`web/src/app/`** — App Router routes, including the chat-first
  `/[projectId]` shell.
- **`web/src/lib/api.ts`** — Hand-written API client (~196 methods).
  All backend access flows through this surface.
- **`web/src/hooks/`** — TanStack Query hooks per domain.
- **`web/src/components/`** — UI primitives (shadcn) + composite views
  (Agent TV grid, workflow canvas, chat).

## Where to start

- New backend feature → read the relevant subsystem in this doc, then look
  for similar handlers in `crates/nexus-http/src/handlers/`.
- New frontend page → start from an existing `/[projectId]/<route>` and
  copy the pattern (server component for initial load, client component
  for interactivity).
- New agent → drop a definition into `crates/nexus-zeroclaw/src/roster.rs`.
- New plugin → see ADR-004 + `crates/nexus-plugins-sdk/`.
