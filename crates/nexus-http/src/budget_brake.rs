//! Per-tenant LLM budget preflight brake — ADR-005 §3.
//!
//! Wraps [`nexus_store::cost_records::CostRecordStore`] as an async-friendly
//! check the LLM dispatcher (`llm_client.rs`) calls before issuing any LLM
//! request. Returns `Ok(())` when the next call estimate fits, or a typed
//! [`BudgetBrakeError`] when it would exceed the daily cap.
//!
//! The error maps to HTTP 402 (Payment Required) when surfaced from a
//! handler. `/governance/kill-switch` remains the platform-wide brake; this
//! is the per-tenant brake that fires first.

use std::sync::Arc;

use tokio::sync::Mutex;

use nexus_store::cost_records::CostRecordStore;

/// Failure mode of [`preflight`]. The integers are USD micros.
#[derive(Debug, thiserror::Error)]
pub enum BudgetBrakeError {
    #[error("daily LLM budget exceeded for tenant '{tenant}': spent {spent_micros}µ¢, cap {cap_micros}µ¢, next call estimated {next_micros}µ¢")]
    DailyExceeded {
        tenant: String,
        spent_micros: i64,
        cap_micros: i64,
        next_micros: i64,
    },
    #[error("budget brake unavailable: {0}")]
    Unavailable(String),
}

impl BudgetBrakeError {
    /// HTTP status code mapping.
    pub fn http_status(&self) -> u16 {
        match self {
            BudgetBrakeError::DailyExceeded { .. } => 402,
            BudgetBrakeError::Unavailable(_) => 503,
        }
    }
}

/// Run a preflight budget check.
///
/// `next_call_micros` is the *estimated* cost of the call about to be made
/// (based on input-token count × model price). Estimates are approximate
/// (±5%), which is acceptable for a guardrail.
///
/// `now_ms` is the current unix-ms; passed in so callers in tests can pin
/// time deterministically.
pub async fn preflight(
    db: &Arc<Mutex<rusqlite::Connection>>,
    tenant_id: &str,
    next_call_micros: i64,
    now_ms: i64,
) -> Result<(), BudgetBrakeError> {
    let result = {
        let conn = db.lock().await;
        let store = CostRecordStore::new(&conn);
        let budget = store
            .budget_for(tenant_id)
            .map_err(|e| BudgetBrakeError::Unavailable(format!("budget_for: {e}")))?;
        let agg = store
            .aggregate_today(tenant_id, now_ms)
            .map_err(|e| BudgetBrakeError::Unavailable(format!("aggregate_today: {e}")))?;
        Ok::<_, BudgetBrakeError>((budget, agg))
    }?;
    let (budget, agg) = result;
    let Some(cap) = budget.day_usd_micros else {
        return Ok(());
    };
    if agg.cost_usd_micros.saturating_add(next_call_micros) > cap {
        return Err(BudgetBrakeError::DailyExceeded {
            tenant: tenant_id.to_string(),
            spent_micros: agg.cost_usd_micros,
            cap_micros: cap,
            next_micros: next_call_micros,
        });
    }
    Ok(())
}

/// Convert a USD float estimate to integer micros, saturating at i64::MAX.
/// Negative estimates are clamped to 0 — the caller is buggy but we don't
/// want negative spend to silently overflow.
pub fn dollars_to_micros(usd: f64) -> i64 {
    if !usd.is_finite() || usd <= 0.0 {
        return 0;
    }
    let scaled = (usd * 1_000_000.0).round();
    if scaled >= i64::MAX as f64 {
        i64::MAX
    } else {
        scaled as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_store::cost_records::CostRecord;
    use tempfile::tempdir;

    async fn open_db() -> Arc<Mutex<rusqlite::Connection>> {
        let dir = tempdir().unwrap();
        let conn = nexus_store::open_connection(&dir.path().join("b.db")).unwrap();
        Box::leak(Box::new(dir));
        Arc::new(Mutex::new(conn))
    }

    #[tokio::test]
    async fn no_budget_set_means_no_brake() {
        let db = open_db().await;
        let now = 1_715_000_000_000_i64;
        // Tenant with no budget row — should always pass.
        preflight(&db, "free-rider", 999_999_999, now).await.unwrap();
    }

    #[tokio::test]
    async fn daily_brake_triggers_at_threshold() {
        let db = open_db().await;
        let now = 1_715_000_000_000_i64;

        {
            let conn = db.lock().await;
            let store = CostRecordStore::new(&conn);
            store.set_budget("acme", Some(10_000), None, now).unwrap();
            // Pre-spend 9_500.
            store
                .record(&CostRecord {
                    occurred_at_ms: now,
                    tenant_id: "acme".into(),
                    project_id: None,
                    call_site: "test".into(),
                    provider: "x".into(),
                    model: "y".into(),
                    input_tokens: 0,
                    output_tokens: 0,
                    cached_tokens: 0,
                    cost_usd_micros: 9_500,
                    duration_ms: 0,
                    request_id: None,
                    error_kind: None,
                })
                .unwrap();
        }

        // 600 micros next would push us to 10_100 > 10_000 cap.
        let err = preflight(&db, "acme", 600, now).await.unwrap_err();
        assert_eq!(err.http_status(), 402);
        match err {
            BudgetBrakeError::DailyExceeded { spent_micros, cap_micros, next_micros, .. } => {
                assert_eq!(spent_micros, 9_500);
                assert_eq!(cap_micros, 10_000);
                assert_eq!(next_micros, 600);
            }
            other => panic!("unexpected: {other:?}"),
        }

        // 400 micros stays under.
        preflight(&db, "acme", 400, now).await.unwrap();
    }

    #[test]
    fn dollars_to_micros_handles_edges() {
        assert_eq!(dollars_to_micros(1.0), 1_000_000);
        assert_eq!(dollars_to_micros(0.0), 0);
        assert_eq!(dollars_to_micros(-5.0), 0);
        assert_eq!(dollars_to_micros(f64::NAN), 0);
        assert_eq!(dollars_to_micros(f64::INFINITY), 0);
    }
}
