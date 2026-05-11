//! Personality System — affects design, agent prompts, and product decisions.
//!
//! Three personality profiles:
//! - **Startup**: Move fast, ship bold, conversational tone
//! - **Enterprise**: Reliable, polished, professional tone
//! - **Creative**: Expressive, unique, artistic tone
//!
//! The personality is either:
//! - Explicitly set by the user
//! - Auto-detected from the project description
//!
//! It affects:
//! - Design system token selection (color palette, typography, spacing)
//! - Agent system prompts (tone, verbosity, formality)
//! - Product decisions (feature prioritization, content style)

use serde::{Deserialize, Serialize};

use crate::intent_engine::{AppType, UiStyle};

// ---------------------------------------------------------------------------
// Personality Profile
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum Personality {
    #[default]
    Startup,
    Enterprise,
    Creative,
}


/// Full personality configuration derived from the profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalityConfig {
    pub profile: Personality,
    pub tone: ToneConfig,
    pub design: DesignInfluence,
    pub product: ProductInfluence,
}

/// How agents should communicate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToneConfig {
    /// System prompt prefix injected into all agent prompts.
    pub agent_prompt_prefix: String,
    /// How chatbot agents greet users.
    pub greeting_style: String,
    /// Error message style.
    pub error_style: String,
    /// CTA button copy style hint.
    pub cta_style: String,
}

/// How the design system is influenced.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesignInfluence {
    /// Preferred UI style override (or None to keep intent-derived).
    pub preferred_ui_style: Option<UiStyle>,
    /// Animation intensity: "none", "subtle", "moderate", "expressive".
    pub animation_intensity: String,
    /// Border radius preference: "sharp", "rounded", "pill".
    pub border_style: String,
    /// Color saturation hint: "muted", "balanced", "vivid".
    pub color_saturation: String,
    /// Typography weight preference: "light", "regular", "bold".
    pub type_weight: String,
}

/// How product decisions are influenced.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductInfluence {
    /// Whether to include social proof sections by default.
    pub include_social_proof: bool,
    /// Whether to include pricing sections by default.
    pub include_pricing: bool,
    /// Whether to add onboarding flows for new users.
    pub include_onboarding: bool,
    /// Content verbosity: "minimal", "balanced", "detailed".
    pub content_verbosity: String,
    /// Whether to use emoji in UI copy.
    pub use_emoji: bool,
}

// ---------------------------------------------------------------------------
// Profile → Config
// ---------------------------------------------------------------------------

/// Generate the full personality configuration for a profile.
pub fn configure(profile: Personality) -> PersonalityConfig {
    match profile {
        Personality::Startup => PersonalityConfig {
            profile,
            tone: ToneConfig {
                agent_prompt_prefix: "You are a friendly, helpful AI assistant. \
                    Be conversational, encouraging, and concise. \
                    Use casual language but stay professional. \
                    Avoid jargon — explain things simply."
                    .into(),
                greeting_style: "Hey! How can I help you today?".into(),
                error_style: "Oops, something went wrong. Let me try to fix that.".into(),
                cta_style: "action-oriented and energetic".into(),
            },
            design: DesignInfluence {
                preferred_ui_style: None, // keep intent-derived
                animation_intensity: "moderate".into(),
                border_style: "rounded".into(),
                color_saturation: "vivid".into(),
                type_weight: "regular".into(),
            },
            product: ProductInfluence {
                include_social_proof: true,
                include_pricing: true,
                include_onboarding: false,
                content_verbosity: "minimal".into(),
                use_emoji: true,
            },
        },

        Personality::Enterprise => PersonalityConfig {
            profile,
            tone: ToneConfig {
                agent_prompt_prefix: "You are a professional AI assistant for a business application. \
                    Be precise, reliable, and thorough. Use formal but approachable language. \
                    Always provide structured, actionable responses. \
                    Cite specifics when possible."
                    .into(),
                greeting_style: "Welcome. How can I assist you?".into(),
                error_style: "We encountered an issue. Our team has been notified and is investigating.".into(),
                cta_style: "professional and trust-building".into(),
            },
            design: DesignInfluence {
                preferred_ui_style: Some(UiStyle::Corporate),
                animation_intensity: "subtle".into(),
                border_style: "sharp".into(),
                color_saturation: "muted".into(),
                type_weight: "regular".into(),
            },
            product: ProductInfluence {
                include_social_proof: true,
                include_pricing: true,
                include_onboarding: true,
                content_verbosity: "detailed".into(),
                use_emoji: false,
            },
        },

        Personality::Creative => PersonalityConfig {
            profile,
            tone: ToneConfig {
                agent_prompt_prefix: "You are an expressive, creative AI assistant. \
                    Be playful, imaginative, and inspiring. Use vivid language. \
                    Surprise users with unexpected but delightful responses. \
                    Think like a designer — visual metaphors are welcome."
                    .into(),
                greeting_style: "Welcome to something special. What shall we create?".into(),
                error_style: "Something unexpected happened — every masterpiece has rough drafts!".into(),
                cta_style: "expressive and evocative".into(),
            },
            design: DesignInfluence {
                preferred_ui_style: None,
                animation_intensity: "expressive".into(),
                border_style: "pill".into(),
                color_saturation: "vivid".into(),
                type_weight: "bold".into(),
            },
            product: ProductInfluence {
                include_social_proof: false,
                include_pricing: false,
                include_onboarding: false,
                content_verbosity: "minimal".into(),
                use_emoji: true,
            },
        },
    }
}

// ---------------------------------------------------------------------------
// Auto-detection from description
// ---------------------------------------------------------------------------

/// Detect personality from a user's project description.
pub fn detect_personality(description: &str, app_type: &AppType) -> Personality {
    let lower = description.to_lowercase();

    // Explicit enterprise signals
    if lower.contains("enterprise") || lower.contains("corporate") || lower.contains("compliance")
        || lower.contains("internal tool") || lower.contains("crm")
        || lower.contains("b2b") || lower.contains("professional")
    {
        return Personality::Enterprise;
    }

    // Explicit creative signals
    if lower.contains("creative") || lower.contains("artistic") || lower.contains("portfolio")
        || lower.contains("gallery") || lower.contains("design studio")
        || lower.contains("photography") || lower.contains("music")
        || lower.contains("fashion") || lower.contains("luxury")
    {
        return Personality::Creative;
    }

    // Explicit startup signals
    if lower.contains("startup") || lower.contains("mvp") || lower.contains("saas")
        || lower.contains("launch") || lower.contains("side project")
    {
        return Personality::Startup;
    }

    // Infer from app type
    match app_type {
        AppType::Crm | AppType::InternalTool | AppType::Dashboard => Personality::Enterprise,
        AppType::Portfolio | AppType::Blog => Personality::Creative,
        AppType::LandingPage | AppType::SaasApp | AppType::ECommerce
        | AppType::Marketplace | AppType::MobileApp | AppType::ApiOnly
        | AppType::Custom => Personality::Startup,
    }
}

/// Merge personality into an agent's system prompt.
pub fn apply_to_agent_prompt(base_prompt: &str, config: &PersonalityConfig) -> String {
    format!("{}\n\n{}", config.tone.agent_prompt_prefix, base_prompt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_enterprise_personality() {
        assert_eq!(
            detect_personality("Build an enterprise CRM for our sales team", &AppType::Crm),
            Personality::Enterprise
        );
    }

    #[test]
    fn detects_creative_personality() {
        assert_eq!(
            detect_personality("Create a photography portfolio", &AppType::Portfolio),
            Personality::Creative
        );
    }

    #[test]
    fn startup_is_default_for_saas() {
        assert_eq!(
            detect_personality("Build a task management app", &AppType::SaasApp),
            Personality::Startup
        );
    }

    #[test]
    fn enterprise_config_is_formal() {
        let config = configure(Personality::Enterprise);
        assert!(!config.product.use_emoji);
        assert_eq!(config.design.animation_intensity, "subtle");
        assert_eq!(config.design.border_style, "sharp");
    }
}
