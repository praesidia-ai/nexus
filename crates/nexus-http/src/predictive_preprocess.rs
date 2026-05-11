//! Predictive Preprocessing — the "mind reading" layer.
//!
//! While the user types, the frontend sends partial input to `/intelligence/predict`.
//! This module pre-computes everything deterministically (<5ms) so results are
//! instant when the user hits "Build":
//!
//! 1. Pre-run intent engine → cache app type, complexity, features
//! 2. Pre-compute architecture decisions → cache tech stack
//! 3. Pre-compute product brief → cache domain, hero, flows
//! 4. Pre-generate skeleton files → cache for instant preview
//! 5. Generate smart suggestions → "Looks like you want a SaaS dashboard"
//! 6. Auto-expand vague prompts → "build CRM" → full product spec
//!
//! The cache persists on `AppState` so it survives across requests.
//! When the oneshot pipeline starts, it calls `consume()` to reuse the
//! pre-computed intent/decisions instead of re-running them.

use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::decision_engine::{self, ArchitectureDecision};
use crate::intent_engine::{self, AppType, FlatIntent};
use crate::personality::{self, Personality};
use crate::product_engine::{self, ProductBrief};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Cached predictions from partial input.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionCache {
    /// The input text these predictions were based on.
    pub input_snapshot: String,
    /// Pre-computed intent (may be low-confidence for short input).
    pub intent: FlatIntent,
    /// Pre-computed architecture decisions.
    pub architecture: ArchitectureDecision,
    /// Pre-computed product brief.
    pub product_brief: ProductBrief,
    /// Auto-detected personality.
    pub personality: Personality,
    /// Confidence in these predictions (lower for shorter input).
    pub overall_confidence: f64,
    /// When this was computed (ms).
    pub computed_at_ms: u64,
}

/// Full prediction result — includes suggestions and expansion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionResult {
    /// Whether new predictions were generated (false if input too short/unchanged).
    pub updated: bool,
    /// The current cache state.
    pub cache: Option<PredictionCache>,
    /// Smart suggestions for the user.
    pub suggestions: Vec<Suggestion>,
    /// Auto-expanded version of the prompt (richer than what the user typed).
    pub expanded_prompt: Option<String>,
    /// Pre-generated skeleton file paths (for instant preview).
    pub skeleton_paths: Vec<String>,
    /// Processing time (ms).
    pub compute_ms: u64,
}

/// A smart suggestion shown to the user while they type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    /// What the system detected.
    pub label: String,
    /// What the user can do about it (actionable).
    pub action: SuggestionAction,
    /// Confidence (0.0–1.0).
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionAction {
    /// Informational — "Looks like you want a dashboard"
    Info,
    /// Offer to add a feature — "Should I add auth + Stripe?"
    AddFeature { feature: String },
    /// Offer to set a style — "I'll use a luxury design for this"
    SetStyle { style: String },
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

/// Predictive preprocessor — lives on `AppState`, cache persists across requests.
pub struct PredictivePreprocessor {
    cache: Arc<Mutex<Option<PredictionCache>>>,
    min_input_len: usize,
}

impl Default for PredictivePreprocessor {
    fn default() -> Self {
        Self::new()
    }
}

impl PredictivePreprocessor {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(None)),
            min_input_len: 10,
        }
    }

    /// Pre-process partial input. All deterministic, <5ms.
    pub async fn predict(&self, partial_input: &str) -> PredictionResult {
        let start = Instant::now();

        // Too short — skip
        if partial_input.len() < self.min_input_len {
            return PredictionResult {
                updated: false,
                cache: None,
                suggestions: vec![],
                expanded_prompt: None,
                skeleton_paths: vec![],
                compute_ms: 0,
            };
        }

        // Check if input changed
        {
            let current = self.cache.lock().await;
            if let Some(ref cached) = *current {
                if cached.input_snapshot == partial_input {
                    // Input unchanged — return cached result with fresh suggestions
                    let suggestions = generate_suggestions(&cached.intent, &cached.architecture);
                    let expanded = expand_prompt(partial_input, &cached.intent, &cached.product_brief);
                    let skeletons = generate_skeleton_paths(&cached.intent);
                    return PredictionResult {
                        updated: false,
                        cache: Some(cached.clone()),
                        suggestions,
                        expanded_prompt: expanded,
                        skeleton_paths: skeletons,
                        compute_ms: 0,
                    };
                }
            }
        }

        // Run all deterministic engines (no LLM — instant)
        let intent = intent_engine::analyze_flat(partial_input);
        let architecture = decision_engine::decide(&intent);
        let product_brief = product_engine::generate_product_brief(&intent, partial_input);
        let detected_personality = personality::detect_personality(partial_input, &intent.app_type);

        // Confidence scales with input length
        let length_factor = (partial_input.len() as f64 / 100.0).min(1.0);
        let overall_confidence = intent.confidence * 0.6 + length_factor * 0.4;

        // Generate suggestions and expansion
        let suggestions = generate_suggestions(&intent, &architecture);
        let expanded = expand_prompt(partial_input, &intent, &product_brief);
        let skeletons = generate_skeleton_paths(&intent);

        let cache = PredictionCache {
            input_snapshot: partial_input.to_string(),
            intent,
            architecture,
            product_brief,
            personality: detected_personality,
            overall_confidence,
            computed_at_ms: start.elapsed().as_millis() as u64,
        };

        {
            let mut current = self.cache.lock().await;
            *current = Some(cache.clone());
        }

        PredictionResult {
            updated: true,
            cache: Some(cache),
            suggestions,
            expanded_prompt: expanded,
            skeleton_paths: skeletons,
            compute_ms: start.elapsed().as_millis() as u64,
        }
    }

    /// Consume the cached prediction for the oneshot pipeline.
    /// Returns None if cache is stale (input doesn't match).
    pub async fn consume(&self, final_input: &str) -> Option<PredictionCache> {
        let mut cache = self.cache.lock().await;
        match cache.take() {
            Some(cached) if cached.input_snapshot == final_input => Some(cached),
            Some(cached) => {
                *cache = Some(cached);
                None
            }
            None => None,
        }
    }

    /// Clear the prediction cache.
    pub async fn clear(&self) {
        let mut cache = self.cache.lock().await;
        *cache = None;
    }
}

// ---------------------------------------------------------------------------
// Smart Suggestions — "Looks like you want X. Should I add Y?"
// ---------------------------------------------------------------------------

fn generate_suggestions(intent: &FlatIntent, _arch: &ArchitectureDecision) -> Vec<Suggestion> {
    let mut suggestions = Vec::new();

    // App type recognition
    suggestions.push(Suggestion {
        label: format!("Building a {}", app_type_label(&intent.app_type)),
        action: SuggestionAction::Info,
        confidence: intent.confidence,
    });

    // Feature suggestions based on what's NOT yet specified
    if !intent.needs_auth && matches!(intent.app_type, AppType::SaasApp | AppType::Dashboard | AppType::Crm | AppType::Marketplace) {
        suggestions.push(Suggestion {
            label: "Should I add user authentication?".into(),
            action: SuggestionAction::AddFeature { feature: "auth".into() },
            confidence: 0.85,
        });
    }

    if !intent.needs_payments && matches!(intent.app_type, AppType::SaasApp | AppType::ECommerce | AppType::Marketplace) {
        suggestions.push(Suggestion {
            label: "Should I add Stripe payments?".into(),
            action: SuggestionAction::AddFeature { feature: "payments".into() },
            confidence: 0.80,
        });
    }

    if intent.suggested_agents.is_empty() && matches!(intent.app_type, AppType::SaasApp | AppType::Dashboard | AppType::ECommerce) {
        suggestions.push(Suggestion {
            label: "Should I add an AI assistant?".into(),
            action: SuggestionAction::AddFeature { feature: "ai_agent".into() },
            confidence: 0.70,
        });
    }

    // Style suggestion
    let style_label = match intent.ui_style {
        intent_engine::UiStyle::Luxurious => "luxury",
        intent_engine::UiStyle::Corporate => "professional",
        intent_engine::UiStyle::Playful => "playful",
        intent_engine::UiStyle::Technical => "technical",
        intent_engine::UiStyle::Bold => "bold",
        intent_engine::UiStyle::Minimal => "clean minimal",
    };
    suggestions.push(Suggestion {
        label: format!("I'll use a {} design style", style_label),
        action: SuggestionAction::SetStyle { style: style_label.into() },
        confidence: 0.75,
    });

    suggestions
}

// ---------------------------------------------------------------------------
// Auto-Expansion — turn vague prompts into rich product specs
// ---------------------------------------------------------------------------

fn expand_prompt(
    original: &str,
    intent: &FlatIntent,
    brief: &ProductBrief,
) -> Option<String> {
    // Only expand if the original is short/vague (< 50 chars)
    if original.len() > 50 {
        return None;
    }

    let mut parts = Vec::new();

    parts.push(original.to_string());

    // Add inferred features
    if intent.needs_auth {
        parts.push("with user authentication (login, signup, profiles)".into());
    }
    if intent.needs_database {
        parts.push("with database for persistent data storage".into());
    }
    if intent.needs_payments {
        parts.push("with Stripe payment integration".into());
    }

    // Add inferred pages
    if !intent.suggested_pages.is_empty() {
        let pages = intent.suggested_pages.iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        parts.push(format!("including pages: {}", pages));
    }

    // Add product context
    if !brief.hero.headline.is_empty() {
        parts.push(format!("hero: \"{}\"", brief.hero.headline));
    }

    Some(parts.join(". "))
}

// ---------------------------------------------------------------------------
// Skeleton path pre-generation
// ---------------------------------------------------------------------------

fn generate_skeleton_paths(intent: &FlatIntent) -> Vec<String> {
    let mut paths = vec!["app/layout.tsx".into(), "app/page.tsx".into()];

    for page in &intent.suggested_pages {
        let route = page.to_lowercase().replace(' ', "-");
        if route != "home" {
            paths.push(format!("app/{}/page.tsx", route));
        }
    }

    if intent.needs_auth {
        paths.push("app/login/page.tsx".into());
        paths.push("app/signup/page.tsx".into());
    }

    paths
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn app_type_label(app_type: &AppType) -> &'static str {
    match app_type {
        AppType::LandingPage => "landing page",
        AppType::SaasApp => "SaaS application",
        AppType::Dashboard => "dashboard",
        AppType::Marketplace => "marketplace",
        AppType::Crm => "CRM system",
        AppType::ECommerce => "e-commerce store",
        AppType::Portfolio => "portfolio website",
        AppType::Blog => "blog",
        AppType::InternalTool => "internal tool",
        AppType::MobileApp => "mobile app",
        AppType::ApiOnly => "API service",
        AppType::Custom => "custom application",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn skips_short_input() {
        let pp = PredictivePreprocessor::new();
        let result = pp.predict("hello").await;
        assert!(!result.updated);
        assert!(result.cache.is_none());
        assert!(result.suggestions.is_empty());
    }

    #[tokio::test]
    async fn predicts_on_sufficient_input() {
        let pp = PredictivePreprocessor::new();
        let result = pp.predict("Build a CRM with user accounts and billing").await;
        assert!(result.updated);
        let cache = result.cache.unwrap();
        assert!(cache.overall_confidence > 0.0);
        assert!(!cache.product_brief.hero.headline.is_empty());
        // Should have suggestions
        assert!(!result.suggestions.is_empty());
        assert!(result.suggestions.iter().any(|s| s.label.contains("CRM")));
    }

    #[tokio::test]
    async fn returns_cached_on_same_input() {
        let pp = PredictivePreprocessor::new();
        let input = "Build an e-commerce store for shoes";
        let _ = pp.predict(input).await;
        let result = pp.predict(input).await;
        assert!(!result.updated);
        assert!(result.cache.is_some());
        // Suggestions still returned even from cache
        assert!(!result.suggestions.is_empty());
    }

    #[tokio::test]
    async fn consume_clears_cache() {
        let pp = PredictivePreprocessor::new();
        let input = "Build a dashboard for analytics";
        let _ = pp.predict(input).await;
        let consumed = pp.consume(input).await;
        assert!(consumed.is_some());
        let consumed2 = pp.consume(input).await;
        assert!(consumed2.is_none());
    }

    #[tokio::test]
    async fn generates_suggestions_for_saas() {
        let pp = PredictivePreprocessor::new();
        let result = pp.predict("Build a SaaS project management tool").await;
        let suggestions = &result.suggestions;
        // Should recognize it as a SaaS app
        assert!(suggestions.iter().any(|s| s.label.contains("SaaS")));
        // Should suggest a design style
        assert!(suggestions.iter().any(|s| matches!(s.action, SuggestionAction::SetStyle { .. })));
    }

    #[tokio::test]
    async fn expands_vague_prompts() {
        let pp = PredictivePreprocessor::new();
        let result = pp.predict("Build a CRM").await;
        assert!(result.expanded_prompt.is_some());
        let expanded = result.expanded_prompt.unwrap();
        assert!(expanded.len() > "Build a CRM".len());
    }

    #[tokio::test]
    async fn skips_expansion_for_detailed_prompts() {
        let pp = PredictivePreprocessor::new();
        let long = "Build a comprehensive CRM system with user authentication, Stripe billing, contact management, email templates, and analytics dashboard";
        let result = pp.predict(long).await;
        assert!(result.expanded_prompt.is_none()); // already detailed enough
    }
}
