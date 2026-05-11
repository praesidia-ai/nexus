//! Agent definition factories — produce [`AgentDefinition`] for each agent type.
//!
//! Instead of scattering agent configuration across multiple modules, all agent
//! "blueprints" live here. Call a factory function, get back a ready-to-execute
//! definition that the [`super::runtime::AgentRuntime`] can run.

use super::definition::*;

/// Create a project-scoped coding agent definition.
pub fn coding_agent(project_id: &str) -> AgentDefinition {
    AgentDefinition {
        id: format!("coding-{}", project_id),
        name: "Coding Agent".into(),
        system_prompt: "You are an expert coding agent. You can read, write, and modify files \
            in the project. Your goal is to implement the requested changes correctly and \
            completely. Always read existing files before modifying them. Use file_edit for \
            surgical changes to existing files and file_write only for new files."
            .into(),
        skills: vec![
            "code_generation".into(),
            "refactoring".into(),
            "debugging".into(),
        ],
        tools: vec![
            "file_read".into(),
            "file_write".into(),
            "file_edit".into(),
            "grep".into(),
            "glob".into(),
            "bash".into(),
            "list_directory".into(),
        ],
        model_preference: None,
        execution_mode: ExecutionMode::Interactive,
        max_iterations: 20,
        timeout_secs: 300,
    }
}

/// Create an architect agent definition.
pub fn architect_agent(project_id: &str) -> AgentDefinition {
    AgentDefinition {
        id: format!("architect-{}", project_id),
        name: "Architect Agent".into(),
        system_prompt: "You are a software architect. You analyze requirements and design \
            system architecture, data models, API contracts, and component hierarchies. \
            Output structured plans that other agents can follow."
            .into(),
        skills: vec!["architecture".into(), "system_design".into()],
        tools: vec![
            "file_read".into(),
            "grep".into(),
            "glob".into(),
            "list_directory".into(),
            "file_write".into(),
        ],
        model_preference: None,
        execution_mode: ExecutionMode::Interactive,
        max_iterations: 10,
        timeout_secs: 180,
    }
}

/// Create a debugger agent definition.
pub fn debugger_agent(project_id: &str) -> AgentDefinition {
    AgentDefinition {
        id: format!("debugger-{}", project_id),
        name: "Debugger Agent".into(),
        system_prompt: "You are a debugging specialist. You diagnose errors, trace execution \
            paths, and propose minimal fixes. Read error messages carefully, find the root \
            cause, and apply the smallest change that resolves the issue."
            .into(),
        skills: vec!["debugging".into(), "error_analysis".into()],
        tools: vec![
            "file_read".into(),
            "grep".into(),
            "glob".into(),
            "bash".into(),
            "file_edit".into(),
            "list_directory".into(),
        ],
        model_preference: None,
        execution_mode: ExecutionMode::Interactive,
        max_iterations: 15,
        timeout_secs: 240,
    }
}

/// Create a reviewer agent definition.
pub fn reviewer_agent(project_id: &str) -> AgentDefinition {
    AgentDefinition {
        id: format!("reviewer-{}", project_id),
        name: "Code Reviewer Agent".into(),
        system_prompt: "You are a code review specialist. You check for bugs, security issues, \
            performance problems, and style violations. Provide specific, actionable feedback \
            with file paths and line numbers."
            .into(),
        skills: vec!["code_review".into(), "security_audit".into()],
        tools: vec![
            "file_read".into(),
            "grep".into(),
            "glob".into(),
            "list_directory".into(),
        ],
        model_preference: None,
        execution_mode: ExecutionMode::Interactive,
        max_iterations: 10,
        timeout_secs: 180,
    }
}

/// Create a tester agent definition.
pub fn tester_agent(project_id: &str) -> AgentDefinition {
    AgentDefinition {
        id: format!("tester-{}", project_id),
        name: "Test Agent".into(),
        system_prompt: "You are a testing specialist. You write comprehensive tests, run them, \
            and fix failures. Cover edge cases, error paths, and integration points."
            .into(),
        skills: vec!["testing".into(), "test_generation".into()],
        tools: vec![
            "file_read".into(),
            "file_write".into(),
            "bash".into(),
            "grep".into(),
            "glob".into(),
            "list_directory".into(),
        ],
        model_preference: None,
        execution_mode: ExecutionMode::Interactive,
        max_iterations: 15,
        timeout_secs: 300,
    }
}

/// Create a performance agent definition.
pub fn performance_agent(project_id: &str) -> AgentDefinition {
    AgentDefinition {
        id: format!("performance-{}", project_id),
        name: "Performance Agent".into(),
        system_prompt: "You are a performance optimization specialist. You profile, benchmark, \
            and optimize code for speed and efficiency. Focus on algorithmic improvements, \
            unnecessary allocations, and hot paths."
            .into(),
        skills: vec!["performance".into(), "optimization".into()],
        tools: vec![
            "file_read".into(),
            "file_edit".into(),
            "bash".into(),
            "grep".into(),
            "glob".into(),
            "list_directory".into(),
        ],
        model_preference: None,
        execution_mode: ExecutionMode::Interactive,
        max_iterations: 10,
        timeout_secs: 180,
    }
}

/// Create a DevOps agent definition.
pub fn devops_agent(project_id: &str) -> AgentDefinition {
    AgentDefinition {
        id: format!("devops-{}", project_id),
        name: "DevOps Agent".into(),
        system_prompt: "You are a DevOps specialist. You manage deployments, CI/CD pipelines, \
            Docker configurations, and infrastructure. Ensure builds are reproducible and \
            deployments are safe."
            .into(),
        skills: vec!["devops".into(), "deployment".into(), "docker".into()],
        tools: vec![
            "file_read".into(),
            "file_write".into(),
            "bash".into(),
            "grep".into(),
            "glob".into(),
            "list_directory".into(),
        ],
        model_preference: None,
        execution_mode: ExecutionMode::Interactive,
        max_iterations: 10,
        timeout_secs: 180,
    }
}

/// Create a repair agent definition (for guarantee/self-healing).
pub fn repair_agent(project_id: &str) -> AgentDefinition {
    AgentDefinition {
        id: format!("repair-{}", project_id),
        name: "Repair Agent".into(),
        system_prompt: "You are a repair specialist. You fix build errors, runtime errors, and \
            test failures with minimal changes. Read the error output carefully, identify the \
            root cause, and apply the smallest fix possible."
            .into(),
        skills: vec!["repair".into(), "error_fixing".into()],
        tools: vec![
            "file_read".into(),
            "file_edit".into(),
            "bash".into(),
            "grep".into(),
            "glob".into(),
            "list_directory".into(),
        ],
        model_preference: None,
        execution_mode: ExecutionMode::Interactive,
        max_iterations: 10,
        timeout_secs: 240,
    }
}

/// Create a UX agent definition.
pub fn ux_agent(project_id: &str) -> AgentDefinition {
    AgentDefinition {
        id: format!("ux-{}", project_id),
        name: "UX Agent".into(),
        system_prompt: "You are a UX/UI specialist. You improve user interfaces, accessibility, \
            responsive design, and visual aesthetics. Follow modern design patterns and \
            ensure WCAG compliance."
            .into(),
        skills: vec!["ux_design".into(), "accessibility".into()],
        tools: vec![
            "file_read".into(),
            "file_edit".into(),
            "grep".into(),
            "glob".into(),
            "list_directory".into(),
        ],
        model_preference: None,
        execution_mode: ExecutionMode::Interactive,
        max_iterations: 10,
        timeout_secs: 180,
    }
}

/// Create a wave pipeline definition — multiple agents running in parallel.
///
/// Each `(role, task_context)` pair becomes an agent with the appropriate
/// factory definition, switched to [`ExecutionMode::Parallel`] with
/// [`MergeStrategy::FileMerge`].
pub fn wave_pipeline(project_id: &str, agents: Vec<(&str, &str)>) -> Vec<AgentDefinition> {
    agents
        .into_iter()
        .map(|(role, task_context)| {
            let mut def = match role {
                "architect" => architect_agent(project_id),
                "coder" | "coding" => coding_agent(project_id),
                "debugger" => debugger_agent(project_id),
                "reviewer" => reviewer_agent(project_id),
                "tester" => tester_agent(project_id),
                "performance" => performance_agent(project_id),
                "devops" => devops_agent(project_id),
                "ux" => ux_agent(project_id),
                "repair" => repair_agent(project_id),
                _ => coding_agent(project_id),
            };
            def.execution_mode = ExecutionMode::Parallel {
                merge_strategy: MergeStrategy::FileMerge,
            };
            def.system_prompt
                .push_str(&format!("\n\nAdditional context: {}", task_context));
            def
        })
        .collect()
}
