# ADR-005 — Cost-record write path

- **Status:** accepted (2026-04-26)
- **Owners:** Rex (observability), Nova (backend)
- **Unblocks roadmap:** #12 cost durability + budget enforcement
- **Closes audit weaknesses:** §1.6 #14 (cost records flushed only on graceful shutdown), #18 (LLM calls bypass `cost_tracker` and `rate_limiter`); §1.3 #12 ("cost tracking is logged but not exposed as a Prometheus metric")
- **Depends on:** ADR-003 (single `LlmClient` enforcement point)

## Context

`crates/nexus-http/src/cost_intelligence.rs:555-579` keeps every `LlmCallRecord` in a `Mutex<Vec<...>>` and flushes the buffer to SQLite **only on graceful shutdown**. An ungraceful crash (SIGKILL, OOM, host reboot, panic past Phase 1) loses the entire window. Self-hosted users see `$0` until shutdown, then a backfill spike that breaks dashboards. Hosted multi-tenant deployments cannot enforce a per-tenant budget at all because the source of truth is on a single process's heap.

Combined with audit §1.6 #18, the actual reality today is even worse: several call sites bypass the tracker entirely, so even the in-memory total is incomplete.

ADR-003 fixes the bypass path by routing every LLM call through `LlmClient`. This ADR locks down what `LlmClient` does on the cost side.

## Decision

### 1. Two-tier write path: synchronous accounting + asynchronous batched persistence

```
LlmClient::complete / stream
        │
        ├──► CostTracker::record_inline(...)         ◀─ synchronous, in-memory
        │       (updates per-tenant counter atomic)
        │
        └──► writer_tx.send(CostRecord { ... })       ◀─ unbounded mpsc, never blocks the request
                  │
                  ▼
            CostWriter task (background tokio task)
                  │
                  ├─ batch up to 200 records or 1s window
                  ├─ single SQLite tx → `cost_records` + `cost_aggregates_today`
                  └─ on shutdown: flush + fsync before returning
```

- **Hot path** (handler thread): one atomic increment + one channel send. Both lock-free; cost is < 1µs.
- **Persistence path**: a single dedicated tokio task owns the SQLite write. Batches by 200-or-1s. On crash, **at most 1s of records** is lost (acceptable; documented in `docs/operations/cost-tracking.md`).
- **Atomicity**: `cost_aggregates_today` is updated in the same tx as `cost_records`. The two are never out of sync on disk.

### 2. Schema (migration `009_cost_records.sql`)

```sql
CREATE TABLE cost_records (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    occurred_at_ms  INTEGER NOT NULL,
    tenant_id       TEXT NOT NULL,
    project_id      TEXT,
    call_site       TEXT NOT NULL,            -- &'static str from CompletionRequest
    provider        TEXT NOT NULL,
    model           TEXT NOT NULL,
    input_tokens    INTEGER NOT NULL DEFAULT 0,
    output_tokens   INTEGER NOT NULL DEFAULT 0,
    cached_tokens   INTEGER NOT NULL DEFAULT 0,
    cost_usd_micros INTEGER NOT NULL,         -- USD * 1e6, integer math
    duration_ms     INTEGER NOT NULL,
    request_id      TEXT,                     -- provider-side id for joining
    error_kind      TEXT                      -- NULL on success
);
CREATE INDEX idx_cost_records_tenant_time ON cost_records(tenant_id, occurred_at_ms);
CREATE INDEX idx_cost_records_project_time ON cost_records(project_id, occurred_at_ms);

-- Bucketed daily totals; truth is `cost_records`, this is an index for budget checks.
CREATE TABLE cost_aggregates_today (
    tenant_id       TEXT NOT NULL,
    day             TEXT NOT NULL,             -- 'YYYY-MM-DD' UTC
    cost_usd_micros INTEGER NOT NULL DEFAULT 0,
    input_tokens    INTEGER NOT NULL DEFAULT 0,
    output_tokens   INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (tenant_id, day)
) WITHOUT ROWID;
```

USD is stored in **integer micros** to avoid float drift across thousands of additions.

### 3. Budget enforcement — preflight, not postmortem

Per-tenant budget lives in `tenant_budgets(tenant_id, day_usd_micros, month_usd_micros)`. `LlmClient::complete` consults `CostTracker::preflight_check(&req)` **before** the LLM call:

```rust
pub async fn preflight_check(&self, req: &CompletionRequest) -> Result<(), BudgetError> {
    let today = self.aggregate_today(&req.tenant_id).await?;
    let budget = self.budget_for(&req.tenant_id).await?;
    let estimate = req.estimated_cost_micros();   // input_tokens × model price
    if today.cost_usd_micros + estimate > budget.day_usd_micros {
        return Err(BudgetError::DailyExceeded { spent: today.cost_usd_micros, cap: budget.day_usd_micros });
    }
    Ok(())
}
```

`BudgetError` maps to `ApiError::BudgetExceeded` (HTTP 402 — *Payment Required*). `/governance/kill-switch` is the platform-wide brake; this is the per-tenant brake that fires first.

### 4. Prometheus exposition

Existing `http_metrics.rs` gets three new counters and one gauge:

```
# HELP nexus_llm_cost_dollars_total Total LLM spend in USD by provider/model/tenant.
# TYPE nexus_llm_cost_dollars_total counter
nexus_llm_cost_dollars_total{provider="anthropic",model="claude-opus-4-7",tenant="acme"} 12.345600

# HELP nexus_llm_tokens_total Total LLM tokens by direction.
# TYPE nexus_llm_tokens_total counter
nexus_llm_tokens_total{provider="...",model="...",tenant="...",direction="input"} 1234567
nexus_llm_tokens_total{provider="...",model="...",tenant="...",direction="output"} 234567
nexus_llm_tokens_total{provider="...",model="...",tenant="...",direction="cached"} 1234

# HELP nexus_llm_call_duration_seconds Histogram of LLM call wallclock duration.
# TYPE nexus_llm_call_duration_seconds histogram

# HELP nexus_tenant_budget_remaining_dollars Day-budget headroom by tenant.
# TYPE nexus_tenant_budget_remaining_dollars gauge
nexus_tenant_budget_remaining_dollars{tenant="acme"} 87.65
```

The hand-rolled Prometheus exposition stays for now — switching to a client lib is out of scope (audit §1.3 #12 noted this; deferred until after #12 ships and contention shows up).

### 5. Observability join keys

Every `cost_records` row carries `call_site` (e.g. `"oneshot.intent_phase"`, `"team_orchestrator.message"`) sourced from `CompletionRequest.call_site` per ADR-003 §2. This joins cleanly to:
- `team_events.payload_json -> '$.trace_id'` (ADR-002) for per-team-run cost roll-ups.
- `agent_traces.run_id` for per-agent cost.
- Existing `audit_log` entries for governance reviews.

### 6. Shutdown / crash discipline

- `CostWriter` listens on a `CancellationToken`. On shutdown it stops accepting new sends, drains the channel, fsync's, then returns.
- `graceful_shutdown.rs` waits for `CostWriter::join()` before exiting the runtime. Default timeout 10s; on timeout we log how many records were lost and exit anyway.
- Crash recovery: on boot, `CostTracker::reconcile_today()` scans `cost_records WHERE occurred_at_ms >= start_of_day_utc()` and refreshes `cost_aggregates_today`. This handles the gap where the writer task crashed independent of the process.

### 7. Retention

- `cost_records`: retained 90 days by default. Tenant-configurable via Settings UI to 30 / 90 / 365 / 7y.
- `cost_aggregates_today`: rolled at UTC midnight into `cost_aggregates_daily(tenant_id, day, …)` table. Daily rollup retained forever (small).

## Consequences

**Positive**
- Self-hosted users see live `$` on `/metrics` and the dashboard.
- Hosted multi-tenant can enforce real budgets in a single SQL query — no scanning of in-memory state.
- One unified write path → bypass call sites become a clippy/CI failure (`grep` rule from ADR-003 §acceptance also catches these).
- Ungraceful crash loses ≤ 1s of cost data; documented and bounded.

**Negative**
- One additional background task; sized cost is trivial (one channel + one tokio task).
- Adds a SQLite write per ~200 LLM calls (or per second). Under heavy load this contends with the single mutex (audit §1.6 #12); deferred to a later ADR (connection pool / wal-mode tuning).
- Estimating cost preflight needs a token-count estimate (`estimated_cost_micros`); the tokenizer is provider-specific and approximate (±5%). Acceptable for a guardrail.

**Neutral**
- The existing `Mutex<Vec<LlmCallRecord>>` in `cost_intelligence.rs:555-579` is **deleted**. No `legacy_cost_buffer` left.

## Alternatives considered

- **Synchronous SQLite write per LLM call.** Rejected: a chatty oneshot can fire 50 LLM calls in one second; the `db.lock().await` would serialise them through one mutex (audit §1.6 #12). Background batch keeps the hot path lock-free.
- **Send to a separate process / sidecar.** Rejected: violates pattern §2.2 #10 (single binary). Maybe later, behind a feature flag, for hosted-cluster deployments.
- **Use an external metrics backend (Prometheus, OTel) as the source of truth.** Rejected: counters are idempotent only via the backend's own dedupe semantics; we need a queryable per-tenant ledger for the budget brake. SQLite is the truth, metrics are the read replica.
- **Estimate cost from response only (postmortem).** Rejected: budget enforcement requires preflight. Preflight estimate ± 5% is fine for a spend cap.
- **Sample a percentage of records.** Rejected: cost data is financial; sampling is not acceptable.

## Acceptance test

1. **Crash test.** Synthetic load fires 100 LLM calls/sec for 60s. `kill -9` the server. On restart, `SELECT count(*) FROM cost_records` is within 100 of expected (1s × 100 calls = 100 lost in worst case).
2. **Bypass detection.** CI grep `cost_tracker.record_inline\|writer_tx.send` MUST appear in `crates/nexus-http/src/llm_client.rs`. CI grep `reqwest::Client.*post.*chat/completions` MUST NOT appear outside `crates/nexus-providers/src/`.
3. **Budget brake.** Set `tenant_budgets.day_usd_micros = 1` for tenant `acme`; next LLM call from `acme` returns `402 BudgetExceeded` in <50 ms (well below the LLM call cost).
4. **Metrics endpoint.** `curl /metrics | grep nexus_llm_cost_dollars_total` shows live counters that increase under load and persist across restart (read from `cost_aggregates_today`).
