//! Unified Agent System — single runtime for all agent types.
//!
//! Replaces the fragmented agent ecosystem (project agents, coding agents,
//! wave pipeline, super agents, background agents) with a unified runtime.
//!
//! # Architecture
//!
//! ```text
//!   AgentDefinition          AgentContext
//!        |                       |
//!        +-------+-------+-------+
//!                |
//!         AgentRuntime::execute()
//!                |
//!        +-------+-------+
//!        |               |
//!   AgentEvent stream    JoinHandle<AgentResult>
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use nexus_http::agents::*;
//!
//! let def = factories::coding_agent("my-project");
//! let ctx = AgentContext { project_id: Some("p1".into()), ..Default::default() };
//! let runtime = AgentRuntime::new(app, registry);
//! let (mut rx, handle) = runtime.execute(&def, "Add a login page", ctx).await?;
//!
//! while let Some(event) = rx.recv().await {
//!     // stream to SSE
//! }
//! let result = handle.await??;
//! ```

pub mod definition;
pub mod events;
pub mod factories;
pub mod planner;
pub mod prompts;
pub mod runtime;
pub mod self_correction;
pub mod team_templates;
pub mod tools;

pub use definition::{AgentDefinition, ExecutionMode, MergeStrategy, ModelPreference, NotifyCondition};
pub use events::{AgentEvent, AgentResult, ToolCallRecord};
pub use planner::{AgentPlanner, ExecutionPlan};
pub use runtime::{AgentContext, AgentRuntime};
pub use self_correction::{CorrectionAction, SelfCorrectionTracker};
pub use tools::{Tool, ToolInput, ToolOutput, ToolRegistry};
