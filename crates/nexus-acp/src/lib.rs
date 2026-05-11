//! Nexus ACP adapter — expose Nexus to any
//! [Agent Client Protocol](https://agentclientprotocol.com)-aware
//! editor (Zed, JetBrains ACP Agent Registry, and ~25 others) via
//! JSON-RPC over stdio.
//!
//! # Minimum-viable ACP surface
//!
//! ACP is a narrow JSON-RPC protocol — only a handful of methods are
//! required for a client to consider an agent "workable". This crate
//! implements the minimum subset:
//!
//!   - `initialize`                       → handshake + capabilities
//! - `initialized` — no-op notification
//! - `authenticate` — optional; we no-op since Nexus uses its own auth
//! - `session/new` — bind an ACP session to a Nexus project
//! - `session/prompt` — send a user prompt; we proxy to `/oneshot`
//!   on the configured `NEXUS_API_URL` and stream chunks back as
//!   `session/update` notifications
//! - `session/cancel` — cancel in-flight prompt
//!
//! Anything else returns a JSON-RPC `MethodNotFound` so the client
//! can fall back to an uncached path. The adapter intentionally
//! stays small — non-core methods (slash commands, file surface,
//! plan primitives) will land as the spec stabilises.
//!
//! # Transport
//!
//! ACP spec mandates **line-delimited** JSON-RPC 2.0 on stdin/stdout
//! — one complete JSON object per line. Diagnostics go to stderr so
//! the protocol stream stays clean.
//!
//! # How editors drive it
//!
//! The `nexus` CLI exposes `nexus acp serve`. Point Zed or the
//! JetBrains ACP Agent Registry at that command. See
//! `docs/NEXUS_MASTER_PLAN.md` §5 for the full positioning rationale.

pub mod server;
pub mod types;

pub use server::AcpServer;
pub use types::{AcpError, JsonRpcMessage};
