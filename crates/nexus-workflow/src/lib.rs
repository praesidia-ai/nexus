//! `nexus-workflow` — Persistent, resumable workflow execution engine.
//!
//! Features:
//!
//! - **Persistence**: all workflow state is stored in SQLite and survives restarts.
//! - **Resumability**: a crashed workflow picks up from the last completed step.
//! - **Parallel execution**: steps without dependencies run concurrently via
//!   `JoinSet`; explicit parallel groups for fine-grained control.
//! - **Conditional branching**: `StepCondition` enum with `OnSuccess`, `OnFailure`,
//!   and expression-based conditions.
//! - **Loop detection**: cycle detection during validation; `max_iterations` per
//!   step to prevent runaway execution.
//! - **Versioning & templates**: `version` field on definitions; register templates
//!   and instantiate with parameter substitution.
//! - **Step timeout**: `timeout_ms` per step with automatic `TimedOut` status.
//! - **Human-in-the-loop**: `HumanApproval` steps pause the workflow until an
//!   operator calls `approve()`.
//! - **Retry with backoff**: each step can specify a retry policy.
//!
//! # Quick start
//!
//! ```rust,ignore
//! use nexus_workflow::{WorkflowEngine, definition::*};
//!
//! let engine = WorkflowEngine::new(std::path::Path::new("/tmp/nexus-wf"))?;
//! let def = WorkflowDefinition { /* ... */ };
//! let instance = engine.start(&def, serde_json::json!({})).await?;
//! ```

pub mod definition;
pub mod engine;
pub mod error;
pub mod store;

pub use engine::WorkflowEngine;
pub use error::WorkflowError;
