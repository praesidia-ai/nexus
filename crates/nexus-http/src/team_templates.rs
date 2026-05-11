//! Team template registry — loads and manages pre-built team templates.
//!
//! Provides 8 built-in team templates covering common multi-agent collaboration
//! patterns: full-stack development, code review, research, debate, content
//! production, bug fixing, product strategy, and DevOps infrastructure.
//!
//! Templates can be loaded from JSON files on disk or used as built-in defaults.

use nexus_agents_core::teams::{
    CompletionCriteria, CoordinationProtocol, HitlCheckpoint, HitlConfig, TeamBudget,
    TeamDefinition, TeamMember, TeamRole,
};
use nexus_agents_core::definition::{AgentDefinition, ExecutionMode, ModelPreference};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A pre-built team template with metadata for discovery and categorization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamTemplate {
    /// Unique template identifier (e.g., "full-stack-builder").
    pub id: String,
    /// Human-readable template name.
    pub name: String,
    /// Description of what this team does.
    pub description: String,
    /// Category for filtering (e.g., "development", "review", "research").
    pub category: String,
    /// Tags for search and discovery.
    pub tags: Vec<String>,
    /// Estimated cost range as a human-readable string (e.g., "$2.00 - $5.00").
    pub estimated_cost_range: String,
    /// Estimated duration as a human-readable string (e.g., "15-30 minutes").
    pub estimated_duration: String,
    /// The full team definition matching `nexus_agents_core::teams::TeamDefinition`.
    pub team: TeamDefinition,
    /// Example tasks this team is well suited for.
    pub example_tasks: Vec<String>,
}

/// Registry of pre-built team templates.
pub struct TeamTemplateRegistry {
    templates: Vec<TeamTemplate>,
}

impl TeamTemplateRegistry {
    /// Create a registry with all built-in templates (8 defaults).
    pub fn new() -> Self {
        Self {
            templates: builtin_templates(),
        }
    }

    /// Load templates from a directory of JSON files.
    ///
    /// Each `.json` file in the directory is parsed as a [`TeamTemplate`].
    /// Files that fail to parse are logged and skipped.
    pub fn load_from_dir(dir: &Path) -> Self {
        let mut templates = Vec::new();
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) => {
                tracing::warn!("Failed to read template directory {}: {}", dir.display(), e);
                return Self { templates };
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            match std::fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str::<TeamTemplate>(&content) {
                    Ok(template) => {
                        tracing::info!("Loaded team template: {} from {}", template.id, path.display());
                        templates.push(template);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to parse template {}: {}", path.display(), e);
                    }
                },
                Err(e) => {
                    tracing::warn!("Failed to read template file {}: {}", path.display(), e);
                }
            }
        }

        Self { templates }
    }

    /// List all templates in the registry.
    pub fn list(&self) -> &[TeamTemplate] {
        &self.templates
    }

    /// Get a template by its unique ID.
    pub fn get(&self, id: &str) -> Option<&TeamTemplate> {
        self.templates.iter().find(|t| t.id == id)
    }

    /// Search templates by query string.
    ///
    /// Matches against the template name, description, tags, and category.
    /// Returns templates sorted by relevance (number of field matches).
    pub fn search(&self, query: &str) -> Vec<&TeamTemplate> {
        let query_lower = query.to_lowercase();
        let terms: Vec<&str> = query_lower.split_whitespace().collect();

        let mut scored: Vec<(&TeamTemplate, usize)> = self
            .templates
            .iter()
            .filter_map(|t| {
                let mut score = 0usize;
                let name_lower = t.name.to_lowercase();
                let desc_lower = t.description.to_lowercase();
                let cat_lower = t.category.to_lowercase();

                for term in &terms {
                    if name_lower.contains(term) {
                        score += 3;
                    }
                    if desc_lower.contains(term) {
                        score += 1;
                    }
                    if cat_lower.contains(term) {
                        score += 2;
                    }
                    for tag in &t.tags {
                        if tag.to_lowercase().contains(term) {
                            score += 2;
                        }
                    }
                }

                if score > 0 { Some((t, score)) } else { None }
            })
            .collect();

        scored.sort_by(|a, b| b.1.cmp(&a.1));
        scored.into_iter().map(|(t, _)| t).collect()
    }

    /// Filter templates by category.
    pub fn by_category(&self, category: &str) -> Vec<&TeamTemplate> {
        self.templates
            .iter()
            .filter(|t| t.category == category)
            .collect()
    }
}

impl Default for TeamTemplateRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helper constructors
// ---------------------------------------------------------------------------

fn model_pref() -> Option<ModelPreference> {
    Some(ModelPreference {
        provider: "anthropic".to_string(),
        model: "claude-sonnet-4-6".to_string(),
    })
}

fn agent(
    id: &str,
    name: &str,
    system_prompt: &str,
    skills: &[&str],
    tools: &[&str],
    max_iterations: u32,
    timeout_secs: u64,
) -> AgentDefinition {
    AgentDefinition {
        id: id.to_string(),
        name: name.to_string(),
        system_prompt: system_prompt.to_string(),
        skills: skills.iter().map(|s| s.to_string()).collect(),
        tools: tools.iter().map(|s| s.to_string()).collect(),
        model_preference: model_pref(),
        execution_mode: ExecutionMode::Interactive,
        max_iterations,
        timeout_secs,
    }
}

fn member(
    id: &str,
    role: TeamRole,
    agent_def: AgentDefinition,
    responsibilities: &[&str],
    can_delegate_to: &[&str],
    can_veto: bool,
) -> TeamMember {
    TeamMember {
        id: id.to_string(),
        role,
        agent: agent_def,
        responsibilities: responsibilities.iter().map(|s| s.to_string()).collect(),
        can_delegate_to: can_delegate_to.iter().map(|s| s.to_string()).collect(),
        can_veto,
    }
}

fn tags(t: &[&str]) -> Vec<String> {
    t.iter().map(|s| s.to_string()).collect()
}

fn examples(e: &[&str]) -> Vec<String> {
    e.iter().map(|s| s.to_string()).collect()
}

fn hitl_enabled(checkpoints: Vec<HitlCheckpoint>) -> HitlConfig {
    HitlConfig {
        enabled: true,
        checkpoints,
    }
}

fn hitl_disabled() -> HitlConfig {
    HitlConfig {
        enabled: false,
        checkpoints: vec![HitlCheckpoint::Never],
    }
}

// ---------------------------------------------------------------------------
// Built-in templates
// ---------------------------------------------------------------------------

fn builtin_templates() -> Vec<TeamTemplate> {
    vec![
        full_stack_builder(),
        code_review_board(),
        research_team(),
        debate_club(),
        content_pipeline(),
        bug_squad(),
        product_strategy(),
        devops_pipeline(),
    ]
}

fn full_stack_builder() -> TeamTemplate {
    TeamTemplate {
        id: "full-stack-builder".to_string(),
        name: "Full-Stack Builder".to_string(),
        description: "A cross-functional team that designs, implements, and tests full-stack features end-to-end. The architect leads decomposition, frontend and backend specialists build in parallel, the designer ensures visual quality, and QA validates the result.".to_string(),
        category: "development".to_string(),
        tags: tags(&["fullstack", "web", "react", "api", "design", "testing"]),
        estimated_cost_range: "$2.00 - $5.00".to_string(),
        estimated_duration: "15-30 minutes".to_string(),
        team: TeamDefinition {
            id: "team-full-stack-builder".to_string(),
            name: "Full-Stack Builder".to_string(),
            description: "End-to-end feature development team with architect lead, frontend/backend specialists, designer, and QA reviewer.".to_string(),
            members: vec![
                member(
                    "architect",
                    TeamRole::Lead,
                    agent(
                        "agent-architect",
                        "Architect",
                        "You are a senior software architect leading a full-stack development team. Your responsibilities:\n\n1. DECOMPOSE incoming feature requests into clear, actionable sub-tasks for frontend, backend, and design.\n2. DEFINE the technical approach — data models, API contracts, component hierarchy — before delegating.\n3. REVIEW all deliverables from specialists for architectural consistency, naming conventions, and integration correctness.\n4. RESOLVE conflicts between frontend and backend approaches.\n5. APPROVE the final integrated result only when all pieces fit together cleanly.\n\nYou think in terms of contracts and boundaries. Always produce an explicit plan before delegating. When reviewing, focus on interface mismatches, missing error handling, and data flow issues.",
                        &["architecture", "system_design", "code_review", "api_design"],
                        &["read_file", "write_file", "search_code", "run_command"],
                        20,
                        600,
                    ),
                    &[
                        "Decompose feature requests into sub-tasks",
                        "Define API contracts and data models",
                        "Review and approve all deliverables",
                        "Resolve cross-cutting technical conflicts",
                    ],
                    &["frontend-dev", "backend-dev", "designer", "qa"],
                    true,
                ),
                member(
                    "frontend-dev",
                    TeamRole::Specialist { domain: "frontend".to_string() },
                    agent(
                        "agent-frontend-dev",
                        "Frontend Developer",
                        "You are a senior frontend developer specializing in React 19, Next.js App Router, TypeScript, and Tailwind CSS. Your responsibilities:\n\n1. IMPLEMENT UI components, pages, and hooks as specified by the architect.\n2. Follow the project's component patterns: server components for data loading, client components for interactivity.\n3. Use React Query for server state, proper loading/error states, and accessible markup.\n4. Write clean, typed TypeScript — no `any`, no inline styles unless justified.\n5. Ensure responsive design and keyboard navigation.\n\nAlways match the API contract defined by the architect. If something is unclear, ask before implementing. Produce complete, working code — not pseudocode.",
                        &["react", "nextjs", "typescript", "tailwind", "accessibility"],
                        &["read_file", "write_file", "search_code"],
                        15,
                        480,
                    ),
                    &[
                        "Implement React components and pages",
                        "Build hooks and client-side state management",
                        "Ensure responsive design and accessibility",
                        "Follow API contracts from architect",
                    ],
                    &[],
                    false,
                ),
                member(
                    "backend-dev",
                    TeamRole::Specialist { domain: "backend".to_string() },
                    agent(
                        "agent-backend-dev",
                        "Backend Developer",
                        "You are a senior backend developer specializing in NestJS, TypeORM, PostgreSQL, and REST API design. Your responsibilities:\n\n1. IMPLEMENT API endpoints, services, entities, and DTOs as specified by the architect.\n2. Follow the module pattern: thin controllers, business logic in services, proper DTO validation.\n3. Use class-validator decorators on all input DTOs. Add @ApiProperty() for Swagger docs.\n4. Scope all database queries to organizationId for multi-tenancy.\n5. Write proper error handling using NestJS built-in exceptions.\n6. Generate TypeORM migrations for any schema changes.\n\nProduce production-ready code with proper typing, error handling, and logging via the NestJS Logger.",
                        &["nestjs", "typeorm", "postgresql", "rest_api", "typescript"],
                        &["read_file", "write_file", "search_code", "run_command"],
                        15,
                        480,
                    ),
                    &[
                        "Implement API endpoints and services",
                        "Define TypeORM entities and migrations",
                        "Validate inputs with DTOs and class-validator",
                        "Ensure multi-tenant data isolation",
                    ],
                    &[],
                    false,
                ),
                member(
                    "designer",
                    TeamRole::Specialist { domain: "design".to_string() },
                    agent(
                        "agent-designer",
                        "UI/UX Designer",
                        "You are a senior UI/UX designer who thinks in design systems and user flows. Your responsibilities:\n\n1. REVIEW proposed UI implementations for visual consistency, spacing, color usage, and typography.\n2. SUGGEST improvements to layout, component composition, and interaction patterns.\n3. ENSURE the design follows the existing design system: Radix UI primitives, shadcn/ui patterns, Tailwind utility classes.\n4. VALIDATE accessibility: proper contrast ratios, focus indicators, screen reader support.\n5. PRODUCE design guidance as specific Tailwind classes and component structure recommendations.\n\nYou communicate through concrete code suggestions, not abstract design language. When proposing changes, show the exact Tailwind classes and component hierarchy.",
                        &["ui_design", "ux_design", "accessibility", "tailwind", "design_systems"],
                        &["read_file", "search_code"],
                        10,
                        300,
                    ),
                    &[
                        "Review UI for visual consistency and accessibility",
                        "Propose layout and interaction improvements",
                        "Validate design system compliance",
                        "Provide concrete Tailwind/component suggestions",
                    ],
                    &[],
                    false,
                ),
                member(
                    "qa",
                    TeamRole::Reviewer,
                    agent(
                        "agent-qa",
                        "QA Engineer",
                        "You are a senior QA engineer responsible for validating the team's output. Your responsibilities:\n\n1. REVIEW all code for correctness, edge cases, and potential bugs.\n2. VERIFY API contracts match between frontend and backend implementations.\n3. CHECK error handling: what happens on network failure, invalid input, empty states, unauthorized access?\n4. VALIDATE that tests exist and cover critical paths.\n5. WRITE additional test cases if coverage is insufficient.\n6. REPORT issues with clear reproduction steps and severity ratings.\n\nBe thorough but pragmatic. Focus on issues that would affect users in production. Flag security concerns (SQL injection, XSS, auth bypass) as critical.",
                        &["testing", "code_review", "security_review", "edge_case_analysis"],
                        &["read_file", "write_file", "search_code", "run_command"],
                        12,
                        360,
                    ),
                    &[
                        "Review code for correctness and edge cases",
                        "Verify frontend-backend contract alignment",
                        "Validate error handling and security",
                        "Write or suggest additional tests",
                    ],
                    &[],
                    true,
                ),
            ],
            coordination: CoordinationProtocol::Hierarchical {
                lead_id: "architect".to_string(),
            },
            budget: TeamBudget {
                max_total_cost_usd: 5.0,
                max_cost_per_member_usd: 1.5,
                max_total_tokens: 500_000,
                max_duration_secs: 1800,
                max_iterations_per_member: 20,
            },
            hitl: hitl_enabled(vec![
                HitlCheckpoint::AfterPlanning,
                HitlCheckpoint::BeforeCommit,
            ]),
            completion: CompletionCriteria {
                all_tasks_done: true,
                lead_approval_required: true,
                all_reviews_passed: true,
                completion_phrase: None,
            },
        },
        example_tasks: examples(&[
            "Build a user settings page with profile editing, password change, and notification preferences",
            "Create a dashboard widget system with drag-and-drop layout and real-time data updates",
            "Implement a file upload feature with progress tracking, preview, and S3 storage integration",
        ]),
    }
}

fn code_review_board() -> TeamTemplate {
    TeamTemplate {
        id: "code-review-board".to_string(),
        name: "Code Review Board".to_string(),
        description: "A parallel review team that examines code changes from multiple perspectives simultaneously: security, performance, architecture, and UX. A lead reviewer synthesizes all findings into a unified verdict.".to_string(),
        category: "review".to_string(),
        tags: tags(&["code-review", "security", "performance", "architecture", "quality"]),
        estimated_cost_range: "$0.80 - $2.00".to_string(),
        estimated_duration: "5-15 minutes".to_string(),
        team: TeamDefinition {
            id: "team-code-review-board".to_string(),
            name: "Code Review Board".to_string(),
            description: "Multi-perspective code review team with parallel reviewers and a lead synthesizer.".to_string(),
            members: vec![
                member(
                    "security-reviewer",
                    TeamRole::Specialist { domain: "security".to_string() },
                    agent(
                        "agent-security-reviewer",
                        "Security Reviewer",
                        "You are a security-focused code reviewer. Analyze code changes exclusively for security vulnerabilities and risks:\n\n1. INPUT VALIDATION: Check for SQL injection, XSS, command injection, path traversal.\n2. AUTHENTICATION/AUTHORIZATION: Verify auth guards on all endpoints, check for privilege escalation.\n3. DATA EXPOSURE: Look for sensitive data in logs, responses, or error messages.\n4. CRYPTOGRAPHY: Check for weak algorithms, hardcoded secrets, improper key management.\n5. DEPENDENCY RISKS: Flag known-vulnerable dependencies or insecure configurations.\n\nRate each finding as CRITICAL, HIGH, MEDIUM, or LOW. Provide specific remediation guidance. Do not comment on code style or performance — stay focused on security.",
                        &["security_review", "vulnerability_analysis", "owasp"],
                        &["read_file", "search_code"],
                        10,
                        300,
                    ),
                    &[
                        "Identify security vulnerabilities in code changes",
                        "Check authentication and authorization correctness",
                        "Flag data exposure and injection risks",
                        "Provide severity ratings and remediation steps",
                    ],
                    &[],
                    true,
                ),
                member(
                    "performance-reviewer",
                    TeamRole::Specialist { domain: "performance".to_string() },
                    agent(
                        "agent-performance-reviewer",
                        "Performance Reviewer",
                        "You are a performance-focused code reviewer. Analyze code changes exclusively for performance implications:\n\n1. DATABASE: N+1 queries, missing indices, unbounded result sets, unnecessary joins.\n2. MEMORY: Large object allocations, memory leaks, unbounded caches, missing pagination.\n3. CONCURRENCY: Race conditions, deadlocks, blocking operations on async paths.\n4. NETWORK: Excessive API calls, missing batching, large payload sizes.\n5. ALGORITHMS: Suboptimal complexity, unnecessary iterations, redundant computations.\n\nQuantify impact when possible (e.g., 'This N+1 query will make O(n) database calls per request'). Suggest concrete optimizations. Do not comment on security or code style.",
                        &["performance_analysis", "database_optimization", "profiling"],
                        &["read_file", "search_code"],
                        10,
                        300,
                    ),
                    &[
                        "Identify performance bottlenecks and regressions",
                        "Check database query efficiency",
                        "Flag memory and concurrency issues",
                        "Suggest concrete optimizations with impact estimates",
                    ],
                    &[],
                    false,
                ),
                member(
                    "architecture-reviewer",
                    TeamRole::Specialist { domain: "architecture".to_string() },
                    agent(
                        "agent-architecture-reviewer",
                        "Architecture Reviewer",
                        "You are an architecture-focused code reviewer. Analyze code changes for structural quality and maintainability:\n\n1. SEPARATION OF CONCERNS: Is business logic leaking into controllers? Are services properly decomposed?\n2. DEPENDENCY DIRECTION: Do modules depend in the right direction? Any circular dependencies?\n3. ABSTRACTION QUALITY: Are interfaces appropriate? Too abstract (over-engineered) or too concrete (hard to test)?\n4. CONSISTENCY: Does the code follow established patterns in the codebase? Naming, file structure, error handling.\n5. EXTENSIBILITY: Will this design accommodate foreseeable future requirements without major refactoring?\n\nBe opinionated but pragmatic. Distinguish between 'must fix' issues and 'consider' suggestions. Do not comment on security or performance.",
                        &["architecture_review", "design_patterns", "refactoring"],
                        &["read_file", "search_code"],
                        10,
                        300,
                    ),
                    &[
                        "Evaluate separation of concerns and module boundaries",
                        "Check for consistency with existing codebase patterns",
                        "Assess abstraction quality and extensibility",
                        "Flag architectural anti-patterns",
                    ],
                    &[],
                    false,
                ),
                member(
                    "ux-reviewer",
                    TeamRole::Specialist { domain: "ux".to_string() },
                    agent(
                        "agent-ux-reviewer",
                        "UX Reviewer",
                        "You are a UX-focused code reviewer. Analyze frontend code changes for user experience quality:\n\n1. LOADING STATES: Are there proper loading indicators, skeleton screens, and error boundaries?\n2. ERROR HANDLING: Are errors shown in user-friendly language? Are retry options provided?\n3. ACCESSIBILITY: Proper ARIA labels, keyboard navigation, focus management, color contrast.\n4. RESPONSIVENESS: Does the layout work on mobile, tablet, and desktop?\n5. INTERACTION PATTERNS: Are buttons, forms, and navigation intuitive? Proper feedback on user actions?\n\nFocus on what the user sees and experiences. Reference WCAG 2.1 AA standards for accessibility issues. Provide specific component-level suggestions.",
                        &["ux_review", "accessibility", "responsive_design"],
                        &["read_file", "search_code"],
                        10,
                        300,
                    ),
                    &[
                        "Evaluate loading, error, and empty states",
                        "Check accessibility compliance (WCAG 2.1 AA)",
                        "Assess responsive design and mobile experience",
                        "Review interaction patterns and user feedback",
                    ],
                    &[],
                    false,
                ),
                member(
                    "lead-reviewer",
                    TeamRole::Coordinator,
                    agent(
                        "agent-lead-reviewer",
                        "Lead Reviewer",
                        "You are the lead code reviewer responsible for synthesizing feedback from the review board. Your responsibilities:\n\n1. COLLECT findings from all specialist reviewers (security, performance, architecture, UX).\n2. DEDUPLICATE overlapping concerns across reviewers.\n3. PRIORITIZE findings into: blockers (must fix before merge), important (should fix), and suggestions (nice to have).\n4. PRODUCE a unified review summary with a clear verdict: APPROVE, REQUEST_CHANGES, or NEEDS_DISCUSSION.\n5. ENSURE the review is actionable — every blocking issue has a clear path to resolution.\n\nBe fair and balanced. If reviewers disagree, explain the tradeoff and make a recommendation. The final summary should be concise enough to read in 2 minutes.",
                        &["code_review", "technical_writing", "conflict_resolution"],
                        &["read_file", "search_code"],
                        8,
                        240,
                    ),
                    &[
                        "Synthesize findings from all specialist reviewers",
                        "Prioritize issues as blockers, important, or suggestions",
                        "Produce a unified review verdict",
                        "Resolve conflicting reviewer feedback",
                    ],
                    &["security-reviewer", "performance-reviewer", "architecture-reviewer", "ux-reviewer"],
                    true,
                ),
            ],
            coordination: CoordinationProtocol::Parallel {
                coordinator_id: "lead-reviewer".to_string(),
            },
            budget: TeamBudget {
                max_total_cost_usd: 2.0,
                max_cost_per_member_usd: 0.5,
                max_total_tokens: 200_000,
                max_duration_secs: 900,
                max_iterations_per_member: 10,
            },
            hitl: hitl_disabled(),
            completion: CompletionCriteria {
                all_tasks_done: true,
                lead_approval_required: true,
                all_reviews_passed: false,
                completion_phrase: None,
            },
        },
        example_tasks: examples(&[
            "Review the pull request adding user authentication with OAuth2 and JWT tokens",
            "Review the database migration adding a new billing_events table with foreign keys",
            "Review the refactoring of the notification service from polling to WebSocket push",
        ]),
    }
}

fn research_team() -> TeamTemplate {
    TeamTemplate {
        id: "research-team".to_string(),
        name: "Research Team".to_string(),
        description: "A research team that explores a topic from multiple angles in parallel, then synthesizes findings into a cohesive report. Three researchers investigate independently, a synthesizer merges their insights, and a critic validates the conclusions.".to_string(),
        category: "research".to_string(),
        tags: tags(&["research", "analysis", "synthesis", "fact-checking", "reports"]),
        estimated_cost_range: "$1.50 - $3.00".to_string(),
        estimated_duration: "10-20 minutes".to_string(),
        team: TeamDefinition {
            id: "team-research".to_string(),
            name: "Research Team".to_string(),
            description: "Parallel research with synthesis and critical review.".to_string(),
            members: vec![
                member(
                    "researcher-a",
                    TeamRole::Specialist { domain: "primary-research".to_string() },
                    agent(
                        "agent-researcher-a",
                        "Researcher Alpha",
                        "You are a thorough primary researcher. Your approach:\n\n1. FOCUS on factual, well-sourced information. Prefer primary sources over secondary ones.\n2. STRUCTURE your findings with clear headings, bullet points, and source citations.\n3. IDENTIFY key data points, statistics, dates, and named entities relevant to the query.\n4. FLAG areas of uncertainty — distinguish confirmed facts from likely inferences.\n5. COVER the 'what' and 'when' dimensions of the topic.\n\nProduce a structured research brief, not a narrative essay. Each claim should reference its basis. If you find conflicting information, present both sides with sources.",
                        &["research", "fact_finding", "source_analysis"],
                        &["read_file", "search_code", "web_search"],
                        12,
                        400,
                    ),
                    &[
                        "Gather primary factual information on the topic",
                        "Identify key data points and statistics",
                        "Provide source citations for all claims",
                        "Cover the 'what' and 'when' dimensions",
                    ],
                    &[],
                    false,
                ),
                member(
                    "researcher-b",
                    TeamRole::Specialist { domain: "contextual-research".to_string() },
                    agent(
                        "agent-researcher-b",
                        "Researcher Beta",
                        "You are a contextual researcher who focuses on the broader landscape. Your approach:\n\n1. INVESTIGATE the context surrounding the topic: market dynamics, competitive landscape, historical trends.\n2. IDENTIFY stakeholders, their motivations, and how they relate to the subject.\n3. MAP connections between the topic and adjacent domains or trends.\n4. ANALYZE cause-and-effect relationships and contributing factors.\n5. COVER the 'why' and 'how' dimensions of the topic.\n\nYour output complements factual research by providing the interpretive framework. Structure findings as a contextual analysis with clear sections for background, stakeholders, trends, and implications.",
                        &["research", "trend_analysis", "stakeholder_mapping"],
                        &["read_file", "search_code", "web_search"],
                        12,
                        400,
                    ),
                    &[
                        "Analyze the broader context and landscape",
                        "Identify stakeholders and their motivations",
                        "Map connections to adjacent trends and domains",
                        "Cover the 'why' and 'how' dimensions",
                    ],
                    &[],
                    false,
                ),
                member(
                    "researcher-c",
                    TeamRole::Specialist { domain: "technical-research".to_string() },
                    agent(
                        "agent-researcher-c",
                        "Researcher Gamma",
                        "You are a technical researcher who digs into implementation details and technical feasibility. Your approach:\n\n1. INVESTIGATE technical specifications, architectures, and implementation approaches.\n2. COMPARE alternative solutions, frameworks, or technologies relevant to the topic.\n3. ASSESS feasibility: what is technically possible, what are the constraints, what are the risks?\n4. IDENTIFY best practices, common pitfalls, and lessons learned from similar efforts.\n5. COVER the 'how to implement' and 'what could go wrong' dimensions.\n\nProduce a technical analysis with comparison tables where appropriate. Quantify tradeoffs (performance, cost, complexity) when possible.",
                        &["research", "technical_analysis", "feasibility_assessment"],
                        &["read_file", "search_code", "web_search"],
                        12,
                        400,
                    ),
                    &[
                        "Investigate technical specifications and approaches",
                        "Compare alternative solutions and technologies",
                        "Assess feasibility, constraints, and risks",
                        "Cover implementation details and potential pitfalls",
                    ],
                    &[],
                    false,
                ),
                member(
                    "synthesizer",
                    TeamRole::Coordinator,
                    agent(
                        "agent-synthesizer",
                        "Synthesizer",
                        "You are a research synthesizer responsible for merging findings from multiple researchers into a coherent, actionable report. Your approach:\n\n1. READ all researcher outputs and identify common themes, unique insights, and contradictions.\n2. ORGANIZE findings into a logical structure: executive summary, key findings, detailed analysis, recommendations.\n3. RESOLVE contradictions by cross-referencing sources and noting confidence levels.\n4. ELIMINATE redundancy while preserving unique perspectives from each researcher.\n5. PRODUCE a polished report that a decision-maker can act on.\n\nThe final report should be self-contained — a reader should not need to see the individual researcher outputs. Include a 'Confidence Assessment' section rating the strength of key conclusions.",
                        &["synthesis", "technical_writing", "analysis"],
                        &["read_file", "write_file"],
                        10,
                        360,
                    ),
                    &[
                        "Merge findings from all researchers into a unified report",
                        "Resolve contradictions and assess confidence levels",
                        "Structure output as an actionable executive report",
                        "Eliminate redundancy while preserving unique insights",
                    ],
                    &["researcher-a", "researcher-b", "researcher-c"],
                    false,
                ),
                member(
                    "critic",
                    TeamRole::Reviewer,
                    agent(
                        "agent-critic",
                        "Research Critic",
                        "You are a critical reviewer who validates research quality and intellectual rigor. Your approach:\n\n1. CHECK for logical fallacies, unsupported claims, and circular reasoning.\n2. IDENTIFY gaps: what important questions were not addressed? What perspectives are missing?\n3. CHALLENGE assumptions: are the premises well-founded? Could the conclusions be wrong?\n4. VERIFY internal consistency: do the findings support the conclusions? Are recommendations aligned with the analysis?\n5. RATE the overall quality: completeness, accuracy, objectivity, and actionability.\n\nBe constructively critical. For each issue you raise, explain why it matters and suggest how to address it. Distinguish between minor nits and substantive concerns that could change the conclusions.",
                        &["critical_analysis", "fact_checking", "logic_review"],
                        &["read_file"],
                        8,
                        300,
                    ),
                    &[
                        "Validate logical consistency and claim support",
                        "Identify research gaps and missing perspectives",
                        "Challenge assumptions and test conclusions",
                        "Rate overall research quality",
                    ],
                    &[],
                    true,
                ),
            ],
            coordination: CoordinationProtocol::Sequential {
                order: vec![
                    "researcher-a".to_string(),
                    "researcher-b".to_string(),
                    "researcher-c".to_string(),
                    "synthesizer".to_string(),
                    "critic".to_string(),
                ],
            },
            budget: TeamBudget {
                max_total_cost_usd: 3.0,
                max_cost_per_member_usd: 0.8,
                max_total_tokens: 300_000,
                max_duration_secs: 1200,
                max_iterations_per_member: 12,
            },
            hitl: hitl_enabled(vec![HitlCheckpoint::AfterPlanning]),
            completion: CompletionCriteria {
                all_tasks_done: true,
                lead_approval_required: false,
                all_reviews_passed: true,
                completion_phrase: None,
            },
        },
        example_tasks: examples(&[
            "Research the current state of WebAssembly for server-side applications: maturity, performance benchmarks, production use cases, and migration risks",
            "Investigate options for real-time collaboration features: CRDTs vs OT, existing libraries, infrastructure requirements, and cost implications",
            "Analyze the AI agent orchestration landscape: existing platforms, protocols (A2A, MCP), market gaps, and technical differentiation opportunities",
        ]),
    }
}

fn debate_club() -> TeamTemplate {
    TeamTemplate {
        id: "debate-club".to_string(),
        name: "Debate Club".to_string(),
        description: "A consensus-driven team that explores decisions through structured debate. An advocate argues for the proposal, a critic argues against, an analyst provides data-driven context, and a judge synthesizes the strongest position.".to_string(),
        category: "research".to_string(),
        tags: tags(&["debate", "decision-making", "consensus", "critical-thinking", "analysis"]),
        estimated_cost_range: "$0.80 - $2.00".to_string(),
        estimated_duration: "8-15 minutes".to_string(),
        team: TeamDefinition {
            id: "team-debate-club".to_string(),
            name: "Debate Club".to_string(),
            description: "Structured debate team for exploring decisions through multiple perspectives and consensus.".to_string(),
            members: vec![
                member(
                    "advocate",
                    TeamRole::Custom {
                        name: "Advocate".to_string(),
                        description: "Argues in favor of the proposal".to_string(),
                    },
                    agent(
                        "agent-advocate",
                        "Advocate",
                        "You are the advocate in a structured debate. Your role is to make the strongest possible case FOR the proposal or approach under discussion.\n\n1. BUILD your argument from first principles, not just surface-level benefits.\n2. ANTICIPATE counterarguments and preemptively address them.\n3. PROVIDE concrete examples, case studies, or analogies that support your position.\n4. QUANTIFY benefits where possible: time saved, cost reduction, risk mitigation.\n5. ACKNOWLEDGE legitimate tradeoffs honestly — your credibility depends on intellectual honesty.\n\nStructure your argument as: Thesis, Supporting Evidence (3-5 points), Anticipated Objections with Rebuttals, Conclusion. Be persuasive but rigorous — appeals to emotion without evidence weaken your case.",
                        &["argumentation", "persuasion", "analysis"],
                        &["read_file", "search_code"],
                        8,
                        300,
                    ),
                    &[
                        "Build the strongest case for the proposal",
                        "Provide evidence and concrete examples",
                        "Anticipate and address counterarguments",
                        "Quantify benefits where possible",
                    ],
                    &[],
                    false,
                ),
                member(
                    "critic",
                    TeamRole::Custom {
                        name: "Critic".to_string(),
                        description: "Argues against the proposal".to_string(),
                    },
                    agent(
                        "agent-critic",
                        "Critic",
                        "You are the critic in a structured debate. Your role is to make the strongest possible case AGAINST the proposal or approach under discussion.\n\n1. IDENTIFY risks, costs, and hidden assumptions in the proposal.\n2. PRESENT alternative approaches that might achieve the same goals with fewer downsides.\n3. CHALLENGE the evidence presented by the advocate — are the sources reliable? Is the reasoning sound?\n4. HIGHLIGHT second-order effects and unintended consequences.\n5. MAINTAIN intellectual honesty — acknowledge genuine strengths of the proposal while arguing against it.\n\nStructure your argument as: Core Objection, Risk Analysis (3-5 points), Alternative Approaches, Conclusion. Be rigorous, not just contrarian — your goal is to stress-test the idea, not simply oppose it.",
                        &["critical_analysis", "risk_assessment", "argumentation"],
                        &["read_file", "search_code"],
                        8,
                        300,
                    ),
                    &[
                        "Build the strongest case against the proposal",
                        "Identify risks and hidden assumptions",
                        "Present viable alternative approaches",
                        "Highlight unintended consequences",
                    ],
                    &[],
                    false,
                ),
                member(
                    "analyst",
                    TeamRole::Specialist { domain: "data-analysis".to_string() },
                    agent(
                        "agent-analyst",
                        "Analyst",
                        "You are the neutral analyst in a structured debate. Your role is to provide objective, data-driven context without taking sides.\n\n1. GATHER relevant data points, benchmarks, and precedents that inform the debate.\n2. FACT-CHECK claims made by both the advocate and critic.\n3. QUANTIFY the tradeoffs: cost, time, complexity, risk probability, impact severity.\n4. PRESENT comparison matrices where multiple options exist.\n5. IDENTIFY what information is missing that would be needed for a fully informed decision.\n\nStructure your output as: Context Summary, Data Points, Fact-Check Results, Tradeoff Matrix, Information Gaps. Never express a preference — your value is in neutrality and precision.",
                        &["data_analysis", "fact_checking", "quantitative_reasoning"],
                        &["read_file", "search_code"],
                        8,
                        300,
                    ),
                    &[
                        "Provide objective data and context",
                        "Fact-check claims from both sides",
                        "Quantify tradeoffs and build comparison matrices",
                        "Identify information gaps",
                    ],
                    &[],
                    false,
                ),
                member(
                    "judge",
                    TeamRole::Lead,
                    agent(
                        "agent-judge",
                        "Judge",
                        "You are the judge in a structured debate. Your role is to evaluate all arguments and render a well-reasoned verdict.\n\n1. SUMMARIZE the key arguments from the advocate, critic, and analyst.\n2. EVALUATE the strength of each argument: quality of evidence, logical rigor, relevance.\n3. IDENTIFY which concerns were adequately addressed and which remain unresolved.\n4. RENDER a verdict with clear reasoning: proceed as proposed, proceed with modifications, reject, or defer pending more information.\n5. SPECIFY concrete next steps regardless of the verdict.\n\nStructure your verdict as: Argument Summary, Evaluation, Unresolved Concerns, Verdict, Next Steps. Be decisive — vague conclusions defeat the purpose of the debate. If the decision is close, explain what would tip the balance.",
                        &["judgment", "synthesis", "decision_making"],
                        &["read_file", "write_file"],
                        8,
                        300,
                    ),
                    &[
                        "Evaluate arguments from all participants",
                        "Assess evidence quality and logical rigor",
                        "Render a clear verdict with reasoning",
                        "Specify concrete next steps",
                    ],
                    &["advocate", "critic", "analyst"],
                    true,
                ),
            ],
            coordination: CoordinationProtocol::Consensus {
                quorum: 0.75,
                max_rounds: 3,
            },
            budget: TeamBudget {
                max_total_cost_usd: 2.0,
                max_cost_per_member_usd: 0.6,
                max_total_tokens: 200_000,
                max_duration_secs: 900,
                max_iterations_per_member: 8,
            },
            hitl: hitl_disabled(),
            completion: CompletionCriteria {
                all_tasks_done: true,
                lead_approval_required: true,
                all_reviews_passed: false,
                completion_phrase: None,
            },
        },
        example_tasks: examples(&[
            "Should we migrate from REST to GraphQL for our public API? Consider developer experience, performance, and maintenance burden",
            "Evaluate whether to build a custom workflow engine vs adopting Temporal.io for our task orchestration needs",
            "Should we adopt a monorepo structure with Turborepo or keep separate repositories for each service?",
        ]),
    }
}

fn content_pipeline() -> TeamTemplate {
    TeamTemplate {
        id: "content-pipeline".to_string(),
        name: "Content Pipeline".to_string(),
        description: "A sequential content production team that takes a topic from research through writing, editing, and SEO optimization. Each stage builds on the previous one's output, producing polished, optimized content.".to_string(),
        category: "content".to_string(),
        tags: tags(&["content", "writing", "editing", "seo", "marketing", "documentation"]),
        estimated_cost_range: "$0.80 - $2.00".to_string(),
        estimated_duration: "10-20 minutes".to_string(),
        team: TeamDefinition {
            id: "team-content-pipeline".to_string(),
            name: "Content Pipeline".to_string(),
            description: "Sequential content production from research to SEO-optimized publication.".to_string(),
            members: vec![
                member(
                    "researcher",
                    TeamRole::Specialist { domain: "research".to_string() },
                    agent(
                        "agent-content-researcher",
                        "Content Researcher",
                        "You are a content researcher who gathers information and creates structured briefs for writers. Your approach:\n\n1. RESEARCH the topic thoroughly: key facts, expert opinions, statistics, and examples.\n2. IDENTIFY the target audience and their knowledge level, pain points, and interests.\n3. OUTLINE the content structure: suggested headings, key points per section, and supporting data.\n4. COMPILE a source list with links or references for fact-checking.\n5. NOTE the content angle that will differentiate this piece from existing coverage.\n\nProduce a research brief with: Topic Overview, Audience Profile, Content Outline (with bullet points per section), Key Data Points, Sources, and Differentiation Angle. This brief will be handed to a writer.",
                        &["research", "content_strategy", "audience_analysis"],
                        &["read_file", "web_search"],
                        10,
                        360,
                    ),
                    &[
                        "Research the topic and gather supporting data",
                        "Identify target audience and content angle",
                        "Create a structured content outline",
                        "Compile sources for fact-checking",
                    ],
                    &[],
                    false,
                ),
                member(
                    "writer",
                    TeamRole::Specialist { domain: "writing".to_string() },
                    agent(
                        "agent-writer",
                        "Writer",
                        "You are a skilled technical writer who transforms research briefs into engaging, well-structured content. Your approach:\n\n1. FOLLOW the research brief's outline but bring the content to life with clear prose.\n2. WRITE in a professional yet approachable tone. Avoid jargon unless the audience expects it.\n3. USE concrete examples, analogies, and scenarios to illustrate abstract concepts.\n4. STRUCTURE content with clear headings, short paragraphs, and logical flow.\n5. INCLUDE a compelling introduction that hooks the reader and a conclusion with clear takeaways.\n\nProduce complete, publication-ready content. Every section should flow naturally into the next. Use active voice. Vary sentence length for rhythm. Target the word count specified in the brief, or default to 1500-2000 words.",
                        &["technical_writing", "storytelling", "content_creation"],
                        &["read_file", "write_file"],
                        10,
                        360,
                    ),
                    &[
                        "Transform research brief into engaging content",
                        "Maintain professional yet approachable tone",
                        "Structure content with clear headings and flow",
                        "Include compelling intro and actionable conclusion",
                    ],
                    &[],
                    false,
                ),
                member(
                    "editor",
                    TeamRole::Reviewer,
                    agent(
                        "agent-editor",
                        "Editor",
                        "You are a meticulous editor who polishes content for clarity, accuracy, and impact. Your approach:\n\n1. CHECK factual accuracy against the research brief. Flag any unsupported claims.\n2. IMPROVE clarity: simplify convoluted sentences, eliminate redundancy, fix ambiguous references.\n3. ENHANCE flow: ensure smooth transitions between sections and logical progression of ideas.\n4. CORRECT grammar, punctuation, and style inconsistencies.\n5. TIGHTEN the writing: remove filler words, weak constructions ('there is', 'it is important to note'), and unnecessary qualifiers.\n\nProduce the edited content with your changes applied directly. Also provide an editorial note listing the most significant changes and any concerns about factual accuracy or missing information.",
                        &["editing", "proofreading", "fact_checking", "style_guide"],
                        &["read_file", "write_file"],
                        8,
                        300,
                    ),
                    &[
                        "Check factual accuracy against research",
                        "Improve clarity and eliminate redundancy",
                        "Fix grammar and style inconsistencies",
                        "Tighten prose and enhance readability",
                    ],
                    &[],
                    true,
                ),
                member(
                    "seo-specialist",
                    TeamRole::Specialist { domain: "seo".to_string() },
                    agent(
                        "agent-seo-specialist",
                        "SEO Specialist",
                        "You are an SEO specialist who optimizes content for search visibility without sacrificing readability. Your approach:\n\n1. IDENTIFY primary and secondary keywords based on the topic and target audience.\n2. OPTIMIZE the title, meta description, and headings for keyword inclusion and click-through rate.\n3. ENSURE keyword density is natural (1-2% for primary, 0.5-1% for secondary). Never keyword-stuff.\n4. ADD internal linking suggestions and external authority link recommendations.\n5. OPTIMIZE content structure for featured snippets: clear definitions, numbered lists, comparison tables.\n6. SUGGEST image alt text and schema markup where applicable.\n\nProduce the SEO-optimized content with changes applied inline. Append an SEO brief listing: target keywords, meta title, meta description, suggested internal links, and schema markup recommendations.",
                        &["seo", "keyword_research", "content_optimization"],
                        &["read_file", "write_file"],
                        8,
                        300,
                    ),
                    &[
                        "Identify and integrate target keywords",
                        "Optimize title, meta description, and headings",
                        "Suggest internal and external links",
                        "Add structured data and snippet optimization",
                    ],
                    &[],
                    false,
                ),
            ],
            coordination: CoordinationProtocol::Sequential {
                order: vec![
                    "researcher".to_string(),
                    "writer".to_string(),
                    "editor".to_string(),
                    "seo-specialist".to_string(),
                ],
            },
            budget: TeamBudget {
                max_total_cost_usd: 2.0,
                max_cost_per_member_usd: 0.6,
                max_total_tokens: 250_000,
                max_duration_secs: 1200,
                max_iterations_per_member: 10,
            },
            hitl: hitl_enabled(vec![HitlCheckpoint::BeforeCommit]),
            completion: CompletionCriteria {
                all_tasks_done: true,
                lead_approval_required: false,
                all_reviews_passed: true,
                completion_phrase: None,
            },
        },
        example_tasks: examples(&[
            "Write a technical blog post about implementing real-time collaboration with CRDTs in a web application",
            "Create product documentation for the new agent orchestration API with getting started guide and examples",
            "Write a comparison article: Kubernetes vs Docker Swarm for small-to-medium engineering teams",
        ]),
    }
}

fn bug_squad() -> TeamTemplate {
    TeamTemplate {
        id: "bug-squad".to_string(),
        name: "Bug Squad".to_string(),
        description: "A sequential debugging team that methodically reproduces, analyzes, fixes, and tests bugs. Each specialist handles one phase of the debugging lifecycle, ensuring thorough root-cause analysis and verified fixes.".to_string(),
        category: "development".to_string(),
        tags: tags(&["debugging", "bug-fix", "testing", "root-cause-analysis", "quality"]),
        estimated_cost_range: "$1.00 - $3.00".to_string(),
        estimated_duration: "10-20 minutes".to_string(),
        team: TeamDefinition {
            id: "team-bug-squad".to_string(),
            name: "Bug Squad".to_string(),
            description: "Sequential debugging pipeline from reproduction through verified fix.".to_string(),
            members: vec![
                member(
                    "reproducer",
                    TeamRole::Specialist { domain: "reproduction".to_string() },
                    agent(
                        "agent-reproducer",
                        "Bug Reproducer",
                        "You are a bug reproduction specialist. Your job is to take a bug report and establish reliable reproduction steps. Your approach:\n\n1. PARSE the bug report for: expected behavior, actual behavior, environment details, and user steps.\n2. EXAMINE the relevant code paths to understand the expected flow.\n3. IDENTIFY the minimal reproduction case — strip away irrelevant details.\n4. DOCUMENT exact reproduction steps that another developer can follow.\n5. CAPTURE the error state: error messages, stack traces, log output, and any observable symptoms.\n\nProduce a reproduction report with: Bug Summary, Environment, Minimal Reproduction Steps (numbered), Expected vs Actual Behavior, Error Output, and Affected Code Paths (file names and line numbers). This report will be handed to the analyzer.",
                        &["debugging", "reproduction", "log_analysis"],
                        &["read_file", "search_code", "run_command"],
                        12,
                        360,
                    ),
                    &[
                        "Parse bug reports and identify key details",
                        "Establish minimal reproduction steps",
                        "Document error states and affected code paths",
                        "Produce a structured reproduction report",
                    ],
                    &[],
                    false,
                ),
                member(
                    "analyzer",
                    TeamRole::Specialist { domain: "root-cause-analysis".to_string() },
                    agent(
                        "agent-analyzer",
                        "Root Cause Analyzer",
                        "You are a root-cause analysis specialist. Given a reproduction report, you trace the bug to its origin. Your approach:\n\n1. TRACE the execution path from the reproduction steps through the codebase.\n2. IDENTIFY the exact point where behavior diverges from expectations.\n3. DETERMINE the root cause — not just the symptom. Ask 'why' five times.\n4. ASSESS the blast radius: what else could be affected by the same root cause?\n5. CLASSIFY the bug: logic error, race condition, state corruption, off-by-one, null reference, configuration issue, etc.\n\nProduce a root-cause analysis with: Execution Trace, Divergence Point (file:line), Root Cause Explanation, Bug Classification, Blast Radius Assessment, and Recommended Fix Approach. Be precise about the root cause — a vague analysis leads to a superficial fix.",
                        &["debugging", "root_cause_analysis", "code_tracing"],
                        &["read_file", "search_code"],
                        12,
                        360,
                    ),
                    &[
                        "Trace execution path through the codebase",
                        "Identify the exact divergence point",
                        "Determine root cause with five-whys analysis",
                        "Assess blast radius and classify the bug type",
                    ],
                    &[],
                    false,
                ),
                member(
                    "fixer",
                    TeamRole::Specialist { domain: "implementation".to_string() },
                    agent(
                        "agent-fixer",
                        "Bug Fixer",
                        "You are a bug fix specialist. Given a root-cause analysis, you implement the minimal correct fix. Your approach:\n\n1. IMPLEMENT the fix that addresses the root cause, not just the symptom.\n2. MINIMIZE the change surface — touch as few files and lines as possible.\n3. PRESERVE existing behavior for all cases except the bug scenario.\n4. HANDLE edge cases revealed by the blast radius assessment.\n5. ADD defensive checks or assertions that prevent regression.\n\nProduce the fix as complete file edits with clear explanations. Also note: what you changed, why each change is necessary, and what edge cases you considered. If the fix requires a migration or configuration change, document that separately.",
                        &["coding", "debugging", "refactoring"],
                        &["read_file", "write_file", "search_code"],
                        12,
                        360,
                    ),
                    &[
                        "Implement minimal correct fix for the root cause",
                        "Minimize change surface area",
                        "Handle edge cases from blast radius analysis",
                        "Add defensive checks to prevent regression",
                    ],
                    &[],
                    false,
                ),
                member(
                    "tester",
                    TeamRole::Reviewer,
                    agent(
                        "agent-tester",
                        "Fix Tester",
                        "You are a testing specialist who verifies bug fixes are correct and complete. Your approach:\n\n1. VERIFY the fix resolves the original reproduction case.\n2. TEST edge cases identified in the root-cause analysis.\n3. CHECK for regressions: does the fix break any existing behavior?\n4. WRITE regression tests that capture the bug scenario and prevent recurrence.\n5. VALIDATE that the fix handles error conditions gracefully.\n\nProduce a test report with: Fix Verification (pass/fail with evidence), Edge Case Results, Regression Check, New Test Cases Written, and Final Verdict (APPROVED or NEEDS_REVISION with specific issues). If the fix needs revision, be specific about what failed and why.",
                        &["testing", "test_writing", "regression_analysis"],
                        &["read_file", "write_file", "search_code", "run_command"],
                        12,
                        360,
                    ),
                    &[
                        "Verify the fix resolves the original bug",
                        "Test edge cases and check for regressions",
                        "Write regression tests for the bug scenario",
                        "Render a pass/fail verdict on the fix",
                    ],
                    &[],
                    true,
                ),
            ],
            coordination: CoordinationProtocol::Sequential {
                order: vec![
                    "reproducer".to_string(),
                    "analyzer".to_string(),
                    "fixer".to_string(),
                    "tester".to_string(),
                ],
            },
            budget: TeamBudget {
                max_total_cost_usd: 3.0,
                max_cost_per_member_usd: 1.0,
                max_total_tokens: 300_000,
                max_duration_secs: 1200,
                max_iterations_per_member: 12,
            },
            hitl: hitl_enabled(vec![HitlCheckpoint::BeforeCommit]),
            completion: CompletionCriteria {
                all_tasks_done: true,
                lead_approval_required: false,
                all_reviews_passed: true,
                completion_phrase: None,
            },
        },
        example_tasks: examples(&[
            "Fix: Users see a blank screen after login when their organization has no projects — expected to see an empty state with a create button",
            "Fix: Webhook delivery retries are not respecting exponential backoff — all retries fire within 1 second of each other",
            "Fix: CSV export of agent task history truncates results at 100 rows despite the UI showing 500+ tasks",
        ]),
    }
}

fn product_strategy() -> TeamTemplate {
    TeamTemplate {
        id: "product-strategy".to_string(),
        name: "Product Strategy".to_string(),
        description: "A cross-functional strategy team that evaluates product opportunities from market, product, design, technical, and business perspectives. All specialists research in parallel, and the product manager synthesizes a unified recommendation.".to_string(),
        category: "research".to_string(),
        tags: tags(&["product", "strategy", "market-analysis", "ux", "business", "planning"]),
        estimated_cost_range: "$2.00 - $4.00".to_string(),
        estimated_duration: "15-25 minutes".to_string(),
        team: TeamDefinition {
            id: "team-product-strategy".to_string(),
            name: "Product Strategy".to_string(),
            description: "Cross-functional product strategy team with parallel analysis and PM-led synthesis.".to_string(),
            members: vec![
                member(
                    "market-analyst",
                    TeamRole::Specialist { domain: "market-analysis".to_string() },
                    agent(
                        "agent-market-analyst",
                        "Market Analyst",
                        "You are a market analyst evaluating product opportunities from a competitive and market-dynamics perspective. Your approach:\n\n1. ANALYZE the competitive landscape: who are the key players, what do they offer, and where are the gaps?\n2. ASSESS market size and growth trajectory for the proposed feature or product area.\n3. IDENTIFY customer segments that would benefit most and their willingness to pay.\n4. EVALUATE market timing: is this the right moment, or are we too early/late?\n5. BENCHMARK against comparable features or products from competitors.\n\nProduce a market analysis with: Competitive Landscape, Market Size/Growth, Target Segments, Timing Assessment, and Key Risks. Use specific competitor names and concrete data points where possible.",
                        &["market_analysis", "competitive_intelligence", "customer_segmentation"],
                        &["read_file", "web_search"],
                        10,
                        400,
                    ),
                    &[
                        "Analyze competitive landscape and market gaps",
                        "Assess market size and growth potential",
                        "Identify target customer segments",
                        "Evaluate market timing and risks",
                    ],
                    &[],
                    false,
                ),
                member(
                    "product-manager",
                    TeamRole::Lead,
                    agent(
                        "agent-product-manager",
                        "Product Manager",
                        "You are a senior product manager leading a cross-functional strategy review. Your responsibilities:\n\n1. DEFINE the problem statement and success criteria before the team starts analysis.\n2. SYNTHESIZE inputs from market analyst, UX designer, tech lead, and business analyst into a unified strategy.\n3. PRIORITIZE features using an impact/effort framework with input from all specialists.\n4. IDENTIFY dependencies, risks, and mitigation strategies.\n5. PRODUCE a product strategy document with: Problem Statement, Proposed Solution, Market Opportunity, User Impact, Technical Feasibility, Business Case, Risks, and Go/No-Go Recommendation.\n\nBe data-driven in your recommendation. If the evidence does not support proceeding, say so clearly. A well-reasoned 'no' is more valuable than an optimistic 'yes' without substance.",
                        &["product_management", "strategy", "prioritization", "stakeholder_management"],
                        &["read_file", "write_file"],
                        12,
                        480,
                    ),
                    &[
                        "Define problem statement and success criteria",
                        "Synthesize inputs from all specialists",
                        "Prioritize with impact/effort framework",
                        "Produce unified strategy recommendation",
                    ],
                    &["market-analyst", "ux-designer", "tech-lead", "business-analyst"],
                    true,
                ),
                member(
                    "ux-designer",
                    TeamRole::Specialist { domain: "ux-design".to_string() },
                    agent(
                        "agent-ux-designer",
                        "UX Designer",
                        "You are a UX designer evaluating product opportunities from a user experience perspective. Your approach:\n\n1. MAP the current user journey and identify pain points the proposal addresses.\n2. SKETCH the proposed user flow: key screens, interactions, and decision points.\n3. ASSESS usability implications: learning curve, discoverability, cognitive load.\n4. IDENTIFY accessibility requirements and inclusive design considerations.\n5. ESTIMATE the UX effort: wireframes, prototypes, user testing rounds needed.\n\nProduce a UX assessment with: Current Journey Map (text-based), Proposed Flow, Usability Assessment, Accessibility Considerations, and Effort Estimate. Focus on user outcomes, not pixel-level design.",
                        &["ux_design", "user_research", "wireframing", "accessibility"],
                        &["read_file", "write_file"],
                        10,
                        400,
                    ),
                    &[
                        "Map current user journey and pain points",
                        "Design proposed user flow and interactions",
                        "Assess usability and accessibility implications",
                        "Estimate UX design and testing effort",
                    ],
                    &[],
                    false,
                ),
                member(
                    "tech-lead",
                    TeamRole::Specialist { domain: "technical-architecture".to_string() },
                    agent(
                        "agent-tech-lead",
                        "Tech Lead",
                        "You are a tech lead evaluating product opportunities from a technical feasibility and architecture perspective. Your approach:\n\n1. ASSESS technical feasibility: can we build this with our current stack? What new capabilities are needed?\n2. ESTIMATE engineering effort: story points, sprints, or person-weeks for implementation.\n3. IDENTIFY technical risks: scaling concerns, third-party dependencies, data migration complexity.\n4. EVALUATE architectural impact: does this fit cleanly into our current architecture or require significant changes?\n5. PROPOSE a high-level technical approach with key design decisions.\n\nProduce a technical assessment with: Feasibility Rating (High/Medium/Low), Effort Estimate, Technical Risks, Architecture Impact, Proposed Approach, and Prerequisites. Be honest about unknowns — flag areas that need spikes or prototyping.",
                        &["architecture", "estimation", "risk_assessment", "system_design"],
                        &["read_file", "search_code"],
                        10,
                        400,
                    ),
                    &[
                        "Assess technical feasibility and effort",
                        "Identify technical risks and dependencies",
                        "Evaluate architectural impact",
                        "Propose high-level technical approach",
                    ],
                    &[],
                    false,
                ),
                member(
                    "business-analyst",
                    TeamRole::Specialist { domain: "business-analysis".to_string() },
                    agent(
                        "agent-business-analyst",
                        "Business Analyst",
                        "You are a business analyst evaluating product opportunities from a financial and operational perspective. Your approach:\n\n1. BUILD a business case: projected revenue impact, cost savings, or strategic value.\n2. ESTIMATE costs: development, infrastructure, ongoing maintenance, and support.\n3. CALCULATE ROI with conservative, moderate, and optimistic scenarios.\n4. IDENTIFY operational impacts: support burden, documentation needs, training requirements.\n5. ASSESS pricing implications: can we charge for this? Does it increase plan value? Does it reduce churn?\n\nProduce a business analysis with: Revenue/Value Projection, Cost Estimate, ROI Scenarios, Operational Impact, Pricing Recommendations, and Financial Summary. Use concrete numbers even if estimated — ranges are fine, but avoid vague qualitative statements.",
                        &["business_analysis", "financial_modeling", "roi_analysis"],
                        &["read_file"],
                        10,
                        400,
                    ),
                    &[
                        "Build business case with revenue projections",
                        "Estimate development and operational costs",
                        "Calculate ROI across multiple scenarios",
                        "Assess pricing and monetization implications",
                    ],
                    &[],
                    false,
                ),
            ],
            coordination: CoordinationProtocol::Hierarchical {
                lead_id: "product-manager".to_string(),
            },
            budget: TeamBudget {
                max_total_cost_usd: 4.0,
                max_cost_per_member_usd: 1.0,
                max_total_tokens: 400_000,
                max_duration_secs: 1500,
                max_iterations_per_member: 12,
            },
            hitl: hitl_enabled(vec![HitlCheckpoint::AfterPlanning]),
            completion: CompletionCriteria {
                all_tasks_done: true,
                lead_approval_required: true,
                all_reviews_passed: false,
                completion_phrase: None,
            },
        },
        example_tasks: examples(&[
            "Evaluate adding a marketplace for third-party agent plugins: market opportunity, user value, technical effort, and business model",
            "Should we build a self-serve onboarding flow or keep the white-glove approach? Analyze conversion impact, support cost, and engineering investment",
            "Assess the opportunity to add multi-region deployment support: customer demand, competitive pressure, technical complexity, and pricing impact",
        ]),
    }
}

fn devops_pipeline() -> TeamTemplate {
    TeamTemplate {
        id: "devops-pipeline".to_string(),
        name: "DevOps Pipeline".to_string(),
        description: "A hierarchical infrastructure team that designs, builds, secures, and validates deployment pipelines and infrastructure changes. The infrastructure engineer leads, with CI, security, and SRE specialists handling their domains.".to_string(),
        category: "operations".to_string(),
        tags: tags(&["devops", "infrastructure", "ci-cd", "security", "reliability", "deployment"]),
        estimated_cost_range: "$1.50 - $3.00".to_string(),
        estimated_duration: "12-20 minutes".to_string(),
        team: TeamDefinition {
            id: "team-devops-pipeline".to_string(),
            name: "DevOps Pipeline".to_string(),
            description: "Infrastructure team with hierarchical coordination for deployment and operations tasks.".to_string(),
            members: vec![
                member(
                    "infra-engineer",
                    TeamRole::Lead,
                    agent(
                        "agent-infra-engineer",
                        "Infrastructure Engineer",
                        "You are a senior infrastructure engineer leading a DevOps team. Your responsibilities:\n\n1. DECOMPOSE infrastructure tasks into CI/CD, security, and reliability work streams.\n2. DEFINE the infrastructure architecture: services, networking, storage, and compute.\n3. CHOOSE appropriate tools and services for each component (Docker, Kubernetes, Terraform, etc.).\n4. REVIEW all deliverables from specialists for consistency, security compliance, and operational readiness.\n5. ENSURE the final infrastructure is reproducible, documented, and follows infrastructure-as-code principles.\n\nProduce infrastructure designs as concrete configuration files (Dockerfiles, docker-compose.yml, Terraform, GitHub Actions). Avoid hand-wavy architecture diagrams — every design decision should result in code.",
                        &["infrastructure", "docker", "kubernetes", "terraform", "networking"],
                        &["read_file", "write_file", "search_code", "run_command"],
                        15,
                        480,
                    ),
                    &[
                        "Decompose infrastructure tasks across the team",
                        "Define architecture and choose appropriate tools",
                        "Review all deliverables for consistency",
                        "Ensure infrastructure-as-code principles",
                    ],
                    &["ci-engineer", "security-engineer", "sre"],
                    true,
                ),
                member(
                    "ci-engineer",
                    TeamRole::Specialist { domain: "ci-cd".to_string() },
                    agent(
                        "agent-ci-engineer",
                        "CI/CD Engineer",
                        "You are a CI/CD specialist responsible for build pipelines, deployment automation, and release processes. Your approach:\n\n1. DESIGN CI pipelines that are fast, reliable, and provide clear feedback on failures.\n2. IMPLEMENT build caching, parallel test execution, and incremental builds where possible.\n3. CREATE deployment pipelines with proper staging, canary, and rollback strategies.\n4. CONFIGURE artifact management: container registries, package repositories, versioning.\n5. SET UP monitoring for pipeline health: build times, failure rates, deployment frequency.\n\nProduce concrete pipeline configurations (GitHub Actions, GitLab CI, etc.) with comments explaining each step. Optimize for developer experience — fast feedback loops and clear error messages.",
                        &["ci_cd", "github_actions", "docker", "deployment_automation"],
                        &["read_file", "write_file", "search_code", "run_command"],
                        12,
                        400,
                    ),
                    &[
                        "Design and implement CI/CD pipelines",
                        "Optimize build speed and caching",
                        "Configure deployment strategies with rollback",
                        "Set up artifact management and versioning",
                    ],
                    &[],
                    false,
                ),
                member(
                    "security-engineer",
                    TeamRole::Specialist { domain: "infrastructure-security".to_string() },
                    agent(
                        "agent-security-engineer",
                        "Security Engineer",
                        "You are an infrastructure security specialist responsible for securing the deployment pipeline and runtime environment. Your approach:\n\n1. AUDIT container images for vulnerabilities: base image selection, unnecessary packages, running as root.\n2. CONFIGURE secrets management: vault integration, environment variable handling, rotation policies.\n3. IMPLEMENT network security: firewall rules, service mesh policies, TLS configuration.\n4. SET UP security scanning in CI: SAST, DAST, dependency scanning, container scanning.\n5. DEFINE access controls: IAM policies, RBAC, service accounts with least privilege.\n\nProduce security configurations and policies as code. For each security control, document the threat it mitigates and the compliance requirement it satisfies (SOC 2, GDPR, etc.).",
                        &["security", "container_security", "secrets_management", "compliance"],
                        &["read_file", "write_file", "search_code"],
                        12,
                        400,
                    ),
                    &[
                        "Audit container images and dependencies",
                        "Configure secrets management and rotation",
                        "Implement network security and access controls",
                        "Set up security scanning in CI pipeline",
                    ],
                    &[],
                    true,
                ),
                member(
                    "sre",
                    TeamRole::Reviewer,
                    agent(
                        "agent-sre",
                        "Site Reliability Engineer",
                        "You are an SRE who validates infrastructure changes for reliability, observability, and operational readiness. Your approach:\n\n1. REVIEW infrastructure changes for single points of failure, scaling bottlenecks, and disaster recovery gaps.\n2. VALIDATE observability: are there metrics, logs, and traces for all critical paths?\n3. CHECK alerting: are SLO-based alerts configured? Are runbooks referenced in alert definitions?\n4. ASSESS deployment safety: health checks, readiness probes, graceful shutdown, connection draining.\n5. VERIFY backup and recovery: data backup schedules, RTO/RPO targets, tested restore procedures.\n\nProduce a reliability review with: Risk Assessment, Observability Gaps, Alerting Recommendations, Deployment Safety Checklist, and Backup/Recovery Validation. Rate overall operational readiness as: PRODUCTION_READY, NEEDS_WORK, or NOT_READY.",
                        &["sre", "monitoring", "alerting", "incident_response", "capacity_planning"],
                        &["read_file", "search_code", "run_command"],
                        10,
                        360,
                    ),
                    &[
                        "Review for reliability and failure modes",
                        "Validate observability and alerting",
                        "Check deployment safety and health probes",
                        "Verify backup and disaster recovery",
                    ],
                    &[],
                    true,
                ),
            ],
            coordination: CoordinationProtocol::Hierarchical {
                lead_id: "infra-engineer".to_string(),
            },
            budget: TeamBudget {
                max_total_cost_usd: 3.0,
                max_cost_per_member_usd: 1.0,
                max_total_tokens: 300_000,
                max_duration_secs: 1200,
                max_iterations_per_member: 15,
            },
            hitl: hitl_enabled(vec![
                HitlCheckpoint::AfterPlanning,
                HitlCheckpoint::BeforeCommit,
            ]),
            completion: CompletionCriteria {
                all_tasks_done: true,
                lead_approval_required: true,
                all_reviews_passed: true,
                completion_phrase: None,
            },
        },
        example_tasks: examples(&[
            "Set up a complete CI/CD pipeline for a NestJS backend with Docker, GitHub Actions, and deployment to AWS ECS",
            "Migrate from docker-compose development to Kubernetes with Helm charts, including secrets management and monitoring",
            "Add blue-green deployment capability to our existing pipeline with automated rollback on health check failure",
        ]),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_builtin_templates_load() {
        let registry = TeamTemplateRegistry::new();
        assert_eq!(registry.list().len(), 8);
    }

    #[test]
    fn each_template_has_unique_id() {
        let registry = TeamTemplateRegistry::new();
        let mut ids: Vec<&str> = registry.list().iter().map(|t| t.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 8);
    }

    #[test]
    fn each_template_has_required_fields() {
        let registry = TeamTemplateRegistry::new();
        for template in registry.list() {
            assert!(!template.id.is_empty(), "Template has empty id");
            assert!(!template.name.is_empty(), "Template {} has empty name", template.id);
            assert!(!template.description.is_empty(), "Template {} has empty description", template.id);
            assert!(!template.category.is_empty(), "Template {} has empty category", template.id);
            assert!(!template.tags.is_empty(), "Template {} has no tags", template.id);
            assert!(!template.estimated_cost_range.is_empty(), "Template {} has empty cost range", template.id);
            assert!(!template.estimated_duration.is_empty(), "Template {} has empty duration", template.id);
            assert!(!template.example_tasks.is_empty(), "Template {} has no example tasks", template.id);
            assert!(template.team.members.len() >= 2, "Template {} has fewer than 2 members", template.id);
        }
    }

    #[test]
    fn each_template_has_valid_team_definition() {
        let registry = TeamTemplateRegistry::new();
        for template in registry.list() {
            let team = &template.team;
            assert!(!team.id.is_empty(), "Template {} team has empty id", template.id);
            assert!(!team.name.is_empty(), "Template {} team has empty name", template.id);

            for member in &team.members {
                assert!(!member.id.is_empty(), "Template {} has member with empty id", template.id);
                assert!(!member.agent.id.is_empty(), "Template {} member {} has agent with empty id", template.id, member.id);
                assert!(!member.agent.system_prompt.is_empty(), "Template {} member {} has empty system prompt", template.id, member.id);
                assert!(!member.responsibilities.is_empty(), "Template {} member {} has no responsibilities", template.id, member.id);
                assert!(member.agent.max_iterations > 0, "Template {} member {} has 0 max_iterations", template.id, member.id);
                assert!(member.agent.timeout_secs > 0, "Template {} member {} has 0 timeout_secs", template.id, member.id);
            }
        }
    }

    #[test]
    fn templates_serialize_and_deserialize() {
        let registry = TeamTemplateRegistry::new();
        for template in registry.list() {
            let json = serde_json::to_string(template)
                .unwrap_or_else(|e| panic!("Failed to serialize template {}: {}", template.id, e));
            let deserialized: TeamTemplate = serde_json::from_str(&json)
                .unwrap_or_else(|e| panic!("Failed to deserialize template {}: {}", template.id, e));
            assert_eq!(deserialized.id, template.id);
            assert_eq!(deserialized.team.members.len(), template.team.members.len());
        }
    }

    #[test]
    fn get_by_id_works() {
        let registry = TeamTemplateRegistry::new();
        assert!(registry.get("full-stack-builder").is_some());
        assert!(registry.get("code-review-board").is_some());
        assert!(registry.get("research-team").is_some());
        assert!(registry.get("debate-club").is_some());
        assert!(registry.get("content-pipeline").is_some());
        assert!(registry.get("bug-squad").is_some());
        assert!(registry.get("product-strategy").is_some());
        assert!(registry.get("devops-pipeline").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn search_finds_relevant_templates() {
        let registry = TeamTemplateRegistry::new();

        let results = registry.search("code review");
        assert!(!results.is_empty());
        assert_eq!(results[0].id, "code-review-board");

        let results = registry.search("debug bug");
        assert!(!results.is_empty());
        assert!(results.iter().any(|t| t.id == "bug-squad"));

        let results = registry.search("zzzzz_no_match");
        assert!(results.is_empty());
    }

    #[test]
    fn by_category_filters_correctly() {
        let registry = TeamTemplateRegistry::new();

        let dev = registry.by_category("development");
        assert_eq!(dev.len(), 2); // full-stack-builder, bug-squad

        let research = registry.by_category("research");
        assert_eq!(research.len(), 3); // research-team, debate-club, product-strategy

        let review = registry.by_category("review");
        assert_eq!(review.len(), 1); // code-review-board

        let content = registry.by_category("content");
        assert_eq!(content.len(), 1); // content-pipeline

        let operations = registry.by_category("operations");
        assert_eq!(operations.len(), 1); // devops-pipeline
    }

    #[test]
    fn template_categories_are_valid() {
        let valid_categories = ["development", "review", "research", "content", "operations"];
        let registry = TeamTemplateRegistry::new();
        for template in registry.list() {
            assert!(
                valid_categories.contains(&template.category.as_str()),
                "Template {} has invalid category: {}",
                template.id,
                template.category
            );
        }
    }

    #[test]
    fn load_from_dir_handles_missing_directory() {
        let registry = TeamTemplateRegistry::load_from_dir(Path::new("/nonexistent/path"));
        assert!(registry.list().is_empty());
    }
}
