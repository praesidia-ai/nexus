//! Lightweight HTTP-level metrics — request count, latency histogram, status
//! code breakdown.
//!
//! We don't pull in a full prometheus client (the existing `/metrics` endpoint
//! hand-rolls its exposition), so this module tracks counters via `AtomicU64`
//! and emits histogram buckets as a single string when scraped.
//!
//! Middleware: `track_request` wraps every request; it records:
//! - `nexus_http_requests_total{method,status_class}` counter
//! - `nexus_http_request_duration_seconds_bucket{method,le}` histogram

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use axum::{extract::Request, middleware::Next, response::Response};

// ---------------------------------------------------------------------------
// Counters
// ---------------------------------------------------------------------------

/// Histogram bucket upper bounds in seconds. Matches common Prometheus ranges.
pub const HIST_BUCKETS_SECS: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0,
];

macro_rules! counter {
    ($name:ident) => {
        pub static $name: AtomicU64 = AtomicU64::new(0);
    };
}

// Request counters by method + status class (2xx, 3xx, 4xx, 5xx, err).
counter!(REQ_GET_2XX);
counter!(REQ_GET_4XX);
counter!(REQ_GET_5XX);
counter!(REQ_POST_2XX);
counter!(REQ_POST_4XX);
counter!(REQ_POST_5XX);
counter!(REQ_PUT_2XX);
counter!(REQ_PUT_4XX);
counter!(REQ_PUT_5XX);
counter!(REQ_DELETE_2XX);
counter!(REQ_DELETE_4XX);
counter!(REQ_DELETE_5XX);
counter!(REQ_OTHER);

// Latency histogram — one array per method.
pub struct LatencyHistogram {
    pub buckets: [AtomicU64; 12], // matches HIST_BUCKETS_SECS
    pub inf: AtomicU64,
    pub sum_ms: AtomicU64,
    pub count: AtomicU64,
}

impl LatencyHistogram {
    const fn new() -> Self {
        Self {
            buckets: [
                AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),
                AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),
                AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),
            ],
            inf: AtomicU64::new(0),
            sum_ms: AtomicU64::new(0),
            count: AtomicU64::new(0),
        }
    }

    fn observe_secs(&self, secs: f64) {
        for (i, &bucket) in HIST_BUCKETS_SECS.iter().enumerate() {
            if secs <= bucket {
                self.buckets[i].fetch_add(1, Ordering::Relaxed);
            }
        }
        self.inf.fetch_add(1, Ordering::Relaxed);
        self.sum_ms.fetch_add((secs * 1000.0) as u64, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
    }
}

pub static LATENCY_GET: LatencyHistogram = LatencyHistogram::new();
pub static LATENCY_POST: LatencyHistogram = LatencyHistogram::new();
pub static LATENCY_OTHER: LatencyHistogram = LatencyHistogram::new();

// ---------------------------------------------------------------------------
// LLM cost / token metrics (ADR-005 §4)
// ---------------------------------------------------------------------------

/// Aggregated label key for LLM metrics. Bounded cardinality is the operator's
/// responsibility — `provider` and `model` are low-cardinality, `tenant` is
/// the only field that grows with customers, and that growth is desired.
#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct LlmLabels {
    pub provider: String,
    pub model: String,
    pub tenant: String,
}

#[derive(Default, Debug, Clone, Copy)]
pub struct LlmAggregate {
    /// Cumulative cost in USD micros (1e-6 USD).
    pub cost_usd_micros: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: u64,
    pub calls: u64,
    pub errors: u64,
    pub timeouts: u64,
}

/// In-memory cost ledger surfaced via `/metrics`. Distinct from the durable
/// SQLite ledger in `nexus-store::cost_records` — this is a hot, lock-free-ish
/// cache that Prometheus scrapes; SQLite is the source of truth.
pub static LLM_METRICS: std::sync::Mutex<Option<std::collections::HashMap<LlmLabels, LlmAggregate>>> =
    std::sync::Mutex::new(None);

fn with_llm<F, R>(f: F) -> R
where
    F: FnOnce(&mut std::collections::HashMap<LlmLabels, LlmAggregate>) -> R,
{
    let mut guard = LLM_METRICS.lock().unwrap_or_else(|e| {
        // Poisoned mutex — clear poisoning by overwriting with empty state.
        // This should never happen; the closures we run are infallible.
        let mut g = e.into_inner();
        *g = Some(std::collections::HashMap::new());
        g
    });
    if guard.is_none() {
        *guard = Some(std::collections::HashMap::new());
    }
    f(guard.as_mut().expect("just initialised"))
}

/// Record one successful LLM call. Idempotent on labels.
pub fn record_llm_call(
    provider: &str,
    model: &str,
    tenant: &str,
    cost_usd_micros: u64,
    input_tokens: u64,
    output_tokens: u64,
    cached_tokens: u64,
) {
    let labels = LlmLabels {
        provider: provider.to_string(),
        model: model.to_string(),
        tenant: tenant.to_string(),
    };
    with_llm(|map| {
        let entry = map.entry(labels).or_default();
        entry.cost_usd_micros = entry.cost_usd_micros.saturating_add(cost_usd_micros);
        entry.input_tokens = entry.input_tokens.saturating_add(input_tokens);
        entry.output_tokens = entry.output_tokens.saturating_add(output_tokens);
        entry.cached_tokens = entry.cached_tokens.saturating_add(cached_tokens);
        entry.calls = entry.calls.saturating_add(1);
    });
}

/// Record an LLM call that failed before producing tokens. Tracked under the
/// same labels so error-rate dashboards Just Work.
pub fn record_llm_error(provider: &str, model: &str, tenant: &str, kind: LlmErrorKind) {
    let labels = LlmLabels {
        provider: provider.to_string(),
        model: model.to_string(),
        tenant: tenant.to_string(),
    };
    with_llm(|map| {
        let entry = map.entry(labels).or_default();
        entry.errors = entry.errors.saturating_add(1);
        if matches!(kind, LlmErrorKind::Timeout) {
            entry.timeouts = entry.timeouts.saturating_add(1);
        }
    });
}

#[derive(Debug, Clone, Copy)]
pub enum LlmErrorKind {
    Timeout,
    Other,
}

/// Snapshot the current ledger for tests / introspection.
pub fn llm_snapshot() -> std::collections::HashMap<LlmLabels, LlmAggregate> {
    with_llm(|map| map.clone())
}

// ---------------------------------------------------------------------------
// Middleware
// ---------------------------------------------------------------------------

fn classify_status(status: u16) -> &'static str {
    match status / 100 {
        2 => "2xx",
        3 => "3xx",
        4 => "4xx",
        5 => "5xx",
        _ => "err",
    }
}

pub async fn track_request(req: Request, next: Next) -> Response {
    let method = req.method().as_str().to_string();
    let start = Instant::now();
    let response = next.run(req).await;
    let status = response.status().as_u16();
    let elapsed_secs = start.elapsed().as_secs_f64();
    let class = classify_status(status);

    // Record counters
    match (method.as_str(), class) {
        ("GET", "2xx") => { REQ_GET_2XX.fetch_add(1, Ordering::Relaxed); }
        ("GET", "4xx") => { REQ_GET_4XX.fetch_add(1, Ordering::Relaxed); }
        ("GET", "5xx") => { REQ_GET_5XX.fetch_add(1, Ordering::Relaxed); }
        ("POST", "2xx") => { REQ_POST_2XX.fetch_add(1, Ordering::Relaxed); }
        ("POST", "4xx") => { REQ_POST_4XX.fetch_add(1, Ordering::Relaxed); }
        ("POST", "5xx") => { REQ_POST_5XX.fetch_add(1, Ordering::Relaxed); }
        ("PUT", "2xx") => { REQ_PUT_2XX.fetch_add(1, Ordering::Relaxed); }
        ("PUT", "4xx") => { REQ_PUT_4XX.fetch_add(1, Ordering::Relaxed); }
        ("PUT", "5xx") => { REQ_PUT_5XX.fetch_add(1, Ordering::Relaxed); }
        ("DELETE", "2xx") => { REQ_DELETE_2XX.fetch_add(1, Ordering::Relaxed); }
        ("DELETE", "4xx") => { REQ_DELETE_4XX.fetch_add(1, Ordering::Relaxed); }
        ("DELETE", "5xx") => { REQ_DELETE_5XX.fetch_add(1, Ordering::Relaxed); }
        _ => { REQ_OTHER.fetch_add(1, Ordering::Relaxed); }
    }

    // Record latency
    match method.as_str() {
        "GET" => LATENCY_GET.observe_secs(elapsed_secs),
        "POST" => LATENCY_POST.observe_secs(elapsed_secs),
        _ => LATENCY_OTHER.observe_secs(elapsed_secs),
    }

    response
}

// ---------------------------------------------------------------------------
// Prometheus exposition
// ---------------------------------------------------------------------------

pub fn render_prometheus() -> String {
    let mut out = String::new();
    out.push_str("# HELP nexus_http_requests_total Total HTTP requests by method and status class.\n");
    out.push_str("# TYPE nexus_http_requests_total counter\n");
    let rows: [(&str, &str, &AtomicU64); 12] = [
        ("GET", "2xx", &REQ_GET_2XX),
        ("GET", "4xx", &REQ_GET_4XX),
        ("GET", "5xx", &REQ_GET_5XX),
        ("POST", "2xx", &REQ_POST_2XX),
        ("POST", "4xx", &REQ_POST_4XX),
        ("POST", "5xx", &REQ_POST_5XX),
        ("PUT", "2xx", &REQ_PUT_2XX),
        ("PUT", "4xx", &REQ_PUT_4XX),
        ("PUT", "5xx", &REQ_PUT_5XX),
        ("DELETE", "2xx", &REQ_DELETE_2XX),
        ("DELETE", "4xx", &REQ_DELETE_4XX),
        ("DELETE", "5xx", &REQ_DELETE_5XX),
    ];
    for (method, class, counter) in rows {
        out.push_str(&format!(
            "nexus_http_requests_total{{method=\"{method}\",status=\"{class}\"}} {}\n",
            counter.load(Ordering::Relaxed)
        ));
    }

    out.push_str("\n# HELP nexus_http_request_duration_seconds HTTP request duration.\n");
    out.push_str("# TYPE nexus_http_request_duration_seconds histogram\n");
    for (method, hist) in [
        ("GET", &LATENCY_GET),
        ("POST", &LATENCY_POST),
        ("OTHER", &LATENCY_OTHER),
    ] {
        for (i, &bound) in HIST_BUCKETS_SECS.iter().enumerate() {
            out.push_str(&format!(
                "nexus_http_request_duration_seconds_bucket{{method=\"{method}\",le=\"{bound}\"}} {}\n",
                hist.buckets[i].load(Ordering::Relaxed)
            ));
        }
        out.push_str(&format!(
            "nexus_http_request_duration_seconds_bucket{{method=\"{method}\",le=\"+Inf\"}} {}\n",
            hist.inf.load(Ordering::Relaxed)
        ));
        out.push_str(&format!(
            "nexus_http_request_duration_seconds_sum{{method=\"{method}\"}} {:.3}\n",
            hist.sum_ms.load(Ordering::Relaxed) as f64 / 1000.0
        ));
        out.push_str(&format!(
            "nexus_http_request_duration_seconds_count{{method=\"{method}\"}} {}\n",
            hist.count.load(Ordering::Relaxed)
        ));
    }

    // -----------------------------------------------------------------
    // LLM cost / tokens (ADR-005 §4).
    // -----------------------------------------------------------------
    let snap = llm_snapshot();
    if !snap.is_empty() {
        out.push_str(
            "\n# HELP nexus_llm_cost_dollars_total Total LLM spend in USD by provider/model/tenant.\n",
        );
        out.push_str("# TYPE nexus_llm_cost_dollars_total counter\n");
        for (k, v) in &snap {
            out.push_str(&format!(
                "nexus_llm_cost_dollars_total{{provider=\"{p}\",model=\"{m}\",tenant=\"{t}\"}} {dollars:.6}\n",
                p = escape_label(&k.provider),
                m = escape_label(&k.model),
                t = escape_label(&k.tenant),
                dollars = v.cost_usd_micros as f64 / 1_000_000.0,
            ));
        }

        out.push_str(
            "\n# HELP nexus_llm_tokens_total Total LLM tokens by provider/model/tenant/direction.\n",
        );
        out.push_str("# TYPE nexus_llm_tokens_total counter\n");
        for (k, v) in &snap {
            for (dir, val) in [
                ("input", v.input_tokens),
                ("output", v.output_tokens),
                ("cached", v.cached_tokens),
            ] {
                out.push_str(&format!(
                    "nexus_llm_tokens_total{{provider=\"{p}\",model=\"{m}\",tenant=\"{t}\",direction=\"{dir}\"}} {val}\n",
                    p = escape_label(&k.provider),
                    m = escape_label(&k.model),
                    t = escape_label(&k.tenant),
                ));
            }
        }

        out.push_str("\n# HELP nexus_llm_calls_total Number of LLM calls.\n");
        out.push_str("# TYPE nexus_llm_calls_total counter\n");
        for (k, v) in &snap {
            out.push_str(&format!(
                "nexus_llm_calls_total{{provider=\"{p}\",model=\"{m}\",tenant=\"{t}\"}} {}\n",
                v.calls,
                p = escape_label(&k.provider),
                m = escape_label(&k.model),
                t = escape_label(&k.tenant),
            ));
        }

        out.push_str("\n# HELP nexus_llm_errors_total LLM call errors by provider/model/tenant.\n");
        out.push_str("# TYPE nexus_llm_errors_total counter\n");
        for (k, v) in &snap {
            out.push_str(&format!(
                "nexus_llm_errors_total{{provider=\"{p}\",model=\"{m}\",tenant=\"{t}\"}} {}\n",
                v.errors,
                p = escape_label(&k.provider),
                m = escape_label(&k.model),
                t = escape_label(&k.tenant),
            ));
        }

        out.push_str("\n# HELP nexus_llm_timeouts_total LLM call timeouts by provider/model/tenant.\n");
        out.push_str("# TYPE nexus_llm_timeouts_total counter\n");
        for (k, v) in &snap {
            out.push_str(&format!(
                "nexus_llm_timeouts_total{{provider=\"{p}\",model=\"{m}\",tenant=\"{t}\"}} {}\n",
                v.timeouts,
                p = escape_label(&k.provider),
                m = escape_label(&k.model),
                t = escape_label(&k.tenant),
            ));
        }
    }

    out
}

/// Minimal Prometheus label-value escaper — backslash, double-quote, newline.
fn escape_label(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str(r"\\"),
            '"' => out.push_str(r#"\""#),
            '\n' => out.push_str(r"\n"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_classification() {
        assert_eq!(classify_status(200), "2xx");
        assert_eq!(classify_status(302), "3xx");
        assert_eq!(classify_status(404), "4xx");
        assert_eq!(classify_status(500), "5xx");
        assert_eq!(classify_status(0), "err");
    }

    #[test]
    fn histogram_observes_to_correct_buckets() {
        let h = LatencyHistogram::new();
        h.observe_secs(0.015);
        // 0.015s lands in the 0.025 bucket and everything larger.
        assert_eq!(h.buckets[0].load(Ordering::Relaxed), 0, "0.005s bucket");
        assert_eq!(h.buckets[1].load(Ordering::Relaxed), 0, "0.01s bucket");
        assert_eq!(h.buckets[2].load(Ordering::Relaxed), 1, "0.025s bucket");
        assert_eq!(h.buckets[3].load(Ordering::Relaxed), 1, "0.05s bucket");
        assert_eq!(h.count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn prometheus_render_is_well_formed() {
        let output = render_prometheus();
        assert!(output.contains("nexus_http_requests_total"));
        assert!(output.contains("nexus_http_request_duration_seconds_bucket"));
        assert!(output.contains("le=\"+Inf\""));
    }
}
