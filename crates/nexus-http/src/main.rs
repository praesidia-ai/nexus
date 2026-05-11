//! `nexus-server` binary — start the Nexus HTTP API server.
//!
//! Configuration via environment variables:
//!   NEXUS_DATA_DIR   — where to store nexus.db + project files (default: ~/.nexus)
//!   NEXUS_PORT       — port to listen on (default: 8080)
//!   OPENAI_API_KEY   — required for LLM chat
//!   ANTHROPIC_API_KEY — optional (for Anthropic model support)
//!   NEXUS_MODEL      — LLM model (default: gpt-4o)

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Tracing (with optional OTel layer). Emit JSON-structured logs when
    // `NEXUS_LOG_FORMAT=json` (or `NODE_ENV=production`) so log aggregators
    // like Loki / Datadog / CloudWatch can parse individual fields. Default
    // is pretty / compact for local dev.
    let otel_guard = nexus_http::telemetry::init_otel();
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());
    let use_json = std::env::var("NEXUS_LOG_FORMAT")
        .map(|v| v.eq_ignore_ascii_case("json"))
        .unwrap_or(false)
        || std::env::var("NODE_ENV")
            .map(|v| v.eq_ignore_ascii_case("production"))
            .unwrap_or(false);
    if use_json {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer().json().with_target(true))
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer())
            .init();
    }

    // Deployment profile (cloud / edge / micro)
    nexus_http::edge::init_profile(nexus_http::edge::ResourceProfile::from_env());

    // Data directory
    let data_dir = std::env::var("NEXUS_DATA_DIR").unwrap_or_else(|_| {
        dirs_home()
            .map(|h| h.join(".nexus").to_string_lossy().into_owned())
            .unwrap_or_else(|| ".nexus".to_string())
    });

    // Port — fail fast on a malformed value so the operator notices instead
    // of silently binding to the default.
    let port: u16 = match std::env::var("NEXUS_PORT") {
        Ok(raw) => raw.parse().map_err(|e| {
            anyhow::anyhow!(
                "NEXUS_PORT is not a valid u16 ({raw:?}): {e}. Set it to a number in 1..=65535 or unset it to use 8080."
            )
        })?,
        Err(_) => 8080,
    };

    let addr = format!("0.0.0.0:{}", port);

    // Init state
    let state = nexus_http::AppState::init(&data_dir).await?;

    // Serve — translate AddrInUse into an actionable error for the operator.
    if let Err(e) = nexus_http::serve(state, &addr).await {
        if let Some(io_err) = e.downcast_ref::<std::io::Error>() {
            if io_err.kind() == std::io::ErrorKind::AddrInUse {
                return Err(anyhow::anyhow!(
                    "Port {port} is already in use. Set NEXUS_PORT to a free port or stop the process holding it."
                ));
            }
        }
        return Err(e);
    }

    // Flush OTel telemetry on shutdown
    drop(otel_guard);
    Ok(())
}

fn dirs_home() -> Option<std::path::PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(std::path::PathBuf::from)
}
