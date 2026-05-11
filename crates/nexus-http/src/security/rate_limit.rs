//! Per-tenant token-bucket rate limiter.
//!
//! This is separate from the global IP-based rate limiter in [`crate::rate_limiter`].
//! It enforces per-tenant limits on different endpoint categories to prevent a single
//! tenant from monopolising shared resources.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

/// Configuration for per-tenant rate limits (requests per minute).
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Max generation (LLM) requests per tenant per minute.
    pub generation_per_minute: u32,
    /// Max chat requests per tenant per minute.
    pub chat_per_minute: u32,
    /// Max agent execution requests per tenant per minute.
    pub agent_per_minute: u32,
    /// Max read (GET) requests per tenant per minute.
    pub read_per_minute: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            generation_per_minute: 10,
            chat_per_minute: 60,
            agent_per_minute: 20,
            read_per_minute: 200,
        }
    }
}

/// A single token bucket with linear refill.
#[derive(Debug, Clone)]
struct TokenBucket {
    /// Current available tokens.
    tokens: f64,
    /// Maximum capacity.
    max_tokens: f64,
    /// Tokens added per second.
    refill_rate: f64,
    /// Last time we refilled.
    last_refill: Instant,
}

impl TokenBucket {
    fn new(max_per_minute: u32) -> Self {
        let max = max_per_minute as f64;
        Self {
            tokens: max,
            max_tokens: max,
            refill_rate: max / 60.0, // tokens per second
            last_refill: Instant::now(),
        }
    }

    /// Try to consume one token. Returns `Ok(remaining)` or `Err(retry_after_secs)`.
    fn try_consume(&mut self) -> Result<u32, f64> {
        self.refill();

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            Ok(self.tokens as u32)
        } else {
            // How long until 1 token is available?
            let wait = (1.0 - self.tokens) / self.refill_rate;
            Err(wait)
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.max_tokens);
        self.last_refill = now;
    }
}

/// Per-tenant rate limiter using the token bucket algorithm.
///
/// Each tenant gets independent buckets for each endpoint category.
/// Thread-safe via internal `Mutex`.
pub struct TenantRateLimiter {
    /// `tenant_id` -> `endpoint_type` -> bucket
    buckets: Mutex<HashMap<String, HashMap<String, TokenBucket>>>,
    /// Configuration defining limits per endpoint type.
    limits: RateLimitConfig,
}

impl TenantRateLimiter {
    /// Create a new rate limiter with the given configuration.
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            buckets: Mutex::new(HashMap::new()),
            limits: config,
        }
    }

    /// Create a new rate limiter with default limits.
    pub fn with_defaults() -> Self {
        Self::new(RateLimitConfig::default())
    }

    /// Check whether a request from the given tenant for the given endpoint type is allowed.
    ///
    /// `endpoint_type` should be one of: `"generation"`, `"chat"`, `"agent"`, `"read"`.
    ///
    /// Returns `Ok(remaining)` if allowed, or `Err(message)` with a retry-after hint.
    pub fn check(&self, tenant_id: &str, endpoint_type: &str) -> Result<u32, String> {
        let max_per_minute = match endpoint_type {
            "generation" => self.limits.generation_per_minute,
            "chat" => self.limits.chat_per_minute,
            "agent" => self.limits.agent_per_minute,
            "read" => self.limits.read_per_minute,
            _ => self.limits.read_per_minute, // default to read limits
        };

        let mut buckets = self.buckets.lock().unwrap_or_else(|p| p.into_inner());

        let tenant_buckets = buckets
            .entry(tenant_id.to_string())
            .or_default();

        let bucket = tenant_buckets
            .entry(endpoint_type.to_string())
            .or_insert_with(|| TokenBucket::new(max_per_minute));

        bucket.try_consume().map_err(|wait_secs| {
            format!(
                "Rate limit exceeded for tenant {tenant_id} on {endpoint_type}. \
                 Retry after {wait_secs:.1}s"
            )
        })
    }

    /// Evict stale tenant buckets that have been idle for more than 10 minutes.
    /// Call this periodically to prevent unbounded memory growth.
    pub fn evict_stale(&self) {
        let cutoff = Instant::now() - std::time::Duration::from_secs(600);
        let mut buckets = self.buckets.lock().unwrap_or_else(|p| p.into_inner());
        buckets.retain(|_, tenant_map| {
            tenant_map.retain(|_, bucket| bucket.last_refill > cutoff);
            !tenant_map.is_empty()
        });
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_rate_limit() {
        let limiter = TenantRateLimiter::new(RateLimitConfig {
            generation_per_minute: 3,
            chat_per_minute: 60,
            agent_per_minute: 20,
            read_per_minute: 200,
        });

        // Should allow 3 generation requests
        assert!(limiter.check("t1", "generation").is_ok());
        assert!(limiter.check("t1", "generation").is_ok());
        assert!(limiter.check("t1", "generation").is_ok());

        // 4th should be rate-limited
        let result = limiter.check("t1", "generation");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Rate limit exceeded"));
    }

    #[test]
    fn test_different_tenants_independent() {
        let limiter = TenantRateLimiter::new(RateLimitConfig {
            generation_per_minute: 1,
            chat_per_minute: 60,
            agent_per_minute: 20,
            read_per_minute: 200,
        });

        // Exhaust tenant A's generation limit
        assert!(limiter.check("tenant-a", "generation").is_ok());
        assert!(limiter.check("tenant-a", "generation").is_err());

        // Tenant B should still be fine
        assert!(limiter.check("tenant-b", "generation").is_ok());
    }

    #[test]
    fn test_different_endpoint_types_independent() {
        let limiter = TenantRateLimiter::new(RateLimitConfig {
            generation_per_minute: 1,
            chat_per_minute: 1,
            agent_per_minute: 20,
            read_per_minute: 200,
        });

        // Exhaust generation limit
        assert!(limiter.check("t1", "generation").is_ok());
        assert!(limiter.check("t1", "generation").is_err());

        // Chat should still work
        assert!(limiter.check("t1", "chat").is_ok());
    }

    #[test]
    fn test_unknown_endpoint_uses_read_limit() {
        let limiter = TenantRateLimiter::new(RateLimitConfig {
            generation_per_minute: 1,
            chat_per_minute: 1,
            agent_per_minute: 1,
            read_per_minute: 2,
        });

        assert!(limiter.check("t1", "unknown-endpoint").is_ok());
        assert!(limiter.check("t1", "unknown-endpoint").is_ok());
        assert!(limiter.check("t1", "unknown-endpoint").is_err());
    }

    #[test]
    fn test_evict_stale() {
        let limiter = TenantRateLimiter::with_defaults();
        let _ = limiter.check("stale-tenant", "read");
        // Eviction should not panic; the tenant was just used so it won't be evicted.
        limiter.evict_stale();
    }
}
