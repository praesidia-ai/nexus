use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The `nexus.toml` manifest — describes an agent package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentManifest {
    pub package: PackageMeta,
    #[serde(default)]
    pub agent: AgentMeta,
    #[serde(default)]
    pub capabilities: CapabilitySet,
    #[serde(default)]
    pub dependencies: HashMap<String, String>,
    #[serde(default)]
    pub tools: Vec<ToolDeclaration>,
    #[serde(default)]
    pub triggers: Vec<TriggerDeclaration>,
    #[serde(default)]
    pub resources: ResourceLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageMeta {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub authors: Vec<String>,
    pub license: Option<String>,
    pub homepage: Option<String>,
    pub repository: Option<String>,
    pub keywords: Vec<String>,
    pub categories: Vec<String>,
}

impl Default for PackageMeta {
    fn default() -> Self {
        Self {
            name: "unnamed-agent".into(),
            version: "0.1.0".into(),
            description: None,
            authors: vec![],
            license: None,
            homepage: None,
            repository: None,
            keywords: vec![],
            categories: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentMeta {
    /// Entry point — e.g. "agent.wasm" or "agent.json" (A2A card)
    pub entrypoint: Option<String>,
    /// Runtime: "wasm" | "a2a" | "native"
    pub runtime: Option<String>,
    /// System prompt or personality file
    pub system_prompt: Option<String>,
    /// Model hint (e.g. "gpt-4o", "claude-3-5-sonnet")
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CapabilitySet {
    pub network: bool,
    pub filesystem: bool,
    pub exec: bool,
    pub mcp: bool,
    pub a2a: bool,
    pub memory: bool,
    pub workflows: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDeclaration {
    pub name: String,
    pub description: String,
    /// JSON Schema for the tool's input parameters
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerDeclaration {
    /// cron | webhook | event
    pub kind: String,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub max_fuel: Option<u64>,
    pub max_memory_mb: Option<u32>,
    pub timeout_secs: Option<u64>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_fuel: Some(1_000_000_000),
            max_memory_mb: Some(128),
            timeout_secs: Some(60),
        }
    }
}

impl AgentManifest {
    pub fn from_toml(content: &str) -> anyhow::Result<Self> {
        let manifest: Self = toml::from_str(content)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn to_toml(&self) -> anyhow::Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }

    fn validate(&self) -> anyhow::Result<()> {
        semver::Version::parse(&self.package.version)?;
        if self.package.name.is_empty() {
            anyhow::bail!("package.name must not be empty");
        }
        Ok(())
    }

    pub fn default_template(name: &str) -> String {
        format!(
            r#"[package]
name = "{name}"
version = "0.1.0"
description = "An agent built with Nexus"
authors = ["your-name <you@example.com>"]
license = "Apache-2.0"
keywords = []
categories = []

[agent]
runtime = "wasm"
entrypoint = "agent.wasm"
system_prompt = "You are a helpful AI agent."

[capabilities]
network = false
filesystem = false
exec = false
mcp = true
a2a = true
memory = true
workflows = true

[resources]
max_fuel = 1000000000
max_memory_mb = 128
timeout_secs = 60
"#
        )
    }
}
