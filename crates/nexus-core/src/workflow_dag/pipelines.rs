//! Predefined workflow pipelines.
//!
//! Each function returns a `WorkflowDag` that can be executed directly by
//! `WorkflowRunner::run`.  Pass a `WorkflowContext` with the inputs listed
//! in each function's doc comment.
//!
//! All pipelines use the named Nexus agents (Nova, Kai, Atlas, etc.) and are
//! designed to build, review, audit, or deploy real software projects.

use nexus_zeroclaw::AgentName;

use super::dag::{WorkflowDag, WorkflowStep};

/// Helper to create a step with default condition/retry/loop fields.
fn step(
    id: &str,
    agent: AgentName,
    description: &str,
    prompt_template: &str,
    depends_on: Vec<String>,
) -> WorkflowStep {
    WorkflowStep {
        id: id.into(),
        agent,
        description: description.into(),
        prompt_template: prompt_template.into(),
        depends_on,
        condition: None,
        max_retries: 0,
        retry_delay_ms: 1000,
        loop_count: 1,
    }
}

/// Like `step` but with retry support (for steps that call external services).
fn step_with_retry(
    id: &str,
    agent: AgentName,
    description: &str,
    prompt_template: &str,
    depends_on: Vec<String>,
    max_retries: u32,
) -> WorkflowStep {
    WorkflowStep {
        id: id.into(),
        agent,
        description: description.into(),
        prompt_template: prompt_template.into(),
        depends_on,
        condition: None,
        max_retries,
        retry_delay_ms: 1000,
        loop_count: 1,
    }
}

/// Like `step` but with a condition gate.
fn step_conditional(
    id: &str,
    agent: AgentName,
    description: &str,
    prompt_template: &str,
    depends_on: Vec<String>,
    condition: &str,
) -> WorkflowStep {
    WorkflowStep {
        id: id.into(),
        agent,
        description: description.into(),
        prompt_template: prompt_template.into(),
        depends_on,
        condition: Some(condition.into()),
        max_retries: 0,
        retry_delay_ms: 1000,
        loop_count: 1,
    }
}

// ---------------------------------------------------------------------------
// app_build_dag
// ---------------------------------------------------------------------------

/// Full app-build pipeline: requirements → architecture → implementation →
/// security audit → documentation → deployment config.
///
/// # Required context inputs
/// - `requirements` — plain-language description of what to build
/// - `stack` — technology stack hint, e.g. `"Rust + Next.js"` (optional)
pub fn app_build_dag() -> WorkflowDag {
    WorkflowDag {
        name: "app-build".into(),
        description: "Build a production-ready application from scratch".into(),
        steps: vec![
            step(
                "research",
                AgentName::Kai,
                "Research requirements and prior art",
                "Research the following requirements thoroughly.\n\
                 Look for similar solutions, best practices, and potential pitfalls.\n\n\
                 Requirements:\n{requirements}\n\
                 Stack hint: {stack}\n\n\
                 Produce a concise research summary with key findings and recommendations.",
                vec![],
            ),
            step(
                "product_spec",
                AgentName::Leo,
                "Write the product spec and user stories",
                "Based on the requirements and research below, write a clear \
                 product specification with user stories, acceptance criteria, and a \
                 prioritised feature list.\n\n\
                 Requirements:\n{requirements}\n\n\
                 Research:\n{research}",
                vec!["research".into()],
            ),
            step(
                "architecture",
                AgentName::Atlas,
                "Design the system architecture",
                "Design a production-ready system architecture for this project.\n\
                 Include: component diagram, data model, API surface, deployment topology, \
                 and technology choices with justification.\n\n\
                 Product spec:\n{product_spec}\n\
                 Stack hint: {stack}",
                vec!["product_spec".into()],
            ),
            step_with_retry(
                "implement",
                AgentName::Nova,
                "Implement the application",
                "Implement the application according to the spec and architecture.\n\
                 Write production-quality code with:\n\
                 - Full project structure and file layout\n\
                 - Core features implemented\n\
                 - Unit and integration tests\n\
                 - README with setup instructions\n\n\
                 Product spec:\n{product_spec}\n\n\
                 Architecture:\n{architecture}",
                vec!["architecture".into()],
                2, // retry up to 2 times — implementation is the most failure-prone step
            ),
            step(
                "security_audit",
                AgentName::Orion,
                "Security audit",
                "Perform a thorough security audit of the implementation below.\n\
                 Check for OWASP Top 10, injection vulnerabilities, auth issues, \
                 secrets in code, and insecure defaults.\n\
                 Provide a prioritised list of findings with remediation steps.\n\n\
                 Implementation:\n{implement}",
                vec!["implement".into()],
            ),
            step(
                "documentation",
                AgentName::Luna,
                "Write documentation",
                "Write comprehensive documentation for this project:\n\
                 - Developer README (setup, architecture overview, contributing guide)\n\
                 - API reference (if applicable)\n\
                 - User guide\n\n\
                 Implementation:\n{implement}\n\n\
                 Architecture:\n{architecture}",
                vec!["implement".into()],
            ),
            step_conditional(
                "security_fix",
                AgentName::Nova,
                "Apply security fixes from audit",
                "Apply the following security audit findings to the implementation.\n\
                 Fix every critical and high-severity issue. For medium issues, add TODO comments.\n\n\
                 Original implementation:\n{implement}\n\n\
                 Security findings:\n{security_audit}",
                vec!["security_audit".into()],
                "{security_audit} contains critical",
            ),
            step(
                "deploy",
                AgentName::Rex,
                "Configure CI/CD and deployment",
                "Create a production-ready deployment configuration:\n\
                 - Dockerfile and docker-compose.yml\n\
                 - GitHub Actions CI/CD pipeline (build, test, deploy)\n\
                 - Environment variable documentation\n\
                 - Health-check and readiness probe configuration\n\n\
                 Implementation:\n{implement}\n\n\
                 Architecture:\n{architecture}\n\n\
                 Security audit findings:\n{security_audit}",
                vec!["implement".into(), "security_audit".into()],
            ),
        ],
    }
}

// ---------------------------------------------------------------------------
// code_review_dag
// ---------------------------------------------------------------------------

/// Code review pipeline: analysis → security → quality → final report.
///
/// # Required context inputs
/// - `code` — the code to review (paste or file paths)
/// - `language` — programming language (optional hint)
pub fn code_review_dag() -> WorkflowDag {
    WorkflowDag {
        name: "code-review".into(),
        description: "Comprehensive code review: quality, security, and architecture".into(),
        steps: vec![
            step(
                "security_review",
                AgentName::Orion,
                "Security-focused review",
                "Review the following code for security issues.\n\
                 Check: injection, auth, crypto, secrets, input validation, \
                 dependency vulnerabilities, and OWASP Top 10.\n\n\
                 Language: {language}\nCode:\n{code}",
                vec![],
            ),
            step(
                "quality_review",
                AgentName::Nova,
                "Code quality and architecture review",
                "Review the following code for quality, correctness, \
                 and maintainability.\n\
                 Check: naming, complexity, test coverage, error handling, \
                 performance, idiomatic patterns, and documentation.\n\n\
                 Language: {language}\nCode:\n{code}",
                vec![],
            ),
            step(
                "final_report",
                AgentName::Luna,
                "Consolidate into a review report",
                "Consolidate the following two reviews into a clear, \
                 actionable code-review report.\n\
                 Structure: Executive Summary → Critical Issues → Recommendations → \
                 Positive Highlights.\n\n\
                 Security review:\n{security_review}\n\n\
                 Quality review:\n{quality_review}",
                vec!["security_review".into(), "quality_review".into()],
            ),
        ],
    }
}

// ---------------------------------------------------------------------------
// security_audit_dag
// ---------------------------------------------------------------------------

/// Standalone security audit pipeline.
///
/// # Required context inputs
/// - `target` — description of what to audit (repo, API, architecture doc)
pub fn security_audit_dag() -> WorkflowDag {
    WorkflowDag {
        name: "security-audit".into(),
        description: "End-to-end security audit and hardening recommendations".into(),
        steps: vec![
            step(
                "threat_model",
                AgentName::Orion,
                "Threat modelling",
                "Perform STRIDE threat modelling for the following target.\n\
                 Identify assets, trust boundaries, data flows, and threats.\n\n\
                 Target:\n{target}",
                vec![],
            ),
            step(
                "vulnerability_scan",
                AgentName::Orion,
                "Vulnerability analysis",
                "Analyse the target for concrete vulnerabilities based on \
                 the threat model.\n\
                 Focus on exploitable weaknesses, CVEs in dependencies, and misconfigurations.\n\n\
                 Target:\n{target}\n\nThreat model:\n{threat_model}",
                vec!["threat_model".into()],
            ),
            step(
                "remediation_plan",
                AgentName::Orion,
                "Remediation roadmap",
                "Create a prioritised remediation roadmap based on the \
                 vulnerability analysis.\n\
                 For each finding: severity, impact, effort, and concrete fix steps.\n\n\
                 Findings:\n{vulnerability_scan}",
                vec!["vulnerability_scan".into()],
            ),
            step(
                "audit_report",
                AgentName::Luna,
                "Final audit report",
                "Write a professional security audit report from the following \
                 materials.\n\
                 Format: Executive Summary → Scope → Threat Model → Findings (by severity) → \
                 Remediation Plan → Conclusion.\n\n\
                 Threat model:\n{threat_model}\n\n\
                 Vulnerabilities:\n{vulnerability_scan}\n\n\
                 Remediation:\n{remediation_plan}",
                vec!["remediation_plan".into()],
            ),
        ],
    }
}

// ---------------------------------------------------------------------------
// feature_dag
// ---------------------------------------------------------------------------

/// Add a new feature to an existing codebase.
///
/// # Required context inputs
/// - `feature` — description of the feature to add
/// - `codebase` — description or listing of the existing codebase
pub fn feature_dag() -> WorkflowDag {
    WorkflowDag {
        name: "feature".into(),
        description: "Research, design, implement, and document a new feature".into(),
        steps: vec![
            step(
                "design",
                AgentName::Leo,
                "Feature design and acceptance criteria",
                "Design the following feature for the existing codebase.\n\
                 Produce: acceptance criteria, API changes, data model changes, \
                 and a short implementation plan.\n\n\
                 Feature:\n{feature}\n\nCodebase:\n{codebase}",
                vec![],
            ),
            step_with_retry(
                "implementation",
                AgentName::Nova,
                "Implement the feature",
                "Implement the feature according to the design.\n\
                 Write the code, tests, and any migrations needed.\n\n\
                 Design:\n{design}\n\nCodebase:\n{codebase}",
                vec!["design".into()],
                1,
            ),
            step(
                "review",
                AgentName::Orion,
                "Security and quality check",
                "Review the implementation for security and quality issues.\n\n\
                 Implementation:\n{implementation}",
                vec!["implementation".into()],
            ),
            step(
                "docs_update",
                AgentName::Luna,
                "Update documentation",
                "Update the documentation to cover the new feature.\n\n\
                 Feature design:\n{design}\n\nImplementation:\n{implementation}",
                vec!["implementation".into()],
            ),
        ],
    }
}

// ---------------------------------------------------------------------------
// deploy_dag
// ---------------------------------------------------------------------------

/// Deployment pipeline for an existing implementation.
///
/// # Required context inputs
/// - `implementation` — the code/project to deploy
/// - `target_env` — deployment target, e.g. `"AWS ECS"`, `"Fly.io"` (optional)
pub fn deploy_dag() -> WorkflowDag {
    WorkflowDag {
        name: "deploy".into(),
        description: "CI/CD pipeline, infrastructure, monitoring, and go-live".into(),
        steps: vec![
            step(
                "infra",
                AgentName::Atlas,
                "Infrastructure design",
                "Design the cloud infrastructure for deploying this application.\n\
                 Include: compute, networking, database, storage, and cost estimate.\n\n\
                 Application:\n{implementation}\nTarget: {target_env}",
                vec![],
            ),
            step(
                "cicd",
                AgentName::Rex,
                "CI/CD pipeline",
                "Write a complete CI/CD pipeline configuration.\n\
                 Include: build, test, security scan, Docker build, and deploy stages.\n\n\
                 Application:\n{implementation}\nInfrastructure:\n{infra}",
                vec!["infra".into()],
            ),
            step(
                "monitoring",
                AgentName::Rex,
                "Monitoring and alerting",
                "Configure monitoring, logging, and alerting for the deployment.\n\
                 Include: health checks, log aggregation, metrics, and on-call runbooks.\n\n\
                 Infrastructure:\n{infra}",
                vec!["infra".into()],
            ),
        ],
    }
}

// ---------------------------------------------------------------------------
// Available pipeline registry
// ---------------------------------------------------------------------------

/// All built-in pipeline names.
pub const PIPELINE_NAMES: &[&str] = &[
    "app-build",
    "code-review",
    "security-audit",
    "feature",
    "deploy",
];

/// Look up a built-in pipeline by name.
pub fn pipeline_by_name(name: &str) -> Option<WorkflowDag> {
    match name {
        "app-build"      => Some(app_build_dag()),
        "code-review"    => Some(code_review_dag()),
        "security-audit" => Some(security_audit_dag()),
        "feature"        => Some(feature_dag()),
        "deploy"         => Some(deploy_dag()),
        _                => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_names_constant_matches_lookup() {
        for name in PIPELINE_NAMES {
            assert!(
                pipeline_by_name(name).is_some(),
                "pipeline_by_name('{}') should return Some",
                name
            );
        }
    }

    #[test]
    fn pipeline_by_name_unknown_returns_none() {
        assert!(pipeline_by_name("nonexistent").is_none());
        assert!(pipeline_by_name("").is_none());
    }

    #[test]
    fn app_build_dag_structure() {
        let dag = app_build_dag();
        assert_eq!(dag.name, "app-build");
        assert!(!dag.description.is_empty());
        assert!(dag.steps.len() >= 6, "app-build should have at least 6 steps");

        // First step has no dependencies
        assert!(dag.steps[0].depends_on.is_empty());

        // Verify step IDs are unique
        let ids: std::collections::HashSet<&str> =
            dag.steps.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids.len(), dag.steps.len(), "Step IDs must be unique");
    }

    #[test]
    fn app_build_dag_dependencies_reference_valid_steps() {
        let dag = app_build_dag();
        let step_ids: std::collections::HashSet<&str> =
            dag.steps.iter().map(|s| s.id.as_str()).collect();

        for s in &dag.steps {
            for dep in &s.depends_on {
                assert!(
                    step_ids.contains(dep.as_str()),
                    "Step '{}' depends on '{}' which does not exist",
                    s.id,
                    dep
                );
            }
        }
    }

    #[test]
    fn code_review_dag_structure() {
        let dag = code_review_dag();
        assert_eq!(dag.name, "code-review");
        assert_eq!(dag.steps.len(), 3);

        // First two steps are independent (no deps)
        assert!(dag.steps[0].depends_on.is_empty());
        assert!(dag.steps[1].depends_on.is_empty());

        // Final report depends on both reviews
        assert_eq!(dag.steps[2].depends_on.len(), 2);
    }

    #[test]
    fn security_audit_dag_is_linear() {
        let dag = security_audit_dag();
        assert_eq!(dag.name, "security-audit");

        // Each step (after the first) depends on exactly the previous one
        for i in 1..dag.steps.len() {
            assert!(
                !dag.steps[i].depends_on.is_empty(),
                "Step {} should have dependencies",
                dag.steps[i].id
            );
        }
    }

    #[test]
    fn feature_dag_structure() {
        let dag = feature_dag();
        assert_eq!(dag.name, "feature");
        assert!(dag.steps.len() >= 3);

        // Design has no deps, others depend on it
        assert!(dag.steps[0].depends_on.is_empty());
    }

    #[test]
    fn deploy_dag_structure() {
        let dag = deploy_dag();
        assert_eq!(dag.name, "deploy");
        assert_eq!(dag.steps.len(), 3);
        // cicd and monitoring both depend on infra
        assert!(dag.steps[1].depends_on.contains(&"infra".to_string()));
        assert!(dag.steps[2].depends_on.contains(&"infra".to_string()));
    }

    #[test]
    fn all_pipelines_have_non_empty_descriptions() {
        for name in PIPELINE_NAMES {
            let dag = pipeline_by_name(name).unwrap();
            assert!(!dag.description.is_empty(), "Pipeline '{}' needs a description", name);
            for s in &dag.steps {
                assert!(!s.description.is_empty(), "Step '{}' in '{}' needs a description", s.id, name);
                assert!(!s.prompt_template.is_empty(), "Step '{}' in '{}' needs a prompt template", s.id, name);
            }
        }
    }

    #[test]
    fn step_helper_creates_correct_defaults() {
        let s = step("test-step", AgentName::Nova, "desc", "template", vec!["dep".into()]);
        assert_eq!(s.id, "test-step");
        assert_eq!(s.agent, AgentName::Nova);
        assert_eq!(s.description, "desc");
        assert_eq!(s.prompt_template, "template");
        assert_eq!(s.depends_on, vec!["dep".to_string()]);
        assert!(s.condition.is_none());
        assert_eq!(s.max_retries, 0);
        assert_eq!(s.loop_count, 1);
    }

    #[test]
    fn step_with_retry_sets_max_retries() {
        let s = step_with_retry("retry-step", AgentName::Atlas, "d", "t", vec![], 3);
        assert_eq!(s.max_retries, 3);
        assert!(s.condition.is_none());
    }

    #[test]
    fn step_conditional_sets_condition() {
        let s = step_conditional("cond", AgentName::Orion, "d", "t", vec![], "some condition");
        assert_eq!(s.condition.as_deref(), Some("some condition"));
        assert_eq!(s.max_retries, 0);
    }

    #[test]
    fn app_build_has_conditional_security_fix() {
        let dag = app_build_dag();
        let fix_step = dag.steps.iter().find(|s| s.id == "security_fix");
        assert!(fix_step.is_some(), "app-build should have a security_fix step");
        assert!(fix_step.unwrap().condition.is_some(), "security_fix should be conditional");
    }
}
