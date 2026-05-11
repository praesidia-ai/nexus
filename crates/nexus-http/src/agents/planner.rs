//! Agent Planning — generates an execution plan before the agent starts working.
//!
//! The planner takes a task description and available tools, makes a fast LLM
//! call to produce a step-by-step plan, and injects that plan into the agent's
//! system prompt. This gives the agent structured guidance and reduces wasted
//! iterations.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::llm_client::{self, LlmConfig};
use crate::state::AppState;

use super::definition::AgentDefinition;
use super::events::AgentEvent;

/// A step in the agent's execution plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    /// Step number (1-indexed).
    pub step: u32,
    /// What the agent should do in this step.
    pub action: String,
    /// Which tool to use (if applicable).
    pub tool: Option<String>,
    /// Expected outcome of this step.
    pub expected_outcome: String,
}

/// A complete execution plan for an agent task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    /// High-level goal summary.
    pub goal: String,
    /// Ordered steps to achieve the goal.
    pub steps: Vec<PlanStep>,
    /// Estimated number of iterations needed.
    pub estimated_iterations: u32,
    /// Potential risks or blockers identified during planning.
    pub risks: Vec<String>,
}

/// Generates execution plans for agents before they start working.
pub struct AgentPlanner {
    app: Arc<AppState>,
}

impl AgentPlanner {
    /// Create a new planner.
    pub fn new(app: Arc<AppState>) -> Self {
        Self { app }
    }

    /// Generate an execution plan for the given task and agent definition.
    ///
    /// Uses a fast, low-temperature LLM call to produce a structured plan.
    /// Returns `None` if planning fails (the agent should proceed without a plan).
    pub async fn create_plan(
        &self,
        definition: &AgentDefinition,
        task: &str,
    ) -> Option<ExecutionPlan> {
        let tool_list: String = definition
            .tools
            .iter()
            .map(|t| format!("- {t}"))
            .collect::<Vec<_>>()
            .join("\n");

        let planning_prompt = format!(
            r#"You are a planning assistant. Given a task and available tools, create a concise execution plan.

Task: {task}

Available tools:
{tool_list}

Agent role: {role}
Max iterations: {max_iter}

Respond with a JSON object:
{{
  "goal": "one-sentence goal summary",
  "steps": [
    {{
      "step": 1,
      "action": "what to do",
      "tool": "tool_name or null",
      "expected_outcome": "what should result"
    }}
  ],
  "estimated_iterations": <number>,
  "risks": ["potential blocker 1", ...]
}}

Keep the plan to 3-8 steps. Be concrete and actionable. Only use tools from the available list."#,
            task = task,
            tool_list = tool_list,
            role = definition.name,
            max_iter = definition.max_iterations,
        );

        let config = self.planning_config();
        let messages = vec![
            json!({"role": "system", "content": "You are a planning assistant. Respond only with valid JSON."}),
            json!({"role": "user", "content": planning_prompt}),
        ];

        let start = std::time::Instant::now();
        let response = llm_client::call_llm_with_tools(&config, &messages, &[]).await;
        let latency_ms = start.elapsed().as_millis() as u64;

        match response {
            Ok(resp) => {
                self.app
                    .cost_tracker
                    .record_call(
                        None,
                        &config.model,
                        &config.provider,
                        resp.input_tokens,
                        resp.output_tokens,
                        latency_ms,
                        "agent_planner",
                    )
                    .await;
                let text = resp.text.unwrap_or_default();
                // Try to parse the JSON from the response
                parse_plan(&text)
            }
            Err(e) => {
                tracing::warn!("Planning LLM call failed: {e}. Agent will proceed without a plan.");
                None
            }
        }
    }

    /// Inject a plan into an agent's system prompt.
    ///
    /// Appends a structured plan section to the end of the system prompt so the
    /// agent knows what steps to follow.
    pub fn inject_plan(system_prompt: &str, plan: &ExecutionPlan) -> String {
        let steps_text: String = plan
            .steps
            .iter()
            .map(|s| {
                let tool_hint = s
                    .tool
                    .as_ref()
                    .map(|t| format!(" [use: {t}]"))
                    .unwrap_or_default();
                format!(
                    "  {}. {}{}\n     Expected: {}",
                    s.step, s.action, tool_hint, s.expected_outcome
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let risks_text = if plan.risks.is_empty() {
            String::from("  None identified.")
        } else {
            plan.risks
                .iter()
                .map(|r| format!("  - {r}"))
                .collect::<Vec<_>>()
                .join("\n")
        };

        format!(
            r#"{system_prompt}

--- EXECUTION PLAN ---
Goal: {goal}

Steps:
{steps}

Potential risks:
{risks}

Follow this plan step by step. If a step fails, adapt and continue with the next step.
Estimated iterations: {est}
--- END PLAN ---"#,
            system_prompt = system_prompt,
            goal = plan.goal,
            steps = steps_text,
            risks = risks_text,
            est = plan.estimated_iterations,
        )
    }

    /// Build a fast, low-cost LLM config for planning.
    fn planning_config(&self) -> LlmConfig {
        // Use a fast model for planning to minimize latency and cost.
        let (provider, model) = if self.app.anthropic_api_key.is_some() {
            ("anthropic", "claude-haiku-4-20250414")
        } else {
            ("openai", "gpt-4.1")
        };

        let api_key = match provider {
            "anthropic" => self.app.anthropic_api_key.clone().unwrap_or_default(),
            _ => self.app.openai_api_key.clone(),
        };

        let api_base = match provider {
            "anthropic" => "https://api.anthropic.com".to_string(),
            _ => "https://api.openai.com/v1".to_string(),
        };

        LlmConfig {
            provider: provider.to_string(),
            model: model.to_string(),
            api_key,
            api_base,
            max_tokens: 2048,
            temperature: 0.0,
        }
    }
}

/// Create an `AgentEvent::PlanCreated` variant.
///
/// Since we cannot modify the existing `AgentEvent` enum without touching
/// other code, we emit it as a `Thinking` event with a plan prefix.
pub fn plan_created_event(agent_id: &str, plan: &ExecutionPlan) -> AgentEvent {
    let summary = format!(
        "Plan created ({} steps, ~{} iterations):\n{}",
        plan.steps.len(),
        plan.estimated_iterations,
        plan.steps
            .iter()
            .map(|s| format!("  {}. {}", s.step, s.action))
            .collect::<Vec<_>>()
            .join("\n")
    );

    AgentEvent::Thinking {
        agent_id: agent_id.to_string(),
        content: summary,
    }
}

/// Attempt to parse an `ExecutionPlan` from LLM output.
///
/// Handles cases where the LLM wraps JSON in markdown code fences.
fn parse_plan(text: &str) -> Option<ExecutionPlan> {
    // Strip markdown code fences if present
    let cleaned = text
        .trim()
        .strip_prefix("```json")
        .or_else(|| text.trim().strip_prefix("```"))
        .unwrap_or(text.trim());
    let cleaned = cleaned
        .strip_suffix("```")
        .unwrap_or(cleaned)
        .trim();

    serde_json::from_str::<ExecutionPlan>(cleaned).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plan_from_json() {
        let json = r#"{
            "goal": "Add a login page",
            "steps": [
                {"step": 1, "action": "Read existing layout", "tool": "file_read", "expected_outcome": "Understand the layout structure"},
                {"step": 2, "action": "Create login component", "tool": "file_write", "expected_outcome": "Login page file created"}
            ],
            "estimated_iterations": 4,
            "risks": ["Existing auth might conflict"]
        }"#;

        let plan = parse_plan(json).expect("Should parse valid JSON");
        assert_eq!(plan.steps.len(), 2);
        assert_eq!(plan.estimated_iterations, 4);
        assert_eq!(plan.risks.len(), 1);
    }

    #[test]
    fn parse_plan_from_markdown_fenced() {
        let text = "```json\n{\"goal\": \"Test\", \"steps\": [], \"estimated_iterations\": 1, \"risks\": []}\n```";
        let plan = parse_plan(text).expect("Should parse markdown-fenced JSON");
        assert_eq!(plan.goal, "Test");
    }

    #[test]
    fn inject_plan_appends_to_prompt() {
        let plan = ExecutionPlan {
            goal: "Build feature X".into(),
            steps: vec![PlanStep {
                step: 1,
                action: "Read the code".into(),
                tool: Some("file_read".into()),
                expected_outcome: "Understand the structure".into(),
            }],
            estimated_iterations: 3,
            risks: vec!["Might be complex".into()],
        };

        let result = AgentPlanner::inject_plan("You are an agent.", &plan);
        assert!(result.contains("EXECUTION PLAN"));
        assert!(result.contains("Build feature X"));
        assert!(result.contains("Read the code"));
        assert!(result.contains("Might be complex"));
    }
}
