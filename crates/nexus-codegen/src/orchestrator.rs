use crate::deploy::DeployConfig;
use crate::team::{AgentRole, CodeGenAgent, CodeGenTeam, CoordinationMode};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationRequest {
    pub description: String,
    pub template: Option<String>,
    pub features: Vec<String>,
    pub deploy: Option<DeployConfig>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationResult {
    pub id: String,
    pub status: GenerationStatus,
    pub files: Vec<GeneratedFile>,
    pub architecture: Option<serde_json::Value>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub agent_outputs: HashMap<String, AgentOutput>,
    pub metrics: GenerationMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum GenerationStatus {
    Planning,
    Generating,
    Testing,
    Deploying,
    Completed,
    Failed { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedFile {
    pub path: String,
    pub content: String,
    pub agent_id: String,
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentOutput {
    pub agent_id: String,
    pub role: AgentRole,
    pub files_generated: usize,
    pub iterations: u32,
    pub tokens_used: u64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GenerationMetrics {
    pub total_files: usize,
    pub total_lines: usize,
    pub total_tokens: u64,
    pub total_duration_ms: u64,
    pub agents_used: usize,
}

pub struct CodeGenOrchestrator {
    team: CodeGenTeam,
}

impl CodeGenOrchestrator {
    pub fn new(team: CodeGenTeam) -> Self {
        Self { team }
    }

    pub fn with_full_stack_team() -> Self {
        Self::new(CodeGenTeam::full_stack())
    }

    pub fn with_minimal_team() -> Self {
        Self::new(CodeGenTeam::minimal())
    }

    pub fn team(&self) -> &CodeGenTeam {
        &self.team
    }

    /// Plan the generation -- determine execution order from DAG.
    pub fn plan_execution(&self) -> Vec<Vec<&CodeGenAgent>> {
        match &self.team.coordination {
            CoordinationMode::Sequential => self.team.agents.iter().map(|a| vec![a]).collect(),
            CoordinationMode::Parallel => {
                vec![self.team.agents.iter().collect()]
            }
            CoordinationMode::DagBased { dependencies } => {
                topological_sort(&self.team.agents, dependencies)
            }
        }
    }

    /// Create a generation result stub.
    pub fn start_generation(&self, _request: &GenerationRequest) -> GenerationResult {
        GenerationResult {
            id: uuid::Uuid::new_v4().to_string(),
            status: GenerationStatus::Planning,
            files: Vec::new(),
            architecture: None,
            started_at: Utc::now(),
            completed_at: None,
            agent_outputs: HashMap::new(),
            metrics: GenerationMetrics::default(),
        }
    }
}

/// Topological sort of agents based on dependencies (Kahn's algorithm).
fn topological_sort<'a>(
    agents: &'a [CodeGenAgent],
    dependencies: &[(String, String)],
) -> Vec<Vec<&'a CodeGenAgent>> {
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();

    for agent in agents {
        in_degree.entry(&agent.id).or_insert(0);
        adj.entry(&agent.id).or_default();
    }

    for (from, to) in dependencies {
        adj.entry(to.as_str()).or_default().push(from.as_str());
        *in_degree.entry(from.as_str()).or_insert(0) += 1;
    }

    let mut levels: Vec<Vec<&CodeGenAgent>> = Vec::new();
    let mut queue: Vec<&str> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(&id, _)| id)
        .collect();

    while !queue.is_empty() {
        let current_level: Vec<&CodeGenAgent> = queue
            .iter()
            .filter_map(|&id| agents.iter().find(|a| a.id == id))
            .collect();

        let mut next_queue = Vec::new();
        for &node in &queue {
            if let Some(neighbors) = adj.get(node) {
                for &neighbor in neighbors {
                    if let Some(deg) = in_degree.get_mut(neighbor) {
                        *deg -= 1;
                        if *deg == 0 {
                            next_queue.push(neighbor);
                        }
                    }
                }
            }
        }

        if !current_level.is_empty() {
            levels.push(current_level);
        }
        queue = next_queue;
    }

    levels
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::team::AgentRole;

    #[test]
    fn full_stack_orchestrator_creates_dag_plan() {
        let orch = CodeGenOrchestrator::with_full_stack_team();
        let plan = orch.plan_execution();

        assert!(!plan.is_empty());
        // First level should be the architect (no dependencies)
        let first_level_ids: Vec<&str> = plan[0].iter().map(|a| a.id.as_str()).collect();
        assert!(first_level_ids.contains(&"architect"));
    }

    #[test]
    fn minimal_orchestrator_creates_sequential_plan() {
        let orch = CodeGenOrchestrator::with_minimal_team();
        let plan = orch.plan_execution();

        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].len(), 1);
        assert_eq!(plan[1].len(), 1);
    }

    #[test]
    fn dag_plan_respects_dependencies() {
        let orch = CodeGenOrchestrator::with_full_stack_team();
        let plan = orch.plan_execution();

        let level_of = |id: &str| -> usize {
            plan.iter()
                .position(|level| level.iter().any(|a| a.id == id))
                .unwrap()
        };

        assert!(level_of("architect") < level_of("backend"));
        assert!(level_of("architect") < level_of("frontend"));
        assert!(level_of("architect") < level_of("database"));
        assert!(level_of("backend") < level_of("auth"));
        assert!(level_of("backend") < level_of("devops"));
        assert!(level_of("frontend") < level_of("devops"));
    }

    #[test]
    fn parallel_agents_are_in_same_level() {
        let orch = CodeGenOrchestrator::with_full_stack_team();
        let plan = orch.plan_execution();

        let level_of = |id: &str| -> usize {
            plan.iter()
                .position(|level| level.iter().any(|a| a.id == id))
                .unwrap()
        };

        // backend, frontend, database all depend only on architect
        assert_eq!(level_of("backend"), level_of("frontend"));
        assert_eq!(level_of("backend"), level_of("database"));
    }

    #[test]
    fn start_generation_returns_planning_status() {
        let orch = CodeGenOrchestrator::with_full_stack_team();
        let request = GenerationRequest {
            description: "Build a SaaS app".to_string(),
            template: Some("saas".to_string()),
            features: vec!["auth".to_string(), "billing".to_string()],
            deploy: None,
            model: None,
        };

        let result = orch.start_generation(&request);
        assert_eq!(result.status, GenerationStatus::Planning);
        assert!(result.files.is_empty());
        assert!(!result.id.is_empty());
    }

    #[test]
    fn generation_result_has_valid_timestamp() {
        let orch = CodeGenOrchestrator::with_minimal_team();
        let request = GenerationRequest {
            description: "Test".to_string(),
            template: None,
            features: vec![],
            deploy: None,
            model: None,
        };

        let before = Utc::now();
        let result = orch.start_generation(&request);
        let after = Utc::now();

        assert!(result.started_at >= before);
        assert!(result.started_at <= after);
        assert!(result.completed_at.is_none());
    }

    #[test]
    fn generation_request_serialization_roundtrip() {
        let request = GenerationRequest {
            description: "Build a marketplace".to_string(),
            template: Some("marketplace".to_string()),
            features: vec!["listings".to_string(), "payments".to_string()],
            deploy: Some(crate::deploy::DeployConfig::vercel("my-marketplace")),
            model: Some("gpt-4.1".to_string()),
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: GenerationRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.description, request.description);
        assert_eq!(deserialized.template, request.template);
    }

    #[test]
    fn generation_status_serializes_correctly() {
        let status = GenerationStatus::Failed {
            reason: "timeout".to_string(),
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("failed"));
        assert!(json.contains("timeout"));
    }

    #[test]
    fn topological_sort_empty_deps() {
        let agents = vec![CodeGenAgent {
            id: "solo".to_string(),
            role: AgentRole::Architect,
            system_prompt: String::new(),
            model: "gpt-4.1".to_string(),
            max_iterations: 1,
            depends_on: vec![],
        }];
        let result = topological_sort(&agents, &[]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0][0].id, "solo");
    }

    #[test]
    fn topological_sort_linear_chain() {
        let agents = vec![
            CodeGenAgent {
                id: "a".to_string(),
                role: AgentRole::Architect,
                system_prompt: String::new(),
                model: "gpt-4.1".to_string(),
                max_iterations: 1,
                depends_on: vec![],
            },
            CodeGenAgent {
                id: "b".to_string(),
                role: AgentRole::BackendEngineer,
                system_prompt: String::new(),
                model: "gpt-4.1".to_string(),
                max_iterations: 1,
                depends_on: vec!["a".to_string()],
            },
            CodeGenAgent {
                id: "c".to_string(),
                role: AgentRole::QaEngineer,
                system_prompt: String::new(),
                model: "gpt-4.1".to_string(),
                max_iterations: 1,
                depends_on: vec!["b".to_string()],
            },
        ];
        let deps = vec![
            ("b".to_string(), "a".to_string()),
            ("c".to_string(), "b".to_string()),
        ];
        let result = topological_sort(&agents, &deps);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0][0].id, "a");
        assert_eq!(result[1][0].id, "b");
        assert_eq!(result[2][0].id, "c");
    }
}
