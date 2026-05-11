//! Business Agent Teams — ready-to-deploy team templates that run
//! ongoing business operations (support, SEO, marketing, content, etc.).
//!
//! Each team is a collection of specialized agents with defined roles,
//! tools, coordination protocols, and autonomy levels.

use nexus_agents_core::{
    definition::{AgentDefinition, ExecutionMode},
    teams::{
        CoordinationProtocol, CompletionCriteria, HitlCheckpoint, HitlConfig,
        TeamBudget, TeamDefinition, TeamMember, TeamRole,
    },
};
use serde::{Deserialize, Serialize};

/// How much human oversight a team requires.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "level", rename_all = "snake_case")]
pub enum AutonomyLevel {
    /// Every action requires explicit human approval before execution.
    FullApproval,
    /// The team operates independently but a human reviews at fixed intervals.
    Supervised {
        review_interval_hours: u32,
        auto_escalate_on: Vec<String>,
    },
    /// Fully autonomous within a budget and escalation boundary.
    Autonomous {
        report_interval_hours: u32,
        budget_limit_usd: f32,
        escalation_rules: Vec<EscalationRule>,
    },
    /// Starts at one level and graduates to a higher level over time.
    Progressive {
        start: Box<AutonomyLevel>,
        target: Box<AutonomyLevel>,
        transition_after_days: u32,
        min_confidence: f32,
    },
}

/// A condition-action pair that triggers when something goes wrong.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EscalationRule {
    /// Human-readable condition expression (e.g. "error_rate > 5%").
    pub condition: String,
    /// What to do when the condition fires.
    pub action: EscalationAction,
}

/// What the system does when an escalation rule fires.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EscalationAction {
    /// Notify a human via a specific channel.
    NotifyHuman { via: String },
    /// Pause the entire team until a human intervenes.
    PauseTeam,
    /// Block the current action and require explicit approval.
    RequireApproval,
}

// ---------------------------------------------------------------------------
// Team & agent specs
// ---------------------------------------------------------------------------

/// A ready-to-deploy business team template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessTeamTemplate {
    pub name: String,
    pub description: String,
    pub members: Vec<BusinessAgentSpec>,
    pub coordination_protocol: String,
    pub default_autonomy: AutonomyLevel,
    pub relevance_keywords: Vec<String>,
}

/// Specification for a single agent within a business team.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessAgentSpec {
    pub id: String,
    pub name: String,
    pub role_description: String,
    pub system_prompt: String,
    pub tools: Vec<String>,
    pub max_iterations: u32,
    pub can_delegate_to: Vec<String>,
}

// ---------------------------------------------------------------------------
// Business events
// ---------------------------------------------------------------------------

/// Domain events emitted by business teams during operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum BusinessEvent {
    TicketReceived { ticket_id: String, subject: String, priority: String },
    TicketResolved { ticket_id: String, resolution_summary: String, elapsed_seconds: u64 },
    TicketEscalated { ticket_id: String, reason: String, escalated_to: String },
    CampaignSent { campaign_id: String, recipients: u32, channel: String },
    ContentPublished { content_id: String, title: String, url: String },
    RankingChanged { keyword: String, old_position: u32, new_position: u32 },
    PageOptimized { url: String, changes: Vec<String>, score_before: f32, score_after: f32 },
    DailyReport { team_name: String, summary: String, metrics: serde_json::Value },
    AnomalyDetected { metric: String, expected: f64, actual: f64, severity: String },
    AgentError { agent_id: String, message: String },
    BudgetWarning { team_name: String, spent_usd: f32, limit_usd: f32 },
    ApprovalNeeded { team_name: String, action_description: String, context: serde_json::Value },
}

// ---------------------------------------------------------------------------
// Overview / dashboard types
// ---------------------------------------------------------------------------

/// Top-level summary of the application's agent-powered status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessOverview {
    pub app_status: AppStatusSummary,
    pub teams: Vec<TeamStatusSummary>,
    pub highlights: Vec<Highlight>,
    pub needs_attention: Vec<AttentionItem>,
    pub cost_today_usd: f32,
    pub cost_this_week_usd: f32,
}

/// Overall health of the application.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppStatusSummary {
    pub status: String,
    pub active_teams: u32,
    pub active_agents: u32,
    pub uptime_pct: f32,
}

/// Status of a single business team.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamStatusSummary {
    pub team_name: String,
    pub status: String,
    pub agents_online: u32,
    pub tasks_completed_today: u32,
    pub errors_today: u32,
}

/// A positive highlight worth surfacing on the dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Highlight {
    pub title: String,
    pub description: String,
    pub team_name: String,
    pub timestamp: String,
}

/// Something that requires human attention.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttentionItem {
    pub title: String,
    pub description: String,
    pub severity: String,
    pub team_name: String,
    pub action_url: Option<String>,
}

// ---------------------------------------------------------------------------
// Template catalogue
// ---------------------------------------------------------------------------

/// Returns the default progressive autonomy level for new teams.
///
/// Starts with full approval, graduates to supervised after 14 days once
/// confidence reaches 0.85.
pub fn default_autonomy() -> AutonomyLevel {
    AutonomyLevel::Progressive {
        start: Box::new(AutonomyLevel::FullApproval),
        target: Box::new(AutonomyLevel::Supervised {
            review_interval_hours: 24,
            auto_escalate_on: vec![
                "error_rate_spike".into(),
                "budget_exceeded".into(),
                "user_complaint".into(),
            ],
        }),
        transition_after_days: 14,
        min_confidence: 0.85,
    }
}

/// Returns all eight built-in business team templates.
pub fn all_team_templates() -> Vec<BusinessTeamTemplate> {
    vec![
        customer_support_team(),
        seo_search_team(),
        marketing_growth_team(),
        content_creation_team(),
        user_onboarding_team(),
        analytics_reporting_team(),
        moderation_safety_team(),
        devops_monitoring_team(),
    ]
}

/// Suggest teams that are relevant to the given application based on its
/// features and type. Returns references into the provided template slice.
pub fn suggest_teams_for_app<'a>(
    templates: &'a [BusinessTeamTemplate],
    features: &[String],
    app_type: &str,
) -> Vec<&'a BusinessTeamTemplate> {
    let lower_features: Vec<String> = features.iter().map(|f| f.to_lowercase()).collect();
    let lower_type = app_type.to_lowercase();

    templates
        .iter()
        .filter(|t| {
            t.relevance_keywords.iter().any(|kw| {
                let kw_lower = kw.to_lowercase();
                lower_type.contains(&kw_lower)
                    || lower_features.iter().any(|f| f.contains(&kw_lower))
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// BusinessTeamTemplate → TeamDefinition conversion
// ---------------------------------------------------------------------------

impl From<BusinessTeamTemplate> for TeamDefinition {
    fn from(template: BusinessTeamTemplate) -> Self {
        let members: Vec<TeamMember> = template
            .members
            .iter()
            .enumerate()
            .map(|(i, spec)| {
                let role = if i == 0 {
                    TeamRole::Lead
                } else {
                    TeamRole::Specialist {
                        domain: spec.role_description.clone(),
                    }
                };
                TeamMember {
                    id: spec.id.clone(),
                    role,
                    agent: AgentDefinition {
                        id: spec.id.clone(),
                        name: spec.name.clone(),
                        system_prompt: spec.system_prompt.clone(),
                        skills: Vec::new(),
                        tools: spec.tools.clone(),
                        model_preference: None,
                        execution_mode: ExecutionMode::Interactive,
                        max_iterations: spec.max_iterations,
                        timeout_secs: 600,
                    },
                    responsibilities: vec![spec.role_description.clone()],
                    can_delegate_to: spec.can_delegate_to.clone(),
                    can_veto: i == 0, // lead can veto
                }
            })
            .collect();

        let lead_id = members.first().map(|m| m.id.clone()).unwrap_or_default();
        let coordination = match template.coordination_protocol.as_str() {
            "parallel" => CoordinationProtocol::Parallel {
                coordinator_id: lead_id.clone(),
            },
            "sequential" => CoordinationProtocol::Sequential {
                order: members.iter().map(|m| m.id.clone()).collect(),
            },
            "round-robin" | "broadcast" => CoordinationProtocol::Swarm {
                max_concurrent: members.len(),
            },
            _ => CoordinationProtocol::Hierarchical { lead_id },
        };

        let hitl = match &template.default_autonomy {
            AutonomyLevel::FullApproval => HitlConfig {
                enabled: true,
                checkpoints: vec![HitlCheckpoint::AfterPlanning, HitlCheckpoint::BeforeCommit],
            },
            AutonomyLevel::Supervised { .. } => HitlConfig {
                enabled: true,
                checkpoints: vec![HitlCheckpoint::BudgetThreshold { percent: 0.8 }],
            },
            _ => HitlConfig {
                enabled: false,
                checkpoints: vec![HitlCheckpoint::Never],
            },
        };

        TeamDefinition {
            id: uuid::Uuid::new_v4().to_string(),
            name: template.name.clone(),
            description: template.description.clone(),
            members,
            coordination,
            budget: TeamBudget {
                max_total_cost_usd: 10.0,
                max_cost_per_member_usd: 5.0,
                max_total_tokens: 500_000,
                max_duration_secs: 3600,
                max_iterations_per_member: 20,
            },
            hitl,
            completion: CompletionCriteria {
                all_tasks_done: true,
                lead_approval_required: false,
                all_reviews_passed: false,
                completion_phrase: None,
            },
        }
    }
}

/// Convert a template into a TeamDefinition with a specific ID and optional autonomy override.
pub fn template_to_team(
    template: &BusinessTeamTemplate,
    team_id: &str,
    control_mode: Option<&str>,
) -> TeamDefinition {
    let mut team: TeamDefinition = template.clone().into();
    team.id = team_id.to_string();
    // Adjust HITL based on control mode
    if let Some(mode) = control_mode {
        team.hitl = match mode {
            "safe" => HitlConfig {
                enabled: true,
                checkpoints: vec![HitlCheckpoint::AfterPlanning, HitlCheckpoint::AfterEachCompletion, HitlCheckpoint::BeforeCommit],
            },
            "autonomous" => HitlConfig { enabled: false, checkpoints: vec![HitlCheckpoint::Never] },
            _ => team.hitl,
        };
    }
    team
}

// ---------------------------------------------------------------------------
// Individual team builders — helper
// ---------------------------------------------------------------------------

fn agent(id: &str, name: &str, role: &str, prompt: &str, tools: &[&str], max_iter: u32, delegates: &[&str]) -> BusinessAgentSpec {
    BusinessAgentSpec {
        id: id.into(), name: name.into(), role_description: role.into(),
        system_prompt: prompt.into(),
        tools: tools.iter().map(|s| (*s).into()).collect(),
        max_iterations: max_iter,
        can_delegate_to: delegates.iter().map(|s| (*s).into()).collect(),
    }
}

fn strs(vals: &[&str]) -> Vec<String> { vals.iter().map(|s| (*s).into()).collect() }

fn customer_support_team() -> BusinessTeamTemplate {
    BusinessTeamTemplate {
        name: "customer-support".into(),
        description: "Handles inbound customer tickets, triages issues, escalates complex cases, and maintains a knowledge base of common solutions.".into(),
        coordination_protocol: "hierarchy".into(),
        default_autonomy: default_autonomy(),
        relevance_keywords: strs(&["support", "tickets", "helpdesk", "saas", "marketplace", "customers"]),
        members: vec![
            agent("support-agent", "Support Agent", "Front-line responder for all incoming tickets.",
                "You are the front-line Support Agent for a business application.\n\
                Your job is to respond to customer tickets quickly, politely, and accurately.\n\
                Always greet the customer by name if available.\n\
                Search the knowledge base before composing a response.\n\
                If the knowledge base has a matching article, reference it in your reply.\n\
                Keep responses concise — under 200 words unless the issue is complex.\n\
                Never promise features that do not exist.\n\
                If you cannot resolve the issue in two exchanges, escalate to the Escalation Agent.\n\
                Tag every response with a sentiment score (positive, neutral, negative).\n\
                Log the resolution category for analytics.\n\
                Always end with a clear next step or invitation to follow up.",
                &["search_knowledge_base", "send_reply", "classify_ticket", "tag_sentiment", "lookup_customer", "create_internal_note"],
                8, &["escalation-agent", "knowledge-agent"]),
            agent("escalation-agent", "Escalation Agent", "Handles complex or high-priority tickets.",
                "You are the Escalation Agent responsible for complex customer issues.\n\
                You receive tickets that the front-line Support Agent could not resolve.\n\
                Review the full conversation history before responding.\n\
                Investigate the root cause using internal logs and system status tools.\n\
                If the issue is a known bug, link the customer to the relevant status page.\n\
                For billing disputes, verify transaction records before taking action.\n\
                You may issue refunds up to $50 without human approval.\n\
                For refunds above $50 or account-level changes, request human approval.\n\
                Communicate with empathy; acknowledge the customer's frustration.\n\
                Document every resolution in the knowledge base for future reference.\n\
                Aim to resolve within one business day.",
                &["search_knowledge_base", "send_reply", "view_system_logs", "issue_refund", "check_billing", "request_human_approval", "update_knowledge_base"],
                12, &["knowledge-agent"]),
            agent("knowledge-agent", "Knowledge Agent", "Maintains and enriches the support knowledge base.",
                "You are the Knowledge Agent responsible for the support knowledge base.\n\
                After every resolved ticket, check whether the solution is already documented.\n\
                If not, create a new knowledge base article with a clear title and body.\n\
                Use simple language that both agents and customers can understand.\n\
                Categorise articles by topic: billing, technical, account, feature-request.\n\
                Periodically review articles older than 90 days for accuracy.\n\
                Merge duplicate articles when detected.\n\
                Track which articles are used most frequently and surface them.\n\
                Ensure every article includes step-by-step instructions.\n\
                Flag articles that reference deprecated features for removal.\n\
                Maintain a glossary of product-specific terms.",
                &["create_article", "update_article", "search_knowledge_base", "delete_article", "list_articles", "merge_articles"],
                6, &[]),
        ],
    }
}

fn seo_search_team() -> BusinessTeamTemplate {
    BusinessTeamTemplate {
        name: "seo-search".into(),
        description: "Monitors search rankings, optimises content for target keywords, and fixes technical SEO issues.".into(),
        coordination_protocol: "round-robin".into(),
        default_autonomy: AutonomyLevel::Supervised {
            review_interval_hours: 48,
            auto_escalate_on: strs(&["ranking_drop_gt_10", "index_error"]),
        },
        relevance_keywords: strs(&["seo", "search", "blog", "content", "marketplace", "organic"]),
        members: vec![
            agent("seo-analyst", "SEO Analyst", "Tracks keyword rankings and competitor positions.",
                "You are the SEO Analyst responsible for tracking organic search performance.\n\
                Run a keyword ranking check at least once per day for all target keywords.\n\
                Compare current positions to the previous period and flag drops > 3 positions.\n\
                Analyse competitor pages that outrank ours and identify content gaps.\n\
                Produce a weekly ranking report with trends and recommendations.\n\
                Prioritise keywords by search volume and business impact.\n\
                Monitor Google Search Console for crawl errors and coverage issues.\n\
                Alert the Content Optimizer when a page drops out of the top 20.\n\
                Track click-through rates and identify title/description improvements.\n\
                Never use black-hat SEO techniques; all strategies must follow guidelines.\n\
                Log every data point for historical trend analysis.",
                &["check_rankings", "fetch_search_console", "analyse_competitors", "generate_report", "alert_team", "log_metric"],
                10, &["content-optimizer", "technical-seo"]),
            agent("content-optimizer", "Content Optimizer", "Rewrites and improves pages for search relevance.",
                "You are the Content Optimizer focused on improving on-page SEO.\n\
                When assigned a page, analyse its keyword density, headings, and meta tags.\n\
                Rewrite meta titles and descriptions to improve click-through rates.\n\
                Ensure every page has one H1, logical H2/H3 hierarchy, and alt text on images.\n\
                Add internal links to related pages to strengthen topical clusters.\n\
                Keep the natural reading flow; never keyword-stuff.\n\
                Produce a before/after score for every optimization.\n\
                Submit changes as drafts for review unless autonomy allows direct publish.\n\
                Target a minimum content score of 80/100 on our internal grading system.\n\
                Preserve the original author's voice and tone.",
                &["analyse_page", "rewrite_meta", "update_content", "score_page", "add_internal_links", "submit_draft"],
                8, &[]),
            agent("technical-seo", "Technical SEO Agent", "Fixes crawl errors, speed issues, and structured data.",
                "You are the Technical SEO Agent responsible for site health.\n\
                Monitor the sitemap for missing or broken URLs and fix them immediately.\n\
                Ensure all pages return correct HTTP status codes; fix redirect chains.\n\
                Validate structured data (JSON-LD) on every page and correct errors.\n\
                Audit Core Web Vitals weekly and recommend performance improvements.\n\
                Ensure robots.txt does not block important pages.\n\
                Check for duplicate content issues and implement canonical tags.\n\
                Monitor page load speed and flag pages exceeding 3-second load time.\n\
                Generate a technical health score and trend it over time.\n\
                Coordinate with DevOps if infrastructure changes are needed.",
                &["crawl_site", "validate_structured_data", "check_page_speed", "fix_redirects", "update_sitemap", "audit_robots_txt", "generate_health_report"],
                10, &[]),
        ],
    }
}

fn marketing_growth_team() -> BusinessTeamTemplate {
    BusinessTeamTemplate {
        name: "marketing-growth".into(),
        description: "Plans email campaigns, manages social media, and analyses funnels to drive acquisition and retention.".into(),
        coordination_protocol: "broadcast".into(),
        default_autonomy: default_autonomy(),
        relevance_keywords: strs(&["marketing", "email", "social", "saas", "growth", "marketplace", "ecommerce"]),
        members: vec![
            agent("email-campaign", "Email Campaign Agent", "Designs, schedules, and analyses email campaigns.",
                "You are the Email Campaign Agent in charge of all email marketing.\n\
                Segment the audience based on behaviour, plan, and lifecycle stage.\n\
                Write compelling subject lines with A/B variants for every campaign.\n\
                Ensure every email has a clear call-to-action and unsubscribe link.\n\
                Schedule sends at optimal times based on historical open-rate data.\n\
                Monitor open rates, click rates, and unsubscribe rates after every send.\n\
                Pause campaigns automatically if the unsubscribe rate exceeds 1%.\n\
                Personalise content using customer attributes when available.\n\
                Never send more than 3 emails per user per week to avoid fatigue.\n\
                Maintain compliance with CAN-SPAM and GDPR regulations.\n\
                Report campaign performance to the Analytics Agent after each send.",
                &["segment_audience", "compose_email", "schedule_send", "ab_test", "track_campaign", "pause_campaign", "check_compliance"],
                10, &["analytics-agent"]),
            agent("social-media", "Social Media Agent", "Creates and schedules social media posts.",
                "You are the Social Media Agent managing the brand's social presence.\n\
                Create platform-appropriate content for Twitter, LinkedIn, and Instagram.\n\
                Maintain a consistent brand voice — professional yet approachable.\n\
                Schedule posts at peak engagement times for each platform.\n\
                Monitor mentions and respond to comments within 2 hours.\n\
                Track follower growth, engagement rates, and share-of-voice.\n\
                Identify trending topics relevant to our industry and create timely content.\n\
                Never post controversial or politically sensitive content.\n\
                Repurpose blog posts and case studies into social-friendly formats.\n\
                Coordinate with the Email Campaign Agent for campaign launches.\n\
                Produce a weekly social performance summary.",
                &["compose_post", "schedule_post", "monitor_mentions", "track_engagement", "fetch_trends", "reply_to_comment"],
                8, &["analytics-agent"]),
            agent("analytics-agent", "Analytics Agent", "Analyses marketing funnels and reports on ROI.",
                "You are the Analytics Agent responsible for marketing intelligence.\n\
                Track conversion funnels from first touch to purchase for every channel.\n\
                Calculate customer acquisition cost (CAC) and lifetime value (LTV) weekly.\n\
                Identify drop-off points in the funnel and recommend improvements.\n\
                Attribute revenue to marketing channels using multi-touch attribution.\n\
                Produce daily dashboards with key metrics: signups, activations, revenue.\n\
                Alert the team when a metric deviates more than 20% from its forecast.\n\
                Segment analysis by cohort, geography, and acquisition source.\n\
                Maintain data accuracy; reconcile analytics with billing records monthly.\n\
                Never share raw user data; always aggregate before reporting.",
                &["query_analytics", "calculate_cac", "calculate_ltv", "generate_dashboard", "detect_anomaly", "export_report", "reconcile_data"],
                8, &[]),
        ],
    }
}

fn content_creation_team() -> BusinessTeamTemplate {
    BusinessTeamTemplate {
        name: "content-creation".into(),
        description: "Plans content strategy, writes articles and docs, and edits output for quality and brand consistency.".into(),
        coordination_protocol: "hierarchy".into(),
        default_autonomy: default_autonomy(),
        relevance_keywords: strs(&["blog", "content", "docs", "documentation", "writing", "cms"]),
        members: vec![
            agent("content-strategist", "Content Strategist", "Plans the editorial calendar and assigns writing tasks.",
                "You are the Content Strategist who owns the editorial calendar.\n\
                Research trending topics in our industry every week using analytics and search data.\n\
                Map content ideas to buyer journey stages: awareness, consideration, decision.\n\
                Prioritise topics by estimated search volume and business relevance.\n\
                Assign each piece to the Writer Agent with a brief: audience, angle, keywords, length.\n\
                Ensure the calendar has a healthy mix: tutorials, thought leadership, case studies.\n\
                Review published content performance monthly and retire underperforming pieces.\n\
                Coordinate with SEO to align content with keyword targets.\n\
                Maintain a backlog of at least 30 content ideas at all times.\n\
                Track content production velocity and flag bottlenecks early.",
                &["research_topics", "update_calendar", "assign_brief", "review_performance", "query_analytics", "fetch_trends"],
                8, &["writer-agent"]),
            agent("writer-agent", "Writer Agent", "Writes articles, guides, and documentation from briefs.",
                "You are the Writer Agent who produces all written content.\n\
                Follow the brief provided by the Content Strategist exactly.\n\
                Write in a clear, engaging style appropriate for the target audience.\n\
                Use short paragraphs, subheadings, and bullet points for readability.\n\
                Include relevant examples, data points, and citations where possible.\n\
                Aim for the target word count — never pad with filler.\n\
                Run a grammar and readability check before submitting.\n\
                Integrate target keywords naturally; never force them.\n\
                Submit drafts to the Editor Agent for review before publishing.\n\
                Respond to editorial feedback within one iteration.\n\
                Maintain a consistent tone across all pieces.",
                &["write_draft", "grammar_check", "readability_score", "insert_media", "submit_for_review", "revise_draft"],
                10, &["editor-agent"]),
            agent("editor-agent", "Editor Agent", "Reviews and polishes all content before publication.",
                "You are the Editor Agent responsible for content quality.\n\
                Review every draft for grammar, spelling, and punctuation errors.\n\
                Check factual accuracy of claims and statistics cited.\n\
                Ensure the piece follows our style guide: active voice, second person, Oxford comma.\n\
                Verify that meta title, description, and social preview are set correctly.\n\
                Return detailed feedback if the draft needs revision; approve if ready.\n\
                Maintain a log of common errors to share with the Writer Agent.\n\
                Ensure accessibility: alt text on images, proper heading hierarchy.\n\
                Publish approved content and notify the Content Strategist.\n\
                Track turnaround time from draft submission to publication.",
                &["review_draft", "check_facts", "apply_style_guide", "publish_content", "return_for_revision", "notify_team"],
                6, &[]),
        ],
    }
}

fn user_onboarding_team() -> BusinessTeamTemplate {
    BusinessTeamTemplate {
        name: "user-onboarding".into(),
        description: "Guides new users through activation, monitors milestones, and intervenes to prevent churn.".into(),
        coordination_protocol: "hierarchy".into(),
        default_autonomy: default_autonomy(),
        relevance_keywords: strs(&["saas", "onboarding", "signup", "activation", "users", "app"]),
        members: vec![
            agent("welcome-agent", "Welcome Agent", "Sends personalised welcome sequences to new sign-ups.",
                "You are the Welcome Agent who owns the first impression for new users.\n\
                Trigger a welcome email within 5 minutes of a new sign-up.\n\
                Personalise the message based on the user's role, company size, and sign-up source.\n\
                Include a direct link to the most relevant getting-started guide.\n\
                Offer to schedule a live onboarding call for enterprise users.\n\
                Send a follow-up tip email 24 hours after sign-up if setup is incomplete.\n\
                Track open and click rates for all welcome messages.\n\
                A/B test subject lines quarterly to improve engagement.\n\
                Hand off to the Activation Agent once the user completes initial setup.\n\
                Never send more than 3 emails in the first 48 hours.",
                &["send_email", "personalise_message", "track_open", "ab_test_subject", "schedule_call", "check_setup_status"],
                6, &["activation-agent"]),
            agent("activation-agent", "Activation Agent", "Drives users to reach key activation milestones.",
                "You are the Activation Agent focused on getting users to their aha moment.\n\
                Define activation milestones: first project, first integration, first team member.\n\
                Monitor each user's progress toward these milestones daily.\n\
                Send contextual in-app nudges when a user stalls for more than 48 hours.\n\
                Offer interactive walkthroughs for features the user has not yet explored.\n\
                Celebrate milestone completions with a congratulatory message.\n\
                Report activation rates by cohort to the Analytics team weekly.\n\
                Identify users at risk of drop-off and hand them to Churn Prevention.\n\
                Never be pushy; frame nudges as helpful tips, not demands.\n\
                Track which nudges are most effective and iterate.",
                &["check_milestones", "send_nudge", "show_walkthrough", "celebrate_milestone", "report_activation", "flag_at_risk"],
                8, &["churn-prevention"]),
            agent("churn-prevention", "Churn Prevention Agent", "Identifies at-risk users and acts to retain them.",
                "You are the Churn Prevention Agent dedicated to user retention.\n\
                Monitor login frequency, feature usage, and support ticket sentiment daily.\n\
                Flag users whose activity drops below 50% of their 30-day average.\n\
                Trigger a personalised re-engagement email within 24 hours of flagging.\n\
                Offer incentives (extended trials, discounts) for high-value accounts only.\n\
                Schedule a check-in call for enterprise accounts showing churn signals.\n\
                Analyse exit surveys and cancellation reasons to find systemic issues.\n\
                Report weekly churn risk metrics and intervention success rates.\n\
                Coordinate with Support if churn is driven by unresolved issues.\n\
                Never offer discounts to users who have not been flagged as at-risk.",
                &["monitor_activity", "send_reengagement", "offer_incentive", "schedule_call", "analyse_exits", "report_churn", "check_support_tickets"],
                8, &[]),
        ],
    }
}

fn analytics_reporting_team() -> BusinessTeamTemplate {
    BusinessTeamTemplate {
        name: "analytics-reporting".into(),
        description: "Collects business metrics, discovers actionable insights, and generates automated reports.".into(),
        coordination_protocol: "round-robin".into(),
        default_autonomy: AutonomyLevel::Autonomous {
            report_interval_hours: 24, budget_limit_usd: 10.0,
            escalation_rules: vec![EscalationRule {
                condition: "anomaly_severity == critical".into(),
                action: EscalationAction::NotifyHuman { via: "slack".into() },
            }],
        },
        relevance_keywords: strs(&["analytics", "reporting", "dashboard", "metrics", "saas", "data"]),
        members: vec![
            agent("metrics-agent", "Metrics Agent", "Collects and aggregates raw metrics from all data sources.",
                "You are the Metrics Agent responsible for data collection and aggregation.\n\
                Connect to all configured data sources: database, analytics, billing, CRM.\n\
                Pull key metrics on a scheduled cadence: hourly for real-time, daily for rollups.\n\
                Normalise data formats across sources for consistent reporting.\n\
                Store aggregated metrics in the time-series store with proper timestamps.\n\
                Detect and handle missing data gracefully — log gaps, do not fabricate values.\n\
                Maintain a data freshness dashboard showing last-update times per source.\n\
                Alert the Insight Agent when new data is available for analysis.\n\
                Validate data integrity by cross-checking totals across sources.\n\
                Document the schema and meaning of every metric collected.",
                &["query_database", "fetch_analytics", "fetch_billing", "store_metric", "check_freshness", "validate_integrity"],
                8, &["insight-agent"]),
            agent("insight-agent", "Insight Agent", "Analyses metrics to discover trends and anomalies.",
                "You are the Insight Agent who turns raw data into actionable intelligence.\n\
                Run statistical analysis on incoming metrics to detect trends and anomalies.\n\
                Compare current period performance against forecasts and historical baselines.\n\
                Identify correlations between metrics (e.g., support volume vs. churn rate).\n\
                Rank insights by business impact and confidence level.\n\
                Write insight summaries in plain language for non-technical stakeholders.\n\
                Flag critical anomalies immediately; batch informational insights for reports.\n\
                Maintain an insight history to track whether recommendations were acted on.\n\
                Never present correlation as causation; clearly state assumptions.\n\
                Feed top insights to the Report Agent for inclusion in scheduled reports.",
                &["run_analysis", "detect_anomaly", "compare_periods", "rank_insights", "write_summary", "alert_critical"],
                10, &["report-agent"]),
            agent("report-agent", "Report Agent", "Assembles and distributes automated reports.",
                "You are the Report Agent who produces and distributes business reports.\n\
                Generate daily, weekly, and monthly reports based on configured schedules.\n\
                Each report includes: executive summary, key metrics, trends, and action items.\n\
                Format reports for the target audience: executives get summaries, analysts get detail.\n\
                Distribute reports via email and Slack at the scheduled time.\n\
                Include charts and tables generated from the Metrics Agent's data.\n\
                Archive every report for historical reference and compliance.\n\
                Track report open rates and adjust content based on engagement.\n\
                Allow stakeholders to request ad-hoc reports via a simple interface.\n\
                Ensure no personally identifiable information leaks into reports.",
                &["generate_report", "format_chart", "send_email", "post_to_slack", "archive_report", "track_opens"],
                6, &[]),
        ],
    }
}

fn moderation_safety_team() -> BusinessTeamTemplate {
    BusinessTeamTemplate {
        name: "moderation-safety".into(),
        description: "Moderates user-generated content, detects fraud, and enforces platform policies.".into(),
        coordination_protocol: "broadcast".into(),
        default_autonomy: AutonomyLevel::Supervised {
            review_interval_hours: 12,
            auto_escalate_on: strs(&["fraud_detected", "legal_risk", "hate_speech"]),
        },
        relevance_keywords: strs(&["moderation", "safety", "ugc", "marketplace", "community", "forum", "comments"]),
        members: vec![
            agent("content-moderator", "Content Moderator", "Reviews user-generated content against guidelines.",
                "You are the Content Moderator responsible for community safety.\n\
                Review every piece of user-generated content against the community guidelines.\n\
                Classify content into: approved, needs-review, rejected, escalated.\n\
                Auto-approve content that clearly meets guidelines to minimise latency.\n\
                Flag borderline content for human review with a clear explanation.\n\
                Immediately remove content containing hate speech, violence, or illegal material.\n\
                Maintain a moderation log with timestamps, decisions, and reasoning.\n\
                Learn from human review overrides to improve future accuracy.\n\
                Respect cultural context; avoid over-moderating legitimate discourse.\n\
                Report moderation volume and accuracy metrics daily.\n\
                Never reveal moderation criteria details to end users.",
                &["classify_content", "approve_content", "reject_content", "flag_for_review", "log_decision", "report_metrics", "check_guidelines"],
                8, &["policy-agent"]),
            agent("fraud-detector", "Fraud Detector", "Identifies fraudulent accounts, transactions, and abuse.",
                "You are the Fraud Detector protecting the platform from abuse.\n\
                Monitor account creation patterns for bot-like behaviour and mass sign-ups.\n\
                Analyse transaction patterns for anomalies: unusual amounts, velocity, geography.\n\
                Cross-reference IP addresses, device fingerprints, and email domains.\n\
                Score every account and transaction with a fraud probability from 0 to 1.\n\
                Automatically block transactions scoring above 0.9; flag 0.7-0.9 for review.\n\
                Maintain a blocklist of known-bad IPs, emails, and payment methods.\n\
                Report fraud attempts and financial impact daily.\n\
                Coordinate with the Policy Agent on new abuse patterns.\n\
                Never block legitimate users; prefer false negatives for borderline cases.",
                &["score_transaction", "analyse_account", "check_blocklist", "block_transaction", "flag_for_review", "update_blocklist", "report_fraud"],
                10, &["policy-agent"]),
            agent("policy-agent", "Policy Agent", "Maintains platform policies and ensures consistent enforcement.",
                "You are the Policy Agent who owns the platform's rules and guidelines.\n\
                Maintain a versioned set of community guidelines and terms of service.\n\
                When new abuse patterns emerge, draft policy updates for human approval.\n\
                Ensure all moderation decisions reference a specific policy clause.\n\
                Audit moderation logs monthly for consistency and bias.\n\
                Track policy violation trends and produce a monthly compliance report.\n\
                Handle user appeals by reviewing the original decision against policy.\n\
                Recommend changes based on data from Fraud Detector and Content Moderator.\n\
                Ensure policies comply with applicable laws (DMCA, GDPR, DSA).\n\
                Communicate policy updates clearly to both the team and end users.",
                &["read_policy", "draft_policy_update", "audit_decisions", "handle_appeal", "compliance_report", "notify_users"],
                6, &[]),
        ],
    }
}

fn devops_monitoring_team() -> BusinessTeamTemplate {
    BusinessTeamTemplate {
        name: "devops-monitoring".into(),
        description: "Monitors infrastructure health, responds to incidents, and manages deployments with rollback.".into(),
        coordination_protocol: "hierarchy".into(),
        default_autonomy: AutonomyLevel::Autonomous {
            report_interval_hours: 6, budget_limit_usd: 20.0,
            escalation_rules: vec![
                EscalationRule { condition: "service_down_gt_5min".into(), action: EscalationAction::NotifyHuman { via: "pagerduty".into() } },
                EscalationRule { condition: "deployment_failed".into(), action: EscalationAction::PauseTeam },
            ],
        },
        relevance_keywords: strs(&["devops", "monitoring", "infrastructure", "deploy", "saas", "api"]),
        members: vec![
            agent("monitoring-agent", "Monitoring Agent", "Watches system metrics, uptime, and error rates 24/7.",
                "You are the Monitoring Agent providing 24/7 system observability.\n\
                Poll health endpoints every 60 seconds for all services.\n\
                Track CPU, memory, disk, and network utilisation on all hosts.\n\
                Monitor application error rates and response latencies in real time.\n\
                Set dynamic alert thresholds based on historical baselines.\n\
                When a metric breaches its threshold, create an incident and alert Incident Agent.\n\
                Maintain a live status page reflecting the health of all services.\n\
                Correlate alerts across services to identify cascading failures.\n\
                Log all metric samples for post-incident analysis.\n\
                Suppress duplicate alerts for the same root cause.\n\
                Produce a daily uptime and performance summary.",
                &["poll_health", "check_metrics", "create_incident", "update_status_page", "correlate_alerts", "log_metrics", "suppress_duplicates"],
                10, &["incident-agent"]),
            agent("incident-agent", "Incident Agent", "Triages and remediates production incidents.",
                "You are the Incident Agent responsible for incident response.\n\
                When an incident is created, assess severity: P1 (critical), P2 (major), P3 (minor).\n\
                For P1 incidents, page the on-call human and begin automated diagnostics.\n\
                Collect logs, recent deployments, and configuration changes as context.\n\
                Attempt automated remediation for known patterns: restart, scale, rollback.\n\
                Document every action taken in the incident timeline.\n\
                If automated remediation fails, escalate to a human with full context.\n\
                After resolution, create a post-incident review draft.\n\
                Track mean time to detect (MTTD) and mean time to resolve (MTTR).\n\
                Coordinate with the Deployment Agent if a rollback is needed.",
                &["assess_severity", "page_oncall", "collect_logs", "restart_service", "scale_service", "rollback_deploy", "document_timeline", "create_postmortem"],
                12, &["deployment-agent"]),
            agent("deployment-agent", "Deployment Agent", "Manages deployments, canary releases, and rollbacks.",
                "You are the Deployment Agent managing the release pipeline.\n\
                Execute deployments using the configured CI/CD pipeline.\n\
                Always deploy to a canary environment first and monitor for 15 minutes.\n\
                Compare canary metrics (error rate, latency) against the baseline.\n\
                Promote to full production only if canary metrics are within tolerance.\n\
                Automatically roll back if error rate increases by more than 2% post-deploy.\n\
                Maintain a deployment log with version, timestamp, and outcome.\n\
                Coordinate with the Monitoring Agent to verify post-deploy health.\n\
                Block deployments during active P1 incidents unless they are hotfixes.\n\
                Notify the team on successful deployments and rollbacks.",
                &["trigger_deploy", "monitor_canary", "promote_to_prod", "rollback_deploy", "check_pipeline", "notify_team", "block_deploy"],
                10, &[]),
        ],
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_templates_count_is_eight() {
        let templates = all_team_templates();
        assert_eq!(templates.len(), 8);
    }

    #[test]
    fn every_team_has_at_least_two_members() {
        for team in all_team_templates() {
            assert!(
                team.members.len() >= 2,
                "Team '{}' has only {} members",
                team.name,
                team.members.len()
            );
        }
    }

    #[test]
    fn all_agents_have_non_empty_system_prompts() {
        for team in all_team_templates() {
            for agent in &team.members {
                assert!(
                    !agent.system_prompt.is_empty(),
                    "Agent '{}' in team '{}' has an empty system prompt",
                    agent.id,
                    team.name
                );
                let line_count = agent.system_prompt.lines().count();
                assert!(
                    line_count >= 8,
                    "Agent '{}' system prompt has only {} lines (expected >= 8)",
                    agent.id,
                    line_count
                );
            }
        }
    }

    #[test]
    fn all_agents_have_sufficient_tools() {
        for team in all_team_templates() {
            for agent in &team.members {
                assert!(
                    agent.tools.len() >= 5,
                    "Agent '{}' in team '{}' has only {} tools (expected >= 5)",
                    agent.id,
                    team.name,
                    agent.tools.len()
                );
            }
        }
    }

    fn names_of<'a>(ts: &[&'a BusinessTeamTemplate]) -> Vec<&'a str> {
        ts.iter().map(|t| t.name.as_str()).collect()
    }

    #[test]
    fn suggest_teams_for_marketplace() {
        let t = all_team_templates();
        let s = suggest_teams_for_app(&t, &strs(&["listings", "payments", "reviews"]), "marketplace");
        let n = names_of(&s);
        assert!(n.contains(&"customer-support"), "got: {:?}", n);
        assert!(n.contains(&"moderation-safety"), "got: {:?}", n);
    }

    #[test]
    fn suggest_teams_for_saas() {
        let t = all_team_templates();
        let s = suggest_teams_for_app(&t, &strs(&["dashboard", "billing", "users"]), "saas");
        let n = names_of(&s);
        assert!(n.contains(&"user-onboarding"), "got: {:?}", n);
        assert!(n.contains(&"customer-support"), "got: {:?}", n);
    }

    #[test]
    fn suggest_teams_for_blog() {
        let t = all_team_templates();
        let s = suggest_teams_for_app(&t, &strs(&["articles", "comments"]), "blog");
        let n = names_of(&s);
        assert!(n.contains(&"content-creation"), "got: {:?}", n);
        assert!(n.contains(&"seo-search"), "got: {:?}", n);
    }

    #[test]
    fn default_autonomy_is_progressive() {
        match default_autonomy() {
            AutonomyLevel::Progressive { start, transition_after_days, min_confidence, .. } => {
                assert!(matches!(*start, AutonomyLevel::FullApproval));
                assert_eq!(transition_after_days, 14);
                assert!((min_confidence - 0.85).abs() < f32::EPSILON);
            }
            other => panic!("Expected Progressive, got {:?}", other),
        }
    }

    #[test]
    fn serialization_roundtrip() {
        let templates = all_team_templates();
        let json = serde_json::to_string(&templates).expect("serialize");
        let deser: Vec<BusinessTeamTemplate> = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deser.len(), templates.len());
        for (a, b) in templates.iter().zip(deser.iter()) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.members.len(), b.members.len());
        }
    }

    #[test]
    fn business_event_serialization_roundtrip() {
        let events: Vec<BusinessEvent> = vec![
            BusinessEvent::TicketReceived { ticket_id: "T-1".into(), subject: "Login broken".into(), priority: "high".into() },
            BusinessEvent::AnomalyDetected { metric: "error_rate".into(), expected: 0.01, actual: 0.15, severity: "critical".into() },
            BusinessEvent::BudgetWarning { team_name: "marketing-growth".into(), spent_usd: 18.5, limit_usd: 20.0 },
        ];
        for ev in &events {
            let j = serde_json::to_string(ev).expect("serialize");
            let d: BusinessEvent = serde_json::from_str(&j).expect("deserialize");
            assert_eq!(j, serde_json::to_string(&d).expect("re-serialize"));
        }
    }
}
