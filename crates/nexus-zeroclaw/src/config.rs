//! Configuration for the Nexus agent roster.
//!
//! `RosterConfig` is the single source of truth for how each agent is
//! initialised: which LLM provider to use, where the workspace lives, and
//! any per-agent overrides.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// RosterConfig
// ---------------------------------------------------------------------------

/// Top-level configuration for all agents in the roster.
///
/// Build from environment variables with [`RosterConfig::from_env`], or
/// construct directly for testing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RosterConfig {
    /// LLM provider name: `"anthropic"`, `"openai"`, or `"ollama"`.
    pub provider: String,

    /// API key for the chosen provider.  Required unless using Ollama.
    pub api_key: Option<String>,

    /// Override the default model for this provider.
    pub model: Option<String>,

    /// Base directory for agent workspaces.
    /// Each agent gets its own sub-directory: `{workspace_dir}/{agent_name}/`.
    pub workspace_dir: PathBuf,

    /// Maximum conversation history messages kept per agent session.
    #[serde(default = "default_history_size")]
    pub history_size: usize,

    /// Optional directory for SQLite-backed state persistence.
    /// When `Some`, agent conversation history and stats survive restarts.
    #[serde(default)]
    pub persistence_dir: Option<PathBuf>,
}

fn default_history_size() -> usize { 20 }

impl Default for RosterConfig {
    fn default() -> Self {
        Self {
            provider: "anthropic".into(),
            api_key: None,
            model: None,
            workspace_dir: PathBuf::from(".nexus/agents"),
            history_size: 20,
            persistence_dir: None,
        }
    }
}

impl RosterConfig {
    /// Build from standard environment variables.
    ///
    /// | Variable | Used for |
    /// |---|---|
    /// | `ANTHROPIC_API_KEY` | Anthropic provider (preferred) |
    /// | `OPENAI_API_KEY` | OpenAI provider |
    /// | `OLLAMA_BASE_URL` | Ollama (no key required) |
    /// | `NEXUS_WORKSPACE_DIR` | Override workspace path |
    /// | `NEXUS_MODEL` | Override model name |
    pub fn from_env() -> Self {
        let (provider, api_key) = if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            ("anthropic".into(), Some(key))
        } else if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            ("openai".into(), Some(key))
        } else {
            ("ollama".into(), None)
        };

        Self {
            provider,
            api_key,
            model: std::env::var("NEXUS_MODEL").ok(),
            workspace_dir: std::env::var("NEXUS_WORKSPACE_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from(".nexus/agents")),
            persistence_dir: std::env::var("NEXUS_PERSISTENCE_DIR")
                .ok()
                .map(PathBuf::from),
            ..Default::default()
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_values() {
        let cfg = RosterConfig::default();
        assert_eq!(cfg.provider, "anthropic");
        assert!(cfg.api_key.is_none());
        assert!(cfg.model.is_none());
        assert_eq!(cfg.workspace_dir, PathBuf::from(".nexus/agents"));
        assert_eq!(cfg.history_size, 20);
    }

    #[test]
    fn serialization_roundtrip() {
        let cfg = RosterConfig {
            provider: "openai".into(),
            api_key: Some("sk-test".into()),
            model: Some("gpt-4o".into()),
            workspace_dir: PathBuf::from("/tmp/agents"),
            history_size: 30,
            persistence_dir: Some(PathBuf::from("/tmp/persist")),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: RosterConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.provider, "openai");
        assert_eq!(parsed.api_key.as_deref(), Some("sk-test"));
        assert_eq!(parsed.model.as_deref(), Some("gpt-4o"));
        assert_eq!(parsed.workspace_dir, PathBuf::from("/tmp/agents"));
        assert_eq!(parsed.history_size, 30);
    }

    #[test]
    fn deserialization_defaults_history_size() {
        let json = r#"{"provider":"ollama","api_key":null,"model":null,"workspace_dir":"/w"}"#;
        let cfg: RosterConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.history_size, 20);
    }

    #[test]
    fn from_env_picks_anthropic_first() {
        std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-test");
        let cfg = RosterConfig::from_env();
        assert_eq!(cfg.provider, "anthropic");
        assert_eq!(cfg.api_key.as_deref(), Some("sk-ant-test"));
        std::env::remove_var("ANTHROPIC_API_KEY");
    }
}
