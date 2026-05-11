//! Anthropic prompt-caching helpers (NEXUS_MASTER_PLAN §3).
//!
//! Anthropic only caches content *up to* an `ephemeral` breakpoint
//! marker, and you may place up to **4** breakpoints per request. The
//! usual wins come from three places:
//!
//! 1. **System prompt** — long, rarely changes → big cache hit
//! 2. **Tools JSON** — changes only on plugin install / redeploy →
//!    stable across an entire run
//! 3. **The first few user turns** — initial CLAUDE.md + memory
//!    slices → cached across follow-ups in the same swarm
//!
//! Anthropic bills cached tokens at a fraction of fresh-input cost
//! (currently 10 % for a 5-min TTL, ~25 % for the 1-hour TTL). We
//! default to 1-hour TTL on the system + tools breakpoints because
//! they very rarely mutate during a swarm run; the message breakpoint
//! uses the 5-min default so short interactive sessions still benefit.
//!
//! # Placement policy
//!
//! | Breakpoint # | Location                              | TTL     |
//! |--------------|---------------------------------------|---------|
//! | 1            | End of the `system` block             | 1 hour  |
//! | 2            | End of the `tools` array              | 1 hour  |
//! | 3            | After the first user message          | 5 min   |
//! | 4            | (reserved for future — e.g. memory)   | —       |
//!
//! The message-level breakpoint is placed on the content-block level
//! of the first user message because Anthropic requires the marker on
//! a content block, not on the message object itself.
//!
//! When the enum input is empty (e.g. no tools, no system), the
//! corresponding breakpoint is simply skipped. There is no way to
//! place more than 4 breakpoints; we stay well under the cap.
//!
//! The helpers here are pure transformations on `serde_json::Value` —
//! they do not make HTTP calls and do not require a client.

use serde_json::{json, Value};

/// Build the `system` field as an array of content blocks with an
/// ephemeral cache breakpoint on the last block. An empty prompt
/// returns an empty array — the caller's JSON request still carries
/// `"system": []` which Anthropic accepts.
pub fn system_blocks_with_breakpoint(system_prompt: &str) -> Value {
    if system_prompt.is_empty() {
        return json!([]);
    }
    json!([
        {
            "type": "text",
            "text": system_prompt,
            "cache_control": { "type": "ephemeral", "ttl": "1h" }
        }
    ])
}

/// Add an ephemeral cache breakpoint to the final tool in the tools
/// array. Anthropic caches everything up to and including the marked
/// tool, which is why it goes on the last one.
pub fn apply_breakpoints_to_tools(mut tools: Vec<Value>) -> Vec<Value> {
    if let Some(last) = tools.last_mut() {
        if let Some(obj) = last.as_object_mut() {
            obj.insert(
                "cache_control".to_string(),
                json!({ "type": "ephemeral", "ttl": "1h" }),
            );
        }
    }
    tools
}

/// Place a 5-min ephemeral breakpoint on the **last content block of
/// the first user message** — this caches the initial CLAUDE.md +
/// memory slices + opening user turn across a multi-turn swarm.
///
/// Messages after that are NOT cached (they change every turn).
pub fn apply_breakpoints_to_messages(mut messages: Vec<Value>) -> Vec<Value> {
    // Find the first user message in submission order.
    let Some(idx) = messages
        .iter()
        .position(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"))
    else {
        return messages;
    };

    let m = &mut messages[idx];

    // Normalise `content` to an array of blocks (accept the string
    // shorthand Anthropic allows for plain user turns).
    if let Some(content_str) = m.get("content").and_then(|c| c.as_str()) {
        let s = content_str.to_string();
        m["content"] = json!([{"type": "text", "text": s}]);
    }

    if let Some(arr) = m.get_mut("content").and_then(|c| c.as_array_mut()) {
        if let Some(last) = arr.last_mut() {
            if let Some(obj) = last.as_object_mut() {
                // Skip tool_result blocks — caching a tool_result
                // would cache a transient payload, not a stable
                // system context.
                let kind = obj.get("type").and_then(|t| t.as_str()).unwrap_or("");
                if kind != "tool_result" {
                    obj.insert(
                        "cache_control".to_string(),
                        json!({ "type": "ephemeral" }),
                    );
                }
            }
        }
    }

    messages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_system_returns_empty_array() {
        assert_eq!(system_blocks_with_breakpoint(""), json!([]));
    }

    #[test]
    fn system_block_carries_one_hour_ttl() {
        let v = system_blocks_with_breakpoint("you are Nova");
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["cache_control"]["type"], "ephemeral");
        assert_eq!(arr[0]["cache_control"]["ttl"], "1h");
    }

    #[test]
    fn tools_breakpoint_is_on_last_tool_only() {
        let tools = vec![
            json!({"name": "a"}),
            json!({"name": "b"}),
            json!({"name": "c"}),
        ];
        let out = apply_breakpoints_to_tools(tools);
        assert!(out[0].get("cache_control").is_none());
        assert!(out[1].get("cache_control").is_none());
        assert_eq!(out[2]["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn tools_breakpoint_noop_on_empty_list() {
        let out = apply_breakpoints_to_tools(Vec::new());
        assert!(out.is_empty());
    }

    #[test]
    fn messages_breakpoint_goes_on_first_user_turn() {
        let msgs = vec![
            json!({"role": "user", "content": "hi"}),
            json!({"role": "assistant", "content": "hello"}),
            json!({"role": "user", "content": "another"}),
        ];
        let out = apply_breakpoints_to_messages(msgs);
        let first_user = &out[0];
        let blocks = first_user["content"].as_array().unwrap();
        assert_eq!(blocks.last().unwrap()["cache_control"]["type"], "ephemeral");
        // Later user turns untouched.
        assert!(out[2]["content"].as_str().is_some()); // stays string
    }

    #[test]
    fn messages_breakpoint_skips_tool_result_block() {
        let msgs = vec![json!({
            "role": "user",
            "content": [{"type": "tool_result", "tool_use_id": "x", "content": "ok"}]
        })];
        let out = apply_breakpoints_to_messages(msgs);
        let block = &out[0]["content"][0];
        assert!(block.get("cache_control").is_none());
    }

    #[test]
    fn messages_breakpoint_noop_with_no_user_turn() {
        let msgs = vec![json!({"role": "assistant", "content": "hi"})];
        let out = apply_breakpoints_to_messages(msgs.clone());
        assert_eq!(out, msgs);
    }
}
