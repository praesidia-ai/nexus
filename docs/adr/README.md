# Architecture Decision Records

Decisions that constrain code under `crates/` and `web/`. One file per decision.

Lifecycle: `proposed` → `accepted` → (`superseded-by:NNNN` | `deprecated`). Once accepted, only a new ADR can override it.

## Index

| ID | Title | Status | Unblocks roadmap |
|---|---|---|---|
| [001](001-first-run-secrets.md) | First-run secrets contract | accepted | #1 boot graceful, #10 SSO |
| [002](002-durable-team-events.md) | Durable team-event schema | accepted | #4 durable team orchestrator |
| [003](003-llm-provider-trait.md) | `LlmProvider` trait surface | accepted | #5 LLM timeout, #9 providers crate, #12 cost tracking |
| [004](004-plugin-sandbox-abi.md) | Plugin sandbox ABI (WASI) | accepted | #7 sandbox plugins |
| [005](005-cost-record-write-path.md) | Cost-record write path | accepted | #12 cost durability + budget |

Roadmap source: `NEXUS_TOP1_RESEARCH.md §2.5`. Audit references: `NEXUS_FEATURE_AUDIT.md §1.6`.

## Authoring rules

- Title is verb-free, noun-first, ≤8 words.
- Body has exactly four headings: **Context**, **Decision**, **Consequences**, **Alternatives considered**.
- One ADR per cross-cutting contract. If two PRs would conflict on the same surface, they need an ADR first.
- Cite file paths with line numbers for any code claim.
- No `_v2` / `_legacy` parallel paths anywhere — replace in place. Spell out what gets deleted.
