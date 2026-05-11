use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefetchPlan {
    pub selected_tools: Vec<String>,
    pub reason: String,
    pub estimated_tokens_saved: u64,
}

/// Analyze the task description and blueprint to pre-select relevant tools.
/// This reduces the tool list from potentially hundreds to just 10-20.
pub fn prefetch_tools(
    task_description: &str,
    available_tools: &[ToolDescriptor],
    max_tools: usize,
) -> PrefetchPlan {
    let keywords = extract_keywords(task_description);
    let mut scored: Vec<(&ToolDescriptor, f64)> = available_tools
        .iter()
        .map(|tool| {
            let score = compute_relevance(tool, &keywords);
            (tool, score)
        })
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(max_tools);

    let total_tools = available_tools.len();
    let selected: Vec<String> = scored.iter().map(|(t, _)| t.name.clone()).collect();
    let tokens_per_tool: usize = 200;
    let saved = ((total_tools - selected.len()) * tokens_per_tool) as u64;

    PrefetchPlan {
        selected_tools: selected,
        reason: format!(
            "Selected {}/{total_tools} tools based on task relevance",
            scored.len()
        ),
        estimated_tokens_saved: saved,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor {
    pub name: String,
    pub description: String,
    pub category: String,
    pub keywords: Vec<String>,
}

fn extract_keywords(text: &str) -> HashSet<String> {
    text.to_lowercase()
        .split_whitespace()
        .filter(|w| w.len() > 3)
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric())
                .to_string()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

fn compute_relevance(tool: &ToolDescriptor, keywords: &HashSet<String>) -> f64 {
    let mut score = 0.0;
    let tool_words: HashSet<String> = tool
        .name
        .to_lowercase()
        .split('_')
        .chain(tool.description.to_lowercase().split_whitespace())
        .chain(tool.keywords.iter().map(|s| s.as_str()))
        .map(|s| s.to_string())
        .collect();

    for kw in keywords {
        if tool_words.contains(kw) {
            score += 1.0;
        }
        for tw in &tool_words {
            if tw.contains(kw.as_str()) || kw.contains(tw.as_str()) {
                score += 0.3;
            }
        }
    }

    for tk in &tool.keywords {
        if keywords.contains(&tk.to_lowercase()) {
            score += 2.0;
        }
    }

    score
}
