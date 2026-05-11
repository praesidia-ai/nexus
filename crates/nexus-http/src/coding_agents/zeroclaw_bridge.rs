//! Bridge between the CodingAgent trait and ZeroClaw's NexusAgent.
//!
//! This module implements Option B from the unification roadmap: wrapping
//! existing CodingAgent instances behind AgentDefinition so they can be
//! scheduled through the kernel while preserving their specialized logic.

use nexus_agents_core::definition::{AgentDefinition, ExecutionMode};

use super::traits::{CodingAgent, CodingAgentContext};

/// Convert any `CodingAgent` implementation into an `AgentDefinition`
/// suitable for kernel scheduling and ZeroClaw execution.
///
/// The resulting definition captures the agent's identity, system prompt,
/// tools, and resource limits. The actual execution still routes through
/// `run_coding_agent_loop` — this bridge is for lifecycle management,
/// not behavioral replacement.
pub fn coding_agent_to_definition(agent: &dyn CodingAgent, ctx: &CodingAgentContext) -> AgentDefinition {
    AgentDefinition {
        id: format!("coding-{}", agent.name().to_lowercase().replace(' ', "-")),
        name: agent.name().to_string(),
        system_prompt: agent.system_prompt(ctx),
        skills: vec![format!("{:?}", agent.role()).to_lowercase()],
        tools: agent.tools_allowed().iter().map(|s| s.to_string()).collect(),
        model_preference: None,
        execution_mode: ExecutionMode::Interactive,
        max_iterations: agent.max_iterations(),
        timeout_secs: 600,
    }
}

/// Convert a mini-agent specification into an `AgentDefinition`.
pub fn mini_agent_to_definition(
    agent_id: &str,
    agent_label: &str,
    system_prompt: &str,
    tools: &[String],
) -> AgentDefinition {
    AgentDefinition {
        id: agent_id.to_string(),
        name: agent_label.to_string(),
        system_prompt: system_prompt.to_string(),
        skills: Vec::new(),
        tools: tools.to_vec(),
        model_preference: None,
        execution_mode: ExecutionMode::Interactive,
        max_iterations: 15,
        timeout_secs: 600,
    }
}
