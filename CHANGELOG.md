# Changelog

All notable changes to Nexus are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Nexus uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

## [1.0.1] — 2026-05-11

## [1.0.0] — 2026-05-11

## [Unreleased]

## [1.0.1] — 2026-05-11

### Added
- Initial public release of the Nexus multi-agent orchestrator
- Axum 0.7 backend with 33 workspace crates and 86+ HTTP handlers
- Ten named coding agents (Nova, Atlas, Kai, Luna, Orion, Sage, Ivy, Rex, Leo, Mia)
- Wave orchestrator for parallel agent execution
- Team orchestrator with human-in-the-loop (HITL) injection
- Oneshot generation pipeline (deterministic intelligence first, LLM fallback)
- Agent TV — live SSE stream of every agent action
- Plugin system with manifest-based install and marketplace browsing
- MCP server registry and client integration
- A2A (Agent-to-Agent) protocol with federation support
- Long-term memory, embeddings, and retrieval via `nexus-memory`
- Code graph analysis via `nexus-graph`
- SQLite persistence with versioned migrations via `nexus-store`
- Ed25519-signed Merkle-chain audit log
- Governance engine with kill-switch and policy hooks
- Trust certificates and forge reputation scoring
- Cost intelligence — per-request token and cost tracking
- Anthropic prompt cache integration
- LLM response cache and multi-provider dispatch (OpenAI / Anthropic / Ollama)
- Taste engine — automated UI quality scoring and auto-redesign
- Outcome guarantee loop — multi-cycle auto-repair
- Invariant enforcer on all generated output
- Sandbox execution for generated apps
- Production traffic simulation and chaos testing
- Next.js 14 App Router frontend with Tailwind CSS 4 + shadcn/ui
- Docker Compose and single-binary self-hosted deployment
- CLI (`cli` crate) for headless operation
- Plugin SDK (`nexus-plugins-sdk`) for third-party extensions
- Client SDK (`nexus-sdk-client`)

### Security
- Fixed P0: audit-log Ed25519 keypair persistence across restarts
- Fixed P0: audit log backed to disk (was in-memory only)
- Fixed P0: `POST /audit/chain` moved to admin-only router
- Fixed P0: `/integrations/notifications/send` SSRF guard and auth scope
- Fixed P0: `POST /a2a` JSON-RPC scoped with tenant attribution and rate limit
- Fixed P0: `/multimodal/*` endpoints scoped with `AuthContext` and cost caps
- Fixed P1: `TenantRateLimiter` wired into `AppState`

---

[Unreleased]: https://github.com/praesidia-ai/nexus/compare/HEAD...HEAD
