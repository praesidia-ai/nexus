//! Shared metrics collection bus — the central nervous system of Super Agents.
//!
//! All subsystems push metrics here; all agents read from here.
//! Metrics are stored in lock-free ring buffers with configurable retention.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::debug;

use super::types::SystemSnapshot;

// ---------------------------------------------------------------------------
// Metric Sample
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSample {
    pub name: String,
    pub value: f64,
    pub tags: HashMap<String, String>,
    pub timestamp_ms: u64,
}

// ---------------------------------------------------------------------------
// Ring Buffer — fixed-size, lock-free-ish for reads
// ---------------------------------------------------------------------------

struct RingBuffer {
    samples: Vec<MetricSample>,
    capacity: usize,
    write_pos: usize,
    count: usize,
}

impl RingBuffer {
    fn new(capacity: usize) -> Self {
        Self {
            samples: Vec::with_capacity(capacity),
            capacity,
            write_pos: 0,
            count: 0,
        }
    }

    fn push(&mut self, sample: MetricSample) {
        if self.samples.len() < self.capacity {
            self.samples.push(sample);
        } else {
            self.samples[self.write_pos] = sample;
        }
        self.write_pos = (self.write_pos + 1) % self.capacity;
        self.count += 1;
    }

    fn recent(&self, n: usize) -> Vec<&MetricSample> {
        let len = self.samples.len();
        if len == 0 {
            return Vec::new();
        }
        let take = n.min(len);
        let start = if len < self.capacity {
            len.saturating_sub(take)
        } else {
            (self.write_pos + self.capacity - take) % self.capacity
        };

        let mut out = Vec::with_capacity(take);
        for i in 0..take {
            let idx = (start + i) % len;
            out.push(&self.samples[idx]);
        }
        out
    }

    fn avg_last_n(&self, n: usize) -> f64 {
        let recent = self.recent(n);
        if recent.is_empty() {
            return 0.0;
        }
        let sum: f64 = recent.iter().map(|s| s.value).sum();
        sum / recent.len() as f64
    }

    fn p95_last_n(&self, n: usize) -> f64 {
        let recent = self.recent(n);
        if recent.is_empty() {
            return 0.0;
        }
        let mut vals: Vec<f64> = recent.iter().map(|s| s.value).collect();
        vals.sort_by(|a, b| a.total_cmp(b));
        let idx = ((vals.len() as f64) * 0.95).ceil() as usize;
        vals[idx.min(vals.len() - 1)]
    }
}

// ---------------------------------------------------------------------------
// Metrics Bus
// ---------------------------------------------------------------------------

pub struct MetricsBus {
    buffers: RwLock<HashMap<String, RingBuffer>>,
    boot_time: Instant,
    total_samples: AtomicU64,
    buffer_capacity: usize,
}

impl MetricsBus {
    pub fn new(buffer_capacity: usize) -> Arc<Self> {
        Arc::new(Self {
            buffers: RwLock::new(HashMap::new()),
            boot_time: Instant::now(),
            total_samples: AtomicU64::new(0),
            buffer_capacity,
        })
    }

    /// Record a metric sample.
    pub async fn record(&self, name: &str, value: f64, tags: HashMap<String, String>) {
        let sample = MetricSample {
            name: name.to_string(),
            value,
            tags,
            timestamp_ms: self.boot_time.elapsed().as_millis() as u64,
        };

        let mut buffers = self.buffers.write().await;
        let buf = buffers
            .entry(name.to_string())
            .or_insert_with(|| RingBuffer::new(self.buffer_capacity));
        buf.push(sample);
        self.total_samples.fetch_add(1, Ordering::Relaxed);
    }

    /// Record with no tags (convenience).
    pub async fn record_value(&self, name: &str, value: f64) {
        self.record(name, value, HashMap::new()).await;
    }

    /// Get average of last N samples for a metric.
    pub async fn avg(&self, name: &str, n: usize) -> f64 {
        let buffers = self.buffers.read().await;
        buffers.get(name).map_or(0.0, |b| b.avg_last_n(n))
    }

    /// Get P95 of last N samples for a metric.
    pub async fn p95(&self, name: &str, n: usize) -> f64 {
        let buffers = self.buffers.read().await;
        buffers.get(name).map_or(0.0, |b| b.p95_last_n(n))
    }

    /// Get latest value for a metric.
    pub async fn latest(&self, name: &str) -> f64 {
        let buffers = self.buffers.read().await;
        buffers
            .get(name)
            .and_then(|b| b.recent(1).first().map(|s| s.value))
            .unwrap_or(0.0)
    }

    /// Get recent samples for a metric.
    pub async fn recent_samples(&self, name: &str, n: usize) -> Vec<MetricSample> {
        let buffers = self.buffers.read().await;
        buffers
            .get(name)
            .map(|b| b.recent(n).into_iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Build a complete system snapshot from current metrics.
    pub async fn snapshot(&self) -> SystemSnapshot {
        let n = 100; // window size
        SystemSnapshot {
            pipeline_avg_latency_ms: self.avg("pipeline.latency_ms", n).await,
            pipeline_p95_latency_ms: self.p95("pipeline.latency_ms", n).await,
            pipeline_error_rate: self.avg("pipeline.error_rate", n).await,
            llm_daily_cost_usd: self.latest("llm.daily_cost_usd").await,
            llm_avg_latency_ms: self.avg("llm.latency_ms", n).await,
            llm_total_tokens_today: self.latest("llm.total_tokens_today").await as u64,
            cache_hit_rate: self.avg("cache.hit_rate", n).await,
            cache_size_entries: self.latest("cache.size_entries").await as u64,
            db_avg_query_ms: self.avg("db.query_ms", n).await,
            db_lock_contention_pct: self.avg("db.lock_contention_pct", n).await,
            sse_active_streams: self.latest("sse.active_streams").await as u64,
            sse_avg_first_byte_ms: self.avg("sse.first_byte_ms", n).await,
            agent_loop_avg_iterations: self.avg("agent_loop.iterations", n).await,
            agent_loop_avg_duration_ms: self.avg("agent_loop.duration_ms", n).await,
            active_projects: self.latest("system.active_projects").await as u64,
            memory_used_mb: self.latest("system.memory_mb").await,
            uptime_secs: self.boot_time.elapsed().as_secs(),
        }
    }

    /// Total samples recorded since boot.
    pub fn total_samples(&self) -> u64 {
        self.total_samples.load(Ordering::Relaxed)
    }

    /// List all metric names currently tracked.
    pub async fn metric_names(&self) -> Vec<String> {
        let buffers = self.buffers.read().await;
        buffers.keys().cloned().collect()
    }

    /// Uptime in seconds.
    pub fn uptime_secs(&self) -> u64 {
        self.boot_time.elapsed().as_secs()
    }
}

// ---------------------------------------------------------------------------
// Well-known metric names
// ---------------------------------------------------------------------------

pub mod metric_names {
    pub const PIPELINE_LATENCY_MS: &str = "pipeline.latency_ms";
    pub const PIPELINE_ERROR_RATE: &str = "pipeline.error_rate";
    pub const PIPELINE_STEP_LATENCY_MS: &str = "pipeline.step.latency_ms";

    pub const LLM_LATENCY_MS: &str = "llm.latency_ms";
    pub const LLM_COST_USD: &str = "llm.cost_usd";
    pub const LLM_DAILY_COST_USD: &str = "llm.daily_cost_usd";
    pub const LLM_TOKENS: &str = "llm.tokens";
    pub const LLM_TOTAL_TOKENS_TODAY: &str = "llm.total_tokens_today";
    pub const LLM_INPUT_TOKENS: &str = "llm.input_tokens";

    pub const CACHE_HIT_RATE: &str = "cache.hit_rate";
    pub const CACHE_SIZE_ENTRIES: &str = "cache.size_entries";
    pub const CACHE_EVICTIONS: &str = "cache.evictions";

    pub const DB_QUERY_MS: &str = "db.query_ms";
    pub const DB_LOCK_CONTENTION_PCT: &str = "db.lock_contention_pct";
    pub const DB_SLOW_QUERIES: &str = "db.slow_queries";

    pub const SSE_ACTIVE_STREAMS: &str = "sse.active_streams";
    pub const SSE_FIRST_BYTE_MS: &str = "sse.first_byte_ms";
    pub const SSE_DROPPED_EVENTS: &str = "sse.dropped_events";

    pub const AGENT_LOOP_ITERATIONS: &str = "agent_loop.iterations";
    pub const AGENT_LOOP_DURATION_MS: &str = "agent_loop.duration_ms";
    pub const AGENT_LOOP_TOOL_CALLS: &str = "agent_loop.tool_calls";

    pub const SYSTEM_MEMORY_MB: &str = "system.memory_mb";
    pub const SYSTEM_ACTIVE_PROJECTS: &str = "system.active_projects";
}

// ---------------------------------------------------------------------------
// Convenience: metric instrumentation helpers
// ---------------------------------------------------------------------------

/// Record pipeline step latency with step name tag.
pub async fn record_pipeline_step(bus: &MetricsBus, step: &str, latency_ms: f64) {
    let mut tags = HashMap::new();
    tags.insert("step".into(), step.into());
    bus.record(metric_names::PIPELINE_STEP_LATENCY_MS, latency_ms, tags)
        .await;
    bus.record_value(metric_names::PIPELINE_LATENCY_MS, latency_ms)
        .await;
    debug!(step = step, latency_ms = latency_ms, "pipeline step metric");
}

/// Record an LLM call with model and purpose tags.
pub async fn record_llm_call(
    bus: &MetricsBus,
    model: &str,
    purpose: &str,
    latency_ms: f64,
    cost_usd: f64,
    tokens: u64,
    input_tokens: u64,
) {
    let mut tags = HashMap::new();
    tags.insert("model".into(), model.into());
    tags.insert("purpose".into(), purpose.into());
    bus.record(metric_names::LLM_LATENCY_MS, latency_ms, tags.clone())
        .await;
    bus.record(metric_names::LLM_COST_USD, cost_usd, tags.clone())
        .await;
    bus.record(metric_names::LLM_TOKENS, tokens as f64, tags.clone())
        .await;
    bus.record(metric_names::LLM_INPUT_TOKENS, input_tokens as f64, tags)
        .await;
}

/// Record a DB query.
pub async fn record_db_query(bus: &MetricsBus, query_type: &str, latency_ms: f64) {
    let mut tags = HashMap::new();
    tags.insert("type".into(), query_type.into());
    bus.record(metric_names::DB_QUERY_MS, latency_ms, tags)
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn records_and_reads_metrics() {
        let bus = MetricsBus::new(100);
        bus.record_value("test.metric", 42.0).await;
        bus.record_value("test.metric", 58.0).await;

        assert_eq!(bus.avg("test.metric", 10).await, 50.0);
        assert_eq!(bus.latest("test.metric").await, 58.0);
        assert_eq!(bus.total_samples(), 2);
    }

    #[tokio::test]
    async fn ring_buffer_wraps() {
        let bus = MetricsBus::new(3);
        for i in 0..5 {
            bus.record_value("wrap", i as f64).await;
        }
        // Should only keep last 3: 2, 3, 4
        let samples = bus.recent_samples("wrap", 10).await;
        assert_eq!(samples.len(), 3);
        let vals: Vec<f64> = samples.iter().map(|s| s.value).collect();
        assert_eq!(vals, vec![2.0, 3.0, 4.0]);
    }

    #[tokio::test]
    async fn snapshot_returns_defaults_when_empty() {
        let bus = MetricsBus::new(100);
        let snap = bus.snapshot().await;
        assert_eq!(snap.pipeline_avg_latency_ms, 0.0);
        assert!(snap.uptime_secs < 2);
    }
}
