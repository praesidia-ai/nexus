//! `nexus mcp` — expose Nexus to MCP-compatible clients (Claude Desktop,
//! Claude Code, Cursor, Zed, etc.) over a JSON-RPC stdio transport.
//!
//! # Quickstart
//!
//! Paste this into `~/Library/Application Support/Claude/claude_desktop_config.json`
//! (or the equivalent on Linux / Windows):
//!
//! ```json
//! {
//!   "mcpServers": {
//!     "nexus": {
//!       "command": "nexus",
//!       "args": ["mcp", "serve"]
//!     }
//!   }
//! }
//! ```
//!
//! Restart Claude Desktop. `/mcp list` now shows `nexus`. Claude can list
//! your projects, read files, trigger oneshot runs, stream Agent TV
//! events, and fetch signed run certificates — all by talking to a
//! locally-running Nexus over stdio.

use anyhow::Result;
use clap::Subcommand;

use crate::output::OutputFormat;

#[derive(Subcommand, Debug)]
pub enum McpAction {
    /// Run Nexus as an MCP server on stdio. Point Claude Desktop / Claude
    /// Code / Cursor / Zed at this process to use Nexus tools from inside
    /// those clients.
    ///
    /// Expected to be launched by an MCP client — the process reads
    /// JSON-RPC requests on stdin and writes responses on stdout. Logs
    /// go to stderr so they don't corrupt the protocol stream.
    Serve,

    /// Print a ready-to-paste `claude_desktop_config.json` snippet that
    /// wires this `nexus` binary into Claude Desktop.
    Config,
}

pub async fn run(_server: &str, _format: &OutputFormat, action: &McpAction) -> Result<()> {
    match action {
        McpAction::Serve => serve_stdio().await,
        McpAction::Config => {
            print_claude_desktop_snippet();
            Ok(())
        }
    }
}

/// Spin up the in-process MCP server over stdio. Never returns until the
/// client closes stdin.
async fn serve_stdio() -> Result<()> {
    tracing::info!("nexus mcp serve — starting stdio transport");

    let server = nexus_mcp::server::NexusMcpServer::new();
    server
        .serve_stdio()
        .await
        .map_err(|e| anyhow::anyhow!("mcp server exited with error: {e}"))?;

    tracing::info!("nexus mcp serve — stdio closed, exiting");
    Ok(())
}

fn print_claude_desktop_snippet() {
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "nexus".to_string());

    println!(
        "{}",
        serde_json::json!({
            "mcpServers": {
                "nexus": {
                    "command": exe,
                    "args": ["mcp", "serve"]
                }
            }
        })
    );
}
