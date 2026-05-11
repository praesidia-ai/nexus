//! `nexus acp` — expose Nexus over the Agent Client Protocol so
//! Zed, the JetBrains ACP Agent Registry, and every other ACP-aware
//! editor can drive Nexus directly.
//!
//! # Quickstart — Zed
//!
//! ```jsonc
//! // ~/.config/zed/settings.json
//! {
//!   "agent_servers": {
//!     "nexus": {
//!       "command": "nexus",
//!       "args": ["acp", "serve"]
//!     }
//!   }
//! }
//! ```
//!
//! # Quickstart — JetBrains ACP Registry
//!
//! Register a new agent with `command=nexus` and `args=acp serve`
//! in the ACP Agent Registry UI. The registry speaks the same
//! JSON-RPC-over-stdio transport.
//!
//! Logs go to stderr so the protocol stream on stdout stays clean.

use anyhow::Result;
use clap::Subcommand;

use crate::output::OutputFormat;

#[derive(Subcommand, Debug)]
pub enum AcpAction {
    /// Run Nexus as an ACP server on stdio. Editors launch this
    /// process — reads JSON-RPC from stdin, writes responses +
    /// `session/update` notifications to stdout.
    Serve,

    /// Print a ready-to-paste Zed `agent_servers` snippet with the
    /// current `nexus` binary path pre-filled.
    Config,
}

pub async fn run(_server: &str, _format: &OutputFormat, action: &AcpAction) -> Result<()> {
    match action {
        AcpAction::Serve => serve_stdio().await,
        AcpAction::Config => {
            print_zed_snippet();
            Ok(())
        }
    }
}

async fn serve_stdio() -> Result<()> {
    tracing::info!("nexus acp serve — starting stdio transport");
    let server = nexus_acp::AcpServer::new();
    server
        .serve_stdio()
        .await
        .map_err(|e| anyhow::anyhow!("acp server exited with error: {e}"))?;
    tracing::info!("nexus acp serve — stdio closed, exiting");
    Ok(())
}

fn print_zed_snippet() {
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "nexus".to_string());
    println!(
        "{}",
        serde_json::json!({
            "agent_servers": {
                "nexus": {
                    "command": exe,
                    "args": ["acp", "serve"]
                }
            }
        })
    );
}
