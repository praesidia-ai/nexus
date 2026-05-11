//! Pre-built team templates — convenience constructors for common team compositions.
//!
//! Each function returns a `TeamDefinition` from `nexus_agents_core::teams` that
//! can be executed directly by the team orchestrator or registered as a template.

use nexus_agents_core::definition::{AgentDefinition, ExecutionMode, ModelPreference};
use nexus_agents_core::teams::{
    CompletionCriteria, CoordinationProtocol, HitlCheckpoint, HitlConfig, TeamBudget,
    TeamDefinition, TeamMember, TeamRole,
};

use super::prompts;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn model_pref() -> Option<ModelPreference> {
    Some(ModelPreference {
        provider: "anthropic".to_string(),
        model: "claude-sonnet-4-6".to_string(),
    })
}

fn make_agent(
    id: &str,
    name: &str,
    role_key: &str,
    extra_instructions: &str,
    tools: &[&str],
    max_iterations: u32,
    timeout_secs: u64,
) -> AgentDefinition {
    let base_prompt = prompts::specialization_prompt(role_key).unwrap_or("");
    let system_prompt = if extra_instructions.is_empty() {
        base_prompt.to_string()
    } else {
        format!("{base_prompt}\n\n{extra_instructions}")
    };

    AgentDefinition {
        id: id.to_string(),
        name: name.to_string(),
        system_prompt,
        skills: vec![role_key.to_string()],
        tools: tools.iter().map(|s| s.to_string()).collect(),
        model_preference: model_pref(),
        execution_mode: ExecutionMode::Interactive,
        max_iterations,
        timeout_secs,
    }
}

fn make_member(
    id: &str,
    role: TeamRole,
    agent: AgentDefinition,
    responsibilities: &[&str],
    delegates_to: &[&str],
    can_veto: bool,
) -> TeamMember {
    TeamMember {
        id: id.to_string(),
        role,
        agent,
        responsibilities: responsibilities.iter().map(|s| s.to_string()).collect(),
        can_delegate_to: delegates_to.iter().map(|s| s.to_string()).collect(),
        can_veto,
    }
}

fn hitl_on(checkpoints: Vec<HitlCheckpoint>) -> HitlConfig {
    HitlConfig {
        enabled: true,
        checkpoints,
    }
}

fn hitl_off() -> HitlConfig {
    HitlConfig {
        enabled: false,
        checkpoints: vec![HitlCheckpoint::Never],
    }
}

// ---------------------------------------------------------------------------
// Team Templates
// ---------------------------------------------------------------------------

/// Website Builder Team — product lead + frontend coder + UX designer + QA reviewer.
///
/// Ideal for building landing pages, marketing sites, and content-heavy web apps.
/// The product lead defines the page structure and copy, the coder implements it,
/// the UX designer ensures quality, and QA validates the final result.
pub fn website_builder_team() -> TeamDefinition {
    let all_tools = &["file_read", "file_write", "file_edit", "grep", "glob", "bash", "list_directory"];
    let read_tools = &["file_read", "grep", "glob", "list_directory"];

    TeamDefinition {
        id: "team-website-builder".to_string(),
        name: "Website Builder".to_string(),
        description: "Builds polished websites with product thinking, quality code, and great UX.".to_string(),
        members: vec![
            make_member(
                "product-lead",
                TeamRole::Lead,
                make_agent(
                    "wb-product-lead",
                    "Product Lead",
                    "product",
                    "You are leading a website builder team. Define page structure, content hierarchy, user flows, and copy before delegating to the coder. Review all output for product quality.",
                    all_tools,
                    15,
                    600,
                ),
                &["Define page structure and content hierarchy", "Write marketing copy and CTAs", "Review final output for product quality"],
                &["frontend-coder", "ux-designer", "qa-reviewer"],
                true,
            ),
            make_member(
                "frontend-coder",
                TeamRole::Specialist { domain: "frontend".to_string() },
                make_agent(
                    "wb-frontend-coder",
                    "Frontend Coder",
                    "coder",
                    "You specialize in Next.js App Router, React 19, Tailwind CSS, and shadcn/ui. Build responsive, accessible pages using the design system blocks. Use lucide-react for icons.",
                    all_tools,
                    20,
                    600,
                ),
                &["Implement pages and components", "Ensure responsive design", "Follow the design system"],
                &[],
                false,
            ),
            make_member(
                "ux-designer",
                TeamRole::Specialist { domain: "ux".to_string() },
                make_agent(
                    "wb-ux-designer",
                    "UX Designer",
                    "ux",
                    "Review the website implementation for visual quality, spacing consistency, accessibility, and mobile responsiveness. Provide concrete Tailwind class suggestions.",
                    read_tools,
                    10,
                    300,
                ),
                &["Review visual quality and spacing", "Check accessibility compliance", "Validate responsive behavior"],
                &[],
                false,
            ),
            make_member(
                "qa-reviewer",
                TeamRole::Reviewer,
                make_agent(
                    "wb-qa-reviewer",
                    "QA Reviewer",
                    "reviewer",
                    "Validate the website: check for broken links, missing alt text, hydration errors, console warnings, and cross-browser compatibility issues.",
                    &["file_read", "grep", "glob", "bash", "list_directory"],
                    10,
                    300,
                ),
                &["Validate build succeeds", "Check for runtime errors", "Verify accessibility and SEO"],
                &[],
                true,
            ),
        ],
        coordination: CoordinationProtocol::Hierarchical {
            lead_id: "product-lead".to_string(),
        },
        budget: TeamBudget {
            max_total_cost_usd: 4.0,
            max_cost_per_member_usd: 1.5,
            max_total_tokens: 400_000,
            max_duration_secs: 1200,
            max_iterations_per_member: 20,
        },
        hitl: hitl_on(vec![HitlCheckpoint::AfterPlanning, HitlCheckpoint::BeforeCommit]),
        completion: CompletionCriteria {
            all_tasks_done: true,
            lead_approval_required: true,
            all_reviews_passed: true,
            completion_phrase: None,
        },
    }
}

/// Full-Stack Builder Team — architect + backend + frontend + tester.
///
/// For building complete features that span the entire stack: API design,
/// database schema, backend services, frontend UI, and test coverage.
pub fn fullstack_builder_team() -> TeamDefinition {
    let all_tools = &["file_read", "file_write", "file_edit", "grep", "glob", "bash", "list_directory"];

    TeamDefinition {
        id: "team-fullstack-builder".to_string(),
        name: "Full-Stack Builder".to_string(),
        description: "End-to-end feature development with architect, backend, frontend, and tester.".to_string(),
        members: vec![
            make_member(
                "architect",
                TeamRole::Lead,
                make_agent(
                    "fs-architect",
                    "Architect",
                    "architect",
                    "Lead a full-stack team. Decompose features into backend and frontend tasks. Define API contracts, data models, and component hierarchy. Review all deliverables for consistency.",
                    all_tools,
                    15,
                    600,
                ),
                &["Decompose features into tasks", "Define API contracts and data models", "Review integration correctness"],
                &["backend-dev", "frontend-dev", "tester"],
                true,
            ),
            make_member(
                "backend-dev",
                TeamRole::Specialist { domain: "backend".to_string() },
                make_agent(
                    "fs-backend-dev",
                    "Backend Developer",
                    "coder",
                    "You specialize in backend development: REST APIs, database schemas, services, and business logic. Follow the architect's API contracts exactly. Use proper validation and error handling.",
                    all_tools,
                    20,
                    600,
                ),
                &["Implement API endpoints and services", "Create database migrations", "Handle error cases thoroughly"],
                &[],
                false,
            ),
            make_member(
                "frontend-dev",
                TeamRole::Specialist { domain: "frontend".to_string() },
                make_agent(
                    "fs-frontend-dev",
                    "Frontend Developer",
                    "coder",
                    "You specialize in frontend: React, Next.js, TypeScript, Tailwind. Match the API contracts defined by the architect. Handle loading, error, and empty states properly.",
                    all_tools,
                    20,
                    600,
                ),
                &["Implement UI components and pages", "Connect to backend APIs", "Handle all UI states"],
                &[],
                false,
            ),
            make_member(
                "tester",
                TeamRole::Reviewer,
                make_agent(
                    "fs-tester",
                    "Tester",
                    "tester",
                    "Write tests for both backend and frontend. Verify API contracts match between the two. Test edge cases, error handling, and integration points.",
                    all_tools,
                    15,
                    480,
                ),
                &["Write unit and integration tests", "Verify API contract alignment", "Test edge cases and error paths"],
                &[],
                true,
            ),
        ],
        coordination: CoordinationProtocol::Hierarchical {
            lead_id: "architect".to_string(),
        },
        budget: TeamBudget {
            max_total_cost_usd: 5.0,
            max_cost_per_member_usd: 2.0,
            max_total_tokens: 500_000,
            max_duration_secs: 1800,
            max_iterations_per_member: 20,
        },
        hitl: hitl_on(vec![HitlCheckpoint::AfterPlanning, HitlCheckpoint::BeforeCommit]),
        completion: CompletionCriteria {
            all_tasks_done: true,
            lead_approval_required: true,
            all_reviews_passed: true,
            completion_phrase: None,
        },
    }
}

/// Bug Fix Team — debugger + coder + tester.
///
/// Specialized for diagnosing and fixing bugs with proper test coverage.
/// The debugger identifies the root cause, the coder implements the fix,
/// and the tester verifies the fix and prevents regressions.
pub fn bug_fix_team() -> TeamDefinition {
    let all_tools = &["file_read", "file_write", "file_edit", "grep", "glob", "bash", "list_directory"];

    TeamDefinition {
        id: "team-bug-fix".to_string(),
        name: "Bug Fix Squad".to_string(),
        description: "Diagnose, fix, and verify bugs with root cause analysis and regression testing.".to_string(),
        members: vec![
            make_member(
                "debugger",
                TeamRole::Lead,
                make_agent(
                    "bf-debugger",
                    "Debugger",
                    "debugger",
                    "Lead the bug fix team. Reproduce the bug, identify the root cause, and create a clear diagnosis for the coder. After the fix, verify it resolves the issue.",
                    all_tools,
                    15,
                    480,
                ),
                &["Reproduce the bug", "Identify root cause", "Verify the fix"],
                &["coder", "tester"],
                true,
            ),
            make_member(
                "coder",
                TeamRole::Specialist { domain: "implementation".to_string() },
                make_agent(
                    "bf-coder",
                    "Fix Implementer",
                    "coder",
                    "Implement the minimal fix identified by the debugger. Change as little code as possible. Ensure the fix handles edge cases properly.",
                    all_tools,
                    15,
                    480,
                ),
                &["Implement the minimal fix", "Handle edge cases", "Ensure no regressions"],
                &[],
                false,
            ),
            make_member(
                "tester",
                TeamRole::Reviewer,
                make_agent(
                    "bf-tester",
                    "Regression Tester",
                    "tester",
                    "Write a test that reproduces the original bug (should fail before fix, pass after). Write additional regression tests for related edge cases. Run the full test suite.",
                    all_tools,
                    12,
                    360,
                ),
                &["Write regression tests", "Verify fix resolves the bug", "Run full test suite"],
                &[],
                true,
            ),
        ],
        coordination: CoordinationProtocol::Sequential {
            order: vec![
                "debugger".to_string(),
                "coder".to_string(),
                "tester".to_string(),
            ],
        },
        budget: TeamBudget {
            max_total_cost_usd: 3.0,
            max_cost_per_member_usd: 1.5,
            max_total_tokens: 300_000,
            max_duration_secs: 1200,
            max_iterations_per_member: 15,
        },
        hitl: hitl_off(),
        completion: CompletionCriteria {
            all_tasks_done: true,
            lead_approval_required: true,
            all_reviews_passed: true,
            completion_phrase: None,
        },
    }
}

/// Security Audit Team — reviewer + coder + tester.
///
/// Performs a security-focused review of the codebase, implements fixes for
/// vulnerabilities found, and verifies fixes with security-oriented tests.
pub fn security_audit_team() -> TeamDefinition {
    let all_tools = &["file_read", "file_write", "file_edit", "grep", "glob", "bash", "list_directory"];
    let read_tools = &["file_read", "grep", "glob", "list_directory"];

    TeamDefinition {
        id: "team-security-audit".to_string(),
        name: "Security Audit".to_string(),
        description: "Security-focused code audit with vulnerability detection, fixes, and verification.".to_string(),
        members: vec![
            make_member(
                "security-reviewer",
                TeamRole::Lead,
                make_agent(
                    "sa-security-reviewer",
                    "Security Reviewer",
                    "reviewer",
                    "You specialize exclusively in security. Audit the codebase for:\n- Injection vulnerabilities (SQL, XSS, command injection, path traversal)\n- Auth/authz issues (missing guards, privilege escalation, token handling)\n- Data exposure (sensitive data in logs, responses, or error messages)\n- Cryptographic weaknesses (weak algorithms, hardcoded secrets)\n- Dependency vulnerabilities\n\nRate each finding as CRITICAL/HIGH/MEDIUM/LOW with remediation steps.",
                    read_tools,
                    12,
                    480,
                ),
                &["Audit code for security vulnerabilities", "Rate findings by severity", "Provide remediation guidance"],
                &["fix-coder", "security-tester"],
                true,
            ),
            make_member(
                "fix-coder",
                TeamRole::Specialist { domain: "security-fixes".to_string() },
                make_agent(
                    "sa-fix-coder",
                    "Security Fix Coder",
                    "coder",
                    "Implement security fixes identified by the reviewer. Prioritize CRITICAL and HIGH findings. Use secure coding patterns: parameterized queries, input validation, output encoding, proper auth checks.",
                    all_tools,
                    15,
                    480,
                ),
                &["Fix CRITICAL and HIGH security issues", "Apply secure coding patterns", "Add input validation"],
                &[],
                false,
            ),
            make_member(
                "security-tester",
                TeamRole::Reviewer,
                make_agent(
                    "sa-security-tester",
                    "Security Tester",
                    "tester",
                    "Write security-focused tests: test for injection attacks, verify auth guards, check for data leakage, validate input sanitization. Verify that each security fix actually prevents the vulnerability.",
                    all_tools,
                    12,
                    360,
                ),
                &["Write security-focused tests", "Verify fixes prevent vulnerabilities", "Test auth and input validation"],
                &[],
                true,
            ),
        ],
        coordination: CoordinationProtocol::Sequential {
            order: vec![
                "security-reviewer".to_string(),
                "fix-coder".to_string(),
                "security-tester".to_string(),
            ],
        },
        budget: TeamBudget {
            max_total_cost_usd: 3.5,
            max_cost_per_member_usd: 1.5,
            max_total_tokens: 350_000,
            max_duration_secs: 1200,
            max_iterations_per_member: 15,
        },
        hitl: hitl_on(vec![HitlCheckpoint::AfterPlanning]),
        completion: CompletionCriteria {
            all_tasks_done: true,
            lead_approval_required: true,
            all_reviews_passed: true,
            completion_phrase: None,
        },
    }
}

/// Documentation Team — reader + writer + reviewer.
///
/// Reads the codebase to understand it, writes clear documentation, and
/// reviews the docs for accuracy and completeness.
pub fn documentation_team() -> TeamDefinition {
    let all_tools = &["file_read", "file_write", "file_edit", "grep", "glob", "bash", "list_directory"];
    let read_tools = &["file_read", "grep", "glob", "list_directory"];

    TeamDefinition {
        id: "team-documentation".to_string(),
        name: "Documentation".to_string(),
        description: "Reads, documents, and reviews codebases for comprehensive documentation.".to_string(),
        members: vec![
            make_member(
                "code-reader",
                TeamRole::Specialist { domain: "analysis".to_string() },
                make_agent(
                    "doc-code-reader",
                    "Code Reader",
                    "architect",
                    "Your job is to read and understand the codebase thoroughly. Map out the architecture, identify key modules, trace data flows, and document the public APIs. Produce a structured analysis that the writer can turn into documentation.",
                    read_tools,
                    15,
                    480,
                ),
                &["Read and understand the codebase", "Map architecture and data flows", "Identify public APIs and key modules"],
                &["doc-writer"],
                false,
            ),
            make_member(
                "doc-writer",
                TeamRole::Lead,
                make_agent(
                    "doc-doc-writer",
                    "Documentation Writer",
                    "product",
                    "Write clear, well-structured documentation based on the code reader's analysis. Include:\n- Getting started guide\n- Architecture overview\n- API reference with examples\n- Configuration reference\n- Common tasks and recipes\n\nWrite for the target audience (developers). Use code examples liberally. Keep paragraphs short.",
                    all_tools,
                    20,
                    600,
                ),
                &["Write getting started guide", "Document APIs with examples", "Create architecture overview"],
                &["code-reader", "doc-reviewer"],
                true,
            ),
            make_member(
                "doc-reviewer",
                TeamRole::Reviewer,
                make_agent(
                    "doc-doc-reviewer",
                    "Documentation Reviewer",
                    "reviewer",
                    "Review documentation for:\n- Technical accuracy (does it match the actual code?)\n- Completeness (are important APIs and features documented?)\n- Clarity (can a new developer follow along?)\n- Code examples (do they compile/run? Are imports included?)\n\nProvide specific, actionable feedback.",
                    read_tools,
                    10,
                    300,
                ),
                &["Verify technical accuracy", "Check completeness and clarity", "Validate code examples"],
                &[],
                true,
            ),
        ],
        coordination: CoordinationProtocol::Sequential {
            order: vec![
                "code-reader".to_string(),
                "doc-writer".to_string(),
                "doc-reviewer".to_string(),
            ],
        },
        budget: TeamBudget {
            max_total_cost_usd: 3.0,
            max_cost_per_member_usd: 1.5,
            max_total_tokens: 400_000,
            max_duration_secs: 1200,
            max_iterations_per_member: 20,
        },
        hitl: hitl_off(),
        completion: CompletionCriteria {
            all_tasks_done: true,
            lead_approval_required: true,
            all_reviews_passed: true,
            completion_phrase: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn website_builder_has_4_members() {
        let team = website_builder_team();
        assert_eq!(team.members.len(), 4);
        assert!(team.members.iter().any(|m| m.id == "product-lead"));
        assert!(team.members.iter().any(|m| m.id == "frontend-coder"));
    }

    #[test]
    fn fullstack_builder_has_4_members() {
        let team = fullstack_builder_team();
        assert_eq!(team.members.len(), 4);
    }

    #[test]
    fn bug_fix_is_sequential() {
        let team = bug_fix_team();
        assert!(matches!(team.coordination, CoordinationProtocol::Sequential { .. }));
    }

    #[test]
    fn security_audit_has_lead() {
        let team = security_audit_team();
        let lead = team.members.iter().find(|m| matches!(m.role, TeamRole::Lead));
        assert!(lead.is_some());
    }

    #[test]
    fn documentation_team_has_3_members() {
        let team = documentation_team();
        assert_eq!(team.members.len(), 3);
    }

    #[test]
    fn all_agents_have_system_prompts() {
        let teams = vec![
            website_builder_team(),
            fullstack_builder_team(),
            bug_fix_team(),
            security_audit_team(),
            documentation_team(),
        ];
        for team in &teams {
            for member in &team.members {
                assert!(
                    !member.agent.system_prompt.is_empty(),
                    "Agent {} in team {} has empty prompt",
                    member.id,
                    team.id
                );
            }
        }
    }
}
