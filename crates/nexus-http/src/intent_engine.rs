//! Intent Engine — deterministic keyword/rule heuristics for intent analysis.
//!
//! Layered intent system that wraps every inferred value with confidence,
//! source, and reasoning. This module is intentionally **LLM-free** so it
//! is fast, reproducible, and safe to call during request preprocessing;
//! the optional LLM-assisted refinement lives in
//! [`crate::intent_engine_semantic`] and must be invoked explicitly.

use std::time::Instant;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Core domain types (app classification, complexity, UI style)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AppType {
    LandingPage,
    SaasApp,
    Dashboard,
    Marketplace,
    Crm,
    ECommerce,
    Portfolio,
    Blog,
    InternalTool,
    MobileApp,
    ApiOnly,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UiStyle {
    Minimal,
    Corporate,
    Playful,
    Luxurious,
    Technical,
    Bold,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Complexity {
    Simple,  // 1-3 pages, no DB, no auth
    Medium,  // 3-8 pages, DB, maybe auth
    Complex, // 8+ pages, DB, auth, payments, agents
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTemplate {
    pub name: String,
    pub agent_type: String,
    pub description: String,
    pub ui_component: String,
    pub api_route: String,
    pub system_prompt: String,
    pub trigger: String,
}

/// Flat intent struct — a simple, non-layered view of the analysis result.
///
/// Used by subsystems that do not need confidence scores or inference metadata.
/// Produced by [`analyze_flat`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlatIntent {
    pub app_type: AppType,
    pub needs_auth: bool,
    pub needs_database: bool,
    pub needs_payments: bool,
    pub ui_style: UiStyle,
    pub complexity: Complexity,
    pub suggested_agents: Vec<AgentTemplate>,
    pub suggested_entities: Vec<String>,
    pub suggested_pages: Vec<String>,
    pub inferred_features: Vec<String>,
    pub confidence: f64,
}

/// Flat hidden requirements — additional features the user did not explicitly state.
///
/// Produced by [`infer_flat_hidden_requirements`] from a [`FlatIntent`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlatHiddenRequirements {
    pub needs_auth: bool,
    pub needs_database: bool,
    pub needs_payments: bool,
    pub additional_features: Vec<String>,
    pub additional_pages: Vec<String>,
    pub additional_entities: Vec<String>,
    pub confidence: f64,
    pub reasoning: Vec<String>,
}

// ---------------------------------------------------------------------------
// Core inference wrapper
// ---------------------------------------------------------------------------

/// Wraps any inferred value with metadata about how confident the engine is,
/// where the inference came from, and a human-readable reasoning string.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentInference<T: Clone> {
    /// The inferred value.
    pub value: T,
    /// Confidence score in `[0.0, 1.0]`.
    pub confidence: f32,
    /// Where this inference originated.
    pub source: InferenceSource,
    /// Human-readable explanation of why this value was chosen.
    pub reasoning: String,
}

/// Describes the origin of an inference.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum InferenceSource {
    /// The user explicitly stated this requirement.
    ExplicitUserRequest,
    /// Matched via keyword heuristics in the deterministic engine.
    KeywordHeuristic,
    /// Derived from a domain pack's defaults.
    DomainPack,
    /// Inferred by the LLM semantic fallback.
    SemanticAnalysis,
    /// Learned from historical project patterns.
    HistoricalLearning,
}

// ---------------------------------------------------------------------------
// Layered intent structs
// ---------------------------------------------------------------------------

/// Layered, confidence-scored analysis of a user description.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Intent {
    /// What the user explicitly said.
    pub explicit: ExplicitIntent,
    /// What the engine inferred beyond the explicit request.
    pub inferred: InferredIntent,
    /// Requirements the user did not mention but almost certainly needs.
    pub hidden: HiddenRequirements,
    /// Metadata about the analysis itself.
    pub meta: IntentMeta,
}

/// Captures what the user explicitly communicated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplicitIntent {
    /// The raw user description, unmodified.
    pub raw_description: String,
    /// Features explicitly mentioned or strongly implied by keywords.
    pub extracted_features: Vec<IntentInference<String>>,
    /// Constraints explicitly mentioned (e.g. "no database", "must be fast").
    pub extracted_constraints: Vec<IntentInference<String>>,
}

/// Engine-inferred properties that go beyond what the user literally said.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferredIntent {
    /// The detected application type.
    pub app_type: IntentInference<AppType>,
    /// Overall complexity classification.
    pub complexity: IntentInference<Complexity>,
    /// The primary business domain.
    pub domain: IntentInference<String>,
    /// Suggested pages for the application.
    pub suggested_pages: Vec<IntentInference<PageSuggestion>>,
    /// Suggested data entities.
    pub suggested_entities: Vec<IntentInference<EntitySuggestion>>,
    /// Suggested AI agents to embed.
    pub suggested_agents: Vec<IntentInference<AgentSuggestion>>,
    /// Whether authentication is needed.
    pub auth_needed: IntentInference<bool>,
    /// Whether a database is needed.
    pub db_needed: IntentInference<bool>,
    /// Whether real-time features are needed.
    pub realtime_needed: IntentInference<bool>,
    /// The suggested UI visual style.
    pub ui_style: IntentInference<UiStyle>,
}

/// A suggested page for the application.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageSuggestion {
    /// Display name of the page.
    pub name: String,
    /// URL route for the page.
    pub route: String,
    /// What this page does.
    pub description: String,
}

/// A suggested data entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitySuggestion {
    /// Entity name (PascalCase).
    pub name: String,
    /// Field names for this entity.
    pub fields: Vec<String>,
}

/// A suggested AI agent to embed in the application.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSuggestion {
    /// Agent display name.
    pub name: String,
    /// The role this agent fills.
    pub role: String,
    /// The system prompt for the agent.
    pub system_prompt: String,
}

/// Requirements the user did not state but the domain demands.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HiddenRequirements {
    /// Individual hidden requirements with confidence scores.
    pub requirements: Vec<IntentInference<Requirement>>,
    /// The matched domain pack name, if any.
    pub domain_pack: Option<String>,
}

/// A single hidden requirement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Requirement {
    /// Category bucket (e.g. "security", "ux", "compliance").
    pub category: String,
    /// Human-readable description of the requirement.
    pub description: String,
    /// How important this requirement is.
    pub priority: RequirementPriority,
}

/// Priority levels for hidden requirements.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RequirementPriority {
    /// Must have — the app will not function correctly without it.
    Critical,
    /// Should have — expected by users in this domain.
    Important,
    /// Could have — adds polish but not essential.
    NiceToHave,
}

/// Metadata about the analysis process itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentMeta {
    /// Weighted average of all field confidences.
    pub overall_confidence: f32,
    /// Fields or aspects that are ambiguous.
    pub ambiguity_flags: Vec<String>,
    /// Suggested questions to ask the user to resolve ambiguity.
    pub clarification_suggestions: Vec<String>,
    /// How long the analysis took in milliseconds.
    pub analysis_duration_ms: u64,
}

// ---------------------------------------------------------------------------
// Domain Packs
// ---------------------------------------------------------------------------

/// A loadable vertical knowledge pack for a specific business domain.
///
/// Domain packs provide pre-built defaults for entities, pages, hidden
/// requirements, and UI hints based on domain expertise.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainPack {
    /// The domain name (e.g. "CRM", "E-commerce").
    pub domain: String,
    /// Keywords that trigger this domain pack.
    pub triggers: Vec<String>,
    /// Default entities for this domain.
    pub default_entities: Vec<EntitySuggestion>,
    /// Default pages for this domain.
    pub default_pages: Vec<PageSuggestion>,
    /// Hidden requirements typical for this domain.
    pub hidden_requirements: Vec<Requirement>,
    /// Suggested UI style for this domain.
    pub ui_style_hint: Option<UiStyle>,
}

/// Returns all built-in domain packs.
pub fn builtin_domain_packs() -> Vec<DomainPack> {
    vec![
        // ---- CRM ----
        DomainPack {
            domain: "CRM".into(),
            triggers: vec![
                "crm".into(), "customer relationship".into(), "sales pipeline".into(),
                "lead management".into(), "contacts".into(), "deals".into(),
            ],
            default_entities: vec![
                EntitySuggestion { name: "Contact".into(), fields: vec!["id".into(), "name".into(), "email".into(), "phone".into(), "company".into(), "status".into(), "created_at".into()] },
                EntitySuggestion { name: "Deal".into(), fields: vec!["id".into(), "title".into(), "value".into(), "stage".into(), "contact_id".into(), "expected_close".into(), "created_at".into()] },
                EntitySuggestion { name: "Activity".into(), fields: vec!["id".into(), "type".into(), "description".into(), "contact_id".into(), "deal_id".into(), "created_at".into()] },
                EntitySuggestion { name: "Pipeline".into(), fields: vec!["id".into(), "name".into(), "stages".into(), "created_at".into()] },
                EntitySuggestion { name: "Note".into(), fields: vec!["id".into(), "body".into(), "contact_id".into(), "author_id".into(), "created_at".into()] },
            ],
            default_pages: vec![
                PageSuggestion { name: "Contacts".into(), route: "/contacts".into(), description: "Browse and manage all contacts".into() },
                PageSuggestion { name: "Deals".into(), route: "/deals".into(), description: "Sales pipeline with drag-and-drop stages".into() },
                PageSuggestion { name: "Dashboard".into(), route: "/dashboard".into(), description: "KPIs, revenue forecasts, activity feed".into() },
                PageSuggestion { name: "Reports".into(), route: "/reports".into(), description: "Sales analytics and performance reports".into() },
                PageSuggestion { name: "Settings".into(), route: "/settings".into(), description: "Team management and CRM configuration".into() },
            ],
            hidden_requirements: vec![
                Requirement { category: "security".into(), description: "Role-based access control for sales team".into(), priority: RequirementPriority::Critical },
                Requirement { category: "ux".into(), description: "Search and filtering across all entities".into(), priority: RequirementPriority::Critical },
                Requirement { category: "data".into(), description: "CSV/Excel data export".into(), priority: RequirementPriority::Important },
                Requirement { category: "integration".into(), description: "Email integration for contact communication".into(), priority: RequirementPriority::Important },
                Requirement { category: "ux".into(), description: "Activity timeline on contact and deal pages".into(), priority: RequirementPriority::Important },
            ],
            ui_style_hint: Some(UiStyle::Corporate),
        },
        // ---- Marketplace ----
        DomainPack {
            domain: "Marketplace".into(),
            triggers: vec![
                "marketplace".into(), "airbnb".into(), "uber".into(), "two-sided".into(),
                "buyer seller".into(), "listing".into(), "booking".into(),
            ],
            default_entities: vec![
                EntitySuggestion { name: "Listing".into(), fields: vec!["id".into(), "title".into(), "description".into(), "price".into(), "seller_id".into(), "category_id".into(), "images".into(), "status".into(), "created_at".into()] },
                EntitySuggestion { name: "Booking".into(), fields: vec!["id".into(), "listing_id".into(), "buyer_id".into(), "start_date".into(), "end_date".into(), "total_price".into(), "status".into(), "created_at".into()] },
                EntitySuggestion { name: "Review".into(), fields: vec!["id".into(), "listing_id".into(), "author_id".into(), "rating".into(), "comment".into(), "created_at".into()] },
                EntitySuggestion { name: "Message".into(), fields: vec!["id".into(), "sender_id".into(), "receiver_id".into(), "listing_id".into(), "body".into(), "read".into(), "created_at".into()] },
                EntitySuggestion { name: "Category".into(), fields: vec!["id".into(), "name".into(), "slug".into(), "parent_id".into()] },
            ],
            default_pages: vec![
                PageSuggestion { name: "Browse".into(), route: "/browse".into(), description: "Search and filter all listings".into() },
                PageSuggestion { name: "Listing Detail".into(), route: "/listings/:id".into(), description: "Full listing page with reviews and booking".into() },
                PageSuggestion { name: "Seller Dashboard".into(), route: "/dashboard/seller".into(), description: "Seller's listing management and earnings".into() },
                PageSuggestion { name: "Messages".into(), route: "/messages".into(), description: "Buyer-seller messaging".into() },
                PageSuggestion { name: "Checkout".into(), route: "/checkout".into(), description: "Secure payment and booking confirmation".into() },
            ],
            hidden_requirements: vec![
                Requirement { category: "trust".into(), description: "Ratings and reviews system".into(), priority: RequirementPriority::Critical },
                Requirement { category: "payments".into(), description: "Secure escrow-style payment processing".into(), priority: RequirementPriority::Critical },
                Requirement { category: "moderation".into(), description: "Content moderation and dispute resolution".into(), priority: RequirementPriority::Important },
                Requirement { category: "ux".into(), description: "Favorites and wishlist functionality".into(), priority: RequirementPriority::NiceToHave },
                Requirement { category: "communication".into(), description: "Real-time messaging between buyers and sellers".into(), priority: RequirementPriority::Important },
            ],
            ui_style_hint: Some(UiStyle::Bold),
        },
        // ---- SaaS ----
        DomainPack {
            domain: "SaaS".into(),
            triggers: vec![
                "saas".into(), "subscription".into(), "multi-tenant".into(),
                "platform".into(), "software as a service".into(),
            ],
            default_entities: vec![
                EntitySuggestion { name: "Organization".into(), fields: vec!["id".into(), "name".into(), "slug".into(), "plan".into(), "created_at".into()] },
                EntitySuggestion { name: "Subscription".into(), fields: vec!["id".into(), "org_id".into(), "plan".into(), "status".into(), "stripe_id".into(), "current_period_end".into()] },
                EntitySuggestion { name: "Invitation".into(), fields: vec!["id".into(), "org_id".into(), "email".into(), "role".into(), "status".into(), "created_at".into()] },
                EntitySuggestion { name: "ApiKey".into(), fields: vec!["id".into(), "org_id".into(), "name".into(), "key_hash".into(), "last_used".into(), "created_at".into()] },
                EntitySuggestion { name: "AuditLog".into(), fields: vec!["id".into(), "org_id".into(), "user_id".into(), "action".into(), "resource".into(), "created_at".into()] },
            ],
            default_pages: vec![
                PageSuggestion { name: "Dashboard".into(), route: "/dashboard".into(), description: "Main workspace and overview".into() },
                PageSuggestion { name: "Onboarding".into(), route: "/onboarding".into(), description: "New user setup wizard".into() },
                PageSuggestion { name: "Settings".into(), route: "/settings".into(), description: "Organization and account settings".into() },
                PageSuggestion { name: "Billing".into(), route: "/billing".into(), description: "Subscription management and invoices".into() },
                PageSuggestion { name: "Team".into(), route: "/team".into(), description: "Team member management and invitations".into() },
            ],
            hidden_requirements: vec![
                Requirement { category: "security".into(), description: "Multi-tenant data isolation".into(), priority: RequirementPriority::Critical },
                Requirement { category: "billing".into(), description: "Subscription lifecycle management with Stripe".into(), priority: RequirementPriority::Critical },
                Requirement { category: "ux".into(), description: "User onboarding flow with progress tracking".into(), priority: RequirementPriority::Important },
                Requirement { category: "security".into(), description: "API key management for integrations".into(), priority: RequirementPriority::Important },
                Requirement { category: "compliance".into(), description: "Audit logging for all state changes".into(), priority: RequirementPriority::Important },
            ],
            ui_style_hint: Some(UiStyle::Minimal),
        },
        // ---- E-commerce ----
        DomainPack {
            domain: "E-commerce".into(),
            triggers: vec![
                "ecommerce".into(), "e-commerce".into(), "shop".into(), "store".into(),
                "buy".into(), "cart".into(), "checkout".into(), "products".into(),
            ],
            default_entities: vec![
                EntitySuggestion { name: "Product".into(), fields: vec!["id".into(), "name".into(), "description".into(), "price".into(), "images".into(), "category_id".into(), "stock".into(), "created_at".into()] },
                EntitySuggestion { name: "Category".into(), fields: vec!["id".into(), "name".into(), "slug".into(), "parent_id".into()] },
                EntitySuggestion { name: "CartItem".into(), fields: vec!["id".into(), "user_id".into(), "product_id".into(), "quantity".into()] },
                EntitySuggestion { name: "Order".into(), fields: vec!["id".into(), "user_id".into(), "total".into(), "status".into(), "shipping_address".into(), "created_at".into()] },
                EntitySuggestion { name: "OrderItem".into(), fields: vec!["id".into(), "order_id".into(), "product_id".into(), "quantity".into(), "price".into()] },
                EntitySuggestion { name: "Review".into(), fields: vec!["id".into(), "product_id".into(), "user_id".into(), "rating".into(), "comment".into(), "created_at".into()] },
            ],
            default_pages: vec![
                PageSuggestion { name: "Product Catalog".into(), route: "/products".into(), description: "Browse products with search and filters".into() },
                PageSuggestion { name: "Product Detail".into(), route: "/products/:id".into(), description: "Product page with images, reviews, add to cart".into() },
                PageSuggestion { name: "Cart".into(), route: "/cart".into(), description: "Shopping cart with quantity management".into() },
                PageSuggestion { name: "Checkout".into(), route: "/checkout".into(), description: "Multi-step checkout with payment".into() },
                PageSuggestion { name: "Order History".into(), route: "/orders".into(), description: "Past orders with tracking status".into() },
                PageSuggestion { name: "Wishlist".into(), route: "/wishlist".into(), description: "Saved products for later".into() },
            ],
            hidden_requirements: vec![
                Requirement { category: "payments".into(), description: "Secure payment processing with PCI compliance".into(), priority: RequirementPriority::Critical },
                Requirement { category: "ux".into(), description: "Product search with faceted filters".into(), priority: RequirementPriority::Critical },
                Requirement { category: "logistics".into(), description: "Inventory tracking and stock management".into(), priority: RequirementPriority::Important },
                Requirement { category: "marketing".into(), description: "Promotional codes and discount system".into(), priority: RequirementPriority::NiceToHave },
                Requirement { category: "communication".into(), description: "Order confirmation and shipping notification emails".into(), priority: RequirementPriority::Important },
            ],
            ui_style_hint: Some(UiStyle::Bold),
        },
        // ---- Blog / CMS ----
        DomainPack {
            domain: "Blog/CMS".into(),
            triggers: vec![
                "blog".into(), "cms".into(), "content management".into(),
                "articles".into(), "publishing".into(), "editorial".into(),
            ],
            default_entities: vec![
                EntitySuggestion { name: "Post".into(), fields: vec!["id".into(), "title".into(), "slug".into(), "body".into(), "excerpt".into(), "author_id".into(), "status".into(), "published_at".into(), "created_at".into()] },
                EntitySuggestion { name: "Category".into(), fields: vec!["id".into(), "name".into(), "slug".into(), "description".into()] },
                EntitySuggestion { name: "Tag".into(), fields: vec!["id".into(), "name".into(), "slug".into()] },
                EntitySuggestion { name: "Comment".into(), fields: vec!["id".into(), "post_id".into(), "author_name".into(), "body".into(), "approved".into(), "created_at".into()] },
                EntitySuggestion { name: "Media".into(), fields: vec!["id".into(), "filename".into(), "url".into(), "mime_type".into(), "size".into(), "created_at".into()] },
            ],
            default_pages: vec![
                PageSuggestion { name: "Home".into(), route: "/".into(), description: "Featured and recent posts".into() },
                PageSuggestion { name: "Article".into(), route: "/posts/:slug".into(), description: "Full article with comments".into() },
                PageSuggestion { name: "Category".into(), route: "/category/:slug".into(), description: "Posts filtered by category".into() },
                PageSuggestion { name: "Archive".into(), route: "/archive".into(), description: "Chronological post archive".into() },
                PageSuggestion { name: "Editor".into(), route: "/admin/editor".into(), description: "Rich text post editor (admin)".into() },
            ],
            hidden_requirements: vec![
                Requirement { category: "seo".into(), description: "SEO metadata, OpenGraph tags, structured data".into(), priority: RequirementPriority::Critical },
                Requirement { category: "content".into(), description: "Rich text editor with media embedding".into(), priority: RequirementPriority::Critical },
                Requirement { category: "distribution".into(), description: "RSS feed generation".into(), priority: RequirementPriority::Important },
                Requirement { category: "moderation".into(), description: "Comment moderation and spam filtering".into(), priority: RequirementPriority::Important },
                Requirement { category: "performance".into(), description: "Static page caching for fast load times".into(), priority: RequirementPriority::NiceToHave },
            ],
            ui_style_hint: Some(UiStyle::Minimal),
        },
        // ---- Dashboard ----
        DomainPack {
            domain: "Dashboard".into(),
            triggers: vec![
                "dashboard".into(), "analytics".into(), "monitoring".into(),
                "metrics".into(), "kpi".into(), "reporting".into(),
            ],
            default_entities: vec![
                EntitySuggestion { name: "Metric".into(), fields: vec!["id".into(), "name".into(), "value".into(), "unit".into(), "source".into(), "timestamp".into()] },
                EntitySuggestion { name: "Report".into(), fields: vec!["id".into(), "title".into(), "type".into(), "config".into(), "created_by".into(), "created_at".into()] },
                EntitySuggestion { name: "DataSource".into(), fields: vec!["id".into(), "name".into(), "type".into(), "connection_config".into(), "status".into()] },
                EntitySuggestion { name: "Alert".into(), fields: vec!["id".into(), "metric_id".into(), "condition".into(), "threshold".into(), "status".into(), "created_at".into()] },
            ],
            default_pages: vec![
                PageSuggestion { name: "Overview".into(), route: "/dashboard".into(), description: "High-level KPI cards and summary charts".into() },
                PageSuggestion { name: "Analytics".into(), route: "/analytics".into(), description: "Deep-dive charts with date range filtering".into() },
                PageSuggestion { name: "Reports".into(), route: "/reports".into(), description: "Saved and scheduled reports".into() },
                PageSuggestion { name: "Alerts".into(), route: "/alerts".into(), description: "Threshold alerts and notification rules".into() },
            ],
            hidden_requirements: vec![
                Requirement { category: "visualization".into(), description: "Interactive charts (line, bar, pie, area)".into(), priority: RequirementPriority::Critical },
                Requirement { category: "ux".into(), description: "Date range picker and time-series filtering".into(), priority: RequirementPriority::Critical },
                Requirement { category: "data".into(), description: "Export to PDF and CSV".into(), priority: RequirementPriority::Important },
                Requirement { category: "realtime".into(), description: "Auto-refreshing data with WebSocket or polling".into(), priority: RequirementPriority::Important },
                Requirement { category: "security".into(), description: "Role-based dashboard access".into(), priority: RequirementPriority::NiceToHave },
            ],
            ui_style_hint: Some(UiStyle::Technical),
        },
        // ---- Social ----
        DomainPack {
            domain: "Social".into(),
            triggers: vec![
                "social".into(), "community".into(), "forum".into(), "feed".into(),
                "follow".into(), "friends".into(), "network".into(), "social media".into(),
            ],
            default_entities: vec![
                EntitySuggestion { name: "Profile".into(), fields: vec!["id".into(), "user_id".into(), "display_name".into(), "bio".into(), "avatar_url".into(), "created_at".into()] },
                EntitySuggestion { name: "Post".into(), fields: vec!["id".into(), "author_id".into(), "body".into(), "media_urls".into(), "likes_count".into(), "created_at".into()] },
                EntitySuggestion { name: "Comment".into(), fields: vec!["id".into(), "post_id".into(), "author_id".into(), "body".into(), "created_at".into()] },
                EntitySuggestion { name: "Follow".into(), fields: vec!["id".into(), "follower_id".into(), "following_id".into(), "created_at".into()] },
                EntitySuggestion { name: "Like".into(), fields: vec!["id".into(), "user_id".into(), "post_id".into(), "created_at".into()] },
                EntitySuggestion { name: "Notification".into(), fields: vec!["id".into(), "user_id".into(), "type".into(), "reference_id".into(), "read".into(), "created_at".into()] },
            ],
            default_pages: vec![
                PageSuggestion { name: "Feed".into(), route: "/feed".into(), description: "Personalized content feed".into() },
                PageSuggestion { name: "Profile".into(), route: "/profile/:id".into(), description: "User profile with posts and followers".into() },
                PageSuggestion { name: "Explore".into(), route: "/explore".into(), description: "Discover trending content and people".into() },
                PageSuggestion { name: "Notifications".into(), route: "/notifications".into(), description: "Activity notifications".into() },
                PageSuggestion { name: "Messages".into(), route: "/messages".into(), description: "Direct messaging".into() },
            ],
            hidden_requirements: vec![
                Requirement { category: "realtime".into(), description: "Real-time notifications and feed updates".into(), priority: RequirementPriority::Critical },
                Requirement { category: "moderation".into(), description: "Content moderation and reporting".into(), priority: RequirementPriority::Critical },
                Requirement { category: "privacy".into(), description: "Privacy controls and blocking".into(), priority: RequirementPriority::Important },
                Requirement { category: "performance".into(), description: "Infinite scroll with efficient pagination".into(), priority: RequirementPriority::Important },
                Requirement { category: "engagement".into(), description: "Push notifications for engagement".into(), priority: RequirementPriority::NiceToHave },
            ],
            ui_style_hint: Some(UiStyle::Playful),
        },
        // ---- Portfolio ----
        DomainPack {
            domain: "Portfolio".into(),
            triggers: vec![
                "portfolio".into(), "personal site".into(), "resume".into(),
                "showcase".into(), "freelancer".into(), "creative".into(),
            ],
            default_entities: vec![
                EntitySuggestion { name: "Project".into(), fields: vec!["id".into(), "title".into(), "description".into(), "image_url".into(), "live_url".into(), "source_url".into(), "tags".into(), "featured".into(), "created_at".into()] },
                EntitySuggestion { name: "Skill".into(), fields: vec!["id".into(), "name".into(), "category".into(), "proficiency".into()] },
                EntitySuggestion { name: "Experience".into(), fields: vec!["id".into(), "company".into(), "role".into(), "description".into(), "start_date".into(), "end_date".into()] },
                EntitySuggestion { name: "ContactMessage".into(), fields: vec!["id".into(), "name".into(), "email".into(), "subject".into(), "body".into(), "created_at".into()] },
            ],
            default_pages: vec![
                PageSuggestion { name: "Home".into(), route: "/".into(), description: "Hero section with featured projects".into() },
                PageSuggestion { name: "Projects".into(), route: "/projects".into(), description: "Full project gallery with filters".into() },
                PageSuggestion { name: "About".into(), route: "/about".into(), description: "Bio, skills, and experience timeline".into() },
                PageSuggestion { name: "Contact".into(), route: "/contact".into(), description: "Contact form and social links".into() },
            ],
            hidden_requirements: vec![
                Requirement { category: "seo".into(), description: "SEO optimization and OpenGraph metadata".into(), priority: RequirementPriority::Critical },
                Requirement { category: "performance".into(), description: "Fast loading with optimized images".into(), priority: RequirementPriority::Important },
                Requirement { category: "ux".into(), description: "Responsive design for mobile viewing".into(), priority: RequirementPriority::Critical },
                Requirement { category: "analytics".into(), description: "Basic visitor analytics".into(), priority: RequirementPriority::NiceToHave },
            ],
            ui_style_hint: Some(UiStyle::Minimal),
        },
    ]
}

// ---------------------------------------------------------------------------
// Domain pack matching
// ---------------------------------------------------------------------------

/// Match the best domain pack for a given description, if any.
/// Returns the pack and the number of trigger keywords matched.
pub fn match_domain_pack(description: &str) -> Option<(DomainPack, usize)> {
    let lower = description.to_lowercase();
    let packs = builtin_domain_packs();
    let mut best: Option<(DomainPack, usize)> = None;

    for pack in packs {
        let matches = pack
            .triggers
            .iter()
            .filter(|t| lower.contains(t.as_str()))
            .count();
        if matches > 0
            && best.as_ref().is_none_or(|(_, best_count)| matches > *best_count) {
                best = Some((pack, matches));
            }
    }

    best
}

// ---------------------------------------------------------------------------
// Confidence scoring helpers
// ---------------------------------------------------------------------------

/// Score confidence based on how a value was determined.
fn keyword_confidence(text: &str, exact_keywords: &[&str], partial_keywords: &[&str]) -> f32 {
    let lower = text.to_lowercase();
    let exact_hits = exact_keywords.iter().filter(|k| lower.contains(**k)).count();
    let partial_hits = partial_keywords.iter().filter(|k| lower.contains(**k)).count();

    if exact_hits >= 2 {
        0.95
    } else if exact_hits == 1 {
        0.85
    } else if partial_hits >= 2 {
        0.7
    } else if partial_hits == 1 {
        0.55
    } else {
        0.35
    }
}

/// Detect realtime needs from description keywords.
fn detect_realtime(text: &str) -> (bool, f32) {
    let lower = text.to_lowercase();
    let rt_keywords = [
        "realtime", "real-time", "live", "websocket", "notification",
        "chat", "streaming", "push", "instant",
    ];
    let hits = rt_keywords.iter().filter(|k| lower.contains(**k)).count();
    if hits >= 2 {
        (true, 0.9)
    } else if hits == 1 {
        (true, 0.7)
    } else {
        // Some app types imply realtime
        if lower.contains("dashboard") || lower.contains("monitoring") || lower.contains("social") {
            (true, 0.5)
        } else {
            (false, 0.8)
        }
    }
}

/// Detect domain from description.
fn detect_domain(text: &str) -> (String, f32) {
    let lower = text.to_lowercase();
    let domains = [
        (&["wine", "sommelier", "vineyard"][..], "Wine"),
        (&["food", "restaurant", "recipe", "cooking"][..], "Food & Dining"),
        (&["travel", "hotel", "flight", "vacation"][..], "Travel"),
        (&["fitness", "gym", "workout", "health"][..], "Health & Fitness"),
        (&["finance", "banking", "investment", "crypto"][..], "Finance"),
        (&["education", "learning", "course", "school"][..], "Education"),
        (&["fashion", "clothing", "apparel"][..], "Fashion"),
        (&["real estate", "property", "housing"][..], "Real Estate"),
        (&["music", "audio", "podcast"][..], "Music & Audio"),
        (&["tech", "developer", "software"][..], "Technology"),
    ];

    for (keywords, domain) in &domains {
        let hits = keywords.iter().filter(|k| lower.contains(**k)).count();
        if hits > 0 {
            let conf = if hits >= 2 { 0.95 } else { 0.8 };
            return (domain.to_string(), conf);
        }
    }

    ("General".into(), 0.4)
}

// ---------------------------------------------------------------------------
// Extract explicit features and constraints
// ---------------------------------------------------------------------------

fn extract_features(description: &str) -> Vec<IntentInference<String>> {
    let lower = description.to_lowercase();
    let feature_patterns: &[(&[&str], &str)] = &[
        (&["login", "signup", "sign up", "register", "authentication"], "User authentication (login/signup)"),
        (&["payment", "billing", "checkout", "stripe"], "Payment processing"),
        (&["search", "filter", "browse"], "Search and filtering"),
        (&["notification", "alert"], "Notification system"),
        (&["dashboard", "analytics", "metrics"], "Analytics dashboard"),
        (&["chat", "messaging", "message"], "Messaging / chat"),
        (&["upload", "image", "photo", "media"], "Media upload and management"),
        (&["admin", "admin panel", "back office"], "Admin panel"),
        (&["api", "rest", "webhook", "integration"], "API / integrations"),
        (&["team", "multi-user", "collaborate"], "Team collaboration"),
        (&["export", "csv", "pdf", "download"], "Data export"),
        (&["mobile", "responsive"], "Mobile-responsive design"),
    ];

    let mut features = Vec::new();
    for (keywords, feature_name) in feature_patterns {
        let hits = keywords.iter().filter(|k| lower.contains(**k)).count();
        if hits > 0 {
            features.push(IntentInference {
                value: feature_name.to_string(),
                confidence: if hits >= 2 { 0.95 } else { 0.8 },
                source: if hits >= 2 {
                    InferenceSource::ExplicitUserRequest
                } else {
                    InferenceSource::KeywordHeuristic
                },
                reasoning: format!(
                    "Matched {} keyword(s): {}",
                    hits,
                    keywords.iter().filter(|k| lower.contains(**k)).collect::<Vec<_>>().iter().map(|k| format!("\"{}\"", k)).collect::<Vec<_>>().join(", ")
                ),
            });
        }
    }
    features
}

fn extract_constraints(description: &str) -> Vec<IntentInference<String>> {
    let lower = description.to_lowercase();
    let constraint_patterns: &[(&[&str], &str)] = &[
        (&["no database", "no db", "static"], "No database required"),
        (&["no auth", "no login", "public"], "No authentication required"),
        (&["simple", "minimal", "basic"], "Keep it simple and minimal"),
        (&["fast", "performance", "speed"], "Performance is a priority"),
        (&["free", "no cost", "open source"], "Must be free / open source"),
        (&["mobile first", "mobile-first"], "Mobile-first design"),
    ];

    let mut constraints = Vec::new();
    for (keywords, constraint_name) in constraint_patterns {
        let hits = keywords.iter().filter(|k| lower.contains(**k)).count();
        if hits > 0 {
            constraints.push(IntentInference {
                value: constraint_name.to_string(),
                confidence: if hits >= 2 { 0.9 } else { 0.75 },
                source: InferenceSource::ExplicitUserRequest,
                reasoning: format!(
                    "User constraint detected via: {}",
                    keywords.iter().filter(|k| lower.contains(**k)).collect::<Vec<_>>().iter().map(|k| format!("\"{}\"", k)).collect::<Vec<_>>().join(", ")
                ),
            });
        }
    }
    constraints
}

// ---------------------------------------------------------------------------
// Main analysis function
// ---------------------------------------------------------------------------

/// Analyze a user description and produce a layered Intent.
///
/// The intent analysis path:
/// 1. Run deterministic heuristics (fast, no LLM)
/// 2. Check confidence scores
/// 3. If any key field < 0.7 confidence, optionally run semantic fallback
/// 4. Merge results (deterministic wins on conflict)
/// 5. Apply domain pack if matched
/// 6. Generate clarification suggestions for low-confidence fields
pub fn analyze(description: &str) -> Intent {
    let start = Instant::now();
    let lower = description.to_lowercase();

    // Step 1: Run the flat deterministic engine for base values
    let v1 = analyze_flat(description);

    // Step 2: Detect domain
    let (domain_name, domain_conf) = detect_domain(&lower);

    // Step 3: Match domain pack
    let domain_pack_match = match_domain_pack(description);
    let matched_pack_name = domain_pack_match.as_ref().map(|(p, _)| p.domain.clone());

    // Step 4: Score app_type confidence
    let app_type_conf = {
        let app_type_keywords: &[&str] = match v1.app_type {
            AppType::LandingPage => &["landing", "homepage"],
            AppType::SaasApp => &["saas", "subscription", "platform"],
            AppType::Dashboard => &["dashboard", "analytics", "monitor"],
            AppType::Marketplace => &["marketplace", "airbnb", "uber"],
            AppType::Crm => &["crm", "customer relationship", "sales"],
            AppType::ECommerce => &["shop", "store", "ecommerce", "e-commerce"],
            AppType::Portfolio => &["portfolio", "resume", "personal"],
            AppType::Blog => &["blog", "content", "articles"],
            AppType::InternalTool => &["internal", "admin", "back office", "tool"],
            AppType::ApiOnly => &["api"],
            AppType::MobileApp => &["mobile app"],
            AppType::Custom => &[],
        };
        let hits = app_type_keywords.iter().filter(|k| lower.contains(**k)).count();
        if hits >= 2 { 0.95 } else if hits == 1 { 0.8 } else { 0.4 }
    };

    // Step 5: Score auth/db confidence
    let auth_conf = keyword_confidence(
        &lower,
        &["login", "signup", "register", "account", "dashboard", "admin"],
        &["user", "member", "profile", "team"],
    );
    let db_conf = keyword_confidence(
        &lower,
        &["database", "store", "manage", "inventory", "records", "crud"],
        &["list", "track", "save", "data"],
    );

    // Step 6: Detect realtime
    let (realtime_needed, realtime_conf) = detect_realtime(&lower);

    // Step 7: Build suggested pages from flat intent + domain pack
    let mut suggested_pages: Vec<IntentInference<PageSuggestion>> = v1
        .suggested_pages
        .iter()
        .map(|name| {
            let route = format!("/{}", name.to_lowercase().replace(' ', "-"));
            IntentInference {
                value: PageSuggestion {
                    name: name.clone(),
                    route,
                    description: format!("{} page", name),
                },
                confidence: 0.7,
                source: InferenceSource::KeywordHeuristic,
                reasoning: format!("Inferred from flat engine for {:?} app type", v1.app_type),
            }
        })
        .collect();

    // Merge domain pack pages (higher confidence if from pack)
    if let Some((ref pack, trigger_count)) = domain_pack_match {
        let pack_conf = if trigger_count >= 2 { 0.9 } else { 0.75 };
        for page in &pack.default_pages {
            if !suggested_pages.iter().any(|p| p.value.name == page.name) {
                suggested_pages.push(IntentInference {
                    value: page.clone(),
                    confidence: pack_conf,
                    source: InferenceSource::DomainPack,
                    reasoning: format!("Default page from {} domain pack", pack.domain),
                });
            }
        }
    }

    // Step 8: Build entities from flat intent + domain pack
    let mut suggested_entities: Vec<IntentInference<EntitySuggestion>> = v1
        .suggested_entities
        .iter()
        .map(|name| IntentInference {
            value: EntitySuggestion {
                name: name.clone(),
                fields: vec!["id".into(), "name".into(), "created_at".into()],
            },
            confidence: 0.65,
            source: InferenceSource::KeywordHeuristic,
            reasoning: format!("Inferred entity '{}' from keyword matching", name),
        })
        .collect();

    if let Some((ref pack, trigger_count)) = domain_pack_match {
        let pack_conf = if trigger_count >= 2 { 0.9 } else { 0.75 };
        for ent in &pack.default_entities {
            if !suggested_entities.iter().any(|e| e.value.name == ent.name) {
                suggested_entities.push(IntentInference {
                    value: ent.clone(),
                    confidence: pack_conf,
                    source: InferenceSource::DomainPack,
                    reasoning: format!("Default entity from {} domain pack", pack.domain),
                });
            }
        }
    }

    // Step 9: Build agents from flat intent
    let suggested_agents: Vec<IntentInference<AgentSuggestion>> = v1
        .suggested_agents
        .iter()
        .map(|a| IntentInference {
            value: AgentSuggestion {
                name: a.name.clone(),
                role: a.agent_type.clone(),
                system_prompt: a.system_prompt.clone(),
            },
            confidence: 0.75,
            source: InferenceSource::KeywordHeuristic,
            reasoning: format!("Agent '{}' suggested by v1 engine", a.name),
        })
        .collect();

    // Step 10: Build hidden requirements from domain pack
    let hidden_requirements = if let Some((ref pack, trigger_count)) = domain_pack_match {
        let pack_conf = if trigger_count >= 2 { 0.85 } else { 0.65 };
        pack.hidden_requirements
            .iter()
            .map(|r| IntentInference {
                value: r.clone(),
                confidence: pack_conf,
                source: InferenceSource::DomainPack,
                reasoning: format!("Hidden requirement from {} domain pack", pack.domain),
            })
            .collect()
    } else {
        Vec::new()
    };

    // Step 11: UI style confidence
    let ui_style_conf = {
        let style_keywords: &[&str] = match v1.ui_style {
            UiStyle::Luxurious => &["luxury", "elegant", "premium", "refined", "wine"],
            UiStyle::Playful => &["playful", "fun", "colorful", "game"],
            UiStyle::Corporate => &["corporate", "enterprise", "business", "professional"],
            UiStyle::Technical => &["technical", "developer", "code", "api"],
            UiStyle::Bold => &["bold", "striking", "modern"],
            UiStyle::Minimal => &[], // default
        };
        let hits = style_keywords.iter().filter(|k| lower.contains(**k)).count();
        if hits >= 2 { 0.95 } else if hits == 1 { 0.8 } else { 0.4 }
    };

    // Step 12: Complexity confidence
    let complexity_conf = match v1.complexity {
        Complexity::Complex => {
            if v1.needs_auth && v1.needs_payments { 0.9 } else { 0.7 }
        }
        Complexity::Medium => {
            if v1.needs_database || v1.needs_auth { 0.8 } else { 0.6 }
        }
        Complexity::Simple => {
            if !v1.needs_auth && !v1.needs_database { 0.85 } else { 0.6 }
        }
    };

    // Step 13: Compute overall confidence
    let key_confidences = [app_type_conf, auth_conf, db_conf, complexity_conf, ui_style_conf];
    let overall_confidence = key_confidences.iter().sum::<f32>() / key_confidences.len() as f32;

    // Step 14: Generate ambiguity flags and clarification suggestions
    let mut ambiguity_flags = Vec::new();
    let mut clarification_suggestions = Vec::new();

    if app_type_conf < 0.5 {
        ambiguity_flags.push("app_type".into());
        clarification_suggestions.push(
            "What type of application do you want? (e.g., SaaS platform, marketplace, dashboard, e-commerce store)".into(),
        );
    }
    if auth_conf < 0.5 {
        ambiguity_flags.push("auth_needed".into());
        clarification_suggestions.push(
            "Will users need to create accounts and log in?".into(),
        );
    }
    if db_conf < 0.5 {
        ambiguity_flags.push("db_needed".into());
        clarification_suggestions.push(
            "Does the app need to store and manage data? (e.g., user profiles, products, orders)".into(),
        );
    }
    if domain_conf < 0.5 {
        ambiguity_flags.push("domain".into());
        clarification_suggestions.push(
            "What industry or domain is this for? (e.g., healthcare, finance, education, retail)".into(),
        );
    }
    if ui_style_conf < 0.5 {
        ambiguity_flags.push("ui_style".into());
        clarification_suggestions.push(
            "What visual style do you prefer? (minimal, corporate, playful, luxurious, bold, technical)".into(),
        );
    }

    // Short descriptions get extra clarification prompts
    if description.split_whitespace().count() < 5 {
        clarification_suggestions.push(
            "Could you describe the main features you need in more detail?".into(),
        );
    }

    let elapsed = start.elapsed();

    Intent {
        explicit: ExplicitIntent {
            raw_description: description.to_string(),
            extracted_features: extract_features(description),
            extracted_constraints: extract_constraints(description),
        },
        inferred: InferredIntent {
            app_type: IntentInference {
                value: v1.app_type.clone(),
                confidence: app_type_conf,
                source: InferenceSource::KeywordHeuristic,
                reasoning: format!("Detected {:?} from keyword analysis", v1.app_type),
            },
            complexity: IntentInference {
                value: v1.complexity.clone(),
                confidence: complexity_conf,
                source: InferenceSource::KeywordHeuristic,
                reasoning: format!("Complexity {:?} based on auth={}, db={}, payments={}", v1.complexity, v1.needs_auth, v1.needs_database, v1.needs_payments),
            },
            domain: IntentInference {
                value: domain_name,
                confidence: domain_conf,
                source: InferenceSource::KeywordHeuristic,
                reasoning: "Domain detected from keyword matching".into(),
            },
            suggested_pages,
            suggested_entities,
            suggested_agents,
            auth_needed: IntentInference {
                value: v1.needs_auth,
                confidence: auth_conf,
                source: InferenceSource::KeywordHeuristic,
                reasoning: format!("Auth {} based on keyword scoring", if v1.needs_auth { "required" } else { "not required" }),
            },
            db_needed: IntentInference {
                value: v1.needs_database,
                confidence: db_conf,
                source: InferenceSource::KeywordHeuristic,
                reasoning: format!("Database {} based on keyword scoring", if v1.needs_database { "required" } else { "not required" }),
            },
            realtime_needed: IntentInference {
                value: realtime_needed,
                confidence: realtime_conf,
                source: InferenceSource::KeywordHeuristic,
                reasoning: format!("Realtime {} based on keyword analysis", if realtime_needed { "detected" } else { "not detected" }),
            },
            ui_style: IntentInference {
                value: v1.ui_style.clone(),
                confidence: ui_style_conf,
                source: if let Some((ref pack, _)) = domain_pack_match {
                    if pack.ui_style_hint.is_some() && ui_style_conf < 0.5 {
                        InferenceSource::DomainPack
                    } else {
                        InferenceSource::KeywordHeuristic
                    }
                } else {
                    InferenceSource::KeywordHeuristic
                },
                reasoning: format!("UI style {:?} from keyword analysis", v1.ui_style),
            },
        },
        hidden: HiddenRequirements {
            requirements: hidden_requirements,
            domain_pack: matched_pack_name,
        },
        meta: IntentMeta {
            overall_confidence,
            ambiguity_flags,
            clarification_suggestions,
            analysis_duration_ms: elapsed.as_millis() as u64,
        },
    }
}

// ---------------------------------------------------------------------------
// Clarification API
// ---------------------------------------------------------------------------

/// A clarification question to present to the user for ambiguous fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClarificationQuestion {
    /// The intent field this question is about.
    pub field: String,
    /// The question to ask the user.
    pub question: String,
    /// Current confidence for this field.
    pub current_confidence: f32,
    /// Suggested answer options.
    pub options: Vec<String>,
}

/// Generate clarification questions for ambiguous intent fields.
///
/// Returns questions sorted by urgency (lowest confidence first).
pub fn generate_clarifications(intent: &Intent) -> Vec<ClarificationQuestion> {
    let mut questions = Vec::new();
    let threshold = 0.7;

    if intent.inferred.app_type.confidence < threshold {
        questions.push(ClarificationQuestion {
            field: "app_type".into(),
            question: "What type of application are you building?".into(),
            current_confidence: intent.inferred.app_type.confidence,
            options: vec![
                "SaaS Platform".into(),
                "Marketplace".into(),
                "Dashboard / Analytics".into(),
                "E-commerce Store".into(),
                "Blog / CMS".into(),
                "Portfolio / Personal Site".into(),
                "CRM".into(),
                "Internal Tool".into(),
                "Landing Page".into(),
            ],
        });
    }

    if intent.inferred.auth_needed.confidence < threshold {
        questions.push(ClarificationQuestion {
            field: "auth_needed".into(),
            question: "Will users need to create accounts and log in?".into(),
            current_confidence: intent.inferred.auth_needed.confidence,
            options: vec![
                "Yes, users need accounts".into(),
                "No, it's a public site".into(),
                "Yes, with different user roles (admin, user, etc.)".into(),
            ],
        });
    }

    if intent.inferred.db_needed.confidence < threshold {
        questions.push(ClarificationQuestion {
            field: "db_needed".into(),
            question: "Does the app need to store and manage data persistently?".into(),
            current_confidence: intent.inferred.db_needed.confidence,
            options: vec![
                "Yes, it needs a database".into(),
                "No, it's mostly static content".into(),
                "Yes, with complex relationships between entities".into(),
            ],
        });
    }

    if intent.inferred.domain.confidence < threshold {
        questions.push(ClarificationQuestion {
            field: "domain".into(),
            question: "What industry or domain is this application for?".into(),
            current_confidence: intent.inferred.domain.confidence,
            options: vec![
                "Technology / Software".into(),
                "Healthcare / Fitness".into(),
                "Finance / Banking".into(),
                "Education / Learning".into(),
                "Retail / E-commerce".into(),
                "Food / Restaurant".into(),
                "Travel / Hospitality".into(),
                "Real Estate".into(),
                "Other".into(),
            ],
        });
    }

    if intent.inferred.ui_style.confidence < threshold {
        questions.push(ClarificationQuestion {
            field: "ui_style".into(),
            question: "What visual style do you prefer for the UI?".into(),
            current_confidence: intent.inferred.ui_style.confidence,
            options: vec![
                "Minimal — clean and simple".into(),
                "Corporate — professional and structured".into(),
                "Playful — colorful and fun".into(),
                "Luxurious — elegant and premium".into(),
                "Bold — striking and modern".into(),
                "Technical — developer-focused".into(),
            ],
        });
    }

    if intent.inferred.realtime_needed.confidence < threshold {
        questions.push(ClarificationQuestion {
            field: "realtime_needed".into(),
            question: "Does the app need real-time features (live updates, notifications, chat)?".into(),
            current_confidence: intent.inferred.realtime_needed.confidence,
            options: vec![
                "Yes, live updates are important".into(),
                "No, standard page loads are fine".into(),
                "Maybe — notifications would be nice".into(),
            ],
        });
    }

    // Sort by lowest confidence first (most urgent)
    questions.sort_by(|a, b| a.current_confidence.partial_cmp(&b.current_confidence).unwrap_or(std::cmp::Ordering::Equal));

    questions
}

// ---------------------------------------------------------------------------
// Flat conversion
// ---------------------------------------------------------------------------

/// Convert the layered [`Intent`] into a simpler [`FlatIntent`].
///
/// Useful for subsystems that need quick access to top-level values without
/// dealing with confidence wrappers.
pub fn to_flat(intent: &Intent) -> FlatIntent {
    FlatIntent {
        app_type: intent.inferred.app_type.value.clone(),
        needs_auth: intent.inferred.auth_needed.value,
        needs_database: intent.inferred.db_needed.value,
        needs_payments: intent
            .explicit
            .extracted_features
            .iter()
            .any(|f| f.value.to_lowercase().contains("payment")),
        ui_style: intent.inferred.ui_style.value.clone(),
        complexity: intent.inferred.complexity.value.clone(),
        suggested_agents: intent
            .inferred
            .suggested_agents
            .iter()
            .map(|a| AgentTemplate {
                name: a.value.name.clone(),
                agent_type: a.value.role.clone(),
                description: format!("{} agent", a.value.name),
                ui_component: "ChatWidget".into(),
                api_route: format!("/api/{}", a.value.role),
                system_prompt: a.value.system_prompt.clone(),
                trigger: "floating_button".into(),
            })
            .collect(),
        suggested_entities: intent
            .inferred
            .suggested_entities
            .iter()
            .map(|e| e.value.name.clone())
            .collect(),
        suggested_pages: intent
            .inferred
            .suggested_pages
            .iter()
            .map(|p| p.value.name.clone())
            .collect(),
        inferred_features: intent
            .explicit
            .extracted_features
            .iter()
            .map(|f| f.value.clone())
            .collect(),
        confidence: intent.meta.overall_confidence as f64,
    }
}

// ---------------------------------------------------------------------------
// Flat (deterministic) analysis — used internally by `analyze()`
// ---------------------------------------------------------------------------

/// Perform a flat, deterministic analysis of a user description.
///
/// This is a fast, keyword-based analysis that does not use confidence scores.
/// It is called internally by [`analyze`] to seed the layered intent, and can
/// also be called directly by subsystems that need a quick, simple result.
pub fn analyze_flat(description: &str) -> FlatIntent {
    let lower = description.to_lowercase();

    let app_type = flat_detect_app_type(&lower);

    let auth_keywords = [
        "login", "signup", "sign up", "register", "account", "user profile",
        "dashboard", "admin", "roles", "permissions", "member", "subscription",
        "billing", "saas", "crm", "portal", "multi-user", "tenant",
    ];
    let no_auth_keywords = [
        "landing", "portfolio", "blog", "calculator", "converter", "display",
        "showcase", "brochure", "single page", "static",
    ];

    let auth_score: i32 = auth_keywords.iter().filter(|k| lower.contains(*k)).count() as i32;
    let no_auth_score: i32 = no_auth_keywords.iter().filter(|k| lower.contains(*k)).count() as i32;
    let needs_auth = match app_type {
        AppType::SaasApp | AppType::Crm | AppType::InternalTool => true,
        AppType::LandingPage | AppType::Portfolio | AppType::Blog => auth_score > no_auth_score + 1,
        _ => auth_score > no_auth_score,
    };

    let db_keywords = [
        "store", "save", "track", "manage", "list", "inventory", "catalog",
        "orders", "products", "customers", "records", "database", "data", "crud",
    ];
    let needs_database = matches!(
        app_type,
        AppType::SaasApp | AppType::Dashboard | AppType::Marketplace
            | AppType::Crm | AppType::ECommerce | AppType::InternalTool
    ) || db_keywords.iter().any(|k| lower.contains(k));

    let payment_keywords = [
        "payment", "pay", "billing", "subscription", "stripe", "checkout",
        "cart", "buy", "purchase", "pricing", "plan",
    ];
    let needs_payments = payment_keywords.iter().any(|k| lower.contains(k));

    let ui_style = flat_detect_ui_style(&lower);

    let complexity = if needs_auth && needs_payments {
        Complexity::Complex
    } else if needs_database || needs_auth {
        Complexity::Medium
    } else {
        Complexity::Simple
    };

    let mut suggested_agents = Vec::new();

    if lower.contains("ai") || lower.contains("agent") || lower.contains("chat")
        || lower.contains("recommend") || lower.contains("suggest")
        || lower.contains("assistant") || lower.contains("sommelier")
        || lower.contains("advisor") || lower.contains("bot")
    {
        let domain = flat_detect_domain(&lower);
        suggested_agents.push(flat_build_chatbot_template(&domain, &lower));
    }

    if needs_auth
        && (lower.contains("support") || lower.contains("help")
            || matches!(app_type, AppType::SaasApp | AppType::ECommerce))
    {
        suggested_agents.push(AgentTemplate {
            name: "Support Assistant".into(),
            agent_type: "support".into(),
            description: "Helps users with questions and issues".into(),
            ui_component: "FloatingChatWidget".into(),
            api_route: "/api/support/chat".into(),
            system_prompt: "You are a friendly support assistant. Help users with their questions about the product. Be concise and helpful.".into(),
            trigger: "floating_button".into(),
        });
    }

    if matches!(app_type, AppType::ECommerce | AppType::Marketplace) || lower.contains("recommend") {
        let domain = flat_detect_domain(&lower);
        suggested_agents.push(AgentTemplate {
            name: format!("{} Recommender", domain),
            agent_type: "recommendation".into(),
            description: format!("Suggests {} based on user preferences", domain.to_lowercase()),
            ui_component: "RecommendationPanel".into(),
            api_route: "/api/recommend".into(),
            system_prompt: format!(
                "You are a {} recommendation expert. Based on the user's preferences and history, suggest the best options. Be specific and explain why each recommendation fits.",
                domain
            ),
            trigger: "page_section".into(),
        });
    }

    if matches!(app_type, AppType::Dashboard | AppType::SaasApp | AppType::InternalTool) {
        suggested_agents.push(AgentTemplate {
            name: "Analytics Assistant".into(),
            agent_type: "analytics".into(),
            description: "Explains data trends and answers questions about metrics".into(),
            ui_component: "AnalyticsChat".into(),
            api_route: "/api/analytics/ask".into(),
            system_prompt: "You are a data analyst. Explain trends, answer questions about metrics, and provide actionable insights. Use numbers and be specific.".into(),
            trigger: "sidebar".into(),
        });
    }

    let suggested_entities = flat_infer_entities(&lower, &app_type);
    let suggested_pages = flat_infer_pages(&app_type, needs_auth, needs_payments, &suggested_agents);

    let mut features = Vec::new();
    if needs_auth { features.push("Authentication (email + password)".into()); }
    if needs_database { features.push("Database (SQLite)".into()); }
    if needs_payments { features.push("Payments (Stripe)".into()); }
    if !suggested_agents.is_empty() { features.push(format!("{} AI agent(s)", suggested_agents.len())); }
    for page in &suggested_pages { features.push(format!("Page: {}", page)); }

    let confidence = if auth_score + no_auth_score > 3 { 0.9 }
        else if auth_score + no_auth_score > 1 { 0.7 }
        else { 0.5 };

    FlatIntent {
        app_type, needs_auth, needs_database, needs_payments, ui_style, complexity,
        suggested_agents, suggested_entities, suggested_pages, inferred_features: features, confidence,
    }
}

fn flat_detect_app_type(text: &str) -> AppType {
    if text.contains("landing") || text.contains("homepage")
        || (text.contains("page") && !text.contains("dashboard"))
    {
        AppType::LandingPage
    } else if text.contains("saas") || text.contains("subscription") || text.contains("platform") {
        AppType::SaasApp
    } else if text.contains("dashboard") || text.contains("analytics") || text.contains("monitor") {
        AppType::Dashboard
    } else if text.contains("marketplace") || text.contains("airbnb") || text.contains("uber") {
        AppType::Marketplace
    } else if text.contains("crm") || text.contains("customer relationship") || text.contains("sales") {
        AppType::Crm
    } else if text.contains("shop") || text.contains("store") || text.contains("ecommerce") || text.contains("e-commerce") || text.contains("buy") {
        AppType::ECommerce
    } else if text.contains("portfolio") || text.contains("resume") || text.contains("personal") {
        AppType::Portfolio
    } else if text.contains("blog") || text.contains("content") || text.contains("articles") {
        AppType::Blog
    } else if text.contains("internal") || text.contains("admin") || text.contains("back office") || text.contains("tool") {
        AppType::InternalTool
    } else if text.contains("api") && !text.contains("page") && !text.contains("ui") {
        AppType::ApiOnly
    } else {
        AppType::Custom
    }
}

fn flat_detect_ui_style(text: &str) -> UiStyle {
    if text.contains("luxury") || text.contains("elegant") || text.contains("premium") || text.contains("wine") || text.contains("refined") {
        UiStyle::Luxurious
    } else if text.contains("playful") || text.contains("fun") || text.contains("colorful") || text.contains("kids") || text.contains("game") {
        UiStyle::Playful
    } else if text.contains("corporate") || text.contains("enterprise") || text.contains("business") || text.contains("professional") {
        UiStyle::Corporate
    } else if text.contains("technical") || text.contains("developer") || text.contains("code") || text.contains("api") {
        UiStyle::Technical
    } else if text.contains("bold") || text.contains("striking") || text.contains("modern") {
        UiStyle::Bold
    } else {
        UiStyle::Minimal
    }
}

fn flat_detect_domain(text: &str) -> String {
    let domains = [
        ("wine", "Wine"), ("food", "Food"), ("restaurant", "Restaurant"),
        ("travel", "Travel"), ("hotel", "Hotel"), ("book", "Book"),
        ("music", "Music"), ("movie", "Movie"), ("fashion", "Fashion"),
        ("fitness", "Fitness"), ("health", "Health"), ("real estate", "Real Estate"),
        ("car", "Automotive"), ("tech", "Technology"), ("education", "Education"),
        ("finance", "Finance"), ("crypto", "Crypto"),
    ];
    for (keyword, domain) in &domains {
        if text.contains(keyword) { return domain.to_string(); }
    }
    "Product".to_string()
}

fn flat_build_chatbot_template(domain: &str, text: &str) -> AgentTemplate {
    let (name, prompt) = match domain.to_lowercase().as_str() {
        "wine" => ("AI Sommelier", "You are an expert sommelier. Help users discover wines based on their taste preferences, food pairings, budget, and occasion. Share interesting facts about wines, regions, and vintages. Be warm and passionate about wine."),
        "food" | "restaurant" => ("Food Advisor", "You are a food expert. Help users find dishes, suggest pairings, accommodate dietary restrictions, and share cooking tips. Be enthusiastic about food."),
        "travel" | "hotel" => ("Travel Concierge", "You are a travel expert. Help users plan trips, find destinations, suggest activities, and navigate logistics. Be adventurous and knowledgeable."),
        "fitness" | "health" => ("Wellness Coach", "You are a wellness expert. Help users with workout plans, nutrition advice, and health goals. Be motivating and supportive."),
        "finance" | "crypto" => ("Financial Advisor", "You are a financial expert. Help users understand investments, budgeting, and financial planning. Always include disclaimers about not being professional financial advice."),
        "education" => ("Learning Assistant", "You are an education expert. Help users learn new topics, explain concepts clearly, suggest learning paths, and quiz them on knowledge. Be patient and encouraging."),
        _ => ("AI Assistant", "You are a helpful AI assistant for this application. Answer user questions, provide recommendations, and help them navigate the platform. Be concise and friendly."),
    };
    AgentTemplate {
        name: name.to_string(),
        agent_type: "chatbot".into(),
        description: format!("{} powered by AI", name),
        ui_component: "ChatWidget".into(),
        api_route: "/api/chat".into(),
        system_prompt: prompt.to_string(),
        trigger: if text.contains("page") || text.contains("section") { "page_section" } else { "floating_button" }.to_string(),
    }
}

fn flat_infer_entities(text: &str, app_type: &AppType) -> Vec<String> {
    let mut entities = Vec::new();
    let entity_patterns: &[(&str, &[&str])] = &[
        ("wine", &["Wine", "WineCategory", "TastingNote"]),
        ("product", &["Product", "Category"]),
        ("order", &["Order", "OrderItem"]),
        ("booking", &["Booking", "Listing"]),
        ("event", &["Event", "Registration"]),
        ("article", &["Article", "Tag"]),
        ("task", &["Task", "Project"]),
        ("customer", &["Customer", "Interaction"]),
        ("invoice", &["Invoice", "LineItem"]),
    ];
    for (keyword, ents) in entity_patterns {
        if text.contains(keyword) { entities.extend(ents.iter().map(|s| s.to_string())); }
    }
    match app_type {
        AppType::ECommerce if entities.is_empty() => entities.extend(["Product", "Order", "Customer"].iter().map(|s| s.to_string())),
        AppType::Marketplace if entities.is_empty() => entities.extend(["Listing", "Booking", "Review"].iter().map(|s| s.to_string())),
        AppType::Crm if entities.is_empty() => entities.extend(["Contact", "Deal", "Activity"].iter().map(|s| s.to_string())),
        AppType::Blog if entities.is_empty() => entities.extend(["Post", "Category", "Comment"].iter().map(|s| s.to_string())),
        _ => {}
    }
    entities.dedup();
    entities
}

fn flat_infer_pages(app_type: &AppType, has_auth: bool, has_payments: bool, agents: &[AgentTemplate]) -> Vec<String> {
    let mut pages = vec!["Home".to_string()];
    match app_type {
        AppType::LandingPage => {}
        AppType::SaasApp => { pages.push("Dashboard".into()); if has_auth { pages.push("Settings".into()); } }
        AppType::Dashboard => { pages.push("Dashboard".into()); pages.push("Reports".into()); }
        AppType::ECommerce => { pages.push("Products".into()); pages.push("Cart".into()); if has_payments { pages.push("Checkout".into()); } }
        AppType::Marketplace => { pages.push("Browse".into()); pages.push("Listing Detail".into()); if has_payments { pages.push("Booking".into()); } }
        AppType::Crm => { pages.push("Contacts".into()); pages.push("Deals".into()); pages.push("Pipeline".into()); }
        AppType::Blog => { pages.push("Articles".into()); pages.push("Article Detail".into()); }
        AppType::Portfolio => { pages.push("Projects".into()); pages.push("About".into()); pages.push("Contact".into()); }
        _ => {}
    }
    if has_auth { pages.push("Login".into()); pages.push("Register".into()); }
    if has_payments { pages.push("Pricing".into()); }
    for agent in agents {
        if agent.trigger == "page_section" { pages.push(format!("{} (AI)", agent.name)); }
    }
    pages
}

/// Infer hidden requirements from a flat intent analysis.
///
/// Deterministic, rule-based. For example:
/// "CRM" implies auth, roles, search, filtering, dashboard, analytics.
/// "Marketplace" implies payments, escrow, reviews, moderation, disputes.
pub fn infer_flat_hidden_requirements(intent: &FlatIntent, description: &str) -> FlatHiddenRequirements {
    let lower = description.to_lowercase();
    let mut features: Vec<String> = Vec::new();
    let mut pages: Vec<String> = Vec::new();
    let mut entities: Vec<String> = Vec::new();
    let mut reasoning: Vec<String> = Vec::new();
    let mut needs_auth = false;
    let mut needs_database = false;
    let mut needs_payments = false;
    let mut confidence: f64 = 0.7;

    match intent.app_type {
        AppType::Crm => {
            needs_auth = true; needs_database = true;
            features.extend(["Role-based access control".into(), "Search and filtering".into(),
                "Contact management".into(), "Activity timeline".into(),
                "Dashboard with KPIs".into(), "Data export (CSV)".into(),
                "Email integration".into(), "Notes and comments".into()]);
            pages.extend(["Contacts".into(), "Deals".into(), "Pipeline".into(),
                "Reports".into(), "Settings".into(), "Activity".into()]);
            entities.extend(["Contact".into(), "Deal".into(), "Activity".into(),
                "Note".into(), "Tag".into(), "Pipeline".into(), "Stage".into()]);
            reasoning.push("CRM requires contact management, pipeline, team access".into());
            confidence = 0.9;
        }
        AppType::Marketplace => {
            needs_auth = true; needs_database = true; needs_payments = true;
            features.extend(["Search with filters".into(), "Ratings and reviews".into(),
                "Secure payments (escrow)".into(), "Seller/buyer profiles".into(),
                "Messaging system".into(), "Content moderation".into(),
                "Dispute resolution".into(), "Listing management".into(),
                "Favorites/wishlist".into(), "Notifications".into()]);
            pages.extend(["Browse/Search".into(), "Listing Detail".into(),
                "Seller Dashboard".into(), "Messages".into(), "Reviews".into(), "Checkout".into()]);
            entities.extend(["Listing".into(), "Review".into(), "Message".into(),
                "Transaction".into(), "Dispute".into(), "Favorite".into(), "Category".into()]);
            reasoning.push("Marketplaces need trust (reviews), payments, moderation".into());
            confidence = 0.92;
        }
        AppType::SaasApp => {
            needs_auth = true; needs_database = true;
            features.extend(["User onboarding flow".into(), "Settings page".into(),
                "Billing/subscription management".into(), "Team management".into(),
                "Notification system".into(), "API key management".into(),
                "Usage analytics".into(), "Data export".into()]);
            pages.extend(["Onboarding".into(), "Settings".into(), "Billing".into(),
                "Team".into(), "API Keys".into()]);
            entities.extend(["Organization".into(), "Subscription".into(),
                "Invitation".into(), "ApiKey".into(), "AuditLog".into()]);
            reasoning.push("SaaS needs multi-tenancy, billing, team mgmt, onboarding".into());
            confidence = 0.88;
        }
        AppType::ECommerce => {
            needs_auth = true; needs_database = true; needs_payments = true;
            features.extend(["Product search with filters".into(), "Shopping cart".into(),
                "Checkout flow".into(), "Order tracking".into(), "Wishlist".into(),
                "Product reviews".into(), "Inventory management".into(),
                "Email notifications".into()]);
            pages.extend(["Product Catalog".into(), "Product Detail".into(),
                "Cart".into(), "Checkout".into(), "Order History".into(), "Wishlist".into()]);
            entities.extend(["Product".into(), "Category".into(), "CartItem".into(),
                "Order".into(), "OrderItem".into(), "Review".into(), "Address".into()]);
            reasoning.push("E-commerce needs cart, checkout, orders, product discovery".into());
            confidence = 0.9;
        }
        AppType::Dashboard => {
            needs_auth = true; needs_database = true;
            features.extend(["Data visualization (charts)".into(), "Date range filtering".into(),
                "Real-time updates".into(), "Export to PDF/CSV".into(),
                "Role-based dashboards".into()]);
            pages.extend(["Overview".into(), "Analytics".into(), "Reports".into()]);
            entities.extend(["Metric".into(), "Report".into(), "DataSource".into()]);
            reasoning.push("Dashboards need visualization, filtering, export".into());
            confidence = 0.85;
        }
        AppType::InternalTool => {
            needs_auth = true; needs_database = true;
            features.extend(["Role-based access".into(), "Audit logging".into(),
                "Data tables with sorting/filtering".into(), "Bulk operations".into(), "Search".into()]);
            pages.extend(["Admin".into(), "Audit Log".into()]);
            entities.extend(["AuditLog".into()]);
            reasoning.push("Internal tools need access control, audit trails".into());
            confidence = 0.82;
        }
        AppType::Blog => {
            needs_database = true;
            features.extend(["Rich text editor".into(), "Categories and tags".into(),
                "SEO metadata".into(), "RSS feed".into()]);
            pages.extend(["Article".into(), "Category".into(), "Archive".into()]);
            entities.extend(["Post".into(), "Category".into(), "Tag".into()]);
            reasoning.push("Blogs need content management, categorization, SEO".into());
            confidence = 0.8;
        }
        _ => {}
    }

    if lower.contains("team") || lower.contains("multi-user") || lower.contains("collaborate") {
        if !features.iter().any(|f| f.contains("Team")) {
            features.push("Team management".into());
            entities.push("TeamMember".into());
        }
        needs_auth = true;
    }
    if (lower.contains("analytics") || lower.contains("metrics") || lower.contains("report"))
        && !features.iter().any(|f| f.to_lowercase().contains("analytics")) {
            features.push("Analytics dashboard".into());
            pages.push("Analytics".into());
        }
    if (lower.contains("notification") || lower.contains("alert"))
        && !features.iter().any(|f| f.contains("Notification")) {
            features.push("Notification system".into());
        }
    if (lower.contains("search") || lower.contains("filter") || lower.contains("browse"))
        && !features.iter().any(|f| f.to_lowercase().contains("search")) {
            features.push("Search and filtering".into());
        }
    if lower.contains("admin") || lower.contains("manage") {
        if !features.iter().any(|f| f.contains("Admin")) {
            features.push("Admin panel".into());
            pages.push("Admin".into());
        }
        needs_auth = true;
    }
    if (lower.contains("api") || lower.contains("webhook") || lower.contains("integration"))
        && !features.iter().any(|f| f.contains("API") || f.contains("REST")) {
            features.push("REST API".into());
        }

    if description.len() > 100 { confidence = (confidence + 0.05).min(1.0); }
    if description.len() < 20 { confidence = (confidence - 0.15).max(0.3); }

    FlatHiddenRequirements {
        needs_auth, needs_database, needs_payments,
        additional_features: features, additional_pages: pages,
        additional_entities: entities, confidence, reasoning,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "intent_engine_tests.rs"]
mod intent_engine_tests;
