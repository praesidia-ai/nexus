<div align="center">

# Nexus

### Hire an AI company. Watch them ship.

**A self-hosted, open-source, multi-agent orchestrator. One command, ten named conductors, on your laptop, with receipts.**

[![CI](https://github.com/praesidia-ai/nexus/actions/workflows/ci.yml/badge.svg)](https://github.com/praesidia-ai/nexus/actions/workflows/ci.yml)
[![Version](https://img.shields.io/badge/version-1.0.0-blue.svg)](CHANGELOG.md)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![Rust 1.82+](https://img.shields.io/badge/rust-1.82%2B-orange.svg)](https://www.rust-lang.org)
[![Self-hosted](https://img.shields.io/badge/deploy-self--hosted-success.svg)](#deploy)
[![Discord](https://img.shields.io/badge/discord-join-5865F2.svg)](https://discord.gg/praesidia)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](CONTRIBUTING.md)

[**Quickstart**](#quickstart) ·
[**Watch the demo**](#agent-tv) ·
[**Architecture**](docs/ARCHITECTURE.md) ·
[**Roadmap**](#roadmap) ·
[**Contributing**](CONTRIBUTING.md)

</div>

---

## What is Nexus?

Type one sentence. Ten named agents — Nova, Atlas, Kai, Luna, Orion, Sage, Ivy, Rex, Leo, Mia — wake up, plan the work, write the code, ship the build, and stay on staff to **run** what they built. The whole team streams live in **Agent TV**, every action is signed and auditable, and the system runs on a single self-hosted Rust binary or a `docker compose up`.

You can describe a startup and get a startup. Or describe a customer-support org, a research lab, a content studio. The team stays. Tomorrow you can talk to them again.

```text
you  › Build a SaaS for booking yoga classes. Run it for me.

nexus › Hiring team ──────── Leo (PM) · Nova (eng) · Sage (data)
                              Ivy (marketing) · Mia (support)
        Designing product ── 3 personas · Stripe checkout
        Generating 24 files
        Building & deploying  https://yoga-tan.nexus.run
        Team standup ──────── 5 agents on staff. Sleep well.
```

This is **not** a one-shot code generator. Bolt, v0, Lovable, Replit Agent all stop at "ship the codebase." Nexus ships the codebase **and** the persistent team that operates it after launch.

---

## Why people are switching

| | **Nexus** | bolt.new | Lovable | Cline / Aider | CrewAI / LangGraph |
|---|:-:|:-:|:-:|:-:|:-:|
| Generates the product | ✅ | ✅ | ✅ | edit-only | library |
| Generates a **persistent team** to run it | ✅ | ❌ | ❌ | ❌ | DIY |
| Self-hosted single binary | ✅ | ❌ | ❌ | ❌ | ❌ |
| Live multi-agent dashboard | ✅ | ❌ | ❌ | ❌ | ❌ |
| Local LLM (Ollama) supported | ✅ | partial | ❌ | partial | partial |
| Per-tenant cost ledger + budget brake | ✅ | ❌ | ❌ | ❌ | ❌ |
| WASI plugin sandbox | ✅ | ❌ | ❌ | ❌ | ❌ |
| Cryptographic audit log (Ed25519 + Merkle) | ✅ | ❌ | ❌ | ❌ | ❌ |
| MCP server out of the box | ✅ | ❌ | ❌ | partial | ❌ |
| Dual MIT / Apache-2.0, no CLA | ✅ | MIT | proprietary | varies | MIT |

Honest reading: bolt.new and Lovable are great single-agent code generators. Cline and Aider are great pair programmers. CrewAI is a Python library you have to wire up yourself. **Nexus is the team layer** — pick it when you want an AI workforce, not a sidekick.

---

## Quickstart

### Option A — Docker (recommended, 60 seconds)

```bash
git clone https://github.com/praesidia-ai/nexus.git
cd nexus
cp .env.example .env
docker compose up
```

Open <http://localhost:8080>, drop your OpenAI or Anthropic key in **Settings**, and click **Build AI Company**. (Skip the keys to run fully offline on Ollama.)

### Option B — Single binary

```bash
curl -fsSL https://install.nexus.praesidia.ai | sh
nexus init my-startup
nexus chat "Build me a yoga booking SaaS"
```

The installer is a single `curl | sh` that auto-detects your platform, drops one binary in `/usr/local/bin`, and asks for nothing.

### Option C — From source (Rust 1.82+, Node 20+)

```bash
git clone https://github.com/praesidia-ai/nexus.git
cd nexus
cp .env.example .env
cargo run -p nexus-http &
cd web && npm install && npm run dev
```

Open <http://localhost:3000>. Backend runs on :8020, frontend on :3000.

### First-run is graceful

You can boot Nexus with **zero environment variables** for local dev. JWT and at-rest-encryption keys auto-generate to `~/.nexus/secrets.toml` (mode `0600`). Set `NEXUS_PRODUCTION=1` to require explicit secrets. See [ADR-001](docs/adr/001-first-run-secrets.md).

---

## Agent TV

The window into your AI team while they work.

Open `/[projectId]/agent-tv` and you get a 2×5 grid — one card per specialist — streaming over SSE. Each card shows status (idle / thinking / writing / reviewing / done), the file the agent is editing right now, a token-by-token thought stream, and a live cost + token ticker.

| Agent | Role | Tools |
|---|---|---|
| **Nova 🚀** | Full-stack engineer | shell, file, git, web_fetch |
| **Atlas ☁️** | Cloud & infra | shell, file, web_fetch |
| **Kai 🔍** | Research | web_fetch, web_search, file |
| **Luna ✍️** | Technical writer | file, web_fetch |
| **Orion 🛡️** | Security review | shell, file, web_fetch |
| **Sage 📊** | Data & analytics | shell, file, web_fetch |
| **Ivy 📣** | Marketing copy | web_fetch, web_search, file |
| **Rex ⚙️** | DevOps | shell, file, git, web_fetch |
| **Leo 🎯** | Product management | file, web_fetch |
| **Mia 💬** | Customer support | file, web_fetch |

When Orion flags an unvalidated input, you see it. When Rex's `npm install` finishes, you see it. The point: 10 agents feel like a team, not a queue.

---

## Drive Nexus from Claude Desktop in 30 seconds

Nexus is an MCP server. Add this to `~/Library/Application Support/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "nexus": { "command": "nexus", "args": ["mcp", "serve"] }
  }
}
```

Restart Claude Desktop. `/mcp list` now shows `nexus`. Claude can now list your projects, read files, trigger generations, stream Agent TV, and verify signed run certificates — all by talking to a locally-running Nexus.

Same flow works for Cursor, Zed, and Claude Code.

---

## What ships in v1.0

### Agents & Orchestration
- **10-agent ZeroClaw roster** — Nova (eng), Atlas (infra), Kai (research), Luna (writer), Orion (security), Sage (data), Ivy (marketing), Rex (devops), Leo (PM), Mia (support)
- **11 specialist coding agents** — architect, coder, debugger, devops, performance, product, refactor, reviewer, tester, UX, plus a mini-agent layer for lightweight scoped tasks
- **Wave orchestrator** — agents fan out per wave over a durable message bus; waves are retried independently on failure
- **Team orchestrator** — multi-agent business teams with human-in-the-loop (HITL) injection at any node
- **8 company blueprints** — SaaS startup, AI agency, support org, research lab, devops squad, content studio, e-commerce ops, sales org
- **Oneshot pipeline** — canonical generation path: intent → decision → coding → taste → guarantee
- **Execution pipeline** — older structured path converging on oneshot; both call plugin hooks
- **Borrowed agents** — share a specialist from one project into another without duplication
- **Background executor + system scheduler** — long-running async tasks with cron-style scheduling
- **Swarm coordination** — fan-out / fan-in across project boundaries
- **App-as-Agent** — every generated app gets a bound agent persona; talk to your app directly

### Generation & Quality
- **Oneshot generation** (`POST /oneshot`) — the primary intelligence-first path; deterministic heuristics first, LLM only for edge cases
- **Intent engine** — pure-deterministic keyword + rule heuristics; zero LLM calls, fully explainable
- **Decision engine + learning** — architecture selection with feedback learning; decisions are auditable
- **Predictive intent + preprocess** — speculative prefetching before the user finishes typing
- **Anticipation engine** — precomputes the most likely next action for sub-100ms response
- **Taste engine + auto-redesign** — UI quality scoring pipeline; auto-triggers redesign when below threshold
- **Taste gate** — hard quality gate before any build is marked complete
- **Mutation engine** — incremental file edits with automatic rollback on degradation
- **Outcome guarantee loop** — multi-cycle auto-repair: compile error → patch → re-run, up to N cycles
- **Invariant enforcer** — 20+ output invariants checked before generated files are committed
- **Quality gate** — pre-deploy checklist: lint, type-check, tests, taste score, invariants
- **Variant engine** — A/B variant generation for UI components or full pages
- **Product engine** — generates product briefs, user personas, monetisation plan, and landing copy from a single sentence
- **Prompt evolution** — automated prompt refinement based on outcome tracking

### Runtime & Execution
- **App runner** — full process lifecycle: spawn, health-check, hot-reload, portal publish, deploy hooks
- **Live build SSE stream** — token-by-token build progress over Server-Sent Events, one bus per project
- **Live update handler** — hot-patch a running app without restart
- **Runtime feedback + observer** — telemetry collected from generated app at runtime
- **Self-healing** — auto-recovery on detected runtime faults
- **Adaptive runtime + control** — adjusts concurrency, timeouts, and model choice based on live metrics
- **Sandbox execution** (`nexus-sandbox`) — WASI + Docker isolation for generated app code
- **Smoke test runner** — fast post-build sanity check on the generated app's HTTP surface
- **Production simulator** — synthetic traffic + chaos injection against the generated app before marking stable
- **Boot orphan reconciler** — runs stuck in `running` for >60s flip to `paused/server_restart` on boot

### Workflows & Planning
- **Visual DAG workflow composer** — drag-and-drop React Flow canvas; nodes: task, parallel, branch, loop, merge, human-approval
- **Durable workflow runner** — DAG execution with per-node retry, state persistence, and resume-on-restart
- **Planner agent** — decomposes a natural-language goal into an ordered task graph
- **Workflow export** — portable JSON; check workflows into git and replay them anywhere

### Knowledge & Memory
- **Long-term memory** (`nexus-memory`) — per-project embeddings, episode store, retrieval-augmented context
- **Memory unification layer** — merges short-term session memory, long-term episodic memory, and global intelligence into one ranked context window
- **Code graph** (`nexus-graph`) — static dependency graph across the generated codebase; used by architect and refactor agents
- **Global intelligence + collective intelligence** — cross-project pattern extraction; what worked in one project informs another
- **Intelligence amplifier** — boosts context relevance scores using causal signals
- **User learning + pattern detector** — learns preferences per user; adapts generation style over time
- **User simulator** — synthetic user testing of generated apps during the quality pass
- **Causal learning** — causal inference on build outcomes; improves decision engine over time
- **CLAUDE.md injection** — per-project CLAUDE.md is automatically injected into every agent's context
- **Skill DNA + skill runtime** — reusable agent skill packs; install, version, and swap agent capabilities without code changes

### Plugins & Extensibility
- **Plugin system** — unified registry with manifest-based install, version resolution, and capability declaration
- **WASI sandbox** (Wasmtime, [ADR-004](docs/adr/004-plugin-sandbox-abi.md)) — fuel + epoch + memory caps; hostile `while(true)` killed in <3s, host unaffected
- **Plugin hooks** — `OnIntentClassified`, `OnTasteScore`, `OnBuildComplete`, `OnDeploy`, `OnAgentMessage`, and 10+ more; called in both oneshot and pipeline paths
- **Plugin marketplace** — browse, install, and rate plugins at [`nexus.praesidia.ai/marketplace`](https://nexus.praesidia.ai/marketplace)
- **Plugin publisher** — sign, version, and publish WASI components to the forge; reputation scoring for publishers
- **MCP server registry** (`nexus-mcp`) — connect any MCP-compatible tool server; agents discover tools automatically

### Integrations
- **External integrations** (`nexus-integrations`) — Stripe (billing), Slack (notifications), webhooks (outbound + inbound), and more via the integrations extension crate
- **Webhooks** — inbound event dispatch and outbound signed delivery with retry
- **A2A protocol** (`nexus-a2a`) — agent-to-agent RPC across Nexus instances; scoped task dispatch with tenant attribution and rate limiting
- **Federation** — cross-instance agent discovery, capability advertisement, and task routing
- **Portal publishing** — one-click publish a generated app to a `*.nexus.run` subdomain

### Security & Governance
- **JWT + API-key auth** — scoped per route (`ProjectRead`, `ProjectWrite`, `AgentExecute`, `SystemAdmin`, …)
- **Tenant isolation** — every DB query enforces `tenant_id`; cross-tenant reads are structurally impossible
- **At-rest encryption** — vault secrets and LLM API keys encrypted with Argon2id-stretched AES-GCM, per-install salt
- **Ed25519 + Merkle-chain audit log** — every governed action is signed and chained; chain verification on demand
- **Governance engine + kill switch** — policy rules evaluated before any privileged action; kill switch halts all generation cluster-wide
- **Trust certificates** — issued per agent run; verifiable proof of what an agent did and when
- **HITL approvals** — pause any workflow node, surface to a human, resume on explicit sign-off
- **Rate limiter** — per-IP + per-tenant concurrency slots; every LLM-bearing public endpoint is gated
- **Input limits + log redaction** — enforced payload caps; PII and key patterns are scrubbed from logs before emission
- **URL guard** — SSRF protection on all user-supplied URLs
- **Path-traversal guard** — every filesystem operation is sandboxed to the project data directory
- **Security preflight script** — `scripts/security-preflight.sh` runs before production deploy

### LLM & Cost
- **Multi-provider dispatch** (`nexus-providers`) — OpenAI, Anthropic, Ollama as first-class trait impls; model router selects provider by task type and cost
- **Settings UI hot-swap** — change API keys or default model at runtime; registry reloads without restart
- **Per-tenant cost ledger** (`cost_records`) — every LLM call records tokens in + out, cost, model, and tenant; daily/monthly aggregates
- **Budget brake** — preflights every LLM call against the daily/monthly cap; returns HTTP 402 before the call if over budget (zero tokens spent)
- **Anthropic prompt cache** — prefix caching on long system prompts; significant cost reduction on repeat calls
- **LLM response cache** — content-hash cache for identical prompt + model combinations
- **Cost intelligence** — token + cost optimisation recommendations surfaced in the dashboard
- **LLM cost optimizer super-agent** — background agent that continuously rewrites prompts to reduce cost without degrading quality

### Super-Agent Background Optimizers
Ten always-on background agents improve the system while it runs:

| Super-agent | What it does |
|---|---|
| `agent_efficiency` | Profiles agent execution time and prunes wasteful steps |
| `build_runtime` | Tunes parallelism of the coding agent pool |
| `cache_optimizer` | Manages LLM response cache eviction and prefix-cache hit rate |
| `concurrency_optimizer` | Adjusts DB and HTTP connection pool sizes based on queue depth |
| `context_compressor` | Shrinks agent context windows without dropping signal |
| `database_optimizer` | Rewrites slow queries, tunes WAL checkpoints |
| `latency_optimizer` | Identifies and preloads the most-hit cold paths |
| `llm_cost_optimizer` | Rewrites prompts to reduce token spend |
| `pipeline_bottleneck` | Detects DAG nodes that consistently delay the critical path |
| `sse_optimizer` | Tunes SSE back-pressure and client heartbeat intervals |

### Observability
- **Prometheus metrics** — `nexus_llm_cost_dollars_total{provider,model,tenant}`, `nexus_llm_tokens_total{…,direction}`, `nexus_llm_calls_total`, `nexus_llm_errors_total`, `nexus_llm_timeouts_total`, HTTP latency histograms per method
- **OpenTelemetry trace export** — gated by `NEXUS_OTEL_ENABLED=1`; compatible with Jaeger, Tempo, Datadog
- **Agent TV SSE stream** — per-agent status, current file, chain-of-thought tokens, live cost ticker
- **Thinking stream** — token-by-token chain-of-thought from every agent, subscribable independently
- **Live build event bus** — one SSE channel per project, fanned out to every connected client
- **Health checks + graceful shutdown** — liveness/readiness endpoints; in-flight SSE streams drained before exit

### Frontend (`web/`)
Next.js 14+ App Router, Tailwind CSS 4, shadcn/ui. Every view is project-scoped under `/[projectId]/`:

| View | Purpose |
|---|---|
| `build` | Chat interface + live build progress |
| `agent-tv` | 2×5 agent grid with live status and thought streams |
| `agents` | Hire, configure, and inspect individual agents |
| `teams` | Multi-agent business teams and org chart |
| `workflows` | DAG canvas (React Flow) and run history |
| `processes` | Background scheduler and live task queue |
| `business` | Company blueprints and team templates |
| `quality` | Taste scores, invariant results, quality gate history |
| `knowledge` | Per-project knowledge base and retrieval preview |
| `memory` | Episode store, embedding viewer, context assembly |
| `learning` | Causal learning dashboard and pattern library |
| `data` | Generated app database tables viewer |
| `files` | Generated app file tree with diff viewer |
| `observability` | Prometheus graphs, OTel traces, cost ledger |
| `audit` | Signed audit log with chain verification |
| `governance` | Policy rules, kill switch, approval queue |
| `approvals` | HITL approval inbox |
| `federation` | Peer Nexus instances and borrowed agents |
| `integrations` | Stripe, Slack, webhooks, MCP servers |
| `vault` | Encrypted secret store |
| `deploy` | Deployment targets, portal publish, deploy history |
| `portal` | Live generated app iframe + share link |

Global views: `admin`, `marketplace`, `security`, `settings`, `trust`, `bench`.

---

## Workflows

Not every job is a chat. Sometimes you want a **DAG** — branches, loops, parallel fan-out, human approval. Nexus ships a drag-and-drop canvas built on React Flow:

```text
            ┌────────────┐
            │  Research  │  (Kai)
            └─────┬──────┘
                  ▼
         ┌────────────────┐
         │     Write      │  (Luna)
         └────────┬───────┘
                  ▼
        ┌─────────────────┐         ┌────────────────┐
        │  Security Audit │ ◀────── │  Human Approve │
        │    (Orion)      │         └────────────────┘
        └────────┬────────┘
                 ▼
          ┌──────────────┐
          │    Deploy    │  (Rex)
          └──────────────┘
```

Open `/[project]/workflows/compose`. Workflows export to portable JSON — check them into git.

---

## How it works

```
┌────────────────────────────────────────────────────────────────────┐
│  Next.js 15+ UI      Chat · Wizard · Workflow Canvas · Agent TV    │
└──────────────────────────────┬─────────────────────────────────────┘
                               │ SSE + REST
┌──────────────────────────────▼─────────────────────────────────────┐
│  nexus-http (Axum)                                                 │
│   intent → decision → wave orchestrator → taste → guarantee        │
│   budget brake · cost ledger · prometheus · audit log              │
└──────┬─────────────────────────────────────────────┬───────────────┘
       │                                             │
┌──────▼──────────────────┐                  ┌───────▼────────────┐
│  nexus-zeroclaw         │                  │  nexus-workflow    │
│  10-agent roster + pool │                  │  Durable DAG       │
│  Anthropic / OpenAI /   │                  │  parallel · retry  │
│  Ollama tool calling    │                  │  human approval    │
└──────┬──────────────────┘                  └───────┬────────────┘
       │                                             │
┌──────▼─────────────────────────────────────────────▼────────────┐
│  nexus-store    SQLite + WAL · 9 migrations                     │
│  team_events · cost_records · audit_log · vault · projects ...  │
└──────────────────────────────────┬──────────────────────────────┘
                                   │
                ┌──────────────────▼───────────────────┐
                │  nexus-sandbox · WASI + Docker       │
                │  Plugin hooks · generated app exec   │
                └──────────────────────────────────────┘
```

**Two principles:**
1. **Deterministic first, LLM second** — every decision-bearing engine starts with rules, calls the LLM only when heuristics can't classify. Cheaper, faster, explainable.
2. **The event log is the source of truth** — in-memory state is a derivation. Crash → resume from the log.

Full details in [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md). Decision records in [`docs/adr/`](docs/adr/).

---

## CLI

```bash
nexus init my-project           # scaffold a new Nexus project
nexus chat "build a yoga app"   # one-shot generation
nexus company new               # AI company builder (TUI)
nexus agents list               # show hired employees
nexus workflow run <id>         # execute a workflow
nexus mcp serve                 # expose Nexus as an MCP server
nexus plugin install <url>      # install a plugin
nexus plugin validate <file>    # lint a plugin's manifest + WASI module
```

Full reference: [`docs/CLI.md`](docs/CLI.md).

---

## Plugins

Plugins are WASI components — sandboxed by default, capability-deny by default, fuel + memory + wallclock-bounded. ABI defined in [ADR-004](docs/adr/004-plugin-sandbox-abi.md).

```toml
# manifest.toml
schema_version = 1
id = "com.example.taste-extras"
name = "Taste Extras"
version = "1.2.0"
nexus_compat = ">=1.0.0,<2.0.0"

[[capabilities]]
kind = "quality-rule"

[[hooks]]
point = "OnTasteScore"
priority = 50

[permissions]
data = true                              # ~/.nexus/plugins/<id>/data/
net.fetch = ["https://api.example.com"]  # explicit allowlist; * is rejected
```

Browse the marketplace at [`nexus.praesidia.ai/marketplace`](https://nexus.praesidia.ai/marketplace) or `Cmd+K → Marketplace`. Publish your own with `nexus plugin publish`.

---

## Production deployment

Nexus runs on a single box — Mac mini, Hetzner VPS, anything. A reference Dockerfile and `docker-compose.yml` are in the repo. The big production-grade pieces are already built in:

- **SQLite + WAL** survives power loss, handles concurrent writers
- **Per-tenant rate limiter** + **per-tenant budget brake**
- **Tenant-scoped cost ledger** with `nexus_llm_cost_dollars_total` Prometheus counters
- **Boot reconciler** for orphaned team runs after restart
- **Ed25519-signed audit log** for governed actions

`docker compose up` ships a 3-container stack: `nexus`, `postgres-not-needed-yet`, and a Caddy reverse proxy with autotls. See [`deploy/`](deploy/) for Terraform and Fly.io configs.

### Configuration

Most operators only need three env vars in production:

```bash
NEXUS_PRODUCTION=1                            # opt into strict mode
NEXUS_JWT_SECRET=$(openssl rand -hex 32)
NEXUS_ENCRYPTION_KEY=$(openssl rand -hex 32)
```

LLM API keys are entered via the Settings UI (per project memory rule — keys are tenant-scoped). Full env reference in [`.env.example`](.env.example).

---

## Testing

```bash
cargo test --workspace          # all crates, ~12s on a recent laptop
cargo clippy --workspace        # quality gate
cd web && npm run test          # frontend unit (Vitest)
cd web && npx playwright test   # E2E (requires Nexus running)
```

Honest disclosure: we do **not** chase 90 % line coverage. A multi-agent AI
orchestrator has too many integration surfaces (LLM providers, plugins,
browser automation, sandbox guests) where end-to-end coverage is more
valuable than line coverage — and most of *that* coverage requires live
LLM credits we won't burn on every PR. We invest in tests that prove the
hard invariants:

- **Plugin sandbox bounds a hostile loop.** A WASI plugin doing `while(true)`
  is killed in under 3 s; the host stays up. Real Wasmtime test, not a stub.
- **Boot reconciler heals orphaned runs.** A row marked `running` with no
  events for >60 s flips to `paused/server_restart`. Tested end-to-end.
- **Budget brake denies over-cap calls.** With a $0.001 daily cap and a
  $0.005 estimate, the LLM is never invoked. Tested with the real
  envelope path.
- **Tenant attribution doesn't leak.** Tenant A's spend never lands on
  tenant B's aggregate. Tested.
- **Migrations are idempotent.** Run twice on the same DB, schema_version
  ends at 9, no errors. Tested.

If you find a path that isn't tested and ought to be, open an issue —
that's exactly where contributors can add the most value.

---

## Roadmap

### Done in v1.0
- [x] 10-agent ZeroClaw roster (Anthropic / OpenAI / Ollama)
- [x] Chat-to-org-chart AI company builder
- [x] 8 company blueprints
- [x] Visual DAG workflow composer
- [x] Auto-repair build loop
- [x] Plugin marketplace v2
- [x] Per-tenant cost ledger + budget brake
- [x] Durable team-event log + boot reconciler
- [x] WASI plugin sandbox with E2E proof
- [x] MCP server (drives from Claude Desktop)
- [x] Settings-UI hot-swap of provider registry
- [x] Single-binary install via `curl | sh`
- [x] Ed25519 + Merkle-chain audit log
- [x] A2A federation protocol
- [x] Trust certificates + forge reputation
- [x] Super-agent background optimizers (cost, latency, concurrency, cache, DB, SSE, pipeline)

### v1.1 (next)
- [ ] **Component-model plugin imports** — `host::data_read`, `host::fetch`, `host::log`
- [ ] **Org-chart visualization** — reporting lines, not just DAG edges
- [ ] **Agent evals** — A/B two versions of an employee on a task battery
- [ ] **OIDC / SSO** — Google, Microsoft, Okta
- [ ] **Live-API integration tests** behind a feature flag
- [ ] **Mobile companion app** — approve escalations from your phone
- [ ] **Hugging Face Hub agents** — import community-authored team members

### v2.0
- [ ] **Multi-tenant cloud** — managed Nexus at nexus.praesidia.ai
- [ ] **Cross-instance federation** — agents borrow capabilities from peer Nexus instances
- [ ] **Zero-config MCP autodiscovery**

Vote / propose in [GitHub Discussions → Roadmap](https://github.com/praesidia-ai/nexus/discussions/categories/roadmap).

---

## Contributing

We want your help. First-time contributors:

- 📖 [`CONTRIBUTING.md`](CONTRIBUTING.md) — dev setup, testing, commit style
- 🤝 [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) — community standards
- 🏗️ [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — crate map and invariants
- 🧠 [`docs/adr/`](docs/adr/) — why we made the decisions you'll see in the code
- 🌱 Issues labelled **`good first issue`** — guided on-ramps

**No CLA. No paperwork. PRs welcome from anyone.**

If you ship something cool with Nexus, post it in [Discussions → Showcase](https://github.com/praesidia-ai/nexus/discussions/categories/showcase). We boost good builds.

### Local development

```bash
git clone https://github.com/praesidia-ai/nexus.git
cd nexus
cp .env.example .env
cargo build --workspace            # one-time, takes ~3 minutes
cargo run -p nexus-http             # backend
cd web && npm install && npm run dev # frontend in a second shell
```

Run the full pre-commit gate before opening a PR:

```bash
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

---

## Security

Found a vulnerability? Please **don't** open a public issue. Email security@praesidia.ai with details — we triage within 48h. Full policy in [`SECURITY.md`](SECURITY.md).

---

## Built with

[**Rust**](https://www.rust-lang.org) · [**Axum**](https://github.com/tokio-rs/axum) · [**Tokio**](https://tokio.rs) · [**rusqlite**](https://github.com/rusqlite/rusqlite) · [**Wasmtime**](https://wasmtime.dev) · [**Anthropic**](https://www.anthropic.com) · [**OpenAI**](https://openai.com) · [**Ollama**](https://ollama.ai) · [**Next.js**](https://nextjs.org) · [**React**](https://react.dev) · [**Tailwind**](https://tailwindcss.com) · [**React Flow**](https://reactflow.dev) · [**shadcn/ui**](https://ui.shadcn.com)

---

## License

Licensed under either of **[Apache License 2.0](LICENSE-APACHE)** or **[MIT license](LICENSE-MIT)** at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project by you, as defined in the Apache-2.0 license, shall be dual-licensed as above, without any additional terms or conditions.

---

<div align="center">

### Built by people who got tired of one-shot demos.

If Nexus saved you from hiring a team — or just saved you from another browser tab full of Cursor, Claude, ChatGPT, and Linear — please [**star the repo**](https://github.com/praesidia-ai/nexus). It's the simplest way to say thanks and helps more builders find the project.

</div>
