-- Migration 009: durable LLM cost records + per-tenant aggregates.
--
-- Per ADR-005. Replaces the ungraceful `Mutex<Vec<LlmCallRecord>>` flush at
-- shutdown in `crates/nexus-http/src/cost_intelligence.rs:555-579` with a
-- durable event ledger. Closes audit weaknesses §1.6 #14 (cost lost on crash)
-- and §1.6 #18 (LLM calls bypass `cost_tracker` and `rate_limiter`) — the
-- latter is enforced by ADR-003's single `LlmClient` entry point.

-- Per-call ledger. USD stored in integer micros to avoid float drift across
-- thousands of additions (1e-6 USD precision).
CREATE TABLE IF NOT EXISTS cost_records (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    occurred_at_ms  INTEGER NOT NULL,
    tenant_id       TEXT NOT NULL,
    project_id      TEXT,
    call_site       TEXT NOT NULL,              -- e.g. "oneshot.intent_phase"
    provider        TEXT NOT NULL,
    model           TEXT NOT NULL,
    input_tokens    INTEGER NOT NULL DEFAULT 0,
    output_tokens   INTEGER NOT NULL DEFAULT 0,
    cached_tokens   INTEGER NOT NULL DEFAULT 0,
    cost_usd_micros INTEGER NOT NULL,           -- USD * 1e6
    duration_ms     INTEGER NOT NULL,
    request_id      TEXT,
    error_kind      TEXT
);

CREATE INDEX IF NOT EXISTS idx_cost_records_tenant_time
    ON cost_records(tenant_id, occurred_at_ms);
CREATE INDEX IF NOT EXISTS idx_cost_records_project_time
    ON cost_records(project_id, occurred_at_ms);
CREATE INDEX IF NOT EXISTS idx_cost_records_call_site
    ON cost_records(call_site, occurred_at_ms);

-- Bucketed daily totals. Truth is `cost_records`; this is an index for the
-- preflight budget brake that must answer in <50ms.
CREATE TABLE IF NOT EXISTS cost_aggregates_today (
    tenant_id       TEXT NOT NULL,
    day             TEXT NOT NULL,              -- 'YYYY-MM-DD' UTC
    cost_usd_micros INTEGER NOT NULL DEFAULT 0,
    input_tokens    INTEGER NOT NULL DEFAULT 0,
    output_tokens   INTEGER NOT NULL DEFAULT 0,
    last_updated_ms INTEGER NOT NULL,
    PRIMARY KEY (tenant_id, day)
) WITHOUT ROWID;

-- Frozen daily rollups, retained forever (small).
CREATE TABLE IF NOT EXISTS cost_aggregates_daily (
    tenant_id       TEXT NOT NULL,
    day             TEXT NOT NULL,              -- 'YYYY-MM-DD' UTC
    cost_usd_micros INTEGER NOT NULL DEFAULT 0,
    input_tokens    INTEGER NOT NULL DEFAULT 0,
    output_tokens   INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (tenant_id, day)
) WITHOUT ROWID;

-- Per-tenant budgets enforced by `LlmClient::preflight_check`.
CREATE TABLE IF NOT EXISTS tenant_budgets (
    tenant_id           TEXT PRIMARY KEY,
    day_usd_micros      INTEGER,                -- NULL = unlimited
    month_usd_micros    INTEGER,                -- NULL = unlimited
    updated_at_ms       INTEGER NOT NULL
);
