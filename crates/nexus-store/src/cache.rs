use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::RwLock;

/// Simple LRU cache keyed by `String`.
///
/// `order` tracks access recency: the **last** element is the most-recently
/// used, and the **first** is the eviction candidate.
pub struct LruCache<V> {
    capacity: usize,
    map: HashMap<String, (V, Instant)>,
    order: Vec<String>,
}

impl<V: Clone> LruCache<V> {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "LruCache capacity must be > 0");
        Self {
            capacity,
            map: HashMap::with_capacity(capacity),
            order: Vec::with_capacity(capacity),
        }
    }

    /// Return a clone of the cached value and promote the key to MRU.
    pub fn get(&mut self, key: &str) -> Option<V> {
        if self.map.contains_key(key) {
            self.promote(key);
            self.map.get(key).map(|(v, _)| v.clone())
        } else {
            None
        }
    }

    /// Insert or update a value. Evicts the LRU entry when at capacity.
    pub fn insert(&mut self, key: String, value: V) {
        if self.map.contains_key(&key) {
            self.promote(&key);
            self.map.insert(key, (value, Instant::now()));
            return;
        }

        if self.map.len() >= self.capacity {
            if let Some(lru_key) = self.order.first().cloned() {
                self.order.remove(0);
                self.map.remove(&lru_key);
            }
        }

        self.order.push(key.clone());
        self.map.insert(key, (value, Instant::now()));
    }

    pub fn invalidate(&mut self, key: &str) {
        self.map.remove(key);
        self.order.retain(|k| k != key);
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    fn promote(&mut self, key: &str) {
        self.order.retain(|k| k != key);
        self.order.push(key.to_owned());
    }
}

/// Thread-safe wrapper around [`LruCache`].
pub struct SharedCache<V> {
    inner: Arc<RwLock<LruCache<V>>>,
}

impl<V: Clone + Send + Sync> SharedCache<V> {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(RwLock::new(LruCache::new(capacity))),
        }
    }

    /// Get requires a write-lock because `LruCache::get` promotes the key.
    pub async fn get(&self, key: &str) -> Option<V> {
        self.inner.write().await.get(key)
    }

    pub async fn insert(&self, key: String, value: V) {
        self.inner.write().await.insert(key, value);
    }

    pub async fn invalidate(&self, key: &str) {
        self.inner.write().await.invalidate(key);
    }

    pub async fn clear(&self) {
        self.inner.write().await.clear();
    }

    pub async fn len(&self) -> usize {
        self.inner.read().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.inner.read().await.is_empty()
    }
}

impl<V: Clone + Send + Sync> Clone for SharedCache<V> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_lru_eviction() {
        let mut cache = LruCache::new(2);
        cache.insert("a".into(), 1);
        cache.insert("b".into(), 2);
        assert_eq!(cache.len(), 2);

        cache.insert("c".into(), 3);
        assert_eq!(cache.len(), 2);
        assert!(cache.get("a").is_none(), "a should have been evicted");
        assert_eq!(cache.get("b"), Some(2));
        assert_eq!(cache.get("c"), Some(3));
    }

    #[test]
    fn access_promotes_and_prevents_eviction() {
        let mut cache = LruCache::new(2);
        cache.insert("a".into(), 1);
        cache.insert("b".into(), 2);

        // Access "a" so it becomes MRU; "b" is now LRU.
        assert_eq!(cache.get("a"), Some(1));

        cache.insert("c".into(), 3);
        assert!(cache.get("b").is_none(), "b should be evicted, not a");
        assert_eq!(cache.get("a"), Some(1));
    }

    #[test]
    fn invalidate_removes_entry() {
        let mut cache = LruCache::new(4);
        cache.insert("x".into(), 10);
        cache.invalidate("x");
        assert!(cache.get("x").is_none());
        assert!(cache.is_empty());
    }

    #[test]
    fn clear_empties_cache() {
        let mut cache = LruCache::new(4);
        cache.insert("a".into(), 1);
        cache.insert("b".into(), 2);
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn update_existing_key() {
        let mut cache = LruCache::new(2);
        cache.insert("a".into(), 1);
        cache.insert("a".into(), 99);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get("a"), Some(99));
    }

    #[tokio::test]
    async fn shared_cache_basic() {
        let cache = SharedCache::new(8);
        cache.insert("k1".into(), "v1".to_string()).await;
        assert_eq!(cache.get("k1").await, Some("v1".to_string()));
        assert_eq!(cache.len().await, 1);

        cache.invalidate("k1").await;
        assert!(cache.is_empty().await);
    }

    #[tokio::test]
    async fn shared_cache_clone_shares_state() {
        let a = SharedCache::new(4);
        let b = a.clone();
        a.insert("shared".into(), 42).await;
        assert_eq!(b.get("shared").await, Some(42));
    }
}
