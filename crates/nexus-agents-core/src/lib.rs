//! nexus-agents-core — Agent runtime types and team definitions.
//!
//! This crate defines the public types and traits for:
//! - Agent definitions (identity, capabilities, execution modes)
//! - Agent events (SSE-streamable execution events)
//! - Tools (the trait agents use to interact with the world)
//! - Teams (multi-agent collaboration protocols)
//! - Communication (inter-agent messaging)
//! - Workspace (shared file system for teams)

pub mod definition;
pub mod events;
pub mod mini;
pub mod tools;
pub mod teams;
pub mod communication;
pub mod workspace;

#[cfg(test)]
mod tests;
