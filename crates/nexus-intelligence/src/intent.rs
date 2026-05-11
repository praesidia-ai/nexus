//! Intent analysis — understanding user descriptions.
//!
//! The intent layer transforms a raw user description into a structured
//! [`Intent`] object with explicit, inferred, and hidden requirements.
//! Implementations must be deterministic (no LLM calls) — use keyword
//! heuristics, domain packs, and historical learning signals.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Confidence-wrapped inference value.
///
/// Every inferred value carries its confidence score, source, and reasoning
/// so downstream consumers can make informed decisions about trust.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inference<T: Clone> {
    /// The inferred value.
    pub value: T,
    /// Confidence score between 0.0 and 1.0.
    pub confidence: f32,
    /// Where this inference came from.
    pub source: InferenceSource,
    /// Human-readable reasoning for this inference.
    pub reasoning: String,
}

/// Source of an inference — used for explainability and debugging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InferenceSource {
    /// User explicitly stated this.
    ExplicitUserRequest,
    /// Matched a keyword heuristic rule.
    KeywordHeuristic,
    /// Came from a domain pack.
    DomainPack,
    /// Derived via semantic analysis.
    SemanticAnalysis,
    /// Based on historical learning signals.
    HistoricalLearning,
}

/// Application type classification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AppType {
    /// Standard web application.
    WebApp,
    /// Marketing or product landing page.
    LandingPage,
    /// Data dashboard with charts and metrics.
    Dashboard,
    /// Online store with cart and checkout.
    Ecommerce,
    /// Content blog or publication.
    Blog,
    /// Personal or company portfolio.
    Portfolio,
    /// Software-as-a-Service product.
    SaaS,
    /// Two-sided marketplace.
    Marketplace,
    /// Social network or community platform.
    Social,
    /// Customer relationship management.
    CRM,
    /// Catch-all for unrecognized types.
    Custom(String),
}

/// Complexity level of the application.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Complexity {
    /// Few pages, no auth, minimal backend.
    Simple,
    /// Auth, database, moderate feature set.
    Medium,
    /// Multiple services, real-time, complex business logic.
    Complex,
}

/// UI style preference for the generated application.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum UiStyle {
    /// Clean, contemporary aesthetic.
    Modern,
    /// Stripped-down, whitespace-heavy design.
    Minimal,
    /// High-contrast, attention-grabbing design.
    Bold,
    /// Professional, enterprise-oriented design.
    Corporate,
    /// Fun, colorful, casual design.
    Playful,
    /// Dark-mode-first design.
    Dark,
    /// Light-mode-first design.
    Light,
}

/// Layered intent structure — the primary output of intent analysis.
///
/// Separates what the user said, what we infer, and what they probably
/// need but didn't mention. The `meta` field tracks analysis quality.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Intent {
    /// What the user explicitly stated.
    pub explicit: ExplicitIntent,
    /// What we inferred from context, keywords, and domain knowledge.
    pub inferred: InferredIntent,
    /// Requirements the user probably needs but didn't mention.
    pub hidden: HiddenRequirements,
    /// Metadata about the analysis quality.
    pub meta: IntentMeta,
}

/// Explicit intent — directly extracted from user input.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplicitIntent {
    /// The raw user description, preserved verbatim.
    pub raw_description: String,
    /// Features explicitly mentioned by the user.
    pub extracted_features: Vec<Inference<String>>,
    /// Constraints explicitly mentioned (e.g., "must use React").
    pub extracted_constraints: Vec<Inference<String>>,
}

/// Inferred intent — derived from analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferredIntent {
    /// What type of application this is.
    pub app_type: Inference<AppType>,
    /// How complex the application is.
    pub complexity: Inference<Complexity>,
    /// The domain vertical (e.g., "fitness", "fintech").
    pub domain: Inference<String>,
    /// Suggested pages for the application.
    pub suggested_pages: Vec<Inference<PageSuggestion>>,
    /// Suggested data entities.
    pub suggested_entities: Vec<Inference<EntitySuggestion>>,
    /// Whether authentication is needed.
    pub auth_needed: Inference<bool>,
    /// Whether a database is needed.
    pub db_needed: Inference<bool>,
    /// Whether real-time features are needed.
    pub realtime_needed: Inference<bool>,
    /// Preferred UI style.
    pub ui_style: Inference<UiStyle>,
}

/// A suggested page for the application.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageSuggestion {
    /// Page name (e.g., "Dashboard").
    pub name: String,
    /// Route path (e.g., "/dashboard").
    pub route: String,
    /// What this page does.
    pub description: String,
}

/// A suggested data entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitySuggestion {
    /// Entity name (e.g., "User", "Product").
    pub name: String,
    /// Field names for this entity.
    pub fields: Vec<String>,
}

/// Hidden requirements the user probably needs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HiddenRequirements {
    /// Inferred requirements not explicitly mentioned.
    pub requirements: Vec<Inference<Requirement>>,
    /// Domain pack that was applied, if any.
    pub domain_pack: Option<String>,
}

/// A single requirement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Requirement {
    /// Category (e.g., "security", "performance", "compliance").
    pub category: String,
    /// Human-readable description.
    pub description: String,
    /// How important this requirement is.
    pub priority: RequirementPriority,
}

/// Priority level for a requirement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RequirementPriority {
    /// Must have — the app won't work without it.
    Critical,
    /// Should have — significantly improves the app.
    Important,
    /// Could have — nice enhancement.
    NiceToHave,
}

/// Metadata about the intent analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentMeta {
    /// Overall confidence in the analysis (0.0 to 1.0).
    pub overall_confidence: f32,
    /// Fields or areas with ambiguity.
    pub ambiguity_flags: Vec<String>,
    /// Suggested clarification questions to improve confidence.
    pub clarification_suggestions: Vec<String>,
    /// How long the analysis took in milliseconds.
    pub analysis_duration_ms: u64,
}

/// A clarification question to resolve ambiguity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClarificationQuestion {
    /// Which field this clarification is for.
    pub field: String,
    /// The question to ask the user.
    pub question: String,
    /// Current confidence for this field.
    pub current_confidence: f32,
    /// Suggested answer options.
    pub options: Vec<String>,
}

/// Trait for intent analysis implementations.
///
/// Implementations should be deterministic — use keyword heuristics, domain
/// packs, and historical learning rather than LLM calls.
#[async_trait]
pub trait IntentAnalyzer: Send + Sync {
    /// Analyze a user description into structured intent.
    async fn analyze(
        &self,
        description: &str,
    ) -> Result<Intent, Box<dyn std::error::Error + Send + Sync>>;

    /// Generate clarification questions for ambiguous fields.
    async fn clarify(
        &self,
        intent: &Intent,
    ) -> Result<Vec<ClarificationQuestion>, Box<dyn std::error::Error + Send + Sync>>;
}
