//! LLM response cache — avoid duplicate calls for identical prompts.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct LlmCache {
    entries: Mutex<HashMap<u64, CacheEntry>>,
    ttl: Duration,
}

struct CacheEntry {
    response: String,
    created: Instant,
}

impl LlmCache {
    pub fn new(ttl_secs: u64) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            ttl: Duration::from_secs(ttl_secs),
        }
    }

    pub fn get(&self, prompt_hash: u64) -> Option<String> {
        let mut entries = self.entries.lock().ok()?;
        match entries.get(&prompt_hash) {
            Some(entry) if entry.created.elapsed() < self.ttl => Some(entry.response.clone()),
            Some(_) => {
                // Drop expired entries lazily on access so the map shrinks
                // even when set() isn't called.
                entries.remove(&prompt_hash);
                None
            }
            None => None,
        }
    }

    pub fn set(&self, prompt_hash: u64, response: String) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.insert(
                prompt_hash,
                CacheEntry {
                    response,
                    created: Instant::now(),
                },
            );
            // Evict old entries if too many
            if entries.len() > 1000 {
                let ttl = self.ttl;
                entries.retain(|_, v| v.created.elapsed() < ttl);
            }
        }
    }

    /// Remove every entry whose TTL has elapsed. Intended for a periodic
    /// background janitor; safe to call from any context (uses the same
    /// `std::sync::Mutex` as the rest of the API).
    pub fn evict_expired(&self) -> usize {
        let Ok(mut entries) = self.entries.lock() else {
            return 0;
        };
        let before = entries.len();
        let ttl = self.ttl;
        entries.retain(|_, v| v.created.elapsed() < ttl);
        before.saturating_sub(entries.len())
    }

    /// Simple FNV-1a hash for cache keys.
    ///
    /// WARNING: Prefer `hash_scoped` for any call that mixes tenant data —
    /// a bare prompt hash shares entries across tenants, which leaks
    /// generated content when two tenants submit identical prompts.
    pub fn hash_prompt(prompt: &str) -> u64 {
        let mut hash: u64 = 0xcbf29ce484222325;
        for byte in prompt.bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }

    /// Tenant-scoped cache key. Combines tenant_id, provider, model, and
    /// prompt so that tenant A's cached response cannot be served to tenant
    /// B even when they submit identical prompts.
    ///
    /// Prefer [`hash_request`] for new call sites — it includes temperature
    /// and max_tokens so a deterministic run cannot collide with a creative
    /// one. This variant is retained for older callers.
    pub fn hash_scoped(tenant_id: &str, provider: &str, model: &str, prompt: &str) -> u64 {
        Self::hash_request(tenant_id, provider, model, prompt, 0.0, 0)
    }

    /// Full cache key including sampling parameters. Two requests with the
    /// same prompt but different `temperature` or `max_tokens` MUST hash to
    /// different values, otherwise a deterministic consumer can receive a
    /// cached creative response (or vice versa).
    ///
    /// Uses SHA-256 truncated to `u64` for collision resistance. The
    /// length-prefixed framing prevents boundary confusion: a key built from
    /// `("a", "b", …)` cannot collide with one built from `("ab", "", …)`.
    pub fn hash_request(
        tenant_id: &str,
        provider: &str,
        model: &str,
        prompt: &str,
        temperature: f32,
        max_tokens: u32,
    ) -> u64 {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        let parts: &[&[u8]] = &[
            tenant_id.as_bytes(),
            provider.as_bytes(),
            model.as_bytes(),
            prompt.as_bytes(),
        ];
        for part in parts {
            hasher.update((part.len() as u64).to_le_bytes());
            hasher.update(part);
        }
        hasher.update(temperature.to_bits().to_le_bytes());
        hasher.update(max_tokens.to_le_bytes());
        let digest = hasher.finalize();
        let mut buf = [0u8; 8];
        buf.copy_from_slice(&digest[..8]);
        u64::from_le_bytes(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_hit_within_ttl() {
        let cache = LlmCache::new(60);
        let hash = LlmCache::hash_prompt("test prompt");
        cache.set(hash, "cached response".into());
        assert_eq!(cache.get(hash), Some("cached response".into()));
    }

    #[test]
    fn cache_miss_for_unknown_hash() {
        let cache = LlmCache::new(60);
        assert_eq!(cache.get(12345), None);
    }

    #[test]
    fn hash_is_deterministic() {
        let h1 = LlmCache::hash_prompt("hello world");
        let h2 = LlmCache::hash_prompt("hello world");
        assert_eq!(h1, h2);
    }

    #[test]
    fn different_prompts_different_hashes() {
        let h1 = LlmCache::hash_prompt("prompt a");
        let h2 = LlmCache::hash_prompt("prompt b");
        assert_ne!(h1, h2);
    }

    #[test]
    fn scoped_hash_isolates_tenants() {
        let a = LlmCache::hash_scoped("tenant-a", "anthropic", "opus", "same prompt");
        let b = LlmCache::hash_scoped("tenant-b", "anthropic", "opus", "same prompt");
        assert_ne!(a, b, "tenants must not share cache entries");
    }

    #[test]
    fn scoped_hash_isolates_providers_and_models() {
        let a = LlmCache::hash_scoped("t", "openai", "gpt-4o", "p");
        let b = LlmCache::hash_scoped("t", "openai", "o3-mini", "p");
        let c = LlmCache::hash_scoped("t", "anthropic", "gpt-4o", "p");
        assert_ne!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn request_hash_differs_on_temperature() {
        let a = LlmCache::hash_request("t", "openai", "gpt-4o", "p", 0.0, 512);
        let b = LlmCache::hash_request("t", "openai", "gpt-4o", "p", 0.7, 512);
        assert_ne!(a, b, "temperature must contribute to the cache key");
    }

    #[test]
    fn request_hash_differs_on_max_tokens() {
        let a = LlmCache::hash_request("t", "openai", "gpt-4o", "p", 0.0, 512);
        let b = LlmCache::hash_request("t", "openai", "gpt-4o", "p", 0.0, 1024);
        assert_ne!(a, b, "max_tokens must contribute to the cache key");
    }

    #[test]
    fn request_hash_length_prefix_prevents_boundary_collision() {
        // Without length-prefix framing, ("a", "bc") and ("ab", "c") could
        // hash identically under naive concatenation. SHA-256 + lengths fix this.
        let a = LlmCache::hash_request("a", "bc", "", "", 0.0, 0);
        let b = LlmCache::hash_request("ab", "c", "", "", 0.0, 0);
        assert_ne!(a, b);
    }
}
