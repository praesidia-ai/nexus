use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single item in the session context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextItem {
    pub id: String,
    pub kind: ContextKind,
    pub content: String,
    pub token_estimate: usize,
    pub priority: ContextPriority,
    pub created_at: DateTime<Utc>,
    pub last_accessed: DateTime<Utc>,
    pub access_count: u32,
    pub pinned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ContextKind {
    UserMessage,
    AssistantMessage,
    ToolResult,
    FileContent,
    SearchResult,
    SystemInstruction,
    CompressedSummary,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ContextPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
    Pinned = 4,
}

impl ContextItem {
    pub fn kind_label(&self) -> &'static str {
        match self.kind {
            ContextKind::UserMessage => "user",
            ContextKind::AssistantMessage => "assistant",
            ContextKind::ToolResult => "tool",
            ContextKind::FileContent => "file",
            ContextKind::SearchResult => "search",
            ContextKind::SystemInstruction => "system",
            ContextKind::CompressedSummary => "summary",
        }
    }
}

/// L2 — Session Working Memory: the active context window.
pub struct SessionMemory {
    items: Vec<ContextItem>,
    max_tokens: usize,
    current_tokens: usize,
    compression_threshold: f64,
}

impl SessionMemory {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            items: Vec::new(),
            max_tokens,
            current_tokens: 0,
            compression_threshold: 0.80,
        }
    }

    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.compression_threshold = threshold;
        self
    }

    pub fn push(&mut self, item: ContextItem) {
        self.current_tokens += item.token_estimate;
        self.items.push(item);
    }

    pub fn needs_compression(&self) -> bool {
        self.current_tokens as f64 > self.max_tokens as f64 * self.compression_threshold
    }

    /// Get IDs of items that should be evicted (lowest priority, oldest, least accessed).
    pub fn eviction_candidates(&self, target_reduction: usize) -> Vec<String> {
        let mut candidates: Vec<&ContextItem> = self
            .items
            .iter()
            .filter(|i| !i.pinned && i.kind != ContextKind::SystemInstruction)
            .collect();

        candidates.sort_by(|a, b| {
            a.priority
                .cmp(&b.priority)
                .then(a.last_accessed.cmp(&b.last_accessed))
                .then(a.access_count.cmp(&b.access_count))
        });

        let mut evict_ids = Vec::new();
        let mut freed = 0usize;
        for item in candidates {
            if freed >= target_reduction {
                break;
            }
            freed += item.token_estimate;
            evict_ids.push(item.id.clone());
        }
        evict_ids
    }

    /// Remove items by their IDs and return the freed token count.
    pub fn evict(&mut self, ids: &[String]) -> usize {
        let mut freed = 0;
        self.items.retain(|item| {
            if ids.contains(&item.id) {
                freed += item.token_estimate;
                false
            } else {
                true
            }
        });
        self.current_tokens = self.current_tokens.saturating_sub(freed);
        freed
    }

    pub fn access(&mut self, id: &str) {
        if let Some(item) = self.items.iter_mut().find(|i| i.id == id) {
            item.last_accessed = Utc::now();
            item.access_count += 1;
        }
    }

    pub fn render_window(&self) -> Vec<&ContextItem> {
        self.items.iter().collect()
    }

    pub fn current_usage(&self) -> (usize, usize) {
        (self.current_tokens, self.max_tokens)
    }

    pub fn item_count(&self) -> usize {
        self.items.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_item(id: &str, kind: ContextKind, tokens: usize, priority: ContextPriority) -> ContextItem {
        ContextItem {
            id: id.to_string(),
            kind,
            content: "x".repeat(tokens * 4),
            token_estimate: tokens,
            priority,
            created_at: Utc::now(),
            last_accessed: Utc::now(),
            access_count: 0,
            pinned: priority == ContextPriority::Pinned,
        }
    }

    #[test]
    fn push_increases_token_count() {
        let mut mem = SessionMemory::new(1000);
        mem.push(make_item("a", ContextKind::UserMessage, 100, ContextPriority::Normal));
        assert_eq!(mem.current_usage(), (100, 1000));
        assert_eq!(mem.item_count(), 1);
    }

    #[test]
    fn needs_compression_at_threshold() {
        let mut mem = SessionMemory::new(1000);
        mem.push(make_item("a", ContextKind::UserMessage, 799, ContextPriority::Normal));
        assert!(!mem.needs_compression());
        mem.push(make_item("b", ContextKind::UserMessage, 2, ContextPriority::Normal));
        assert!(mem.needs_compression());
    }

    #[test]
    fn custom_threshold() {
        let mut mem = SessionMemory::new(1000).with_threshold(0.50);
        mem.push(make_item("a", ContextKind::UserMessage, 499, ContextPriority::Normal));
        assert!(!mem.needs_compression());
        mem.push(make_item("b", ContextKind::UserMessage, 2, ContextPriority::Normal));
        assert!(mem.needs_compression());
    }

    #[test]
    fn eviction_prefers_low_priority() {
        let mut mem = SessionMemory::new(1000);
        mem.push(make_item("high", ContextKind::UserMessage, 100, ContextPriority::High));
        mem.push(make_item("low", ContextKind::ToolResult, 100, ContextPriority::Low));
        mem.push(make_item("normal", ContextKind::UserMessage, 100, ContextPriority::Normal));

        let candidates = mem.eviction_candidates(100);
        assert_eq!(candidates[0], "low");
    }

    #[test]
    fn pinned_items_never_evicted() {
        let mut mem = SessionMemory::new(1000);
        mem.push(make_item("pinned", ContextKind::UserMessage, 500, ContextPriority::Pinned));
        mem.push(make_item("normal", ContextKind::ToolResult, 100, ContextPriority::Low));

        let candidates = mem.eviction_candidates(600);
        assert!(!candidates.contains(&"pinned".to_string()));
        assert!(candidates.contains(&"normal".to_string()));
    }

    #[test]
    fn system_instructions_never_evicted() {
        let mut mem = SessionMemory::new(1000);
        mem.push(make_item("sys", ContextKind::SystemInstruction, 200, ContextPriority::Normal));
        mem.push(make_item("tool", ContextKind::ToolResult, 100, ContextPriority::Normal));

        let candidates = mem.eviction_candidates(300);
        assert!(!candidates.contains(&"sys".to_string()));
        assert!(candidates.contains(&"tool".to_string()));
    }

    #[test]
    fn evict_frees_tokens() {
        let mut mem = SessionMemory::new(1000);
        mem.push(make_item("a", ContextKind::UserMessage, 200, ContextPriority::Normal));
        mem.push(make_item("b", ContextKind::UserMessage, 300, ContextPriority::Normal));
        assert_eq!(mem.current_usage().0, 500);

        let freed = mem.evict(&["a".to_string()]);
        assert_eq!(freed, 200);
        assert_eq!(mem.current_usage().0, 300);
        assert_eq!(mem.item_count(), 1);
    }

    #[test]
    fn access_bumps_count_and_timestamp() {
        let mut mem = SessionMemory::new(1000);
        let mut item = make_item("a", ContextKind::UserMessage, 100, ContextPriority::Normal);
        item.last_accessed = chrono::DateTime::from_timestamp(0, 0).unwrap();
        mem.push(item);

        mem.access("a");
        let items = mem.render_window();
        assert_eq!(items[0].access_count, 1);
        assert!(items[0].last_accessed.timestamp() > 0);
    }

    #[test]
    fn render_window_returns_all_items_in_order() {
        let mut mem = SessionMemory::new(1000);
        mem.push(make_item("first", ContextKind::UserMessage, 50, ContextPriority::Normal));
        mem.push(make_item("second", ContextKind::AssistantMessage, 50, ContextPriority::Normal));
        let window = mem.render_window();
        assert_eq!(window.len(), 2);
        assert_eq!(window[0].id, "first");
        assert_eq!(window[1].id, "second");
    }

    #[test]
    fn eviction_stops_once_target_met() {
        let mut mem = SessionMemory::new(1000);
        mem.push(make_item("a", ContextKind::ToolResult, 100, ContextPriority::Low));
        mem.push(make_item("b", ContextKind::ToolResult, 100, ContextPriority::Low));
        mem.push(make_item("c", ContextKind::ToolResult, 100, ContextPriority::Low));

        let candidates = mem.eviction_candidates(150);
        assert_eq!(candidates.len(), 2);
    }

    #[test]
    fn kind_label_coverage() {
        let item = make_item("x", ContextKind::FileContent, 10, ContextPriority::Normal);
        assert_eq!(item.kind_label(), "file");
        let item = make_item("x", ContextKind::SearchResult, 10, ContextPriority::Normal);
        assert_eq!(item.kind_label(), "search");
        let item = make_item("x", ContextKind::CompressedSummary, 10, ContextPriority::Normal);
        assert_eq!(item.kind_label(), "summary");
    }
}
