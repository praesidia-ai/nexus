//! Request rate limiting and concurrency control.
//!
//! Two mechanisms:
//! 1. `ConcurrencyGuard` — a semaphore-based concurrency limiter for expensive LLM endpoints.
//!    Prevents thundering-herd scenarios where N parallel requests all hit the LLM simultaneously.
//! 2. `RequestRateLimiter` — a sliding-window counter per remote IP for abuse prevention.
//!
//! Usage in handlers:
//! ```ignore
//! let _guard = app.rate_limiter.acquire_llm_slot().await?;
//! // LLM call happens here; slot is released when _guard is dropped
//! ```

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

// ---------------------------------------------------------------------------
// Concurrency limiter (for LLM-heavy paths)
// ---------------------------------------------------------------------------

/// Limits how many LLM calls can be in-flight at once.
/// Prevents resource starvation and protects against API rate limits.
pub struct ConcurrencyGuard(#[allow(dead_code)] OwnedSemaphorePermit);

pub struct LlmConcurrencyLimiter {
    semaphore: Arc<Semaphore>,
}

impl LlmConcurrencyLimiter {
    /// `max_concurrent` — max simultaneous LLM requests (recommend 8-16 for most setups).
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
        }
    }

    /// Acquire a slot. Returns immediately if a slot is available; blocks until one frees up.
    /// Returns `Err` if the semaphore is closed (shutdown path).
    pub async fn acquire(&self) -> Result<ConcurrencyGuard, String> {
        self.semaphore
            .clone()
            .acquire_owned()
            .await
            .map(ConcurrencyGuard)
            .map_err(|e| format!("Concurrency limiter closed: {}", e))
    }

    /// Try to acquire without blocking. Returns `None` if all slots are taken.
    pub fn try_acquire(&self) -> Option<ConcurrencyGuard> {
        self.semaphore
            .clone()
            .try_acquire_owned()
            .ok()
            .map(ConcurrencyGuard)
    }

    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }
}

// ---------------------------------------------------------------------------
// Sliding-window IP rate limiter (abuse prevention)
// ---------------------------------------------------------------------------

/// Per-IP sliding-window counters. We store window-start as a `u64` of
/// milliseconds since the limiter's epoch so we can update it lock-free
/// alongside the count.
struct WindowEntry {
    count: AtomicU32,
    /// Window start in millis-since-epoch (relative to `IpRateLimiter::epoch`).
    window_start_ms: AtomicU64,
}

/// Sharded, lock-free per-IP rate limiter.
///
/// The previous implementation took a single `std::sync::Mutex<HashMap>` on
/// EVERY non-public request and held it during `HashMap::entry`/`insert`/
/// `retain` (which could scan up to 10k entries on a Tokio worker thread).
/// Under burst the contention was worse than the abuse it was protecting
/// against. This version:
///   • shards the map via `DashMap` (16 shards by default), so distinct IPs
///     contend on different locks;
///   • stores per-IP `(count, window_start_ms)` as atomics, so the hot path
///     under a shard read-lock is a couple of `compare_exchange`s;
///   • runs eviction on a separate periodic background task instead of
///     inline-on-Nth-call.
pub struct IpRateLimiter {
    windows: DashMap<String, WindowEntry>,
    max_per_window: u32,
    window_duration: Duration,
    epoch: Instant,
}

impl IpRateLimiter {
    pub fn new(max_per_minute: u32) -> Self {
        Self {
            windows: DashMap::new(),
            max_per_window: max_per_minute,
            window_duration: Duration::from_secs(60),
            epoch: Instant::now(),
        }
    }

    fn now_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }

    /// Returns `Ok(remaining)` on success or `Err(retry_after_secs)` when rate-limited.
    pub fn check(&self, ip: &str) -> Result<u32, u64> {
        let now_ms = self.now_ms();
        let window_ms = self.window_duration.as_millis() as u64;

        // Fast path: the IP already has an entry. Run a CAS loop to either
        // (a) reset the window and bump count to 1, or
        // (b) increment count if we're still within the window.
        loop {
            if let Some(entry) = self.windows.get(ip) {
                let start = entry.window_start_ms.load(Ordering::Acquire);
                let elapsed = now_ms.saturating_sub(start);

                if elapsed >= window_ms {
                    // Window expired — try to reset it. If another thread won
                    // the reset race, retry.
                    if entry
                        .window_start_ms
                        .compare_exchange(start, now_ms, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                    {
                        entry.count.store(1, Ordering::Release);
                        return Ok(self.max_per_window.saturating_sub(1));
                    }
                    continue;
                }

                let new_count = entry.count.fetch_add(1, Ordering::AcqRel) + 1;
                if new_count > self.max_per_window {
                    // Saturate the counter so we don't wrap if the same IP
                    // keeps hammering after the limit is hit.
                    entry.count.store(self.max_per_window + 1, Ordering::Release);
                    let retry_after = (window_ms - elapsed).saturating_div(1000).max(1);
                    return Err(retry_after);
                }
                return Ok(self.max_per_window - new_count);
            }
            break;
        }

        // Slow path: insert a new entry. Use `entry().or_insert_with` so two
        // concurrent first-requests from the same IP don't race past the
        // initial limit.
        let entry = self.windows.entry(ip.to_string()).or_insert_with(|| WindowEntry {
            count: AtomicU32::new(0),
            window_start_ms: AtomicU64::new(now_ms),
        });
        let new_count = entry.count.fetch_add(1, Ordering::AcqRel) + 1;
        if new_count > self.max_per_window {
            return Err(self.window_duration.as_secs());
        }
        Ok(self.max_per_window - new_count)
    }

    /// Drop entries whose window has elapsed. Call from a periodic background
    /// task; this is O(n) but no longer holds a global lock.
    pub fn evict_expired(&self) -> usize {
        let now_ms = self.now_ms();
        let window_ms = self.window_duration.as_millis() as u64;
        let before = self.windows.len();
        self.windows.retain(|_, v| {
            let start = v.window_start_ms.load(Ordering::Acquire);
            now_ms.saturating_sub(start) < window_ms
        });
        before.saturating_sub(self.windows.len())
    }

    pub fn entry_count(&self) -> usize {
        self.windows.len()
    }
}

// ---------------------------------------------------------------------------
// Combined facade stored in AppState
// ---------------------------------------------------------------------------

pub struct RateLimiter {
    /// Limits concurrent LLM calls globally
    pub llm_slots: LlmConcurrencyLimiter,
    /// Limits requests per IP per minute for expensive paths
    pub ip_limiter: IpRateLimiter,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            // At most 12 concurrent LLM calls — protects against runaway parallel requests
            llm_slots: LlmConcurrencyLimiter::new(12),
            // At most 300 requests per IP per minute across the whole API.
            // Individual heavy endpoints can still opt into additional limits.
            ip_limiter: IpRateLimiter::new(300),
        }
    }

    /// Acquire an LLM concurrency slot.
    pub async fn acquire_llm_slot(&self) -> Result<ConcurrencyGuard, String> {
        self.llm_slots.acquire().await
    }

    /// Check IP rate limit. Returns `Ok(remaining)` or `Err(retry_after_secs)`.
    pub fn check_ip(&self, ip: &str) -> Result<u32, u64> {
        self.ip_limiter.check(ip)
    }
}

// ---------------------------------------------------------------------------
// Axum middleware — global per-IP rate limiter
// ---------------------------------------------------------------------------

use axum::{
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::net::SocketAddr;

/// Global per-IP rate-limit middleware. Health probes and CORS preflights
/// bypass the limit; everything else is counted against the caller's IP.
pub async fn rate_limit_middleware(
    State(state): State<Arc<crate::state::AppState>>,
    connect_info: Option<ConnectInfo<SocketAddr>>,
    req: Request,
    next: Next,
) -> Response {
    let path = req.uri().path().to_string();
    if req.method() == axum::http::Method::OPTIONS
        || crate::security::auth::is_public_path(&path)
    {
        return next.run(req).await;
    }

    // Prefer the trusted TCP peer when available. X-Forwarded-For is not used
    // here because a caller can forge it; operators who terminate TLS at a
    // proxy should configure that proxy to rewrite the peer address.
    let ip = connect_info
        .map(|ConnectInfo(addr)| addr.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    match state.rate_limiter.check_ip(&ip) {
        Ok(_remaining) => next.run(req).await,
        Err(retry_after) => (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", retry_after.to_string())],
            axum::Json(serde_json::json!({
                "error": {
                    "code": "RATE_LIMITED",
                    "message": "Too many requests — slow down.",
                    "retry_after_secs": retry_after,
                }
            })),
        )
            .into_response(),
    }
}
