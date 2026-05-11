//! Product intelligence — personas, monetization, copy generation.
//!
//! The product layer takes analyzed intent and generates a product brief
//! including hero copy, feature descriptions, user personas, and
//! monetization strategy. This powers landing pages, onboarding flows,
//! and pricing pages in generated applications.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::intent::Intent;

/// A complete product brief generated from intent analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductBrief {
    /// The domain vertical (e.g., "fitness", "fintech").
    pub domain: String,
    /// Hero section headline.
    pub hero_headline: String,
    /// Hero section subheadline.
    pub hero_subheadline: String,
    /// Call-to-action button text.
    pub cta_text: String,
    /// Product features with descriptions and priorities.
    pub features: Vec<Feature>,
    /// Target user personas.
    pub personas: Vec<Persona>,
    /// Monetization strategy, if applicable.
    pub monetization: Option<MonetizationStrategy>,
}

/// A product feature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feature {
    /// Feature name.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Priority (lower is higher priority, starting from 1).
    pub priority: u32,
}

/// A target user persona.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Persona {
    /// Persona name (e.g., "Sarah the Startup Founder").
    pub name: String,
    /// Role or title.
    pub role: String,
    /// What this persona wants to achieve.
    pub goals: Vec<String>,
    /// Problems this persona faces.
    pub pain_points: Vec<String>,
}

/// Monetization strategy for the product.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonetizationStrategy {
    /// Model type (e.g., "freemium", "subscription", "one-time").
    pub model: String,
    /// Pricing tiers.
    pub tiers: Vec<PricingTier>,
}

/// A pricing tier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingTier {
    /// Tier name (e.g., "Free", "Pro", "Enterprise").
    pub name: String,
    /// Price display string (e.g., "$0/mo", "$29/mo").
    pub price: String,
    /// Features included in this tier.
    pub features: Vec<String>,
}

/// Trait for product intelligence.
///
/// Implementations generate product briefs from analyzed intent,
/// typically using LLM calls for copy generation and domain knowledge
/// for structure.
#[async_trait]
pub trait ProductIntelligence: Send + Sync {
    /// Generate a product brief from analyzed intent.
    async fn brief(
        &self,
        intent: &Intent,
    ) -> Result<ProductBrief, Box<dyn std::error::Error + Send + Sync>>;
}
