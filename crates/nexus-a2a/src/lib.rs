//! nexus-a2a — Agent-to-Agent (A2A) protocol implementation for Nexus.
//!
//! The A2A protocol (Google / Linux Foundation, Apache 2.0) is the emerging
//! open standard for agent-to-agent interoperability, complementing MCP
//! (which handles agent-to-tool communication).
//!
//! # Roles
//!
//! - **A2A Server**: Nexus exposes an A2A endpoint at `POST /a2a` so external
//!   agents can delegate tasks to Nexus agents. An Agent Card is published at
//!   `GET /.well-known/agent.json`.
//!
//! - **A2A Client**: Nexus can discover and call remote A2A agents, delegating
//!   work via `tasks/send`. This powers cross-instance federation.
//!
//! # Protocol summary
//!
//! ```text
//! External agent                         Nexus A2A server
//!       │                                        │
//!       │  GET /.well-known/agent.json           │
//!       │ ─────────────────────────────────────► │  AgentCard (discover)
//!       │                                        │
//!       │  POST /a2a { method:"tasks/send", ...} │
//!       │ ─────────────────────────────────────► │
//!       │                                        │  route to agent
//!       │  { result: Task }                      │  execute
//!       │ ◄───────────────────────────────────── │
//! ```

pub mod client;
pub mod error;
pub mod federation;
pub mod nexus_handler;
pub mod registry;
pub mod server;
pub mod types;

pub use client::A2aClient;
pub use federation::{FederationManager, NexusPeer};
pub use error::A2aError;
pub use nexus_handler::NexusTaskHandler;
pub use registry::AgentCardRegistry;
pub use server::{A2aServer, TaskHandler};
pub use types::{
    AgentCard, AgentCapabilities, AgentProvider, AgentSkill, Artifact, AuthScheme,
    Message, MessageRole, Part, Task, TaskEvent, TaskId, TaskSendParams, TaskState,
    TaskStatus,
};
