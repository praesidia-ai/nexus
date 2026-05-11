//! Agent Auto-Designer — automatically designs AI agents per project.
//!
//! Given an intent and description, this module determines which agents
//! should exist in the generated app, their roles, system prompts,
//! tools, and trigger conditions.
//!
//! This is 100% deterministic (no LLM). The LLM only generates the
//! agent implementation code later in the pipeline.

use serde::Serialize;

use crate::intent_engine::{AppType, FlatIntent};

/// A fully designed agent ready for implementation.
#[derive(Debug, Clone, Serialize)]
pub struct DesignedAgent {
    /// Agent name (e.g., "Support Assistant", "AI Sommelier").
    pub name: String,
    /// Agent type category.
    pub agent_type: AgentType,
    /// One-line description of what this agent does.
    pub description: String,
    /// System prompt for the agent.
    pub system_prompt: String,
    /// Tools the agent should have access to.
    pub tools: Vec<String>,
    /// When this agent should be triggered.
    pub trigger: AgentTrigger,
    /// Where in the UI this agent appears.
    pub ui_placement: String,
    /// API route for this agent.
    pub api_route: String,
    /// Priority (1 = highest, lower is more important).
    pub priority: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentType {
    Support,
    Recommendation,
    Analytics,
    Onboarding,
    FraudDetection,
    ContentModeration,
    PersonalAssistant,
    DataAnalysis,
    Search,
    Notification,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentTrigger {
    /// Always visible (floating button).
    AlwaysVisible,
    /// Triggered after a delay.
    AfterDelay { seconds: u32 },
    /// Triggered on a specific page.
    OnPage { route: String },
    /// Triggered by user action.
    OnAction { action: String },
    /// Runs in the background.
    Background,
    /// Inline in a page section.
    Inline { section: String },
}

/// Design agents automatically based on intent and description.
pub fn design_agents(intent: &FlatIntent, description: &str) -> Vec<DesignedAgent> {
    let lower = description.to_lowercase();
    let mut agents = Vec::new();

    // ── App-type-specific agents ────────────────────────────────────────

    match intent.app_type {
        AppType::SaasApp => {
            agents.push(DesignedAgent {
                name: "Support Assistant".into(),
                agent_type: AgentType::Support,
                description: "Helps users navigate the app, answers questions, resolves issues".into(),
                system_prompt: "You are a helpful support assistant for this SaaS application. \
                    Help users understand features, troubleshoot issues, and get the most value. \
                    Be concise, friendly, and solution-oriented. If you can't help, suggest contacting human support.".into(),
                tools: vec!["search_docs".into(), "create_ticket".into()],
                trigger: AgentTrigger::AfterDelay { seconds: 30 },
                ui_placement: "floating_button_bottom_right".into(),
                api_route: "/api/chat/support".into(),
                priority: 1,
            });
            agents.push(DesignedAgent {
                name: "Onboarding Guide".into(),
                agent_type: AgentType::Onboarding,
                description: "Walks new users through setup and first actions".into(),
                system_prompt: "You are an onboarding guide. Help new users set up their account, \
                    understand key features, and complete their first task. Be encouraging and step-by-step.".into(),
                tools: vec!["get_onboarding_status".into(), "mark_step_complete".into()],
                trigger: AgentTrigger::OnAction { action: "first_login".into() },
                ui_placement: "sidebar_panel".into(),
                api_route: "/api/chat/onboarding".into(),
                priority: 2,
            });
            if lower.contains("analytics") || lower.contains("dashboard") || lower.contains("metric") {
                agents.push(DesignedAgent {
                    name: "Analytics Assistant".into(),
                    agent_type: AgentType::Analytics,
                    description: "Explains metrics, generates insights, answers data questions".into(),
                    system_prompt: "You are a data analyst assistant. Help users understand their metrics, \
                        spot trends, and make data-driven decisions. Provide specific, actionable insights.".into(),
                    tools: vec!["query_metrics".into(), "generate_chart".into()],
                    trigger: AgentTrigger::OnPage { route: "/dashboard".into() },
                    ui_placement: "dashboard_header_button".into(),
                    api_route: "/api/chat/analytics".into(),
                    priority: 3,
                });
            }
        }

        AppType::Marketplace => {
            agents.push(DesignedAgent {
                name: "Shopping Assistant".into(),
                agent_type: AgentType::Recommendation,
                description: "Helps buyers find the right listings based on preferences".into(),
                system_prompt: "You are a helpful shopping assistant. Help users find what they're looking for \
                    by asking about preferences and recommending listings. Be knowledgeable about the catalog.".into(),
                tools: vec!["search_listings".into(), "get_listing_details".into()],
                trigger: AgentTrigger::AfterDelay { seconds: 15 },
                ui_placement: "floating_button_bottom_right".into(),
                api_route: "/api/chat/assistant".into(),
                priority: 1,
            });
            agents.push(DesignedAgent {
                name: "Fraud Detector".into(),
                agent_type: AgentType::FraudDetection,
                description: "Monitors transactions and listings for suspicious activity".into(),
                system_prompt: "You analyze transactions and listings for fraud signals. \
                    Flag suspicious patterns, fake reviews, and unusual payment behavior.".into(),
                tools: vec!["analyze_transaction".into(), "flag_listing".into()],
                trigger: AgentTrigger::Background,
                ui_placement: "admin_only".into(),
                api_route: "/api/internal/fraud".into(),
                priority: 2,
            });
            agents.push(DesignedAgent {
                name: "Content Moderator".into(),
                agent_type: AgentType::ContentModeration,
                description: "Reviews user-generated content for policy compliance".into(),
                system_prompt: "You review user-generated content (listings, reviews, messages) \
                    for policy violations. Flag inappropriate content while allowing legitimate use.".into(),
                tools: vec!["review_content".into(), "flag_content".into()],
                trigger: AgentTrigger::Background,
                ui_placement: "admin_only".into(),
                api_route: "/api/internal/moderation".into(),
                priority: 3,
            });
        }

        AppType::ECommerce => {
            agents.push(DesignedAgent {
                name: "Shopping Assistant".into(),
                agent_type: AgentType::Recommendation,
                description: "Recommends products based on browsing history and preferences".into(),
                system_prompt: "You are a personal shopping assistant. Help customers find products \
                    they'll love based on their preferences, budget, and past purchases. Be enthusiastic but honest.".into(),
                tools: vec!["search_products".into(), "get_recommendations".into()],
                trigger: AgentTrigger::AfterDelay { seconds: 20 },
                ui_placement: "floating_button_bottom_right".into(),
                api_route: "/api/chat/shopping".into(),
                priority: 1,
            });
            agents.push(DesignedAgent {
                name: "Order Support".into(),
                agent_type: AgentType::Support,
                description: "Handles order inquiries, returns, and shipping questions".into(),
                system_prompt: "You help customers with order-related questions: tracking, returns, \
                    exchanges, and shipping. Be empathetic and solution-focused.".into(),
                tools: vec!["get_order_status".into(), "initiate_return".into()],
                trigger: AgentTrigger::OnPage { route: "/orders".into() },
                ui_placement: "order_page_sidebar".into(),
                api_route: "/api/chat/orders".into(),
                priority: 2,
            });
        }

        AppType::Crm => {
            agents.push(DesignedAgent {
                name: "Sales Assistant".into(),
                agent_type: AgentType::PersonalAssistant,
                description: "Helps sales reps manage leads, draft emails, and prioritize deals".into(),
                system_prompt: "You are a sales productivity assistant. Help reps draft follow-up emails, \
                    summarize contact history, suggest next actions, and prioritize their pipeline.".into(),
                tools: vec!["search_contacts".into(), "draft_email".into(), "get_deal_context".into()],
                trigger: AgentTrigger::AlwaysVisible,
                ui_placement: "sidebar_panel".into(),
                api_route: "/api/chat/sales".into(),
                priority: 1,
            });
            agents.push(DesignedAgent {
                name: "Data Analyst".into(),
                agent_type: AgentType::DataAnalysis,
                description: "Analyzes CRM data, generates reports, answers business questions".into(),
                system_prompt: "You analyze CRM data to provide business insights. Answer questions about \
                    conversion rates, pipeline health, revenue forecasts, and team performance.".into(),
                tools: vec!["query_crm_data".into(), "generate_report".into()],
                trigger: AgentTrigger::OnPage { route: "/reports".into() },
                ui_placement: "reports_header_button".into(),
                api_route: "/api/chat/analytics".into(),
                priority: 2,
            });
        }

        AppType::Dashboard => {
            agents.push(DesignedAgent {
                name: "Insights Assistant".into(),
                agent_type: AgentType::Analytics,
                description: "Explains metrics, detects anomalies, suggests actions".into(),
                system_prompt: "You are a data insights assistant. Help users understand what their metrics mean, \
                    spot anomalies, and suggest data-driven actions. Be specific and actionable.".into(),
                tools: vec!["query_metrics".into(), "detect_anomalies".into()],
                trigger: AgentTrigger::AlwaysVisible,
                ui_placement: "dashboard_sidebar".into(),
                api_route: "/api/chat/insights".into(),
                priority: 1,
            });
        }

        _ => {}
    }

    // ── Domain-specific agents (from keywords) ──────────────────────────

    if lower.contains("wine") || lower.contains("sommelier") {
        // Replace generic recommendation with domain-specific
        agents.retain(|a| a.name != "Shopping Assistant");
        agents.push(DesignedAgent {
            name: "AI Sommelier".into(),
            agent_type: AgentType::Recommendation,
            description: "Expert wine recommendations based on preferences, food pairings, and occasions".into(),
            system_prompt: "You are an expert AI sommelier. Help users discover wines they'll love based on \
                their taste preferences, food pairings, budget, and occasion. Share tasting notes, \
                region insights, and pairing suggestions with enthusiasm and expertise.".into(),
            tools: vec!["search_wines".into(), "get_pairing".into()],
            trigger: AgentTrigger::Inline { section: "featured_wines".into() },
            ui_placement: "floating_button_with_text".into(),
            api_route: "/api/chat/sommelier".into(),
            priority: 1,
        });
    }

    if lower.contains("fitness") || lower.contains("workout") || lower.contains("training") {
        agents.push(DesignedAgent {
            name: "AI Coach".into(),
            agent_type: AgentType::PersonalAssistant,
            description: "Personalized fitness coaching, form tips, and motivation".into(),
            system_prompt: "You are a certified fitness coach. Help users with workout form, \
                program selection, nutrition basics, and motivation. Be encouraging but safe — \
                always recommend consulting a doctor for medical concerns.".into(),
            tools: vec!["get_user_progress".into(), "suggest_workout".into()],
            trigger: AgentTrigger::AlwaysVisible,
            ui_placement: "floating_button_bottom_right".into(),
            api_route: "/api/chat/coach".into(),
            priority: 1,
        });
    }

    if lower.contains("education") || lower.contains("learning") || lower.contains("course") {
        agents.push(DesignedAgent {
            name: "AI Tutor".into(),
            agent_type: AgentType::PersonalAssistant,
            description: "Explains concepts, answers questions, provides practice problems".into(),
            system_prompt: "You are a patient, knowledgeable tutor. Explain concepts clearly, \
                use analogies, and adapt to the student's level. Ask follow-up questions to \
                check understanding. Never just give answers — guide the student to figure it out.".into(),
            tools: vec!["get_course_content".into(), "generate_quiz".into()],
            trigger: AgentTrigger::AlwaysVisible,
            ui_placement: "sidebar_panel".into(),
            api_route: "/api/chat/tutor".into(),
            priority: 1,
        });
    }

    if lower.contains("travel") || lower.contains("hotel") || lower.contains("booking") {
        agents.push(DesignedAgent {
            name: "AI Concierge".into(),
            agent_type: AgentType::PersonalAssistant,
            description: "Helps plan trips, suggests activities, handles booking questions".into(),
            system_prompt: "You are a luxury travel concierge. Help travelers plan their trips, \
                suggest activities and restaurants, answer destination questions, and assist with bookings.".into(),
            tools: vec!["search_destinations".into(), "get_availability".into()],
            trigger: AgentTrigger::AfterDelay { seconds: 10 },
            ui_placement: "floating_button_bottom_right".into(),
            api_route: "/api/chat/concierge".into(),
            priority: 1,
        });
    }

    // ── Universal agents (if not already present) ───────────────────────

    // Every app with auth should have a support agent
    if intent.needs_auth && !agents.iter().any(|a| matches!(a.agent_type, AgentType::Support)) {
        agents.push(DesignedAgent {
            name: "Help Assistant".into(),
            agent_type: AgentType::Support,
            description: "General help and support for users".into(),
            system_prompt: "You are a helpful assistant for this application. Answer user questions, \
                guide them through features, and help resolve issues. Be concise and friendly.".into(),
            tools: vec!["search_docs".into()],
            trigger: AgentTrigger::AfterDelay { seconds: 45 },
            ui_placement: "floating_button_bottom_right".into(),
            api_route: "/api/chat/help".into(),
            priority: 10,
        });
    }

    // Sort by priority
    agents.sort_by_key(|a| a.priority);
    agents
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent_engine::analyze_flat;

    #[test]
    fn saas_gets_support_and_onboarding() {
        let intent = analyze_flat("SaaS project management tool");
        let agents = design_agents(&intent, "SaaS project management tool");
        assert!(agents.iter().any(|a| matches!(a.agent_type, AgentType::Support)));
        assert!(agents.iter().any(|a| matches!(a.agent_type, AgentType::Onboarding)));
    }

    #[test]
    fn marketplace_gets_fraud_and_moderation() {
        let intent = analyze_flat("Online marketplace for handmade crafts");
        let agents = design_agents(&intent, "Online marketplace for handmade crafts");
        assert!(agents.iter().any(|a| matches!(a.agent_type, AgentType::FraudDetection)));
        assert!(agents.iter().any(|a| matches!(a.agent_type, AgentType::ContentModeration)));
    }

    #[test]
    fn wine_app_gets_sommelier() {
        let intent = analyze_flat("Wine marketplace with AI sommelier");
        let agents = design_agents(&intent, "Wine marketplace with AI sommelier");
        assert!(agents.iter().any(|a| a.name == "AI Sommelier"));
    }

    #[test]
    fn crm_gets_sales_assistant() {
        let intent = analyze_flat("CRM for sales teams");
        let agents = design_agents(&intent, "CRM for sales teams");
        assert!(agents.iter().any(|a| a.name == "Sales Assistant"));
    }
}
