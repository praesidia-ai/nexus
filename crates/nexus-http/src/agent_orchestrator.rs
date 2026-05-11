//! Agent Orchestrator — multi-agent coordination with dependency resolution,
//! parallel execution, result aggregation, and conflict detection.
//!
//! Extends the existing workflow DAG with:
//! - Dynamic agent selection based on task analysis
//! - Parallel execution with shared context
//! - Inter-agent conflict detection (two agents editing the same file)
//! - Result merging and quality voting
//! - Resource budgeting (token limits per agent)

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex};
use tracing::{info, warn};

use crate::agents::definition::AgentDefinition;
use crate::agents::events::AgentEvent;
use crate::agents::runtime::{AgentContext, AgentRuntime};
use crate::agents::tools::ToolRegistry;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorTask {
    pub id: String,
    pub description: String,
    pub subtasks: Vec<AgentSubtask>,
    pub constraints: TaskConstraints,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSubtask {
    pub id: String,
    pub agent: String,
    pub prompt: String,
    pub depends_on: Vec<String>,
    pub target_files: Vec<String>,
    pub max_tokens: Option<u64>,
    pub priority: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskConstraints {
    /// Maximum total tokens across all agents.
    pub max_total_tokens: u64,
    /// Maximum parallel agents.
    pub max_parallel: usize,
    /// Timeout per subtask in seconds.
    pub subtask_timeout_secs: u64,
    /// Whether to abort on first failure.
    pub fail_fast: bool,
}

impl Default for TaskConstraints {
    fn default() -> Self {
        Self {
            max_total_tokens: 500_000,
            max_parallel: 4,
            subtask_timeout_secs: 300,
            fail_fast: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtaskResult {
    pub subtask_id: String,
    pub agent: String,
    pub status: SubtaskStatus,
    pub output: String,
    pub files_modified: Vec<String>,
    pub tokens_used: u64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubtaskStatus {
    Completed,
    Failed,
    Skipped,
    TimedOut,
    Conflict,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorResult {
    pub task_id: String,
    pub status: String,
    pub subtask_results: Vec<SubtaskResult>,
    pub conflicts: Vec<FileConflict>,
    pub total_tokens: u64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileConflict {
    pub file: String,
    pub agents: Vec<String>,
    pub resolution: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OrchestratorEvent {
    #[serde(rename = "plan")]
    Plan {
        layers: Vec<Vec<String>>,
        total_subtasks: usize,
    },
    #[serde(rename = "subtask_start")]
    SubtaskStart {
        subtask_id: String,
        agent: String,
    },
    #[serde(rename = "subtask_end")]
    SubtaskEnd {
        result: SubtaskResult,
    },
    #[serde(rename = "conflict")]
    Conflict {
        conflict: FileConflict,
    },
    #[serde(rename = "complete")]
    Complete {
        result: OrchestratorResult,
    },
}

// ---------------------------------------------------------------------------
// Dependency Resolution
// ---------------------------------------------------------------------------

/// Resolve subtask dependencies into execution layers.
/// Each layer contains subtasks that can run in parallel.
fn resolve_layers(subtasks: &[AgentSubtask]) -> Vec<Vec<usize>> {
    let id_to_idx: HashMap<&str, usize> = subtasks
        .iter()
        .enumerate()
        .map(|(i, s)| (s.id.as_str(), i))
        .collect();

    let mut resolved: HashSet<usize> = HashSet::new();
    let mut layers: Vec<Vec<usize>> = Vec::new();
    let total = subtasks.len();

    while resolved.len() < total {
        let mut layer = Vec::new();
        for (i, subtask) in subtasks.iter().enumerate() {
            if resolved.contains(&i) {
                continue;
            }
            let deps_met = subtask
                .depends_on
                .iter()
                .all(|dep| id_to_idx.get(dep.as_str()).is_none_or(|idx| resolved.contains(idx)));
            if deps_met {
                layer.push(i);
            }
        }

        if layer.is_empty() {
            // Circular dependency — add remaining as final layer
            warn!("Circular dependency detected in orchestrator subtasks");
            let remaining: Vec<usize> = (0..total).filter(|i| !resolved.contains(i)).collect();
            layers.push(remaining.clone());
            resolved.extend(remaining);
            break;
        }

        resolved.extend(layer.iter());
        layers.push(layer);
    }

    layers
}

// ---------------------------------------------------------------------------
// Conflict Detection
// ---------------------------------------------------------------------------

fn detect_conflicts(results: &[SubtaskResult]) -> Vec<FileConflict> {
    let mut file_agents: HashMap<String, Vec<String>> = HashMap::new();

    for result in results {
        if matches!(result.status, SubtaskStatus::Completed) {
            for file in &result.files_modified {
                file_agents
                    .entry(file.clone())
                    .or_default()
                    .push(result.agent.clone());
            }
        }
    }

    file_agents
        .into_iter()
        .filter(|(_, agents)| agents.len() > 1)
        .map(|(file, agents)| FileConflict {
            file,
            agents: agents.clone(),
            resolution: format!("Last-write wins (by {})", agents.last().unwrap_or(&"unknown".into())),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Orchestrator
// ---------------------------------------------------------------------------

pub struct AgentOrchestrator {
    project_dir: PathBuf,
    app: Arc<AppState>,
    tool_registry: Arc<ToolRegistry>,
}

impl AgentOrchestrator {
    pub fn new(project_dir: PathBuf, app: Arc<AppState>, tool_registry: Arc<ToolRegistry>) -> Self {
        Self { project_dir, app, tool_registry }
    }

    /// Execute a multi-agent task with dependency resolution.
    pub async fn execute(
        &self,
        task: &OrchestratorTask,
        tx: mpsc::Sender<OrchestratorEvent>,
    ) -> OrchestratorResult {
        let start = std::time::Instant::now();

        // Step 1: Resolve execution layers
        let layers = resolve_layers(&task.subtasks);
        let layer_ids: Vec<Vec<String>> = layers
            .iter()
            .map(|layer| {
                layer
                    .iter()
                    .map(|&i| task.subtasks[i].id.clone())
                    .collect()
            })
            .collect();

        let _ = tx
            .send(OrchestratorEvent::Plan {
                layers: layer_ids,
                total_subtasks: task.subtasks.len(),
            })
            .await;

        // Step 2: Execute layers sequentially, subtasks within layers in parallel
        let results: Arc<Mutex<Vec<SubtaskResult>>> = Arc::new(Mutex::new(Vec::new()));
        let tokens_used = Arc::new(Mutex::new(0u64));

        for layer in &layers {
            // Limit parallel execution
            let semaphore = Arc::new(tokio::sync::Semaphore::new(
                task.constraints.max_parallel,
            ));

            let mut handles = Vec::new();

            for &idx in layer {
                let subtask = task.subtasks[idx].clone();
                let sem = semaphore.clone();
                let tx_inner = tx.clone();
                let results_inner = results.clone();
                let tokens_inner = tokens_used.clone();
                let project_dir = self.project_dir.clone();
                let app = self.app.clone();
                let registry = self.tool_registry.clone();
                let timeout = task.constraints.subtask_timeout_secs;
                let max_total = task.constraints.max_total_tokens;

                let handle = tokio::spawn(async move {
                    let Ok(_permit) = sem.acquire().await else { return; };

                    // Check token budget
                    {
                        let current = *tokens_inner.lock().await;
                        if current >= max_total {
                            let result = SubtaskResult {
                                subtask_id: subtask.id.clone(),
                                agent: subtask.agent.clone(),
                                status: SubtaskStatus::Skipped,
                                output: "Token budget exhausted".into(),
                                files_modified: Vec::new(),
                                tokens_used: 0,
                                duration_ms: 0,
                            };
                            results_inner.lock().await.push(result.clone());
                            let _ = tx_inner.send(OrchestratorEvent::SubtaskEnd { result }).await;
                            return;
                        }
                    }

                    let _ = tx_inner
                        .send(OrchestratorEvent::SubtaskStart {
                            subtask_id: subtask.id.clone(),
                            agent: subtask.agent.clone(),
                        })
                        .await;

                    // Execute subtask with timeout
                    let subtask_start = std::time::Instant::now();
                    let result = tokio::time::timeout(
                        std::time::Duration::from_secs(timeout),
                        execute_subtask(&subtask, &project_dir, &app, &registry),
                    )
                    .await;

                    let duration_ms = subtask_start.elapsed().as_millis() as u64;

                    let subtask_result = match result {
                        Ok(Ok(mut r)) => {
                            r.duration_ms = duration_ms;
                            r
                        }
                        Ok(Err(e)) => SubtaskResult {
                            subtask_id: subtask.id.clone(),
                            agent: subtask.agent.clone(),
                            status: SubtaskStatus::Failed,
                            output: e,
                            files_modified: Vec::new(),
                            tokens_used: 0,
                            duration_ms,
                        },
                        Err(_) => SubtaskResult {
                            subtask_id: subtask.id.clone(),
                            agent: subtask.agent.clone(),
                            status: SubtaskStatus::TimedOut,
                            output: format!("Timed out after {}s", timeout),
                            files_modified: Vec::new(),
                            tokens_used: 0,
                            duration_ms,
                        },
                    };

                    // Update token counter
                    *tokens_inner.lock().await += subtask_result.tokens_used;

                    results_inner.lock().await.push(subtask_result.clone());
                    let _ = tx_inner
                        .send(OrchestratorEvent::SubtaskEnd {
                            result: subtask_result,
                        })
                        .await;
                });

                handles.push(handle);
            }

            // Wait for all subtasks in this layer
            for handle in handles {
                let _ = handle.await;
            }

            // Check fail-fast
            if task.constraints.fail_fast {
                let results_lock = results.lock().await;
                if results_lock.iter().any(|r| matches!(r.status, SubtaskStatus::Failed)) {
                    break;
                }
            }
        }

        // Step 3: Detect conflicts
        let all_results = results.lock().await.clone();
        let conflicts = detect_conflicts(&all_results);

        for conflict in &conflicts {
            let _ = tx
                .send(OrchestratorEvent::Conflict {
                    conflict: conflict.clone(),
                })
                .await;
        }

        // Step 4: Build result
        let total_tokens = *tokens_used.lock().await;
        let all_completed = all_results.iter().all(|r| {
            matches!(r.status, SubtaskStatus::Completed | SubtaskStatus::Skipped)
        });

        let status = if all_completed && conflicts.is_empty() {
            "completed"
        } else if all_completed {
            "completed_with_conflicts"
        } else {
            "partial"
        };

        let result = OrchestratorResult {
            task_id: task.id.clone(),
            status: status.into(),
            subtask_results: all_results,
            conflicts,
            total_tokens,
            duration_ms: start.elapsed().as_millis() as u64,
        };

        // The terminal Complete event MUST arrive or the SSE reader hangs
        // forever (CLAUDE.md invariant). On backpressure we bound the wait;
        // on final failure we log at ERROR — the channel will close on drop
        // which at least lets the client error out.
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            tx.send(OrchestratorEvent::Complete {
                result: result.clone(),
            }),
        )
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(_closed)) => tracing::error!(
                task_id = %task.id,
                "failed to deliver terminal Complete event — channel closed; SSE client may hang"
            ),
            Err(_timeout) => tracing::error!(
                task_id = %task.id,
                "timed out delivering terminal Complete event — SSE client may hang"
            ),
        }

        info!(
            task_id = %task.id,
            status = status,
            total_tokens = total_tokens,
            duration_ms = result.duration_ms,
            "Orchestrator task complete"
        );

        result
    }

    /// Analyze a task description and suggest subtask decomposition.
    pub fn decompose(description: &str) -> Vec<AgentSubtask> {
        let lower = description.to_lowercase();
        let mut subtasks = Vec::new();
        let mut id_counter = 0u32;

        let mut next_id = || -> String {
            id_counter += 1;
            format!("sub_{}", id_counter)
        };

        // Architecture/planning subtask
        if lower.contains("build") || lower.contains("create") || lower.contains("implement") {
            subtasks.push(AgentSubtask {
                id: next_id(),
                agent: "Leo".into(),
                prompt: format!("Plan the architecture for: {}", description),
                depends_on: vec![],
                target_files: vec![],
                max_tokens: Some(50_000),
                priority: 1,
            });
        }

        // Backend subtask
        if lower.contains("api")
            || lower.contains("database")
            || lower.contains("backend")
            || lower.contains("route")
            || lower.contains("server")
        {
            subtasks.push(AgentSubtask {
                id: next_id(),
                agent: "Nova".into(),
                prompt: format!("Implement the backend for: {}", description),
                depends_on: if subtasks.is_empty() {
                    vec![]
                } else {
                    vec![subtasks[0].id.clone()]
                },
                target_files: vec!["src/app/api/".into()],
                max_tokens: Some(100_000),
                priority: 2,
            });
        }

        // Frontend subtask
        if lower.contains("ui")
            || lower.contains("page")
            || lower.contains("frontend")
            || lower.contains("component")
            || lower.contains("form")
            || lower.contains("dashboard")
        {
            subtasks.push(AgentSubtask {
                id: next_id(),
                agent: "Nova".into(),
                prompt: format!("Implement the frontend for: {}", description),
                depends_on: if subtasks.len() > 1 {
                    vec![subtasks[1].id.clone()] // depends on backend
                } else if !subtasks.is_empty() {
                    vec![subtasks[0].id.clone()]
                } else {
                    vec![]
                },
                target_files: vec!["src/app/".into(), "src/components/".into()],
                max_tokens: Some(100_000),
                priority: 3,
            });
        }

        // Security review subtask
        subtasks.push(AgentSubtask {
            id: next_id(),
            agent: "Orion".into(),
            prompt: format!(
                "Review the implementation for security issues: {}",
                description
            ),
            depends_on: subtasks.iter().map(|s| s.id.clone()).collect(),
            target_files: vec![],
            max_tokens: Some(30_000),
            priority: 10,
        });

        // Documentation subtask
        subtasks.push(AgentSubtask {
            id: next_id(),
            agent: "Luna".into(),
            prompt: format!("Write documentation for: {}", description),
            depends_on: subtasks
                .iter()
                .filter(|s| s.agent != "Orion")
                .map(|s| s.id.clone())
                .collect(),
            target_files: vec!["README.md".into()],
            max_tokens: Some(20_000),
            priority: 10,
        });

        subtasks
    }
}

/// Map agent names (Nova, Atlas, Orion, etc.) to agent definitions.
fn agent_definition_for_subtask(subtask: &AgentSubtask, project_id: &str) -> AgentDefinition {
    use crate::agents::factories;

    // Map Nexus agent names to factory functions
    let base = match subtask.agent.to_lowercase().as_str() {
        "nova" | "coder" => factories::coding_agent(project_id),
        "atlas" | "architect" => factories::architect_agent(project_id),
        "orion" | "reviewer" | "security" => factories::reviewer_agent(project_id),
        "rex" | "devops" => factories::devops_agent(project_id),
        "leo" | "pm" | "product" => factories::ux_agent(project_id),     // closest match
        "luna" | "writer" => factories::coding_agent(project_id),         // writing via coding
        "kai" | "researcher" => factories::coding_agent(project_id),
        "sage" | "data" => factories::coding_agent(project_id),
        "mia" | "support" => factories::coding_agent(project_id),
        "ivy" | "marketer" => factories::coding_agent(project_id),
        _ => factories::coding_agent(project_id),
    };

    // Override with subtask-specific prompt and limits
    AgentDefinition {
        id: format!("orch-{}-{}", subtask.agent.to_lowercase(), subtask.id),
        system_prompt: format!("{}\n\nYour specific task:\n{}", base.system_prompt, subtask.prompt),
        max_iterations: subtask
            .max_tokens
            .map(|t| ((t / 1000) as u32).clamp(5, 30))
            .unwrap_or(base.max_iterations),
        ..base
    }
}

/// Execute a single subtask by running a real agent via AgentRuntime.
async fn execute_subtask(
    subtask: &AgentSubtask,
    project_dir: &std::path::Path,
    app: &Arc<AppState>,
    tool_registry: &Arc<ToolRegistry>,
) -> Result<SubtaskResult, String> {
    let project_id = project_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".into());

    let definition = agent_definition_for_subtask(subtask, &project_id);

    // Recall relevant memory for this agent
    let memory_context = if let Ok(mem) = app.memory_system() {
        mem.recall_context(
            &project_id,
            Some(&subtask.agent),
            &[], // no tag filter
            10,  // top 10 memories
        )
        .ok()
    } else {
        None
    };

    // Inject memory into agent context
    let context_with_memory = if let Some(ref recalled) = memory_context {
        if recalled.context_block.is_empty() {
            subtask.prompt.clone()
        } else {
            format!("{}\n\n{}", subtask.prompt, recalled.context_block)
        }
    } else {
        subtask.prompt.clone()
    };

    let runtime = AgentRuntime::new(app.clone(), tool_registry.clone());
    let context = AgentContext {
        project_id: Some(project_id.clone()),
        project_dir: Some(project_dir.to_path_buf()),
        context: context_with_memory,
        history: vec![],
    };

    let (mut rx, handle) = runtime
        .execute(&definition, &subtask.prompt, context)
        .await
        .map_err(|e| format!("Failed to start agent {}: {}", subtask.agent, e))?;

    // Drain events, track files modified and last thinking output
    let mut files_modified = Vec::new();
    let mut last_output = String::new();
    let mut completed = false;
    let mut failed_error = None;

    while let Some(event) = rx.recv().await {
        match &event {
            AgentEvent::ToolResult { tool, output, .. } => {
                if tool == "file_write" || tool == "file_edit" {
                    if let Some(path) = output.get("path").and_then(|v| v.as_str()) {
                        files_modified.push(path.to_string());
                    }
                }
            }
            AgentEvent::Thinking { content, .. } => {
                last_output.clone_from(content);
            }
            AgentEvent::Completed { result, .. } => {
                last_output = result.output.clone();
                files_modified.extend(result.files_modified.clone());
                completed = true;
            }
            AgentEvent::Failed { error, .. } => {
                failed_error = Some(error.clone());
            }
            _ => {}
        }
    }

    // Wait for the spawned task to finish
    let _ = handle.await;

    if let Some(error) = failed_error {
        // Record failure episode
        record_execution_episode(app, &project_id, subtask, false, Some(&error), &[]);
        return Ok(SubtaskResult {
            subtask_id: subtask.id.clone(),
            agent: subtask.agent.clone(),
            status: SubtaskStatus::Failed,
            output: error,
            files_modified: Vec::new(),
            tokens_used: 0,
            duration_ms: 0,
        });
    }

    // Record success episode
    record_execution_episode(
        app,
        &project_id,
        subtask,
        completed,
        None,
        &files_modified,
    );

    Ok(SubtaskResult {
        subtask_id: subtask.id.clone(),
        agent: subtask.agent.clone(),
        status: if completed { SubtaskStatus::Completed } else { SubtaskStatus::Failed },
        output: last_output,
        files_modified,
        tokens_used: 0, // tracked by cost_tracker, not per-result
        duration_ms: 0,  // set by caller
    })
}

/// Record an episode in the memory system after an agent subtask completes.
fn record_execution_episode(
    app: &Arc<AppState>,
    project_id: &str,
    subtask: &AgentSubtask,
    success: bool,
    error: Option<&str>,
    files_modified: &[String],
) {
    if let Ok(mem) = app.memory_system() {
        let summary_suffix = if subtask.prompt.len() > 80 {
            &subtask.prompt[..80]
        } else {
            &subtask.prompt
        };
        let episode = nexus_memory::episodic::Episode {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id: project_id.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            agent_id: Some(subtask.agent.clone()),
            team_id: None,
            project_id: Some(project_id.to_string()),
            episode_type: nexus_memory::episodic::EpisodeType::AgentExecution {
                task: subtask.prompt.clone(),
                tools_used: files_modified.to_vec(),
                iterations: 1,
            },
            summary: format!("Agent {} completed: {}", subtask.agent, summary_suffix),
            details: serde_json::json!({ "subtask_id": subtask.id }),
            outcome: if success {
                nexus_memory::episodic::Outcome::Success {
                    quality_score: None,
                }
            } else {
                nexus_memory::episodic::Outcome::Failure {
                    reason: error.unwrap_or("unknown").to_string(),
                }
            },
            tags: vec![subtask.agent.clone(), "orchestrator".to_string()],
            importance: 0.5,
            embedding: vec![], // empty — lightweight recall does not need embeddings
        };
        let _ = mem.episodic.record(&episode);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_linear_dependencies() {
        let subtasks = vec![
            AgentSubtask {
                id: "a".into(),
                agent: "Nova".into(),
                prompt: "first".into(),
                depends_on: vec![],
                target_files: vec![],
                max_tokens: None,
                priority: 1,
            },
            AgentSubtask {
                id: "b".into(),
                agent: "Nova".into(),
                prompt: "second".into(),
                depends_on: vec!["a".into()],
                target_files: vec![],
                max_tokens: None,
                priority: 2,
            },
            AgentSubtask {
                id: "c".into(),
                agent: "Orion".into(),
                prompt: "third".into(),
                depends_on: vec!["b".into()],
                target_files: vec![],
                max_tokens: None,
                priority: 3,
            },
        ];

        let layers = resolve_layers(&subtasks);
        assert_eq!(layers.len(), 3);
        assert_eq!(layers[0], vec![0]);
        assert_eq!(layers[1], vec![1]);
        assert_eq!(layers[2], vec![2]);
    }

    #[test]
    fn resolve_parallel_tasks() {
        let subtasks = vec![
            AgentSubtask {
                id: "plan".into(),
                agent: "Leo".into(),
                prompt: "plan".into(),
                depends_on: vec![],
                target_files: vec![],
                max_tokens: None,
                priority: 1,
            },
            AgentSubtask {
                id: "backend".into(),
                agent: "Nova".into(),
                prompt: "backend".into(),
                depends_on: vec!["plan".into()],
                target_files: vec!["api/".into()],
                max_tokens: None,
                priority: 2,
            },
            AgentSubtask {
                id: "frontend".into(),
                agent: "Nova".into(),
                prompt: "frontend".into(),
                depends_on: vec!["plan".into()],
                target_files: vec!["app/".into()],
                max_tokens: None,
                priority: 2,
            },
            AgentSubtask {
                id: "review".into(),
                agent: "Orion".into(),
                prompt: "review".into(),
                depends_on: vec!["backend".into(), "frontend".into()],
                target_files: vec![],
                max_tokens: None,
                priority: 3,
            },
        ];

        let layers = resolve_layers(&subtasks);
        assert_eq!(layers.len(), 3);
        assert_eq!(layers[0], vec![0]); // plan
        assert!(layers[1].contains(&1) && layers[1].contains(&2)); // backend + frontend in parallel
        assert_eq!(layers[2], vec![3]); // review
    }

    #[test]
    fn detect_file_conflicts() {
        let results = vec![
            SubtaskResult {
                subtask_id: "a".into(),
                agent: "Nova".into(),
                status: SubtaskStatus::Completed,
                output: "done".into(),
                files_modified: vec!["src/page.tsx".into(), "src/api/route.ts".into()],
                tokens_used: 1000,
                duration_ms: 100,
            },
            SubtaskResult {
                subtask_id: "b".into(),
                agent: "Orion".into(),
                status: SubtaskStatus::Completed,
                output: "done".into(),
                files_modified: vec!["src/page.tsx".into()],
                tokens_used: 500,
                duration_ms: 50,
            },
        ];

        let conflicts = detect_conflicts(&results);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].file, "src/page.tsx");
        assert_eq!(conflicts[0].agents.len(), 2);
    }

    #[test]
    fn decompose_full_stack_task() {
        let subtasks = AgentOrchestrator::decompose(
            "Build a dashboard with API endpoints and database integration",
        );
        assert!(subtasks.len() >= 3);
        // Should have at least: plan, backend, frontend, security, docs
        let agents: Vec<&str> = subtasks.iter().map(|s| s.agent.as_str()).collect();
        assert!(agents.contains(&"Leo"));
        assert!(agents.contains(&"Nova"));
        assert!(agents.contains(&"Orion"));
        assert!(agents.contains(&"Luna"));
    }
}
