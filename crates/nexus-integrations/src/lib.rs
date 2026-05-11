//! nexus-integrations — First-party integrations for GitHub, databases, and notifications.
//!
//! This crate provides real HTTP-based integrations with external services:
//!
//! - **GitHub**: repository management, pull requests, issues via the GitHub REST API
//! - **Database**: health checks and schema introspection (Supabase REST, others via MCP)
//! - **Notifications**: Slack, Discord, and generic webhook delivery

pub mod config;
pub mod database;
pub mod error;
pub mod github;
pub mod notifications;

pub use error::IntegrationError;
