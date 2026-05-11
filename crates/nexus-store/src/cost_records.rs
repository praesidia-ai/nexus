//! Durable LLM cost ledger.
//!
//! Per ADR-005. Replaces the in-memory `Mutex<Vec<LlmCallRecord>>` flush at
//! shutdown in `crates/nexus-http/src/cost_intelligence.rs:555-579`. Closes
//! audit weaknesses §1.6 #14 (cost lost on crash) and §1.6 #18 (LLM calls
//! bypass cost tracking).
//!
//! USD is stored in **integer micros** (USD * 1e6) — financial values must
//! not be subject to f64 drift across thousands of additions.

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::Result;

/// One row in `cost_records`.
#[derive(Debug, Clone)]
pub struct CostRecord {
    pub occurred_at_ms: i64,
    pub tenant_id: String,
    pub project_id: Option<String>,
    pub call_site: String,
    pub provider: String,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_tokens: i64,
    pub cost_usd_micros: i64,
    pub duration_ms: i64,
    pub request_id: Option<String>,
    pub error_kind: Option<String>,
}

/// Daily aggregate row.
#[derive(Debug, Clone, Default)]
pub struct DailyAggregate {
    pub tenant_id: String,
    pub day: String,
    pub cost_usd_micros: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

/// Per-tenant budget caps.
#[derive(Debug, Clone, Default)]
pub struct TenantBudget {
    pub tenant_id: String,
    /// `None` = unlimited.
    pub day_usd_micros: Option<i64>,
    pub month_usd_micros: Option<i64>,
}

pub struct CostRecordStore<'a> {
    conn: &'a Connection,
}

impl<'a> CostRecordStore<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Insert a single cost record AND increment `cost_aggregates_today`
    /// in one transaction so the dashboard read never sees a partial write.
    pub fn record(&self, r: &CostRecord) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO cost_records \
             (occurred_at_ms, tenant_id, project_id, call_site, provider, model, \
              input_tokens, output_tokens, cached_tokens, cost_usd_micros, duration_ms, \
              request_id, error_kind) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                r.occurred_at_ms,
                r.tenant_id,
                r.project_id,
                r.call_site,
                r.provider,
                r.model,
                r.input_tokens,
                r.output_tokens,
                r.cached_tokens,
                r.cost_usd_micros,
                r.duration_ms,
                r.request_id,
                r.error_kind,
            ],
        )?;

        let day = day_string_utc(r.occurred_at_ms);
        tx.execute(
            "INSERT INTO cost_aggregates_today \
                (tenant_id, day, cost_usd_micros, input_tokens, output_tokens, last_updated_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT(tenant_id, day) DO UPDATE SET \
                cost_usd_micros = cost_usd_micros + excluded.cost_usd_micros, \
                input_tokens    = input_tokens    + excluded.input_tokens, \
                output_tokens   = output_tokens   + excluded.output_tokens, \
                last_updated_ms = excluded.last_updated_ms",
            params![
                r.tenant_id,
                day,
                r.cost_usd_micros,
                r.input_tokens,
                r.output_tokens,
                r.occurred_at_ms,
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Batch-insert N records in one transaction. Aggregates updated per row.
    pub fn record_batch(&self, batch: &[CostRecord]) -> Result<()> {
        if batch.is_empty() {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        for r in batch {
            tx.execute(
                "INSERT INTO cost_records \
                 (occurred_at_ms, tenant_id, project_id, call_site, provider, model, \
                  input_tokens, output_tokens, cached_tokens, cost_usd_micros, duration_ms, \
                  request_id, error_kind) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    r.occurred_at_ms,
                    r.tenant_id,
                    r.project_id,
                    r.call_site,
                    r.provider,
                    r.model,
                    r.input_tokens,
                    r.output_tokens,
                    r.cached_tokens,
                    r.cost_usd_micros,
                    r.duration_ms,
                    r.request_id,
                    r.error_kind,
                ],
            )?;
            let day = day_string_utc(r.occurred_at_ms);
            tx.execute(
                "INSERT INTO cost_aggregates_today \
                    (tenant_id, day, cost_usd_micros, input_tokens, output_tokens, last_updated_ms) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
                 ON CONFLICT(tenant_id, day) DO UPDATE SET \
                    cost_usd_micros = cost_usd_micros + excluded.cost_usd_micros, \
                    input_tokens    = input_tokens    + excluded.input_tokens, \
                    output_tokens   = output_tokens   + excluded.output_tokens, \
                    last_updated_ms = excluded.last_updated_ms",
                params![
                    r.tenant_id,
                    day,
                    r.cost_usd_micros,
                    r.input_tokens,
                    r.output_tokens,
                    r.occurred_at_ms,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Today's spend for `tenant_id` as integer micros. 0 if nothing billed.
    pub fn aggregate_today(&self, tenant_id: &str, now_ms: i64) -> Result<DailyAggregate> {
        let day = day_string_utc(now_ms);
        let row = self
            .conn
            .query_row(
                "SELECT cost_usd_micros, input_tokens, output_tokens \
                 FROM cost_aggregates_today \
                 WHERE tenant_id = ?1 AND day = ?2",
                params![tenant_id, day],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?;
        Ok(match row {
            Some((c, i, o)) => DailyAggregate {
                tenant_id: tenant_id.into(),
                day,
                cost_usd_micros: c,
                input_tokens: i,
                output_tokens: o,
            },
            None => DailyAggregate {
                tenant_id: tenant_id.into(),
                day,
                ..Default::default()
            },
        })
    }

    /// Read budget, or default-unlimited if absent.
    pub fn budget_for(&self, tenant_id: &str) -> Result<TenantBudget> {
        let row = self
            .conn
            .query_row(
                "SELECT day_usd_micros, month_usd_micros \
                 FROM tenant_budgets WHERE tenant_id = ?1",
                params![tenant_id],
                |r| Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, Option<i64>>(1)?)),
            )
            .optional()?;
        Ok(match row {
            Some((d, m)) => TenantBudget {
                tenant_id: tenant_id.into(),
                day_usd_micros: d,
                month_usd_micros: m,
            },
            None => TenantBudget {
                tenant_id: tenant_id.into(),
                ..Default::default()
            },
        })
    }

    pub fn set_budget(
        &self,
        tenant_id: &str,
        day_usd_micros: Option<i64>,
        month_usd_micros: Option<i64>,
        now_ms: i64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO tenant_budgets (tenant_id, day_usd_micros, month_usd_micros, updated_at_ms) \
             VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(tenant_id) DO UPDATE SET \
                day_usd_micros = excluded.day_usd_micros, \
                month_usd_micros = excluded.month_usd_micros, \
                updated_at_ms = excluded.updated_at_ms",
            params![tenant_id, day_usd_micros, month_usd_micros, now_ms],
        )?;
        Ok(())
    }

    /// Recompute today's aggregate from `cost_records`. Used by the boot-time
    /// reconcile path described in ADR-005 §6.
    pub fn reconcile_today(&self, tenant_id: &str, now_ms: i64) -> Result<DailyAggregate> {
        let day = day_string_utc(now_ms);
        let day_start_ms = day_start_ms_utc(now_ms);
        let day_end_ms = day_start_ms + 86_400_000;
        let row = self.conn.query_row(
            "SELECT \
                COALESCE(SUM(cost_usd_micros), 0), \
                COALESCE(SUM(input_tokens), 0), \
                COALESCE(SUM(output_tokens), 0) \
             FROM cost_records \
             WHERE tenant_id = ?1 AND occurred_at_ms >= ?2 AND occurred_at_ms < ?3",
            params![tenant_id, day_start_ms, day_end_ms],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)),
        )?;
        self.conn.execute(
            "INSERT INTO cost_aggregates_today \
                (tenant_id, day, cost_usd_micros, input_tokens, output_tokens, last_updated_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT(tenant_id, day) DO UPDATE SET \
                cost_usd_micros = excluded.cost_usd_micros, \
                input_tokens    = excluded.input_tokens, \
                output_tokens   = excluded.output_tokens, \
                last_updated_ms = excluded.last_updated_ms",
            params![tenant_id, day, row.0, row.1, row.2, now_ms],
        )?;
        Ok(DailyAggregate {
            tenant_id: tenant_id.into(),
            day,
            cost_usd_micros: row.0,
            input_tokens: row.1,
            output_tokens: row.2,
        })
    }

    /// Convenience: would the next call exceed the daily budget?
    pub fn would_exceed_daily(
        &self,
        tenant_id: &str,
        next_call_micros: i64,
        now_ms: i64,
    ) -> Result<bool> {
        let budget = self.budget_for(tenant_id)?;
        let Some(cap) = budget.day_usd_micros else {
            return Ok(false);
        };
        let today = self.aggregate_today(tenant_id, now_ms)?;
        Ok(today.cost_usd_micros.saturating_add(next_call_micros) > cap)
    }
}

/// Convert a unix-ms timestamp into `'YYYY-MM-DD'` UTC.
fn day_string_utc(ms: i64) -> String {
    let secs = ms / 1000;
    // Naive day breakdown — chrono is a workspace dep but pulling it here would
    // bloat a leaf module. SQLite's strftime would also work but requires an
    // open conn round-trip; this is hot-path code, so we compute it inline.
    let days_since_epoch = secs.div_euclid(86_400);
    let (y, m, d) = days_to_ymd(days_since_epoch);
    format!("{y:04}-{m:02}-{d:02}")
}

fn day_start_ms_utc(ms: i64) -> i64 {
    let day = ms.div_euclid(86_400_000);
    day * 86_400_000
}

/// Howard Hinnant date algorithm — convert "days from 1970-01-01" to (Y, M, D).
fn days_to_ymd(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_connection;
    use tempfile::tempdir;

    #[test]
    fn record_increments_today() {
        let dir = tempdir().unwrap();
        let conn = open_connection(&dir.path().join("c.db")).unwrap();
        let svc = CostRecordStore::new(&conn);
        let now = 1_715_000_000_000_i64; // 2024-05-06 ish
        svc.record(&CostRecord {
            occurred_at_ms: now,
            tenant_id: "acme".into(),
            project_id: Some("p1".into()),
            call_site: "oneshot.intent".into(),
            provider: "anthropic".into(),
            model: "claude-opus-4-7".into(),
            input_tokens: 1000,
            output_tokens: 500,
            cached_tokens: 0,
            cost_usd_micros: 12_345,
            duration_ms: 800,
            request_id: None,
            error_kind: None,
        })
        .unwrap();
        let agg = svc.aggregate_today("acme", now).unwrap();
        assert_eq!(agg.cost_usd_micros, 12_345);
        assert_eq!(agg.input_tokens, 1000);
        assert_eq!(agg.output_tokens, 500);
    }

    #[test]
    fn budget_brake_triggers() {
        let dir = tempdir().unwrap();
        let conn = open_connection(&dir.path().join("b.db")).unwrap();
        let svc = CostRecordStore::new(&conn);
        let now = 1_715_000_000_000_i64;
        svc.set_budget("acme", Some(10_000), None, now).unwrap();
        // Spend 9_000 already.
        svc.record(&CostRecord {
            occurred_at_ms: now,
            tenant_id: "acme".into(),
            project_id: None,
            call_site: "test".into(),
            provider: "x".into(),
            model: "y".into(),
            input_tokens: 0,
            output_tokens: 0,
            cached_tokens: 0,
            cost_usd_micros: 9_000,
            duration_ms: 0,
            request_id: None,
            error_kind: None,
        })
        .unwrap();
        // Next call of 2_000 micros exceeds the 10_000 cap.
        assert!(svc.would_exceed_daily("acme", 2_000, now).unwrap());
        // 500 micros stays under.
        assert!(!svc.would_exceed_daily("acme", 500, now).unwrap());
    }

    #[test]
    fn reconcile_today_recovers_aggregate() {
        let dir = tempdir().unwrap();
        let conn = open_connection(&dir.path().join("r.db")).unwrap();
        let svc = CostRecordStore::new(&conn);
        let now = 1_715_000_000_000_i64;
        // Insert a raw record but corrupt the aggregate manually.
        svc.record(&CostRecord {
            occurred_at_ms: now,
            tenant_id: "t1".into(),
            project_id: None,
            call_site: "s".into(),
            provider: "p".into(),
            model: "m".into(),
            input_tokens: 100,
            output_tokens: 200,
            cached_tokens: 0,
            cost_usd_micros: 5_000,
            duration_ms: 0,
            request_id: None,
            error_kind: None,
        })
        .unwrap();
        conn.execute(
            "UPDATE cost_aggregates_today SET cost_usd_micros = 0",
            [],
        )
        .unwrap();
        let agg = svc.reconcile_today("t1", now).unwrap();
        assert_eq!(agg.cost_usd_micros, 5_000);
        assert_eq!(agg.input_tokens, 100);
        assert_eq!(agg.output_tokens, 200);
    }
}
