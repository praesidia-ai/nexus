//! Agent tool system — registry and trait definitions.
//!
//! Tools are the actions agents can take: reading files, writing code,
//! running commands, searching, etc. Each tool implements the [`Tool`] trait
//! and is registered in a [`ToolRegistry`] at startup.

pub mod registry;

pub use registry::{Tool, ToolInput, ToolOutput, ToolRegistry};
