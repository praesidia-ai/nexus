use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeGenTeam {
    pub agents: Vec<CodeGenAgent>,
    pub coordination: CoordinationMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeGenAgent {
    pub id: String,
    pub role: AgentRole,
    pub system_prompt: String,
    pub model: String,
    pub max_iterations: u32,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Architect,
    FrontendEngineer,
    BackendEngineer,
    DatabaseEngineer,
    AuthEngineer,
    DevOps,
    QaEngineer,
    SecurityAuditor,
    DocumentationWriter,
    PaymentSpecialist,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoordinationMode {
    Sequential,
    Parallel,
    DagBased { dependencies: Vec<(String, String)> },
}

impl CodeGenTeam {
    /// Build the default full-stack code generation team.
    pub fn full_stack() -> Self {
        Self {
            agents: vec![
                CodeGenAgent {
                    id: "architect".to_string(),
                    role: AgentRole::Architect,
                    system_prompt: ARCHITECT_PROMPT.to_string(),
                    model: "gpt-4.1".to_string(),
                    max_iterations: 5,
                    depends_on: vec![],
                },
                CodeGenAgent {
                    id: "backend".to_string(),
                    role: AgentRole::BackendEngineer,
                    system_prompt: BACKEND_PROMPT.to_string(),
                    model: "gpt-4.1".to_string(),
                    max_iterations: 10,
                    depends_on: vec!["architect".to_string()],
                },
                CodeGenAgent {
                    id: "frontend".to_string(),
                    role: AgentRole::FrontendEngineer,
                    system_prompt: FRONTEND_PROMPT.to_string(),
                    model: "gpt-4.1".to_string(),
                    max_iterations: 10,
                    depends_on: vec!["architect".to_string()],
                },
                CodeGenAgent {
                    id: "database".to_string(),
                    role: AgentRole::DatabaseEngineer,
                    system_prompt: DATABASE_PROMPT.to_string(),
                    model: "gpt-4.1".to_string(),
                    max_iterations: 5,
                    depends_on: vec!["architect".to_string()],
                },
                CodeGenAgent {
                    id: "auth".to_string(),
                    role: AgentRole::AuthEngineer,
                    system_prompt: AUTH_PROMPT.to_string(),
                    model: "gpt-4.1".to_string(),
                    max_iterations: 5,
                    depends_on: vec!["backend".to_string()],
                },
                CodeGenAgent {
                    id: "devops".to_string(),
                    role: AgentRole::DevOps,
                    system_prompt: DEVOPS_PROMPT.to_string(),
                    model: "gpt-4.1".to_string(),
                    max_iterations: 5,
                    depends_on: vec!["backend".to_string(), "frontend".to_string()],
                },
                CodeGenAgent {
                    id: "qa".to_string(),
                    role: AgentRole::QaEngineer,
                    system_prompt: QA_PROMPT.to_string(),
                    model: "gpt-4.1".to_string(),
                    max_iterations: 8,
                    depends_on: vec!["backend".to_string(), "frontend".to_string()],
                },
            ],
            coordination: CoordinationMode::DagBased {
                dependencies: vec![
                    ("backend".into(), "architect".into()),
                    ("frontend".into(), "architect".into()),
                    ("database".into(), "architect".into()),
                    ("auth".into(), "backend".into()),
                    ("devops".into(), "backend".into()),
                    ("devops".into(), "frontend".into()),
                    ("qa".into(), "backend".into()),
                    ("qa".into(), "frontend".into()),
                ],
            },
        }
    }

    /// Minimal team for quick prototypes.
    pub fn minimal() -> Self {
        Self {
            agents: vec![
                CodeGenAgent {
                    id: "architect".to_string(),
                    role: AgentRole::Architect,
                    system_prompt: ARCHITECT_PROMPT.to_string(),
                    model: "gpt-4.1".to_string(),
                    max_iterations: 3,
                    depends_on: vec![],
                },
                CodeGenAgent {
                    id: "fullstack".to_string(),
                    role: AgentRole::BackendEngineer,
                    system_prompt: "You are a full-stack engineer. Generate both backend API and frontend UI based on the architect's plan.".to_string(),
                    model: "gpt-4.1".to_string(),
                    max_iterations: 10,
                    depends_on: vec!["architect".to_string()],
                },
            ],
            coordination: CoordinationMode::Sequential,
        }
    }
}

const ARCHITECT_PROMPT: &str = "You are a senior software architect. Given a project description, produce:\n1. System architecture document\n2. Database schema (tables, relations, indexes)\n3. API contract (REST endpoints with request/response types)\n4. Frontend page structure\n5. Tech stack decisions with rationale\nOutput as structured JSON.";

const BACKEND_PROMPT: &str = "You are a backend engineer. Given an architecture document, generate production-quality backend code:\n- API routes with validation\n- Business logic services\n- Database models and migrations\n- Error handling\n- Authentication middleware\nUse Next.js API routes or Express.js.";

const FRONTEND_PROMPT: &str = "You are a frontend engineer. Given an architecture document, generate production-quality React/Next.js code:\n- Page components with routing\n- Reusable UI components using shadcn/ui\n- Data fetching hooks\n- Form handling with validation\n- Responsive design with Tailwind CSS";

const DATABASE_PROMPT: &str = "You are a database engineer. Given an architecture document, generate:\n- SQL migrations (CREATE TABLE with proper types, constraints, indexes)\n- Seed data for development\n- Query optimization recommendations";

const AUTH_PROMPT: &str = "You are an auth engineer. Generate authentication and authorization:\n- NextAuth.js or similar auth setup\n- Login/signup pages\n- Session management\n- Role-based access control\n- API key generation for developer APIs";

const DEVOPS_PROMPT: &str = "You are a DevOps engineer. Generate deployment infrastructure:\n- Dockerfile\n- docker-compose.yml\n- CI/CD pipeline (GitHub Actions)\n- Environment variable templates\n- Health check endpoints";

const QA_PROMPT: &str = "You are a QA engineer. Generate comprehensive tests:\n- Unit tests for business logic\n- Integration tests for API endpoints\n- E2E tests for critical user flows\n- Test fixtures and factories";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_stack_team_has_seven_agents() {
        let team = CodeGenTeam::full_stack();
        assert_eq!(team.agents.len(), 7);
    }

    #[test]
    fn full_stack_team_architect_has_no_dependencies() {
        let team = CodeGenTeam::full_stack();
        let architect = team.agents.iter().find(|a| a.id == "architect").unwrap();
        assert!(architect.depends_on.is_empty());
        assert_eq!(architect.role, AgentRole::Architect);
    }

    #[test]
    fn full_stack_team_uses_dag_coordination() {
        let team = CodeGenTeam::full_stack();
        match &team.coordination {
            CoordinationMode::DagBased { dependencies } => {
                assert_eq!(dependencies.len(), 8);
            }
            _ => panic!("Expected DagBased coordination"),
        }
    }

    #[test]
    fn minimal_team_has_two_agents() {
        let team = CodeGenTeam::minimal();
        assert_eq!(team.agents.len(), 2);
    }

    #[test]
    fn minimal_team_uses_sequential_coordination() {
        let team = CodeGenTeam::minimal();
        assert!(matches!(team.coordination, CoordinationMode::Sequential));
    }

    #[test]
    fn full_stack_agents_have_correct_dependency_chain() {
        let team = CodeGenTeam::full_stack();
        let backend = team.agents.iter().find(|a| a.id == "backend").unwrap();
        assert!(backend.depends_on.contains(&"architect".to_string()));

        let auth = team.agents.iter().find(|a| a.id == "auth").unwrap();
        assert!(auth.depends_on.contains(&"backend".to_string()));

        let devops = team.agents.iter().find(|a| a.id == "devops").unwrap();
        assert!(devops.depends_on.contains(&"backend".to_string()));
        assert!(devops.depends_on.contains(&"frontend".to_string()));
    }

    #[test]
    fn team_serialization_roundtrip() {
        let team = CodeGenTeam::full_stack();
        let json = serde_json::to_string(&team).unwrap();
        let deserialized: CodeGenTeam = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.agents.len(), team.agents.len());
    }

    #[test]
    fn agent_role_serializes_to_snake_case() {
        let role = AgentRole::FrontendEngineer;
        let json = serde_json::to_string(&role).unwrap();
        assert_eq!(json, "\"frontend_engineer\"");
    }
}
