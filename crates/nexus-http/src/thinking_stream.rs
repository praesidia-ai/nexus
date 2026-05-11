//! Thinking Stream — human-readable narrative of what Nexus is doing.
//!
//! Upgrades raw `PipelineEvent` steps into friendly, non-technical thinking
//! messages that make the system feel intelligent and transparent.
//!
//! Examples:
//! - "Analyzing your request..."
//! - "Designing the database schema..."
//! - "Optimizing the user experience..."
//!
//! Rules:
//! - Must be human-readable (no technical jargon)
//! - Must reflect real pipeline steps (not fake/generic)
//! - Must feel progressive (each message builds on the last)

use serde::{Deserialize, Serialize};

use crate::explain_engine::ExplainCategory;
use crate::intent_engine::{AppType, Complexity, FlatIntent};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single thinking message streamed to the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingMessage {
    /// The user-facing narrative.
    pub message: String,
    /// Optional detail (shown smaller / expandable).
    pub detail: Option<String>,
    /// Category icon hint for the frontend.
    pub icon: ThinkingIcon,
    /// Progress percentage (0–100).
    pub progress: u32,
    /// Elapsed time since pipeline start.
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingIcon {
    Brain,
    Palette,
    Code,
    Database,
    Shield,
    Rocket,
    Sparkle,
    Search,
    Check,
    Warning,
}

// ---------------------------------------------------------------------------
// Step → Thinking message mapping
// ---------------------------------------------------------------------------

/// Map a pipeline step name to a human-readable thinking message.
///
/// The step names come from `ExecutionPipeline::build_step_list()`:
/// "generate_spec", "validate_spec", "generate_schema", "generate_api",
/// "generate_pages", "generate_agents", "validate_all", "commit_files",
/// "install_deps", "health_check"
pub fn step_to_thinking(
    step_name: &str,
    step_status: &str,
    intent: &FlatIntent,
    progress: u32,
    elapsed_ms: u64,
) -> ThinkingMessage {
    match (step_name, step_status) {
        // ── Intent analysis ──
        ("analyze_intent", "running") => ThinkingMessage {
            message: "Understanding what you want to build...".into(),
            detail: Some(format!(
                "Detected a {} application",
                app_type_friendly(&intent.app_type)
            )),
            icon: ThinkingIcon::Brain,
            progress,
            elapsed_ms,
        },
        ("analyze_intent", "completed") => ThinkingMessage {
            message: format!(
                "Got it — building a {} for you",
                app_type_friendly(&intent.app_type)
            ),
            detail: Some(format!(
                "{} complexity, {} pages planned",
                complexity_friendly(&intent.complexity),
                intent.suggested_pages.len()
            )),
            icon: ThinkingIcon::Check,
            progress,
            elapsed_ms,
        },

        // ── Spec generation ──
        ("generate_spec", "running") => ThinkingMessage {
            message: "Designing the complete application...".into(),
            detail: Some(feature_summary(intent)),
            icon: ThinkingIcon::Brain,
            progress,
            elapsed_ms,
        },
        ("generate_spec", "completed") => ThinkingMessage {
            message: "Application blueprint ready".into(),
            detail: None,
            icon: ThinkingIcon::Check,
            progress,
            elapsed_ms,
        },

        // ── Spec validation ──
        ("validate_spec", "running") => ThinkingMessage {
            message: "Reviewing the design for quality...".into(),
            detail: Some("Checking consistency, completeness, and best practices".into()),
            icon: ThinkingIcon::Search,
            progress,
            elapsed_ms,
        },
        ("validate_spec", "completed") => ThinkingMessage {
            message: "Design validated — everything checks out".into(),
            detail: None,
            icon: ThinkingIcon::Check,
            progress,
            elapsed_ms,
        },

        // ── Schema generation ──
        ("generate_schema", "running") => ThinkingMessage {
            message: "Setting up the database structure...".into(),
            detail: Some(format!(
                "Creating tables for {}",
                intent.suggested_entities.join(", ")
            )),
            icon: ThinkingIcon::Database,
            progress,
            elapsed_ms,
        },
        ("generate_schema", "completed") => ThinkingMessage {
            message: "Database structure ready".into(),
            detail: None,
            icon: ThinkingIcon::Check,
            progress,
            elapsed_ms,
        },

        // ── API generation ──
        ("generate_api", "running") => ThinkingMessage {
            message: "Building the API layer...".into(),
            detail: Some("Creating endpoints, validation, and error handling".into()),
            icon: ThinkingIcon::Code,
            progress,
            elapsed_ms,
        },
        ("generate_api", "completed") => ThinkingMessage {
            message: "API endpoints ready".into(),
            detail: None,
            icon: ThinkingIcon::Check,
            progress,
            elapsed_ms,
        },

        // ── UI page generation ──
        ("generate_pages", "running") => ThinkingMessage {
            message: "Crafting the user interface...".into(),
            detail: Some(format!(
                "Building {} pages with {} design style",
                intent.suggested_pages.len(),
                ui_style_friendly(&intent.ui_style)
            )),
            icon: ThinkingIcon::Palette,
            progress,
            elapsed_ms,
        },
        ("generate_pages", "completed") => ThinkingMessage {
            message: "All pages designed and built".into(),
            detail: None,
            icon: ThinkingIcon::Check,
            progress,
            elapsed_ms,
        },

        // ── Agent generation ──
        ("generate_agents", "running") => ThinkingMessage {
            message: "Adding AI intelligence to the app...".into(),
            detail: Some(format!(
                "Setting up {} AI {}",
                intent.suggested_agents.len(),
                if intent.suggested_agents.len() == 1 {
                    "agent"
                } else {
                    "agents"
                }
            )),
            icon: ThinkingIcon::Sparkle,
            progress,
            elapsed_ms,
        },
        ("generate_agents", "completed") => ThinkingMessage {
            message: "AI agents integrated".into(),
            detail: None,
            icon: ThinkingIcon::Check,
            progress,
            elapsed_ms,
        },

        // ── Validation ──
        ("validate_all", "running") => ThinkingMessage {
            message: "Running quality checks on everything...".into(),
            detail: Some(
                "Static analysis, accessibility, responsiveness, security"
                    .into(),
            ),
            icon: ThinkingIcon::Shield,
            progress,
            elapsed_ms,
        },
        ("validate_all", "completed") => ThinkingMessage {
            message: "All quality checks passed".into(),
            detail: None,
            icon: ThinkingIcon::Check,
            progress,
            elapsed_ms,
        },

        // ── File commit ──
        ("commit_files", "running") => ThinkingMessage {
            message: "Saving all files to your project...".into(),
            detail: None,
            icon: ThinkingIcon::Code,
            progress,
            elapsed_ms,
        },
        ("commit_files", "completed") => ThinkingMessage {
            message: "All files saved".into(),
            detail: None,
            icon: ThinkingIcon::Check,
            progress,
            elapsed_ms,
        },

        // ── Install dependencies ──
        ("install_deps", "running") => ThinkingMessage {
            message: "Installing dependencies...".into(),
            detail: Some("This usually takes a few seconds".into()),
            icon: ThinkingIcon::Rocket,
            progress,
            elapsed_ms,
        },
        ("install_deps", "completed") => ThinkingMessage {
            message: "Dependencies installed".into(),
            detail: None,
            icon: ThinkingIcon::Check,
            progress,
            elapsed_ms,
        },

        // ── Health check ──
        ("health_check", "running") => ThinkingMessage {
            message: "Starting your app and checking everything works...".into(),
            detail: None,
            icon: ThinkingIcon::Rocket,
            progress,
            elapsed_ms,
        },
        ("health_check", "completed") => ThinkingMessage {
            message: "Your app is live and ready!".into(),
            detail: None,
            icon: ThinkingIcon::Sparkle,
            progress,
            elapsed_ms,
        },

        // ── Skipped steps ──
        (name, "skipped") => ThinkingMessage {
            message: format!("Skipping {} — not needed for this app", step_friendly(name)),
            detail: None,
            icon: ThinkingIcon::Check,
            progress,
            elapsed_ms,
        },

        // ── Failed steps ──
        (name, "failed") => ThinkingMessage {
            message: format!("Hit an issue with {} — working around it", step_friendly(name)),
            detail: None,
            icon: ThinkingIcon::Warning,
            progress,
            elapsed_ms,
        },

        // ── Fallback ──
        (name, status) => ThinkingMessage {
            message: format!("{} {}...", step_friendly(name), status),
            detail: None,
            icon: ThinkingIcon::Brain,
            progress,
            elapsed_ms,
        },
    }
}

/// Generate a thinking message for an explanation event.
pub fn explain_to_thinking(
    category: &ExplainCategory,
    decision: &str,
    progress: u32,
    elapsed_ms: u64,
) -> ThinkingMessage {
    let (icon, prefix) = match category {
        ExplainCategory::Architecture => (ThinkingIcon::Brain, "Decided"),
        ExplainCategory::Design => (ThinkingIcon::Palette, "Choosing"),
        ExplainCategory::Agent => (ThinkingIcon::Sparkle, "Configuring"),
        ExplainCategory::Content => (ThinkingIcon::Code, "Generating"),
        ExplainCategory::Validation => (ThinkingIcon::Shield, "Validating"),
        ExplainCategory::Optimization => (ThinkingIcon::Rocket, "Optimizing"),
        ExplainCategory::Improvement => (ThinkingIcon::Sparkle, "Improving"),
    };

    ThinkingMessage {
        message: format!("{}: {}", prefix, lowercase_first(decision)),
        detail: None,
        icon,
        progress,
        elapsed_ms,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn app_type_friendly(app_type: &AppType) -> &'static str {
    match app_type {
        AppType::LandingPage => "landing page",
        AppType::SaasApp => "SaaS application",
        AppType::Dashboard => "dashboard",
        AppType::Marketplace => "marketplace",
        AppType::Crm => "CRM system",
        AppType::ECommerce => "e-commerce store",
        AppType::Portfolio => "portfolio",
        AppType::Blog => "blog",
        AppType::InternalTool => "internal tool",
        AppType::MobileApp => "mobile app",
        AppType::ApiOnly => "API service",
        AppType::Custom => "custom application",
    }
}

fn complexity_friendly(complexity: &Complexity) -> &'static str {
    match complexity {
        Complexity::Simple => "Simple",
        Complexity::Medium => "Medium",
        Complexity::Complex => "Full",
    }
}

fn ui_style_friendly(style: &crate::intent_engine::UiStyle) -> &'static str {
    match style {
        crate::intent_engine::UiStyle::Minimal => "minimal",
        crate::intent_engine::UiStyle::Corporate => "professional",
        crate::intent_engine::UiStyle::Playful => "playful",
        crate::intent_engine::UiStyle::Luxurious => "luxury",
        crate::intent_engine::UiStyle::Technical => "technical",
        crate::intent_engine::UiStyle::Bold => "bold",
    }
}

fn step_friendly(step_name: &str) -> String {
    match step_name {
        "generate_spec" => "application design".into(),
        "validate_spec" => "design review".into(),
        "generate_schema" => "database setup".into(),
        "generate_api" => "API creation".into(),
        "generate_pages" => "page building".into(),
        "generate_agents" => "AI agent setup".into(),
        "validate_all" => "quality checks".into(),
        "commit_files" => "file saving".into(),
        "install_deps" => "dependency installation".into(),
        "health_check" => "launch check".into(),
        other => other.replace('_', " "),
    }
}

fn feature_summary(intent: &FlatIntent) -> String {
    let mut parts = Vec::new();
    if intent.needs_auth {
        parts.push("user accounts");
    }
    if intent.needs_database {
        parts.push("data storage");
    }
    if intent.needs_payments {
        parts.push("payments");
    }
    if !intent.suggested_agents.is_empty() {
        parts.push("AI agents");
    }
    if parts.is_empty() {
        "A clean, focused application".into()
    } else {
        format!("Including {}", parts.join(", "))
    }
}

fn lowercase_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
    }
}
