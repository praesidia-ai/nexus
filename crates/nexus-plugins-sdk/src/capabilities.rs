//! Plugin capabilities — declares what a plugin can provide.

use serde::{Deserialize, Serialize};

/// Capabilities a plugin can provide.
///
/// Capabilities are declared in the plugin manifest and used by Nexus
/// to understand what a plugin offers without loading its code.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PluginCapability {
    /// Provides a domain pack (vertical knowledge).
    DomainPack {
        /// Domain name.
        domain: String,
    },
    /// Provides one or more agent tools.
    Tools {
        /// Tool names provided.
        tool_names: Vec<String>,
    },
    /// Provides a code generator for a specific framework/language.
    Generator {
        /// Framework or language this generator targets.
        target: String,
    },
    /// Provides a quality scoring dimension.
    QualityDimension {
        /// Dimension name.
        dimension: String,
    },
    /// Provides a deployment target.
    DeployTarget {
        /// Target platform name (e.g., "vercel", "aws", "cloudflare").
        platform: String,
    },
    /// Provides an LLM provider integration.
    LlmProvider {
        /// Provider name.
        provider: String,
    },
    /// Provides a UI theme.
    Theme {
        /// Theme name.
        theme_name: String,
    },
    /// Custom capability not covered by built-in types.
    Custom {
        /// Capability name.
        name: String,
        /// Capability description.
        description: String,
    },
}
