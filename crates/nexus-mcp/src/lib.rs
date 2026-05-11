//! nexus-mcp — MCP (Model Context Protocol) integration for Nexus.
//!
//! This crate makes Nexus both an **MCP client** (can connect to external MCP servers
//! and use their tools, resources, and prompts) and an **MCP server** (external systems
//! like Claude Desktop or Cursor can discover and invoke Nexus capabilities as MCP tools).
//!
//! # Architecture
//!
//! ```text
//!                        ┌─────────────────┐
//!                        │  McpServerRegistry │
//!                        │  (manages N servers)│
//!                        └────────┬────────┘
//!                                 │
//!               ┌─────────────────┼─────────────────┐
//!               │                 │                 │
//!        ┌──────┴──────┐   ┌──────┴──────┐   ┌──────┴──────┐
//!        │  McpClient  │   │  McpClient  │   │  McpClient  │
//!        │  (stdio)    │   │  (SSE)      │   │  (stdio)    │
//!        └──────┬──────┘   └──────┬──────┘   └──────┬──────┘
//!               │                 │                 │
//!        ┌──────┴──────┐   ┌──────┴──────┐   ┌──────┴──────┐
//!        │ filesystem  │   │  GitHub     │   │ Brave Search│
//!        │ MCP server  │   │  MCP server │   │ MCP server  │
//!        └─────────────┘   └─────────────┘   └─────────────┘
//! ```
//!
//! # Modules
//!
//! - [`types`] — Wire-format types for the MCP protocol (JSON-RPC, tools, resources, prompts).
//! - [`error`] — Unified error type for all MCP operations.
//! - [`client`] — MCP client for connecting to external servers.
//! - [`adapter`] — Wraps MCP tools as generic async callables for Nexus integration.
//! - [`registry`] — Manages multiple MCP server configurations and connections.
//! - [`server`] — Nexus as an MCP server, exposing its capabilities to external clients.

pub mod adapter;
pub mod client;
pub mod error;
pub mod proxy;
pub mod registry;
pub mod server;
pub mod types;
