//! Parser for the `<nexus_state>…</nexus_state>` JSON blocks that the Nexus AI
//! embeds at the end of every response.
//!
//! The state block drives the live knowledge graph, materialization actions, and
//! phase transitions — all without the user seeing raw JSON.
//!
//! # Example
//! ```
//! use nexus_core::nexus_state::{parse_nexus_state, strip_nexus_state};
//!
//! let raw = "Here is the plan.\n<nexus_state>{\"phase\":2,\"phase_label\":\"Structuring\",\"new_items\":[],\"actions\":[],\"milestone\":null}</nexus_state>";
//! let state = parse_nexus_state(raw).unwrap();
//! assert_eq!(state.phase, 2);
//! let clean = strip_nexus_state(raw);
//! assert_eq!(clean, "Here is the plan.");
//! ```

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Full `nexus_state` payload emitted by the Nexus AI at the end of each turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusState {
    /// Current conversation phase (1–4).
    pub phase: u8,
    pub phase_label: String,
    #[serde(default)]
    pub new_items: Vec<NexusItem>,
    #[serde(default)]
    pub actions: Vec<NexusAction>,
    pub milestone: Option<String>,
}

/// A knowledge-graph item introduced during this turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusItem {
    /// One of: entity | relationship | rule | agent | integration | portal | database
    pub r#type: String,
    pub name: String,
    pub description: String,
    /// Single emoji picked by the AI.
    pub icon: String,
    /// When true the backend should trigger real materialization.
    #[serde(default)]
    pub materialize: bool,
}

/// A backend action to execute as a result of this turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusAction {
    /// One of: create_table | create_agent | create_integration | create_portal
    pub action: String,
    pub payload: serde_json::Value,
}

// Strongly-typed action payloads ----------------------------------------

/// Payload for `create_table` actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTablePayload {
    pub entity_name: String,
    pub fields: Vec<FieldSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSpec {
    pub name: String,
    pub r#type: String,
    #[serde(default)]
    pub primary_key: bool,
    #[serde(default)]
    pub not_null: bool,
}

/// Payload for `create_agent` actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAgentPayload {
    pub name: String,
    pub role: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default = "default_memory_type")]
    pub memory_type: String,
    pub system_prompt: String,
}

fn default_memory_type() -> String {
    "persistent".to_string()
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

const OPEN_TAG: &str = "<nexus_state>";
const CLOSE_TAG: &str = "</nexus_state>";

/// Extract and parse the first `<nexus_state>…</nexus_state>` block found in
/// `text`. Returns `None` if no block is present or JSON parsing fails.
pub fn parse_nexus_state(text: &str) -> Option<NexusState> {
    let json = extract_block(text)?;
    serde_json::from_str(json).ok()
}

/// Remove all `<nexus_state>…</nexus_state>` blocks and trim the result.
/// This gives the display-safe content to show the user.
pub fn strip_nexus_state(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        match rest.find(OPEN_TAG) {
            None => {
                out.push_str(rest);
                break;
            }
            Some(start) => {
                out.push_str(&rest[..start]);
                match rest[start..].find(CLOSE_TAG) {
                    None => break, // malformed — stop here
                    Some(rel_end) => {
                        rest = &rest[start + rel_end + CLOSE_TAG.len()..];
                    }
                }
            }
        }
    }
    out.trim().to_string()
}

fn extract_block(text: &str) -> Option<&str> {
    let start = text.find(OPEN_TAG)? + OPEN_TAG.len();
    let end = text[start..].find(CLOSE_TAG).map(|i| start + i)?;
    Some(text[start..end].trim())
}

// ---------------------------------------------------------------------------
// Action helpers
// ---------------------------------------------------------------------------

impl NexusAction {
    /// Try to deserialise the payload as a `CreateTablePayload`.
    pub fn as_create_table(&self) -> Option<CreateTablePayload> {
        if self.action == "create_table" {
            serde_json::from_value(self.payload.clone()).ok()
        } else {
            None
        }
    }

    /// Try to deserialise the payload as a `CreateAgentPayload`.
    pub fn as_create_agent(&self) -> Option<CreateAgentPayload> {
        if self.action == "create_agent" {
            serde_json::from_value(self.payload.clone()).ok()
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"Here is your plan.

<nexus_state>
{
  "phase": 2,
  "phase_label": "Structuring",
  "new_items": [
    {
      "type": "entity",
      "name": "Client",
      "description": "A person who hires you for freelance work",
      "icon": "👤",
      "materialize": false
    }
  ],
  "actions": [],
  "milestone": "Knowledge structure mapped"
}
</nexus_state>"#;

    #[test]
    fn parse_returns_state() {
        let s = parse_nexus_state(SAMPLE).expect("should parse");
        assert_eq!(s.phase, 2);
        assert_eq!(s.phase_label, "Structuring");
        assert_eq!(s.new_items.len(), 1);
        assert_eq!(s.new_items[0].name, "Client");
        assert_eq!(s.milestone, Some("Knowledge structure mapped".into()));
    }

    #[test]
    fn strip_removes_block() {
        let clean = strip_nexus_state(SAMPLE);
        assert_eq!(clean, "Here is your plan.");
        assert!(!clean.contains("<nexus_state>"));
    }

    #[test]
    fn strip_multiple_blocks() {
        let text = "A<nexus_state>{}</nexus_state>B<nexus_state>{}</nexus_state>C";
        assert_eq!(strip_nexus_state(text), "ABC");
    }

    #[test]
    fn parse_create_table_action() {
        let json = r#"{"phase":3,"phase_label":"Materialization","new_items":[],"actions":[{"action":"create_table","payload":{"entity_name":"Invoice","fields":[{"name":"id","type":"TEXT","primary_key":true},{"name":"amount","type":"REAL","not_null":true}]}}],"milestone":null}"#;
        let wrapped = format!("<nexus_state>{}</nexus_state>", json);
        let state = parse_nexus_state(&wrapped).unwrap();
        let ct = state.actions[0].as_create_table().unwrap();
        assert_eq!(ct.entity_name, "Invoice");
        assert_eq!(ct.fields.len(), 2);
    }

    #[test]
    fn parse_missing_block_returns_none() {
        assert!(parse_nexus_state("No state block here.").is_none());
    }

    #[test]
    fn strip_no_block_unchanged() {
        let text = "Just a normal message.";
        assert_eq!(strip_nexus_state(text), text);
    }
}
