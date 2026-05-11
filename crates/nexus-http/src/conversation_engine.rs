//! Smart Conversation Engine — transforms Nexus from a chatbot into an
//! intelligent product discovery + building + operations platform.
//!
//! Tracks conversation phases, classifies user intent, generates contextual
//! questions, and produces structured app proposals.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationPhase {
    Discovery,
    Proposal,
    Building,
    Refinement,
    TeamCreation,
    Operating,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeScope {
    Content,
    Design,
    Feature,
    Page,
    Architecture,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionAnswer {
    pub area: String,
    pub question: String,
    pub answer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryState {
    pub app_description: String,
    pub questions_asked: Vec<QuestionAnswer>,
    pub areas_covered: Vec<String>,
    pub areas_remaining: Vec<String>,
    /// Overall confidence we have enough info to propose (0.0 – 1.0).
    pub confidence: f64,
}

impl DiscoveryState {
    fn default_areas() -> Vec<String> {
        ["core_features", "target_users", "monetization", "auth_needs",
         "data_model", "integrations", "design_preferences"]
            .iter().map(|s| s.to_string()).collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageType {
    Dashboard, List, Detail, Form, Settings, Auth, Landing, Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageSpec {
    pub route: String,
    pub title: String,
    pub description: String,
    pub page_type: PageType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppProposal {
    pub name: String,
    pub description: String,
    pub pages: Vec<PageSpec>,
    pub features: Vec<String>,
    pub tech_stack: Vec<String>,
    pub data_model: Vec<String>,
    pub suggested_teams: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageIntent {
    pub intent: String,
    pub key_entities: Vec<String>,
    pub sentiment: String,
    pub urgency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildingState {
    pub started_at: String,
    pub current_phase: String,
    pub files_created: Vec<String>,
    pub features_completed: Vec<String>,
    pub preview_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessTeamState {
    pub team_name: String,
    pub member_count: u32,
    pub autonomy_level: String,
    pub status: String,
    pub tickets_today: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BuildProgressEvent {
    PhaseStarted { phase: String, description: String },
    PhaseProgress { phase: String, progress: f64, detail: String },
    PhaseCompleted { phase: String, duration_ms: u64 },
    FileCreated { path: String, language: String, lines: u32 },
    Narration { message: String },
    PreviewReady { url: String },
    FeatureCompleted { feature: String, files: Vec<String> },
    Suggestion { message: String, scope: ChangeScope },
    QualityUpdate { score: f64, issues: Vec<String> },
    Complete { total_files: u32, total_duration_ms: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationState {
    pub phase: ConversationPhase,
    pub discovery: DiscoveryState,
    pub proposal: Option<AppProposal>,
    pub building: Option<BuildingState>,
    pub agent_teams: Vec<BusinessTeamState>,
}

impl Default for ConversationState {
    fn default() -> Self {
        Self::new()
    }
}

impl ConversationState {
    /// Create a fresh conversation in the Discovery phase.
    pub fn new() -> Self {
        Self {
            phase: ConversationPhase::Discovery,
            discovery: DiscoveryState {
                app_description: String::new(),
                questions_asked: Vec::new(),
                areas_covered: Vec::new(),
                areas_remaining: DiscoveryState::default_areas(),
                confidence: 0.0,
            },
            proposal: None,
            building: None,
            agent_teams: Vec::new(),
        }
    }

    /// Advance phase: Discovery->Proposal (confidence>=0.7), Proposal->Building
    /// (proposal exists), Building->Refinement (complete), Refinement->TeamCreation,
    /// TeamCreation->Operating (active team).
    pub fn advance_phase(&mut self) {
        self.phase = match &self.phase {
            ConversationPhase::Discovery if self.discovery.confidence >= 0.7 => {
                ConversationPhase::Proposal
            }
            ConversationPhase::Proposal if self.proposal.is_some() => {
                ConversationPhase::Building
            }
            ConversationPhase::Building => match &self.building {
                Some(b) if !b.features_completed.is_empty() && b.current_phase == "complete" => {
                    ConversationPhase::Refinement
                }
                _ => return,
            },
            ConversationPhase::Refinement => ConversationPhase::TeamCreation,
            ConversationPhase::TeamCreation
                if self.agent_teams.iter().any(|t| t.status == "active") =>
            {
                ConversationPhase::Operating
            }
            _ => return,
        };
    }
}

/// Build the LLM system prompt appropriate for the current conversation phase.
pub fn build_conversation_system_prompt(
    phase: &ConversationPhase,
    discovery: &DiscoveryState,
    project_context: &str,
) -> String {
    let base = "You are Nexus, a friendly AI assistant that helps anyone build and run their own apps — no coding needed.\n\
                IMPORTANT: Never use technical jargon. Speak in plain, everyday language. \
                Be warm, encouraging, and concise (2-3 sentences per response unless asked for more).";
    let phase_block = match phase {
        ConversationPhase::Discovery => {
            let remaining_display: Vec<&str> = discovery.areas_remaining.iter().map(|a| match a.as_str() {
                "core_features" => "what features you need",
                "target_users" => "who will use it",
                "monetization" => "how you'll make money",
                "auth_needs" => "whether users need accounts",
                "data_model" => "what information to track",
                "integrations" => "connections to other services",
                "design_preferences" => "how it should look",
                _ => a.as_str(),
            }).collect();
            let remaining = remaining_display.join(", ");
            let covered_count = discovery.areas_covered.len();
            let total = covered_count + discovery.areas_remaining.len();
            format!(
                "You're getting to know what the user wants to build.\n\
                 Ask ONE friendly question at a time. Keep it conversational, not like a form.\n\
                 You've covered {covered_count} of {total} topics. Still want to learn about: {remaining}\n\
                 Confidence: {:.0}%\n\
                 When you feel you understand enough (70%+), offer to show them a plan of what you'll build.\n\
                 Do NOT start building yet — just listen and ask.",
                discovery.confidence * 100.0
            )
        }
        ConversationPhase::Proposal => format!(
            "Time to show the user what you'll build! Present it as an exciting preview:\n\
             - Give the app a catchy name\n\
             - List the pages they'll get (describe what each page DOES, not its route)\n\
             - List the key features in plain language\n\
             - Mention how their data will be organized (without saying 'database' or 'schema')\n\
             - If relevant, suggest AI teams that could run parts of their business\n\
             End with: 'Ready to build? Say the word and I'll get started!'\n\
             If they want changes, happily adjust the plan.\n\n\
             What you know so far:\n{}", format_discovery_context(discovery)
        ),
        ConversationPhase::Building => "The user said go — you're building their app now!\n\
             Narrate what's happening in exciting, non-technical language:\n\
             - 'Setting up your homepage...' not 'Generating src/app/page.tsx'\n\
             - 'Creating your login system...' not 'Configuring NextAuth.js'\n\
             - 'Building your dashboard...' not 'Scaffolding the admin layout'\n\
             If they ask questions or want changes, be flexible and handle it.\n\
             Celebrate milestones: 'Your booking page is ready!'".into(),
        ConversationPhase::Refinement => "Your app is built! Now help the user make it perfect.\n\
             - Suggest improvements they might not have thought of\n\
             - When they ask for changes, be specific about what you'll update\n\
             - Show before/after when possible\n\
             - Keep changes surgical — don't rebuild everything for a small tweak\n\
             Always end with a suggestion: 'Want to adjust anything else, or shall we make it live?'".into(),
        ConversationPhase::TeamCreation => "Help the user set up AI teams that run parts of their business automatically.\n\
             Available teams: Customer Support, Marketing, SEO, Analytics, Content, Operations.\n\
             Explain each in business terms:\n\
             - 'A Support team answers customer questions 24/7'\n\
             - 'A Marketing team writes emails and social posts'\n\
             - 'An Analytics team tracks what's working and what's not'\n\
             Let them choose how much control to keep: 'fully automatic' or 'review before sending'.".into(),
        ConversationPhase::Operating => "Everything is running! You're now the user's business co-pilot.\n\
             - Share updates in plain language: '12 support tickets handled today, 2 need your attention'\n\
             - Flag anything unusual: 'Website traffic spiked 300% — looks like your campaign is working!'\n\
             - Suggest improvements: 'Your FAQ page gets the most visits — want me to expand it?'\n\
             - Accept new feature requests: 'Sure, I can add a blog section. Want me to start?'".into(),
    };
    let ctx = if project_context.is_empty() { String::new() }
              else { format!("\n\nProject context:\n{project_context}") };
    format!("{base}\n\n{phase_block}{ctx}")
}

/// Classify the user's message intent using keyword heuristics (no LLM).
pub fn classify_message_heuristic(message: &str, phase: &ConversationPhase) -> MessageIntent {
    let lower = message.to_lowercase();
    let trimmed = lower.trim();

    let approval = ["yes", "go ahead", "looks good", "approved", "approve",
        "ship it", "let's go", "do it", "perfect", "sounds good", "lgtm"];
    if approval.iter().any(|p| trimmed == *p || trimmed.starts_with(p)) {
        return intent("approving", vec![], "positive", "medium");
    }

    let change = ["change", "modify", "update", "fix", "adjust", "tweak", "replace", "rename"];
    if change.iter().any(|w| lower.contains(w)) {
        let urg = if lower.contains("fix") || lower.contains("bug") { "high" } else { "medium" };
        return intent("requesting_change", extract_entities(&lower), "neutral", urg);
    }

    let agent = ["agent", "team", "support", "marketing", "seo", "analytics", "automate"];
    if agent.iter().any(|w| lower.contains(w)) {
        return intent("requesting_agents", extract_entities(&lower), "positive", "low");
    }

    if lower.contains('?') || lower.starts_with("how") || lower.starts_with("what")
        || lower.starts_with("why") || lower.starts_with("can you")
    {
        return intent("asking_question", extract_entities(&lower), "neutral", "low");
    }

    let default = match phase {
        ConversationPhase::Discovery => {
            if !message.trim().is_empty() && message.len() > 20 { "describing_app" }
            else { "answering_question" }
        }
        ConversationPhase::Refinement => "requesting_change",
        _ => "general",
    };
    intent(default, extract_entities(&lower), "neutral", "low")
}

fn intent(i: &str, entities: Vec<String>, sentiment: &str, urgency: &str) -> MessageIntent {
    MessageIntent {
        intent: i.into(), key_entities: entities,
        sentiment: sentiment.into(), urgency: urgency.into(),
    }
}

fn extract_entities(text: &str) -> Vec<String> {
    let mut entities = Vec::new();
    let mut in_quote = false;
    let mut current = String::new();
    for ch in text.chars() {
        if ch == '"' || ch == '\'' {
            if in_quote && !current.is_empty() { entities.push(current.clone()); current.clear(); }
            in_quote = !in_quote;
        } else if in_quote { current.push(ch); }
    }
    let tech = ["react", "next.js", "nextjs", "vue", "angular", "tailwind",
        "postgres", "mongodb", "redis", "stripe", "auth", "api", "dashboard", "admin"];
    for t in &tech { if text.contains(*t) { entities.push(t.to_string()); } }
    entities.dedup();
    entities
}

/// Update discovery state from a user message and its classified intent.
pub fn update_discovery(state: &mut ConversationState, message: &str, intent: &MessageIntent) {
    let disc = &mut state.discovery;
    match intent.intent.as_str() {
        "describing_app" => {
            if disc.app_description.is_empty() { disc.app_description = message.into(); }
            else { disc.app_description.push('\n'); disc.app_description.push_str(message); }
            mark_area_covered(disc, "core_features");
            disc.confidence = calculate_confidence(disc);
        }
        "answering_question" => {
            if let Some(area) = disc.areas_remaining.first().cloned() {
                disc.questions_asked.push(QuestionAnswer {
                    area: area.clone(), question: String::new(), answer: message.into(),
                });
                mark_area_covered(disc, &area);
            }
            disc.confidence = calculate_confidence(disc);
        }
        _ => {}
    }
}

fn mark_area_covered(disc: &mut DiscoveryState, area: &str) {
    disc.areas_remaining.retain(|a| a != area);
    if !disc.areas_covered.contains(&area.to_string()) { disc.areas_covered.push(area.into()); }
}

fn calculate_confidence(disc: &DiscoveryState) -> f64 {
    let total = disc.areas_covered.len() + disc.areas_remaining.len();
    if total == 0 { return 0.0; }
    let base = disc.areas_covered.len() as f64 / total as f64;
    let bonus = if disc.app_description.len() > 50 { 0.1 } else { 0.0 };
    (base + bonus).min(1.0)
}

/// Return the most important discovery areas still to explore, in priority order.
pub fn suggest_next_areas(discovery: &DiscoveryState) -> Vec<&str> {
    let order = ["core_features", "target_users", "data_model", "auth_needs",
        "monetization", "integrations", "design_preferences"];
    order.iter().copied().filter(|a| discovery.areas_remaining.iter().any(|r| r == *a)).collect()
}

/// Format everything we know from discovery into a context string for the LLM.
pub fn format_discovery_context(discovery: &DiscoveryState) -> String {
    let mut p = Vec::new();
    if !discovery.app_description.is_empty() {
        p.push(format!("App description: {}", discovery.app_description));
    }
    if !discovery.questions_asked.is_empty() {
        p.push("Detailed answers:".into());
        for qa in &discovery.questions_asked { p.push(format!("  [{}] {}", qa.area, qa.answer)); }
    }
    if !discovery.areas_covered.is_empty() {
        p.push(format!("Areas covered: {}", discovery.areas_covered.join(", ")));
    }
    if !discovery.areas_remaining.is_empty() {
        p.push(format!("Areas still unknown: {}", discovery.areas_remaining.join(", ")));
    }
    p.push(format!("Confidence: {:.0}%", discovery.confidence * 100.0));
    p.join("\n")
}

/// Generate a structured AppProposal from discovery state.
/// Uses the gathered information to produce a concrete proposal the user can approve.
pub fn generate_proposal_from_discovery(discovery: &DiscoveryState) -> AppProposal {
    let name = extract_app_name(&discovery.app_description);
    let features = extract_features(discovery);
    let pages = infer_pages(&features);
    let data_model = infer_data_model(&features);
    let suggested_teams = infer_teams(&features, &discovery.app_description);

    AppProposal {
        name,
        description: discovery.app_description.clone(),
        pages,
        features,
        tech_stack: vec![
            "Next.js 15".into(),
            "TypeScript".into(),
            "Tailwind CSS 4".into(),
            "shadcn/ui".into(),
            "SQLite".into(),
        ],
        data_model,
        suggested_teams,
    }
}

fn extract_app_name(description: &str) -> String {
    // Take the first significant noun phrase or fallback to "My App"
    let words: Vec<&str> = description.split_whitespace().take(5).collect();
    if words.len() >= 2 {
        words[..words.len().min(4)].join(" ")
    } else {
        "My App".into()
    }
}

fn extract_features(discovery: &DiscoveryState) -> Vec<String> {
    let mut features = Vec::new();
    // Extract from app description
    let desc = discovery.app_description.to_lowercase();
    let feature_keywords = [
        ("auth", "User authentication & login"),
        ("dashboard", "Dashboard with analytics"),
        ("payment", "Payment processing"),
        ("stripe", "Stripe payment integration"),
        ("email", "Email notifications"),
        ("chat", "Real-time chat"),
        ("search", "Search functionality"),
        ("api", "REST API"),
        ("admin", "Admin panel"),
        ("profile", "User profiles"),
        ("upload", "File upload"),
        ("notification", "Push notifications"),
        ("team", "Team management"),
        ("billing", "Billing & subscriptions"),
        ("report", "Reporting & analytics"),
    ];
    for (keyword, feature) in &feature_keywords {
        if desc.contains(keyword) {
            features.push(feature.to_string());
        }
    }
    // Extract from Q&A
    for qa in &discovery.questions_asked {
        if qa.area == "core_features" && !qa.answer.is_empty() {
            for part in qa.answer.split([',', ';', '\n']) {
                let trimmed = part.trim();
                if !trimmed.is_empty() && trimmed.len() > 3 {
                    features.push(trimmed.to_string());
                }
            }
        }
    }
    if features.is_empty() {
        features.push("Landing page".into());
        features.push("User authentication".into());
        features.push("Main dashboard".into());
    }
    features.dedup();
    features
}

fn infer_pages(features: &[String]) -> Vec<PageSpec> {
    let mut pages = vec![
        PageSpec {
            route: "/".into(),
            title: "Home".into(),
            description: "Landing page".into(),
            page_type: PageType::Landing,
        },
    ];
    let lower_features: Vec<String> = features.iter().map(|f| f.to_lowercase()).collect();
    if lower_features.iter().any(|f| f.contains("auth") || f.contains("login")) {
        pages.push(PageSpec { route: "/login".into(), title: "Login".into(), description: "Authentication page".into(), page_type: PageType::Auth });
        pages.push(PageSpec { route: "/register".into(), title: "Register".into(), description: "User registration".into(), page_type: PageType::Auth });
    }
    if lower_features.iter().any(|f| f.contains("dashboard") || f.contains("analytics")) {
        pages.push(PageSpec { route: "/dashboard".into(), title: "Dashboard".into(), description: "Main dashboard".into(), page_type: PageType::Dashboard });
    }
    if lower_features.iter().any(|f| f.contains("admin")) {
        pages.push(PageSpec { route: "/admin".into(), title: "Admin".into(), description: "Admin panel".into(), page_type: PageType::Dashboard });
    }
    if lower_features.iter().any(|f| f.contains("profile")) {
        pages.push(PageSpec { route: "/profile".into(), title: "Profile".into(), description: "User profile".into(), page_type: PageType::Detail });
    }
    pages.push(PageSpec { route: "/settings".into(), title: "Settings".into(), description: "App settings".into(), page_type: PageType::Settings });
    pages
}

fn infer_data_model(features: &[String]) -> Vec<String> {
    let mut models = vec!["users".into()];
    let lower_features: Vec<String> = features.iter().map(|f| f.to_lowercase()).collect();
    if lower_features.iter().any(|f| f.contains("team")) { models.push("teams".into()); }
    if lower_features.iter().any(|f| f.contains("billing") || f.contains("payment") || f.contains("subscription")) { models.push("subscriptions".into()); }
    if lower_features.iter().any(|f| f.contains("chat") || f.contains("message")) { models.push("messages".into()); }
    if lower_features.iter().any(|f| f.contains("notification")) { models.push("notifications".into()); }
    if lower_features.iter().any(|f| f.contains("report") || f.contains("analytics")) { models.push("analytics_events".into()); }
    models
}

fn infer_teams(features: &[String], description: &str) -> Vec<String> {
    let mut teams = Vec::new();
    let lower = description.to_lowercase();
    let lower_features: Vec<String> = features.iter().map(|f| f.to_lowercase()).collect();
    if lower.contains("support") || lower.contains("ticket") { teams.push("customer-support".into()); }
    if lower.contains("market") || lower_features.iter().any(|f| f.contains("email")) { teams.push("marketing-growth".into()); }
    if lower.contains("seo") || lower.contains("search engine") { teams.push("seo-search".into()); }
    if lower.contains("content") || lower.contains("blog") { teams.push("content-creation".into()); }
    teams
}

/// Map a change request to a ChangeScope based on keywords.
pub fn classify_change_scope(message: &str) -> ChangeScope {
    let lower = message.to_lowercase();
    if lower.contains("color") || lower.contains("font") || lower.contains("layout")
        || lower.contains("design") || lower.contains("theme") || lower.contains("style")
        || lower.contains("css") || lower.contains("spacing")
    {
        ChangeScope::Design
    } else if lower.contains("text") || lower.contains("copy") || lower.contains("title")
        || lower.contains("label") || lower.contains("heading") || lower.contains("description")
    {
        ChangeScope::Content
    } else if lower.contains("page") || lower.contains("route") || lower.contains("screen") {
        ChangeScope::Page
    } else if lower.contains("database") || lower.contains("schema") || lower.contains("architecture")
        || lower.contains("restructure")
    {
        ChangeScope::Architecture
    } else {
        ChangeScope::Feature
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_intent(i: &str) -> MessageIntent {
        MessageIntent { intent: i.into(), key_entities: vec![], sentiment: "neutral".into(), urgency: "low".into() }
    }

    #[test]
    fn new_state_starts_in_discovery() {
        let s = ConversationState::new();
        assert_eq!(s.phase, ConversationPhase::Discovery);
        assert_eq!(s.discovery.areas_remaining.len(), 7);
        assert!(s.discovery.areas_covered.is_empty());
        assert!((s.discovery.confidence).abs() < f64::EPSILON);
    }

    #[test]
    fn advance_discovery_requires_confidence() {
        let mut s = ConversationState::new();
        s.advance_phase();
        assert_eq!(s.phase, ConversationPhase::Discovery);
        s.discovery.confidence = 0.75;
        s.advance_phase();
        assert_eq!(s.phase, ConversationPhase::Proposal);
    }

    #[test]
    fn advance_proposal_requires_proposal() {
        let mut s = ConversationState::new();
        s.phase = ConversationPhase::Proposal;
        s.advance_phase();
        assert_eq!(s.phase, ConversationPhase::Proposal);
        s.proposal = Some(AppProposal {
            name: "T".into(), description: "T".into(), pages: vec![],
            features: vec!["auth".into()], tech_stack: vec!["next.js".into()],
            data_model: vec!["users".into()], suggested_teams: vec![],
        });
        s.advance_phase();
        assert_eq!(s.phase, ConversationPhase::Building);
    }

    #[test]
    fn advance_building_requires_completion() {
        let mut s = ConversationState::new();
        s.phase = ConversationPhase::Building;
        s.building = Some(BuildingState {
            started_at: "2026-01-01".into(), current_phase: "generating".into(),
            files_created: vec!["index.tsx".into()], features_completed: vec!["auth".into()],
            preview_url: None,
        });
        s.advance_phase();
        assert_eq!(s.phase, ConversationPhase::Building);
        s.building.as_mut().unwrap().current_phase = "complete".into();
        s.advance_phase();
        assert_eq!(s.phase, ConversationPhase::Refinement);
    }

    #[test]
    fn advance_team_creation_requires_active_team() {
        let mut s = ConversationState::new();
        s.phase = ConversationPhase::TeamCreation;
        s.advance_phase();
        assert_eq!(s.phase, ConversationPhase::TeamCreation);
        s.agent_teams.push(BusinessTeamState {
            team_name: "Support".into(), member_count: 2, autonomy_level: "supervised".into(),
            status: "active".into(), tickets_today: 0,
        });
        s.advance_phase();
        assert_eq!(s.phase, ConversationPhase::Operating);
    }

    #[test]
    fn classify_approval() {
        assert_eq!(classify_message_heuristic("yes", &ConversationPhase::Proposal).intent, "approving");
        assert_eq!(classify_message_heuristic("Looks good, ship it", &ConversationPhase::Proposal).intent, "approving");
    }

    #[test]
    fn classify_change_request() {
        let i = classify_message_heuristic("change the navbar color", &ConversationPhase::Refinement);
        assert_eq!(i.intent, "requesting_change");
        assert_eq!(i.urgency, "medium");
        let i2 = classify_message_heuristic("fix the login bug", &ConversationPhase::Building);
        assert_eq!(i2.intent, "requesting_change");
        assert_eq!(i2.urgency, "high");
    }

    #[test]
    fn classify_agent_request() {
        assert_eq!(classify_message_heuristic("I want a marketing team for SEO", &ConversationPhase::Refinement).intent, "requesting_agents");
    }

    #[test]
    fn classify_question() {
        assert_eq!(classify_message_heuristic("How does the auth system work?", &ConversationPhase::Building).intent, "asking_question");
    }

    #[test]
    fn classify_defaults_by_phase() {
        assert_eq!(classify_message_heuristic("I want to build a SaaS invoicing platform with Stripe integration", &ConversationPhase::Discovery).intent, "describing_app");
        assert_eq!(classify_message_heuristic("sure, paid plans", &ConversationPhase::Discovery).intent, "answering_question");
        assert_eq!(classify_message_heuristic("make the dashboard prettier", &ConversationPhase::Refinement).intent, "requesting_change");
    }

    #[test]
    fn discovery_update_and_confidence() {
        let mut s = ConversationState::new();
        let i = new_intent("describing_app");
        update_discovery(&mut s, "I want to build a project management tool with kanban boards and time tracking", &i);
        assert!(!s.discovery.app_description.is_empty());
        assert!(s.discovery.areas_covered.contains(&"core_features".to_string()));
        assert!(!s.discovery.areas_remaining.contains(&"core_features".to_string()));
        let expected = 1.0 / 7.0 + 0.1;
        assert!((s.discovery.confidence - expected).abs() < 0.01);
    }

    #[test]
    fn suggest_next_areas_respects_priority() {
        let disc = DiscoveryState {
            app_description: String::new(), questions_asked: Vec::new(),
            areas_covered: vec!["core_features".into(), "target_users".into()],
            areas_remaining: vec!["monetization".into(), "auth_needs".into(), "data_model".into(), "integrations".into(), "design_preferences".into()],
            confidence: 0.3,
        };
        let next = suggest_next_areas(&disc);
        assert_eq!(next[0], "data_model");
        assert_eq!(next[1], "auth_needs");
    }

    #[test]
    fn system_prompt_includes_phase_context() {
        let disc = DiscoveryState {
            app_description: "A todo app".into(), questions_asked: Vec::new(),
            areas_covered: vec!["core_features".into()],
            areas_remaining: vec!["target_users".into(), "auth_needs".into()],
            confidence: 0.3,
        };
        let p = build_conversation_system_prompt(&ConversationPhase::Discovery, &disc, "");
        // Updated: prompts now use consumer-friendly language
        assert!(p.contains("who will use it") && p.contains("30%"), "Discovery prompt: {p}");
        let pp = build_conversation_system_prompt(&ConversationPhase::Proposal, &disc, "some context");
        assert!(pp.contains("exciting preview") && pp.contains("some context"), "Proposal prompt: {pp}");
    }

    #[test]
    fn format_discovery_context_output() {
        let disc = DiscoveryState {
            app_description: "Invoice SaaS".into(),
            questions_asked: vec![QuestionAnswer { area: "monetization".into(), question: "How?".into(), answer: "Monthly subscription".into() }],
            areas_covered: vec!["core_features".into(), "monetization".into()],
            areas_remaining: vec!["auth_needs".into()],
            confidence: 0.5,
        };
        let ctx = format_discovery_context(&disc);
        assert!(ctx.contains("Invoice SaaS") && ctx.contains("[monetization] Monthly subscription") && ctx.contains("50%"));
    }

    #[test]
    fn build_progress_event_serialization() {
        let json = serde_json::to_value(BuildProgressEvent::FileCreated {
            path: "src/app/page.tsx".into(), language: "typescript".into(), lines: 42,
        }).unwrap();
        assert_eq!(json["type"], "file_created");
        assert_eq!(json["lines"], 42);
        let json2 = serde_json::to_value(BuildProgressEvent::Complete {
            total_files: 15, total_duration_ms: 8500,
        }).unwrap();
        assert_eq!(json2["type"], "complete");
        assert_eq!(json2["total_files"], 15);
    }
}
