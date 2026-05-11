//! Loadable domain knowledge packs.
//!
//! A domain pack provides vertical knowledge for a specific domain
//! (e.g., "ecommerce", "fitness", "fintech"). When the intent engine
//! detects a matching domain, the pack contributes default entities,
//! pages, hidden requirements, and UI style hints.
//!
//! Domain packs can be shipped with Nexus or loaded from plugins.

use serde::{Deserialize, Serialize};

use super::intent::{EntitySuggestion, PageSuggestion, Requirement, UiStyle};

/// A domain pack provides vertical knowledge for a specific domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainPack {
    /// Domain name (e.g., "ecommerce", "fitness").
    pub domain: String,
    /// Keywords that trigger this domain pack.
    pub triggers: Vec<String>,
    /// Default entities for this domain.
    pub default_entities: Vec<EntitySuggestion>,
    /// Default pages for this domain.
    pub default_pages: Vec<PageSuggestion>,
    /// Hidden requirements implied by this domain.
    pub hidden_requirements: Vec<Requirement>,
    /// Suggested UI style for this domain.
    pub ui_style_hint: Option<UiStyle>,
}
