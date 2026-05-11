//! Inter-agent message types.
//!
//! `AgentMessage` is the envelope that travels between agents in a workflow.
//! It carries the content, metadata about who sent it, and optional context
//! snippets that the receiving agent should be aware of.

use serde::{Deserialize, Serialize};

use crate::roster::AgentName;

// ---------------------------------------------------------------------------
// AgentMessage
// ---------------------------------------------------------------------------

/// A message sent to (or received from) a named agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    /// The agent that produced this message (`None` for user-originated messages).
    pub from: Option<AgentName>,

    /// The intended recipient.
    pub to: AgentName,

    /// Plain-text (or Markdown) content.
    pub content: String,

    /// Short label describing what kind of work this message requests or reports.
    pub kind: MessageKind,

    /// Optional structured context injected before `content` in the LLM prompt
    /// (e.g. previous step outputs, project metadata).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context: Vec<ContextSnippet>,
}

impl AgentMessage {
    /// Convenience constructor for a user → agent task request.
    pub fn task(to: AgentName, content: impl Into<String>) -> Self {
        Self {
            from: None,
            to,
            content: content.into(),
            kind: MessageKind::Task,
            context: vec![],
        }
    }

    /// Build the full prompt string the agent will receive.
    ///
    /// Context snippets are prepended as labelled sections, then the
    /// message `content` follows.
    pub fn build_prompt(&self) -> String {
        if self.context.is_empty() {
            return self.content.clone();
        }

        let mut out = String::new();
        for snip in &self.context {
            out.push_str(&format!("### {}\n{}\n\n", snip.label, snip.content));
        }
        out.push_str(&self.content);
        out
    }
}

// ---------------------------------------------------------------------------
// MessageKind
// ---------------------------------------------------------------------------

/// Categorises what an `AgentMessage` is asking the recipient to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    /// A new task to perform end-to-end.
    Task,
    /// Continuation of a previous turn.
    Followup,
    /// The sender is handing off an artefact for review/validation.
    Review,
    /// Summary of completed work passed downstream.
    Handoff,
    /// An informational message requiring no action.
    Info,
}

// ---------------------------------------------------------------------------
// ContextSnippet
// ---------------------------------------------------------------------------

/// A labelled block of text injected as context before the agent's prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSnippet {
    /// Section heading, e.g. `"Research findings"`, `"Architecture design"`.
    pub label: String,
    /// Content of the snippet (plain text or Markdown).
    pub content: String,
}

impl ContextSnippet {
    pub fn new(label: impl Into<String>, content: impl Into<String>) -> Self {
        Self { label: label.into(), content: content.into() }
    }
}

// ---------------------------------------------------------------------------
// AgentReply
// ---------------------------------------------------------------------------

/// The response produced by an agent after processing an `AgentMessage`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentReply {
    pub from: AgentName,
    pub content: String,
    /// Any artefacts (file paths, URLs) produced during the turn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artefacts: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_message_constructor() {
        let msg = AgentMessage::task(AgentName::Nova, "Build an API");
        assert!(msg.from.is_none());
        assert_eq!(msg.to, AgentName::Nova);
        assert_eq!(msg.content, "Build an API");
        assert_eq!(msg.kind, MessageKind::Task);
        assert!(msg.context.is_empty());
    }

    #[test]
    fn build_prompt_no_context() {
        let msg = AgentMessage::task(AgentName::Atlas, "Deploy to AWS");
        assert_eq!(msg.build_prompt(), "Deploy to AWS");
    }

    #[test]
    fn build_prompt_with_context_snippets() {
        let msg = AgentMessage {
            from: Some(AgentName::Leo),
            to: AgentName::Nova,
            content: "Implement the feature".into(),
            kind: MessageKind::Handoff,
            context: vec![
                ContextSnippet::new("Requirements", "Build a REST API"),
                ContextSnippet::new("Architecture", "Use Axum + PostgreSQL"),
            ],
        };
        let prompt = msg.build_prompt();
        assert!(prompt.starts_with("### Requirements\n"));
        assert!(prompt.contains("### Architecture\n"));
        assert!(prompt.ends_with("Implement the feature"));
    }

    #[test]
    fn message_kind_serialization() {
        assert_eq!(serde_json::to_string(&MessageKind::Task).unwrap(), "\"task\"");
        assert_eq!(serde_json::to_string(&MessageKind::Followup).unwrap(), "\"followup\"");
        assert_eq!(serde_json::to_string(&MessageKind::Review).unwrap(), "\"review\"");
        assert_eq!(serde_json::to_string(&MessageKind::Handoff).unwrap(), "\"handoff\"");
        assert_eq!(serde_json::to_string(&MessageKind::Info).unwrap(), "\"info\"");
    }

    #[test]
    fn message_kind_deserialization() {
        let kinds = [
            MessageKind::Task, MessageKind::Followup,
            MessageKind::Review, MessageKind::Handoff, MessageKind::Info,
        ];
        for k in &kinds {
            let json = serde_json::to_string(k).unwrap();
            let parsed: MessageKind = serde_json::from_str(&json).unwrap();
            assert_eq!(*k, parsed);
        }
    }

    #[test]
    fn context_snippet_new() {
        let snip = ContextSnippet::new("Label", "Content");
        assert_eq!(snip.label, "Label");
        assert_eq!(snip.content, "Content");
    }

    #[test]
    fn agent_message_serialization_roundtrip() {
        let msg = AgentMessage {
            from: Some(AgentName::Kai),
            to: AgentName::Luna,
            content: "Research findings".into(),
            kind: MessageKind::Handoff,
            context: vec![ContextSnippet::new("Data", "Some data")],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.from, Some(AgentName::Kai));
        assert_eq!(parsed.to, AgentName::Luna);
        assert_eq!(parsed.kind, MessageKind::Handoff);
        assert_eq!(parsed.context.len(), 1);
    }

    #[test]
    fn agent_reply_serialization_omits_empty_artefacts() {
        let reply = AgentReply {
            from: AgentName::Nova,
            content: "Done".into(),
            artefacts: vec![],
        };
        let json = serde_json::to_string(&reply).unwrap();
        assert!(!json.contains("artefacts"));
    }

    #[test]
    fn agent_reply_with_artefacts() {
        let reply = AgentReply {
            from: AgentName::Rex,
            content: "Deployed".into(),
            artefacts: vec!["Dockerfile".into(), "docker-compose.yml".into()],
        };
        let json = serde_json::to_string(&reply).unwrap();
        assert!(json.contains("artefacts"));
        let parsed: AgentReply = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.artefacts.len(), 2);
    }

    #[test]
    fn agent_message_context_default_empty_on_deserialize() {
        let json = r#"{"from":null,"to":"nova","content":"test","kind":"task"}"#;
        let msg: AgentMessage = serde_json::from_str(json).unwrap();
        assert!(msg.context.is_empty());
    }
}
