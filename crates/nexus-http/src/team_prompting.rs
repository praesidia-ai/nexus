//! Team system prompt builder — constructs team-aware system prompts for agents.
//!
//! When an agent runs as part of a team, its system prompt is augmented with:
//! - Its role and responsibilities within the team
//! - The full team roster (who else is on the team)
//! - Communication instructions (how to send/receive messages)
//! - Current workspace state (task board, artifacts)
//! - Available team-specific tools

use nexus_agents_core::teams::{TeamDefinition, TeamMember, TeamRole};
use nexus_agents_core::workspace::{TaskStatus, WorkspaceSnapshot};

use crate::team_orchestrator::TeamWorkspace;

/// Build a complete system prompt for a team member.
///
/// The prompt includes the agent's original system prompt plus team-specific
/// sections for role awareness, team roster, communication protocol, and
/// current workspace state.
#[allow(clippy::vec_init_then_push)]
pub async fn build_member_prompt(
    member: &TeamMember,
    team: &TeamDefinition,
    workspace: &TeamWorkspace,
) -> String {
    let snapshot = workspace.snapshot().await;

    let mut sections = Vec::new();

    // Section 1: Original system prompt.
    sections.push(member.agent.system_prompt.clone());

    // Section 2: Team context.
    sections.push(build_team_context(member, team));

    // Section 3: Communication instructions.
    sections.push(build_communication_instructions(member));

    // Section 4: Current workspace state.
    sections.push(build_workspace_state(&snapshot, &member.id));

    // Section 5: Available team tools.
    sections.push(build_team_tools_description());

    sections.join("\n\n---\n\n")
}

/// Build the team context section describing the member's role and teammates.
fn build_team_context(member: &TeamMember, team: &TeamDefinition) -> String {
    let mut lines = Vec::new();

    lines.push(format!("## Team: {}", team.name));
    lines.push(format!("**Description:** {}", team.description));
    lines.push(String::new());

    // This member's role.
    lines.push(format!("## Your Role: {}", format_role(&member.role)));
    if !member.responsibilities.is_empty() {
        lines.push("**Responsibilities:**".to_string());
        for r in &member.responsibilities {
            lines.push(format!("- {}", r));
        }
    }
    if member.can_veto {
        lines.push("You have **veto power** on team decisions.".to_string());
    }
    if !member.can_delegate_to.is_empty() {
        lines.push(format!(
            "You can delegate tasks to: {}",
            member.can_delegate_to.join(", ")
        ));
    }
    lines.push(String::new());

    // Team roster.
    lines.push("## Team Members".to_string());
    for m in &team.members {
        let you_marker = if m.id == member.id { " **(you)**" } else { "" };
        lines.push(format!(
            "- **{}**{}: {} — {}",
            m.id,
            you_marker,
            format_role(&m.role),
            m.responsibilities.join(", ")
        ));
    }

    lines.join("\n")
}

/// Build communication instructions for the member.
#[allow(clippy::vec_init_then_push)]
fn build_communication_instructions(member: &TeamMember) -> String {
    let mut lines = Vec::new();

    lines.push("## Communication Protocol".to_string());
    lines.push(String::new());
    lines.push(
        "You are part of a collaborative team. Communicate with teammates using structured messages."
            .to_string(),
    );
    lines.push(String::new());
    lines.push("**Guidelines:**".to_string());
    lines.push("- When you need another member to do something, send a **task_assignment** message.".to_string());
    lines.push("- When you finish your work, send a **task_completion** message with a summary.".to_string());
    lines.push("- When you want feedback on your work, send a **review** request.".to_string());
    lines.push("- When you need to share important context, use **context_share**.".to_string());
    lines.push(
        "- For general discussion or questions, use **discussion** messages.".to_string(),
    );
    lines.push(
        "- If you encounter a problem you cannot solve, send an **escalation** to the lead or human."
            .to_string(),
    );
    lines.push(String::new());
    lines.push("**Message targets:**".to_string());
    lines.push("- `direct:<member_id>` — send to a specific member".to_string());
    lines.push("- `broadcast` — send to all team members".to_string());
    lines.push("- `role:<role>` — send to all members with a specific role".to_string());
    lines.push("- `human` — escalate to the human operator".to_string());

    if !member.can_delegate_to.is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "You can delegate to: {}",
            member.can_delegate_to.join(", ")
        ));
    }

    lines.join("\n")
}

/// Build the current workspace state section.
fn build_workspace_state(snapshot: &WorkspaceSnapshot, member_id: &str) -> String {
    let mut lines = Vec::new();

    lines.push("## Current Workspace State".to_string());

    // Task board summary.
    if snapshot.tasks.is_empty() {
        lines.push("\n**Task Board:** No tasks yet.".to_string());
    } else {
        lines.push("\n**Task Board:**".to_string());
        for task in &snapshot.tasks {
            let assigned = task
                .assigned_to
                .as_deref()
                .unwrap_or("unassigned");
            let is_mine = task.assigned_to.as_deref() == Some(member_id);
            let marker = if is_mine { " ← YOUR TASK" } else { "" };
            lines.push(format!(
                "- [{}] {} — assigned to: {}{}",
                task.status, task.title, assigned, marker
            ));
        }

        // Summary counts.
        let done_count = snapshot
            .tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Done)
            .count();
        let total = snapshot.tasks.len();
        lines.push(format!(
            "\n*Progress: {}/{} tasks done*",
            done_count, total
        ));
    }

    // Artifacts.
    if !snapshot.artifacts.is_empty() {
        lines.push("\n**Published Artifacts:**".to_string());
        for art in &snapshot.artifacts {
            lines.push(format!(
                "- {} ({:?}) by {} (v{})",
                art.name, art.artifact_type, art.produced_by, art.version
            ));
        }
    }

    // Scratchpad.
    if !snapshot.scratchpad.is_empty() {
        lines.push("\n**Scratchpad Notes:**".to_string());
        for (author, topic, content) in &snapshot.scratchpad {
            lines.push(format!("- **{}** [{}]: {}", author, topic, content));
        }
    }

    lines.join("\n")
}

/// Build the team-specific tools description.
///
/// These tools are injected into the agent's tool registry by the orchestrator
/// so the agent can interact with the team bus and workspace.
#[allow(clippy::vec_init_then_push)]
fn build_team_tools_description() -> String {
    let mut lines = Vec::new();

    lines.push("## Team Tools".to_string());
    lines.push(String::new());
    lines.push(
        "In addition to your standard tools, you have these team collaboration tools:"
            .to_string(),
    );
    lines.push(String::new());
    lines.push("- **send_message**: Send a message to another team member or broadcast to all.".to_string());
    lines.push("  Parameters: `target` (direct:<id>, broadcast, role:<role>, human), `content_type`, `content`".to_string());
    lines.push(String::new());
    lines.push("- **read_messages**: Read messages addressed to you.".to_string());
    lines.push("  Returns all unread messages in your inbox.".to_string());
    lines.push(String::new());
    lines.push("- **create_task**: Create a new task on the team workspace board.".to_string());
    lines.push(
        "  Parameters: `title`, `description`, `assigned_to` (optional), `priority`".to_string(),
    );
    lines.push(String::new());
    lines.push("- **update_task**: Update a task's status.".to_string());
    lines.push(
        "  Parameters: `task_id`, `status` (backlog, ready, in_progress, in_review, blocked, done, failed)"
            .to_string(),
    );
    lines.push(String::new());
    lines.push("- **publish_artifact**: Publish an artifact to the shared workspace.".to_string());
    lines.push(
        "  Parameters: `name`, `artifact_type` (code, document, plan, decision, data, review), `content`"
            .to_string(),
    );
    lines.push(String::new());
    lines.push("- **get_workspace**: Get the current workspace snapshot (tasks, artifacts, state).".to_string());

    lines.join("\n")
}

/// Format a team role for display.
fn format_role(role: &TeamRole) -> String {
    match role {
        TeamRole::Lead => "Team Lead".to_string(),
        TeamRole::Specialist { domain } => format!("Specialist ({})", domain),
        TeamRole::Reviewer => "Code Reviewer".to_string(),
        TeamRole::Coordinator => "Coordinator".to_string(),
        TeamRole::Observer => "Observer".to_string(),
        TeamRole::Custom { name, .. } => name.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_agents_core::definition::{AgentDefinition, ExecutionMode};

    fn test_member(id: &str, role: TeamRole) -> TeamMember {
        TeamMember {
            id: id.to_string(),
            role,
            agent: AgentDefinition {
                id: id.to_string(),
                name: id.to_string(),
                system_prompt: format!("You are {}", id),
                skills: Vec::new(),
                tools: Vec::new(),
                model_preference: None,
                execution_mode: ExecutionMode::Interactive,
                max_iterations: 10,
                timeout_secs: 300,
            },
            responsibilities: vec!["Build things".to_string()],
            can_delegate_to: vec![],
            can_veto: false,
        }
    }

    #[test]
    fn test_format_role() {
        assert_eq!(format_role(&TeamRole::Lead), "Team Lead");
        assert_eq!(
            format_role(&TeamRole::Specialist {
                domain: "backend".to_string()
            }),
            "Specialist (backend)"
        );
        assert_eq!(format_role(&TeamRole::Reviewer), "Code Reviewer");
    }

    #[test]
    fn test_build_team_context() {
        let member = test_member("alice", TeamRole::Lead);
        let team = TeamDefinition {
            id: "team-1".to_string(),
            name: "Dev Team".to_string(),
            description: "Builds features".to_string(),
            members: vec![
                member.clone(),
                test_member("bob", TeamRole::Specialist {
                    domain: "frontend".to_string(),
                }),
            ],
            coordination: nexus_agents_core::teams::CoordinationProtocol::Hierarchical {
                lead_id: "alice".to_string(),
            },
            budget: nexus_agents_core::teams::TeamBudget {
                max_total_cost_usd: 10.0,
                max_cost_per_member_usd: 5.0,
                max_total_tokens: 100000,
                max_duration_secs: 600,
                max_iterations_per_member: 20,
            },
            hitl: nexus_agents_core::teams::HitlConfig {
                enabled: false,
                checkpoints: vec![],
            },
            completion: nexus_agents_core::teams::CompletionCriteria {
                all_tasks_done: true,
                lead_approval_required: false,
                all_reviews_passed: false,
                completion_phrase: None,
            },
        };

        let context = build_team_context(&member, &team);
        assert!(context.contains("Dev Team"));
        assert!(context.contains("alice"));
        assert!(context.contains("**(you)**"));
        assert!(context.contains("bob"));
    }

    #[test]
    fn test_build_workspace_state_empty() {
        let snapshot = WorkspaceSnapshot {
            tasks: Vec::new(),
            artifacts: Vec::new(),
            state: Vec::new(),
            scratchpad: Vec::new(),
        };
        let state = build_workspace_state(&snapshot, "alice");
        assert!(state.contains("No tasks yet"));
    }
}
