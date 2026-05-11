use crate::layer2::{ContextItem, ContextKind, ContextPriority, SessionMemory};
use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompressionLevel {
    Micro,
    Auto,
    Full,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionResult {
    pub level: CompressionLevel,
    pub tokens_before: usize,
    pub tokens_after: usize,
    pub items_evicted: usize,
    pub summary_generated: bool,
}

/// Micro-compact: evict a small number of lowest-priority items. Zero API calls.
pub fn micro_compact(session: &mut SessionMemory) -> CompressionResult {
    let before = session.current_usage().0;
    let candidates = session.eviction_candidates(before / 4);
    let tool_result_candidates: Vec<String> = candidates.into_iter().take(5).collect();
    let evicted_count = tool_result_candidates.len();
    session.evict(&tool_result_candidates);

    info!(
        level = "micro",
        tokens_freed = before.saturating_sub(session.current_usage().0),
        items_evicted = evicted_count,
        "Micro-compact completed"
    );

    CompressionResult {
        level: CompressionLevel::Micro,
        tokens_before: before,
        tokens_after: session.current_usage().0,
        items_evicted: evicted_count,
        summary_generated: false,
    }
}

/// Auto-compact: evict low-priority items and inject a compressed summary.
pub fn auto_compact(session: &mut SessionMemory) -> CompressionResult {
    let before = session.current_usage().0;
    let target = before / 2;
    let candidates = session.eviction_candidates(target);
    let evicted_count = candidates.len();

    let evicted_content: Vec<String> = session
        .render_window()
        .iter()
        .filter(|item| candidates.contains(&item.id))
        .map(|item| format!("[{}] {}", item.kind_label(), truncate(&item.content, 100)))
        .collect();

    session.evict(&candidates);

    if !evicted_content.is_empty() {
        let summary = format!(
            "## Compressed Context Summary\n{} items compressed:\n{}",
            evicted_count,
            evicted_content.join("\n")
        );
        let summary_item = ContextItem {
            id: uuid::Uuid::new_v4().to_string(),
            kind: ContextKind::CompressedSummary,
            content: summary,
            token_estimate: 200,
            priority: ContextPriority::High,
            created_at: chrono::Utc::now(),
            last_accessed: chrono::Utc::now(),
            access_count: 0,
            pinned: false,
        };
        session.push(summary_item);
    }

    info!(
        level = "auto",
        items_evicted = evicted_count,
        "Auto-compact completed"
    );

    CompressionResult {
        level: CompressionLevel::Auto,
        tokens_before: before,
        tokens_after: session.current_usage().0,
        items_evicted: evicted_count,
        summary_generated: true,
    }
}

/// Full-compact: reset session, keeping only pinned items and system instructions.
pub fn full_compact(session: &mut SessionMemory) -> CompressionResult {
    let before = session.current_usage().0;
    let all_ids: Vec<String> = session
        .render_window()
        .iter()
        .filter(|i| !i.pinned && i.kind != ContextKind::SystemInstruction)
        .map(|i| i.id.clone())
        .collect();
    let evicted_count = all_ids.len();
    session.evict(&all_ids);

    info!(
        level = "full",
        items_evicted = evicted_count,
        "Full-compact completed — session reset"
    );

    CompressionResult {
        level: CompressionLevel::Full,
        tokens_before: before,
        tokens_after: session.current_usage().0,
        items_evicted: evicted_count,
        summary_generated: false,
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.len() <= max_chars {
        s.to_string()
    } else {
        format!("{}...", &s[..max_chars])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_item(
        id: &str,
        kind: ContextKind,
        tokens: usize,
        priority: ContextPriority,
    ) -> ContextItem {
        ContextItem {
            id: id.to_string(),
            kind,
            content: format!("Content for {id}"),
            token_estimate: tokens,
            priority,
            created_at: Utc::now(),
            last_accessed: Utc::now(),
            access_count: 0,
            pinned: priority == ContextPriority::Pinned,
        }
    }

    #[test]
    fn micro_compact_evicts_few_items() {
        let mut session = SessionMemory::new(1000);
        for i in 0..10 {
            session.push(make_item(
                &format!("t{i}"),
                ContextKind::ToolResult,
                50,
                ContextPriority::Low,
            ));
        }
        assert_eq!(session.current_usage().0, 500);

        let result = micro_compact(&mut session);
        assert!(matches!(result.level, CompressionLevel::Micro));
        assert_eq!(result.tokens_before, 500);
        assert!(result.items_evicted <= 5);
        assert!(!result.summary_generated);
        assert!(session.current_usage().0 < 500);
    }

    #[test]
    fn auto_compact_generates_summary() {
        let mut session = SessionMemory::new(1000);
        for i in 0..8 {
            session.push(make_item(
                &format!("m{i}"),
                ContextKind::UserMessage,
                100,
                ContextPriority::Normal,
            ));
        }

        let result = auto_compact(&mut session);
        assert!(matches!(result.level, CompressionLevel::Auto));
        assert!(result.summary_generated);
        assert!(result.items_evicted > 0);

        let window = session.render_window();
        let has_summary = window
            .iter()
            .any(|i| i.kind == ContextKind::CompressedSummary);
        assert!(has_summary);
    }

    #[test]
    fn full_compact_keeps_only_pinned_and_system() {
        let mut session = SessionMemory::new(1000);
        session.push(make_item(
            "sys",
            ContextKind::SystemInstruction,
            100,
            ContextPriority::Critical,
        ));
        session.push(make_item(
            "pin",
            ContextKind::UserMessage,
            100,
            ContextPriority::Pinned,
        ));
        session.push(make_item(
            "normal1",
            ContextKind::UserMessage,
            100,
            ContextPriority::Normal,
        ));
        session.push(make_item(
            "normal2",
            ContextKind::ToolResult,
            100,
            ContextPriority::Low,
        ));

        let result = full_compact(&mut session);
        assert!(matches!(result.level, CompressionLevel::Full));
        assert_eq!(result.items_evicted, 2);

        let remaining: Vec<&str> = session
            .render_window()
            .iter()
            .map(|i| i.id.as_str())
            .collect();
        assert!(remaining.contains(&"sys"));
        assert!(remaining.contains(&"pin"));
        assert!(!remaining.contains(&"normal1"));
        assert!(!remaining.contains(&"normal2"));
    }

    #[test]
    fn auto_compact_summary_content_has_evicted_info() {
        let mut session = SessionMemory::new(1000);
        session.push(make_item(
            "a",
            ContextKind::ToolResult,
            200,
            ContextPriority::Low,
        ));
        session.push(make_item(
            "b",
            ContextKind::UserMessage,
            200,
            ContextPriority::Normal,
        ));

        auto_compact(&mut session);

        let summary = session
            .render_window()
            .iter()
            .find(|i| i.kind == ContextKind::CompressedSummary)
            .map(|i| i.content.clone());
        assert!(summary.is_some());
        let text = summary.unwrap();
        assert!(text.contains("Compressed Context Summary"));
        assert!(text.contains("items compressed"));
    }

    #[test]
    fn micro_compact_on_empty_session_is_noop() {
        let mut session = SessionMemory::new(1000);
        let result = micro_compact(&mut session);
        assert_eq!(result.items_evicted, 0);
        assert_eq!(result.tokens_before, 0);
        assert_eq!(result.tokens_after, 0);
    }
}
