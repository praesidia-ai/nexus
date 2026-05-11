//! CLAUDE.md injection helpers.
//!
//! Every Nexus-generated project carries a `CLAUDE.md` at its root
//! (see `claude_md.rs`). Any LLM call made *on behalf of that project*
//! gets the CLAUDE.md prepended to its system prompt so Claude
//! inherits project memory automatically — matching the convention
//! Claude Code uses. Matches `NEXUS_MASTER_PLAN.md` §3.
//!
//! This module is a single function: pass a `messages` vector (the
//! OpenAI-style array that `llm_client::call_llm_with_tools` accepts)
//! plus the project's data-dir root, and get back an augmented
//! vector whose first `system` message has CLAUDE.md prepended. If
//! there's no `system` message the helper inserts one.
//!
//! The injector is idempotent — a system message that already begins
//! with the `<nexus:claude_md>` marker is left alone, so chain-of-
//! tool-calls don't accumulate duplicate project memory.

use std::path::Path;

use serde_json::{json, Value};

use crate::claude_md::{self, as_system_prompt};

/// Marker prefix the injector writes into the system prompt. Any
/// subsequent injection sees this and no-ops.
pub const MARKER: &str = "<nexus:claude_md>";

/// Inject the CLAUDE.md for `project_root` into `messages[*].system`.
///
/// - If the project has no CLAUDE.md on disk, a bootstrap document
///   (the canonical 5-section template) is used rather than nothing,
///   because Claude responds measurably better with the header
///   structure in place even when the sections are empty.
/// - On any filesystem error the messages are returned unchanged —
///   the LLM call path must never be blocked by a missing CLAUDE.md.
pub async fn inject_for_project(
    messages: Vec<Value>,
    project_root: &Path,
    project_name: &str,
) -> Vec<Value> {
    let md = match claude_md::load(project_root, project_name).await {
        Ok(md) => md,
        Err(e) => {
            tracing::warn!(error = %e, "CLAUDE.md load failed, leaving messages unchanged");
            return messages;
        }
    };
    let preamble = as_system_prompt(&md);
    merge_system_prompt(messages, &preamble)
}

/// Core merge primitive. Kept public + pure so it's unit-testable
/// without touching the filesystem.
pub fn merge_system_prompt(mut messages: Vec<Value>, preamble: &str) -> Vec<Value> {
    if preamble.is_empty() {
        return messages;
    }

    let idx = messages
        .iter()
        .position(|m| m.get("role").and_then(|r| r.as_str()) == Some("system"));

    match idx {
        Some(i) => {
            let existing = messages[i]
                .get("content")
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .to_string();
            if existing.starts_with(MARKER) {
                // Already injected upstream — don't stack.
                return messages;
            }
            let merged = if existing.is_empty() {
                preamble.to_string()
            } else {
                format!("{preamble}\n\n{existing}")
            };
            messages[i] = json!({ "role": "system", "content": merged });
            messages
        }
        None => {
            // Insert a fresh system message at the front.
            let mut out = Vec::with_capacity(messages.len() + 1);
            out.push(json!({ "role": "system", "content": preamble }));
            out.extend(messages);
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_preamble_is_noop() {
        let msgs = vec![json!({"role": "user", "content": "hi"})];
        assert_eq!(merge_system_prompt(msgs.clone(), ""), msgs);
    }

    #[test]
    fn inserts_system_when_missing() {
        let msgs = vec![json!({"role": "user", "content": "hi"})];
        let out = merge_system_prompt(msgs, "<nexus:claude_md>\n# P\n</nexus:claude_md>");
        assert_eq!(out.len(), 2);
        assert_eq!(out[0]["role"], "system");
        assert!(out[0]["content"]
            .as_str()
            .unwrap()
            .starts_with("<nexus:claude_md>"));
    }

    #[test]
    fn prepends_to_existing_system() {
        let msgs = vec![
            json!({"role": "system", "content": "You are Nova."}),
            json!({"role": "user", "content": "hi"}),
        ];
        let out = merge_system_prompt(msgs, "<nexus:claude_md>\n# P\n</nexus:claude_md>");
        assert_eq!(out.len(), 2);
        let sys = out[0]["content"].as_str().unwrap();
        assert!(sys.starts_with("<nexus:claude_md>"));
        assert!(sys.contains("You are Nova."));
    }

    #[test]
    fn noop_when_marker_already_present() {
        let msgs = vec![
            json!({"role": "system", "content": "<nexus:claude_md>\nold\n</nexus:claude_md>\n\nYou are Nova."}),
        ];
        let before = msgs.clone();
        let out = merge_system_prompt(msgs, "<nexus:claude_md>\nnew\n</nexus:claude_md>");
        assert_eq!(out, before);
    }

    #[tokio::test]
    async fn inject_for_project_uses_bootstrap_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let msgs = vec![json!({"role": "user", "content": "hi"})];
        let out = inject_for_project(msgs, dir.path(), "Demo").await;
        let sys = out[0]["content"].as_str().unwrap();
        assert!(sys.contains("Demo"));
        assert!(sys.contains("Architecture"));
    }
}
