//! Inter-agent communication types.
//!
//! Agents within a team communicate via typed messages routed through a
//! [`TeamBus`] (defined in `nexus-http`). This module defines the message
//! envelope, content variants, targets, and priorities.

use serde::{Deserialize, Serialize};

/// A message sent between agents (or between a human and an agent).
///
/// Messages are immutable once created. The `in_reply_to` field enables
/// threaded conversations within the team bus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    /// Unique message identifier.
    pub id: String,
    /// Sender agent ID (or "human" for injected messages).
    pub from: String,
    /// Who this message is addressed to.
    pub to: MessageTarget,
    /// The message payload.
    pub content: MessageContent,
    /// ISO 8601 timestamp of when the message was created.
    pub timestamp: String,
    /// If this message is a reply, the ID of the message it replies to.
    pub in_reply_to: Option<String>,
    /// Priority level for delivery ordering.
    pub priority: MessagePriority,
}

/// Target for a message — who should receive it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "target", rename_all = "snake_case")]
pub enum MessageTarget {
    /// Send to a specific agent by ID.
    Direct {
        /// The recipient agent ID.
        member_id: String,
    },
    /// Broadcast to all team members.
    Broadcast,
    /// Send to all agents with a specific role name.
    Role {
        /// The role to target (e.g., "Reviewer", "Specialist(backend)").
        role: String,
    },
    /// Send to the human operator (HITL escalation).
    Human,
}

/// The content of an inter-agent message.
///
/// Each variant carries the structured data needed for that communication
/// pattern. The runtime logs all messages for observability.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessageContent {
    /// Assign a task to another agent.
    TaskAssignment {
        /// The task ID (matches workspace task board).
        task_id: String,
        /// Description of what needs to be done.
        description: String,
        /// Any constraints or requirements.
        constraints: Vec<String>,
    },
    /// Report that an assigned task has been completed.
    TaskCompletion {
        /// The task ID that was completed.
        task_id: String,
        /// Summary of what was done.
        summary: String,
        /// IDs of artifacts produced.
        artifact_ids: Vec<String>,
        /// Whether the task succeeded.
        success: bool,
    },
    /// Request a review of work or an artifact.
    Review {
        /// The artifact ID to review.
        artifact_id: String,
        /// What to focus on during review.
        focus_areas: Vec<String>,
    },
    /// A review verdict on submitted work.
    ReviewVerdict {
        /// The artifact ID that was reviewed.
        artifact_id: String,
        /// The verdict.
        verdict: Verdict,
        /// Detailed feedback comments.
        comments: Vec<String>,
        /// Suggested changes (if any).
        suggested_changes: Vec<String>,
    },
    /// A proposal for a decision or approach.
    Proposal {
        /// Short title for the proposal.
        title: String,
        /// Detailed description.
        description: String,
        /// Options being proposed (for multi-option proposals).
        options: Vec<String>,
    },
    /// A vote on a proposal.
    Vote {
        /// The proposal message ID being voted on.
        proposal_id: String,
        /// The vote cast.
        vote: VoteChoice,
        /// Rationale for the vote.
        rationale: String,
    },
    /// Escalation to a higher authority (lead, coordinator, or human).
    Escalation {
        /// What is being escalated.
        issue: String,
        /// Why it needs escalation.
        reason: String,
        /// Severity level.
        severity: EscalationSeverity,
    },
    /// Share context or information with other agents.
    ContextShare {
        /// Topic/key for the shared context.
        topic: String,
        /// The context content.
        content: String,
    },
    /// Broadcast a status update to the team.
    StatusUpdate {
        /// Current status summary.
        summary: String,
        /// Percentage complete (0-100), if applicable.
        percent_complete: Option<u8>,
    },
    /// Free-form discussion message.
    Discussion {
        /// The discussion content.
        content: String,
    },
}

/// Verdict on a review.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Work is approved as-is.
    Approved,
    /// Approved with minor changes suggested.
    ApprovedWithChanges,
    /// Changes required before approval.
    RequestChanges,
    /// Work is rejected outright.
    Rejected,
}

/// A vote on a proposal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VoteChoice {
    /// Approve the proposal.
    Approve,
    /// Reject the proposal.
    Reject,
    /// Abstain from voting.
    Abstain,
    /// Approve only if certain conditions are met.
    ConditionalApprove {
        /// The conditions that must be met.
        conditions: Vec<String>,
    },
}

/// Severity of an escalation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EscalationSeverity {
    /// Low — informational, no action needed urgently.
    Low,
    /// Medium — needs attention but not blocking.
    Medium,
    /// High — blocking progress, needs prompt resolution.
    High,
    /// Critical — team cannot continue without resolution.
    Critical,
}

/// Priority level for messages.
///
/// Higher priority messages are delivered first when an agent checks its inbox.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, PartialOrd)]
pub enum MessagePriority {
    /// Low priority — can be handled when convenient.
    Low,
    /// Normal priority — handle in order.
    Normal,
    /// High priority — handle as soon as possible.
    High,
    /// Critical — interrupt current work to handle.
    Critical,
}
