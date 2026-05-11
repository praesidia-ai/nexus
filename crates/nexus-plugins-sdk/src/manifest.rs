//! Plugin manifest — declares what a plugin is and what it can do.

use serde::{Deserialize, Serialize};

use super::capabilities::PluginCapability;
use super::hooks::HookPoint;

/// A plugin manifest declares the plugin's identity, capabilities, and hooks.
///
/// Plugin authors create a `nexus-plugin.json` manifest file that is
/// deserialized into this struct at load time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Unique plugin identifier (e.g., "com.example.my-plugin").
    pub id: String,
    /// Human-readable plugin name.
    pub name: String,
    /// Plugin version (semver).
    pub version: String,
    /// Short description of what the plugin does.
    pub description: String,
    /// Plugin author.
    pub author: String,
    /// License identifier (e.g., "MIT", "Apache-2.0").
    pub license: Option<String>,
    /// URL for plugin homepage or repository.
    pub homepage: Option<String>,
    /// Minimum Nexus version required.
    pub min_nexus_version: Option<String>,
    /// Capabilities this plugin provides.
    pub capabilities: Vec<PluginCapability>,
    /// Hook points this plugin listens to.
    pub hooks: Vec<HookRegistration>,
    /// Configuration schema (JSON Schema) for plugin settings.
    pub config_schema: Option<serde_json::Value>,
}

/// A hook registration — binds a plugin function to a hook point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookRegistration {
    /// Which hook point to bind to.
    pub hook: HookPoint,
    /// Priority (lower runs first, default 100).
    pub priority: u32,
    /// Optional condition for when to run this hook.
    pub condition: Option<String>,
}

impl Default for HookRegistration {
    fn default() -> Self {
        Self {
            hook: HookPoint::PostGenerate,
            priority: 100,
            condition: None,
        }
    }
}
