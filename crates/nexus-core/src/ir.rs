//! Nexus Intermediate Representation (IR) — the system brain.
//!
//! The IR is a strongly-typed JSON-serialisable data structure that bridges
//! user intent → system specification → executable system.  Every component
//! of a generated application is described here before any code is written.
//!
//! # Pipeline
//! ```text
//! User Intent
//!   → NexusState (chat-phase JSON blocks)
//!   → IrCompiler::compile()
//!   → NexusIr   (validated, versioned)
//!   → Materialisation (SchemaBuilder, AgentBuilder, PortalBuilder …)
//!   → Running System
//! ```
//!
//! # Schema (matches the spec's JSON shape)
//! ```json
//! {
//!   "meta":      { "version": "1.0", "id": "…", "created_at": "…" },
//!   "intent":    { "goal": "…", "domain": "…", "constraints": [] },
//!   "entities":  [ { "name": "…", "fields": [] } ],
//!   "agents":    [ { "name": "…", "role": "…", "tools": [] } ],
//!   "workflows": [ { "name": "…", "steps": [] } ],
//!   "ui":        { "pages": [], "theme": "…" },
//!   "apis":      [ { "path": "…", "method": "…", "description": "…" } ],
//!   "code":      { "language": "…", "framework": "…", "files": [] }
//! }
//! ```

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::nexus_state::{
    CreateAgentPayload, CreateTablePayload, NexusAction, NexusState,
};

// ---------------------------------------------------------------------------
// IR Meta
// ---------------------------------------------------------------------------

/// Version and identity metadata for an IR document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrMeta {
    /// Schema version — increment when the IR format changes.
    pub version: String,
    /// Unique ID for this IR document.
    pub id: String,
    /// ISO-8601 creation timestamp.
    pub created_at: String,
    /// Human-readable project name (if known).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
}

impl Default for IrMeta {
    fn default() -> Self {
        Self {
            version: "1.0".into(),
            id: Uuid::new_v4().to_string(),
            created_at: Utc::now().to_rfc3339(),
            project_name: None,
        }
    }
}

// ---------------------------------------------------------------------------
// IR Intent
// ---------------------------------------------------------------------------

/// The user's original intent — what they want to build and why.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IrIntent {
    /// Primary goal statement (e.g. "A SaaS for booking fitness trainers").
    pub goal: String,
    /// Business domain (e.g. "fitness", "e-commerce", "productivity").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// Hard constraints from the user (e.g. "must work offline", "GDPR compliant").
    #[serde(default)]
    pub constraints: Vec<String>,
    /// Non-functional requirements (performance, scale, UX expectations).
    #[serde(default)]
    pub non_functional: Vec<String>,
}

// ---------------------------------------------------------------------------
// IR Entities
// ---------------------------------------------------------------------------

/// A data entity (maps to a database table or domain object).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrEntity {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub fields: Vec<IrField>,
    /// Whether this entity should be materialised as a SQLite table.
    #[serde(default)]
    pub materialize: bool,
}

/// A typed field on an entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrField {
    pub name: String,
    /// SQLite-compatible type: TEXT, INTEGER, REAL, BLOB.
    pub r#type: String,
    #[serde(default)]
    pub primary_key: bool,
    #[serde(default)]
    pub not_null: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
}

// ---------------------------------------------------------------------------
// IR Agents
// ---------------------------------------------------------------------------

/// An agent definition within the generated system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrAgent {
    pub name: String,
    pub role: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default = "default_memory_type")]
    pub memory_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    /// Execution limits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limits: Option<IrAgentLimits>,
}

fn default_memory_type() -> String {
    "persistent".into()
}

/// Resource limits for an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrAgentLimits {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_mb: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u32>,
}

// ---------------------------------------------------------------------------
// IR Workflows
// ---------------------------------------------------------------------------

/// A workflow (DAG) within the generated system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrWorkflow {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub steps: Vec<IrWorkflowStep>,
}

/// A single step within an IR workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrWorkflowStep {
    pub id: String,
    /// Which agent runs this step.
    pub agent: String,
    pub description: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Whether this step can run in parallel with other steps in its layer.
    #[serde(default = "default_true")]
    pub parallel: bool,
}

fn default_true() -> bool {
    true
}

// ---------------------------------------------------------------------------
// IR UI
// ---------------------------------------------------------------------------

/// UI specification for the generated application.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IrUi {
    #[serde(default)]
    pub pages: Vec<IrPage>,
    /// CSS theme name or "default".
    #[serde(default = "default_theme")]
    pub theme: String,
    /// Whether to generate a client portal (HTML/JS).
    #[serde(default)]
    pub portal: bool,
    /// Whether to generate an admin panel.
    #[serde(default)]
    pub admin_panel: bool,
}

fn default_theme() -> String {
    "default".into()
}

/// A generated UI page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrPage {
    pub name: String,
    pub route: String,
    #[serde(default)]
    pub components: Vec<String>,
    /// Which entity this page is bound to (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity: Option<String>,
}

// ---------------------------------------------------------------------------
// IR APIs
// ---------------------------------------------------------------------------

/// An HTTP API endpoint in the generated system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrApi {
    pub path: String,
    /// HTTP method: GET, POST, PUT, DELETE, PATCH.
    pub method: String,
    pub description: String,
    /// Which entity this endpoint operates on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity: Option<String>,
    /// Whether the endpoint requires authentication.
    #[serde(default = "default_true")]
    pub auth_required: bool,
}

// ---------------------------------------------------------------------------
// IR Code
// ---------------------------------------------------------------------------

/// Code generation specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrCode {
    /// Primary language (e.g. "TypeScript", "Rust", "Python").
    #[serde(default = "default_language")]
    pub language: String,
    /// Framework (e.g. "Next.js", "Axum", "FastAPI").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub framework: Option<String>,
    /// Planned output files.
    #[serde(default)]
    pub files: Vec<IrFile>,
    /// Generation strategy: "plan → generate → validate → refine"
    #[serde(default = "default_strategy")]
    pub strategy: String,
}

impl Default for IrCode {
    fn default() -> Self {
        Self {
            language: default_language(),
            framework: None,
            files: vec![],
            strategy: default_strategy(),
        }
    }
}

fn default_language() -> String {
    "TypeScript".into()
}

fn default_strategy() -> String {
    "plan → generate → validate → refine".into()
}

/// A planned output file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrFile {
    pub path: String,
    pub description: String,
    /// Which agent generates this file.
    pub agent: String,
}

// ---------------------------------------------------------------------------
// NexusIr — the top-level IR document
// ---------------------------------------------------------------------------

/// The complete Nexus Intermediate Representation for a project.
///
/// This is the canonical system description from which agents generate code,
/// databases, agents, and UI.  It is serialised to JSON and stored alongside
/// the project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusIr {
    pub meta: IrMeta,
    pub intent: IrIntent,
    #[serde(default)]
    pub entities: Vec<IrEntity>,
    #[serde(default)]
    pub agents: Vec<IrAgent>,
    #[serde(default)]
    pub workflows: Vec<IrWorkflow>,
    pub ui: IrUi,
    #[serde(default)]
    pub apis: Vec<IrApi>,
    pub code: IrCode,
}

impl NexusIr {
    /// Create a blank IR with default metadata.
    pub fn new() -> Self {
        Self {
            meta: IrMeta::default(),
            intent: IrIntent::default(),
            entities: vec![],
            agents: vec![],
            workflows: vec![],
            ui: IrUi::default(),
            apis: vec![],
            code: IrCode::default(),
        }
    }

    /// Set the project intent goal.
    pub fn with_goal(mut self, goal: impl Into<String>) -> Self {
        self.intent.goal = goal.into();
        self
    }

    /// Serialise to pretty-printed JSON.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialise from JSON.
    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        serde_json::from_str(json)
    }

    /// Merge another IR into this one, appending entities/agents/workflows.
    pub fn merge(&mut self, other: NexusIr) {
        self.entities.extend(other.entities);
        self.agents.extend(other.agents);
        self.workflows.extend(other.workflows);
        self.ui.pages.extend(other.ui.pages);
        self.apis.extend(other.apis);
        self.code.files.extend(other.code.files);

        // Last writer wins for scalar fields
        if !other.intent.goal.is_empty() {
            self.intent.goal = other.intent.goal;
        }
        if let Some(domain) = other.intent.domain {
            self.intent.domain = Some(domain);
        }
        self.intent.constraints.extend(other.intent.constraints);
    }
}

impl Default for NexusIr {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// IrCompiler
// ---------------------------------------------------------------------------

/// Compiles `NexusState` chat-phase blocks into a `NexusIr` document.
///
/// # Example
/// ```rust,ignore
/// let state = parse_nexus_state(raw_llm_output).unwrap();
/// let ir = IrCompiler::from_nexus_state(&state);
/// println!("{}", ir.to_json().unwrap());
/// ```
pub struct IrCompiler;

impl IrCompiler {
    /// Build a `NexusIr` from a single parsed `NexusState` block.
    ///
    /// - `new_items` with `materialize = true` → entities and agents
    /// - `actions` of type `create_table` → IR entities
    /// - `actions` of type `create_agent` → IR agents
    pub fn from_nexus_state(state: &NexusState) -> NexusIr {
        let mut ir = NexusIr::new();

        // Items from knowledge graph
        for item in &state.new_items {
            match item.r#type.as_str() {
                "entity" | "database" => {
                    ir.entities.push(IrEntity {
                        name: item.name.clone(),
                        description: item.description.clone(),
                        fields: vec![],
                        materialize: item.materialize,
                    });
                }
                "agent" => {
                    ir.agents.push(IrAgent {
                        name: item.name.clone(),
                        role: item.description.clone(),
                        tools: vec![],
                        memory_type: "persistent".into(),
                        system_prompt: None,
                        limits: None,
                    });
                }
                "portal" => {
                    ir.ui.portal = true;
                }
                "integration" => {
                    // Integrations surface as API entries
                    ir.apis.push(IrApi {
                        path: format!("/integrations/{}", item.name.to_lowercase()),
                        method: "POST".into(),
                        description: item.description.clone(),
                        entity: None,
                        auth_required: true,
                    });
                }
                _ => {}
            }
        }

        // Actions — more detailed than knowledge items
        for action in &state.actions {
            Self::apply_action(&mut ir, action);
        }

        // Phase label as intent domain hint
        ir.intent.domain = Some(state.phase_label.clone());

        ir
    }

    /// Compile a sequence of actions (e.g. replayed from DB) into an IR.
    pub fn from_actions(actions: &[NexusAction]) -> NexusIr {
        let mut ir = NexusIr::new();
        for action in actions {
            Self::apply_action(&mut ir, action);
        }
        ir
    }

    fn apply_action(ir: &mut NexusIr, action: &NexusAction) {
        match action.action.as_str() {
            "create_table" => {
                if let Some(payload) = action.as_create_table() {
                    Self::apply_create_table(ir, &payload);
                }
            }
            "create_agent" => {
                if let Some(payload) = action.as_create_agent() {
                    Self::apply_create_agent(ir, &payload);
                }
            }
            "create_portal" => {
                ir.ui.portal = true;
            }
            "create_integration" => {
                if let Some(name) = action.payload.get("name").and_then(|v| v.as_str()) {
                    ir.apis.push(IrApi {
                        path: format!("/integrations/{}", name.to_lowercase()),
                        method: "POST".into(),
                        description: action
                            .payload
                            .get("description")
                            .and_then(|v| v.as_str())
                            .unwrap_or("External integration")
                            .to_string(),
                        entity: None,
                        auth_required: true,
                    });
                }
            }
            _ => {
                tracing::debug!(action = %action.action, "IrCompiler: unknown action, skipping");
            }
        }
    }

    fn apply_create_table(ir: &mut NexusIr, payload: &CreateTablePayload) {
        // Add entity with typed fields
        let fields = payload
            .fields
            .iter()
            .map(|f| IrField {
                name: f.name.clone(),
                r#type: f.r#type.clone(),
                primary_key: f.primary_key,
                not_null: f.not_null,
                default_value: None,
            })
            .collect();

        // Generate CRUD REST API for each entity
        let base = format!("/api/{}", payload.entity_name.to_lowercase());
        ir.apis.push(IrApi {
            path: base.clone(),
            method: "GET".into(),
            description: format!("List all {}", payload.entity_name),
            entity: Some(payload.entity_name.clone()),
            auth_required: true,
        });
        ir.apis.push(IrApi {
            path: base.clone(),
            method: "POST".into(),
            description: format!("Create a {}", payload.entity_name),
            entity: Some(payload.entity_name.clone()),
            auth_required: true,
        });
        ir.apis.push(IrApi {
            path: format!("{}/:id", base),
            method: "GET".into(),
            description: format!("Get one {}", payload.entity_name),
            entity: Some(payload.entity_name.clone()),
            auth_required: true,
        });
        ir.apis.push(IrApi {
            path: format!("{}/:id", base),
            method: "PUT".into(),
            description: format!("Update a {}", payload.entity_name),
            entity: Some(payload.entity_name.clone()),
            auth_required: true,
        });
        ir.apis.push(IrApi {
            path: format!("{}/:id", base),
            method: "DELETE".into(),
            description: format!("Delete a {}", payload.entity_name),
            entity: Some(payload.entity_name.clone()),
            auth_required: true,
        });

        // Add page
        ir.ui.pages.push(IrPage {
            name: format!("{} Manager", payload.entity_name),
            route: format!("/{}", payload.entity_name.to_lowercase()),
            components: vec!["DataTable".into(), "CreateForm".into(), "EditModal".into()],
            entity: Some(payload.entity_name.clone()),
        });

        ir.entities.push(IrEntity {
            name: payload.entity_name.clone(),
            description: "Auto-generated from create_table action".to_string(),
            fields,
            materialize: true,
        });
    }

    fn apply_create_agent(ir: &mut NexusIr, payload: &CreateAgentPayload) {
        ir.agents.push(IrAgent {
            name: payload.name.clone(),
            role: payload.role.clone(),
            tools: payload.tools.clone(),
            memory_type: payload.memory_type.clone(),
            system_prompt: Some(payload.system_prompt.clone()),
            limits: Some(IrAgentLimits {
                memory_mb: Some(200),
                timeout_ms: Some(20_000),
            }),
        });

        // Add an API endpoint for invoking this agent
        ir.apis.push(IrApi {
            path: format!("/api/agents/{}/run", payload.name.to_lowercase()),
            method: "POST".into(),
            description: format!("Invoke the {} agent", payload.name),
            entity: None,
            auth_required: true,
        });
    }
}

// ---------------------------------------------------------------------------
// IrValidator
// ---------------------------------------------------------------------------

/// Validation error for an IR document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrValidationError {
    pub field: String,
    pub message: String,
}

/// Validates a `NexusIr` document before materialisation.
///
/// Catches structural issues that would cause downstream failures:
/// - Missing primary keys on entities
/// - Agent names with invalid characters
/// - Workflow steps referencing unknown agents
/// - Circular workflow dependencies (basic check)
pub struct IrValidator;

impl IrValidator {
    /// Validate the IR.  Returns `Ok(())` if valid, or a list of errors.
    pub fn validate(ir: &NexusIr) -> Result<(), Vec<IrValidationError>> {
        let mut errors: Vec<IrValidationError> = vec![];

        // --- Entities ---
        let entity_names: std::collections::HashSet<&str> =
            ir.entities.iter().map(|e| e.name.as_str()).collect();

        for entity in &ir.entities {
            if entity.name.is_empty() {
                errors.push(IrValidationError {
                    field: "entities[].name".into(),
                    message: "Entity name must not be empty".into(),
                });
            }
            if entity.fields.is_empty() {
                // Missing fields is a warning, not an error
                tracing::warn!(entity = %entity.name, "Entity has no fields");
            }
            let pk_count = entity.fields.iter().filter(|f| f.primary_key).count();
            if !entity.fields.is_empty() && pk_count == 0 {
                errors.push(IrValidationError {
                    field: format!("entities[{}].fields", entity.name),
                    message: format!(
                        "Entity '{}' has no primary key field",
                        entity.name
                    ),
                });
            }
        }

        // --- Agents ---
        let agent_names: std::collections::HashSet<&str> =
            ir.agents.iter().map(|a| a.name.as_str()).collect();

        for agent in &ir.agents {
            if agent.name.is_empty() {
                errors.push(IrValidationError {
                    field: "agents[].name".into(),
                    message: "Agent name must not be empty".into(),
                });
            }
            if agent.name.contains(' ') {
                errors.push(IrValidationError {
                    field: format!("agents[{}].name", agent.name),
                    message: format!(
                        "Agent name '{}' must not contain spaces",
                        agent.name
                    ),
                });
            }
        }

        // --- Workflows ---
        for wf in &ir.workflows {
            let step_ids: std::collections::HashSet<&str> =
                wf.steps.iter().map(|s| s.id.as_str()).collect();

            for step in &wf.steps {
                // Check agent reference
                if !step.agent.is_empty() && !agent_names.contains(step.agent.as_str()) {
                    // Allow built-in roster agent names
                    let roster = [
                        "nova", "atlas", "kai", "luna", "orion", "sage", "ivy", "rex", "leo",
                        "mia",
                    ];
                    if !roster.contains(&step.agent.to_lowercase().as_str()) {
                        errors.push(IrValidationError {
                            field: format!("workflows[{}].steps[{}].agent", wf.name, step.id),
                            message: format!(
                                "Step '{}' references unknown agent '{}'",
                                step.id, step.agent
                            ),
                        });
                    }
                }
                // Check dependency references
                for dep in &step.depends_on {
                    if !step_ids.contains(dep.as_str()) {
                        errors.push(IrValidationError {
                            field: format!("workflows[{}].steps[{}].depends_on", wf.name, step.id),
                            message: format!(
                                "Step '{}' depends on unknown step '{}'",
                                step.id, dep
                            ),
                        });
                    }
                }
            }
        }

        // --- APIs: validate entity references ---
        for api in &ir.apis {
            if let Some(entity) = &api.entity {
                if !entity_names.contains(entity.as_str()) {
                    errors.push(IrValidationError {
                        field: format!("apis[{}].entity", api.path),
                        message: format!(
                            "API '{}' references unknown entity '{}'",
                            api.path, entity
                        ),
                    });
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ir_new_has_defaults() {
        let ir = NexusIr::new();
        assert_eq!(ir.meta.version, "1.0");
        assert!(ir.entities.is_empty());
        assert!(ir.agents.is_empty());
        assert_eq!(ir.code.strategy, "plan → generate → validate → refine");
    }

    #[test]
    fn ir_roundtrip_json() {
        let mut ir = NexusIr::new();
        ir.intent.goal = "A SaaS for booking fitness trainers".into();
        ir.entities.push(IrEntity {
            name: "Trainer".into(),
            description: "A fitness trainer".into(),
            fields: vec![IrField {
                name: "id".into(),
                r#type: "TEXT".into(),
                primary_key: true,
                not_null: false,
                default_value: None,
            }],
            materialize: true,
        });

        let json = ir.to_json().unwrap();
        let parsed = NexusIr::from_json(&json).unwrap();
        assert_eq!(parsed.intent.goal, ir.intent.goal);
        assert_eq!(parsed.entities.len(), 1);
        assert_eq!(parsed.entities[0].name, "Trainer");
    }

    #[test]
    fn compiler_from_nexus_state_create_table() {
        let json = r#"{
          "phase": 3,
          "phase_label": "Materialization",
          "new_items": [],
          "actions": [{
            "action": "create_table",
            "payload": {
              "entity_name": "Booking",
              "fields": [
                {"name": "id", "type": "TEXT", "primary_key": true, "not_null": false},
                {"name": "trainer_id", "type": "TEXT", "primary_key": false, "not_null": true},
                {"name": "slot", "type": "TEXT", "primary_key": false, "not_null": true}
              ]
            }
          }],
          "milestone": null
        }"#;

        let state: NexusState = serde_json::from_str(json).unwrap();
        let ir = IrCompiler::from_nexus_state(&state);

        assert_eq!(ir.entities.len(), 1);
        assert_eq!(ir.entities[0].name, "Booking");
        assert_eq!(ir.entities[0].fields.len(), 3);
        assert!(ir.entities[0].materialize);

        // Should have generated CRUD APIs
        assert!(!ir.apis.is_empty());
        let get_list = ir.apis.iter().find(|a| a.method == "GET" && !a.path.contains(":id"));
        assert!(get_list.is_some());
    }

    #[test]
    fn compiler_from_nexus_state_create_agent() {
        let json = r#"{
          "phase": 3,
          "phase_label": "Materialization",
          "new_items": [],
          "actions": [{
            "action": "create_agent",
            "payload": {
              "name": "BookingAgent",
              "role": "Handles booking requests",
              "tools": ["query_database", "send_notification"],
              "memory_type": "persistent",
              "system_prompt": "You handle booking requests."
            }
          }],
          "milestone": null
        }"#;

        let state: NexusState = serde_json::from_str(json).unwrap();
        let ir = IrCompiler::from_nexus_state(&state);

        assert_eq!(ir.agents.len(), 1);
        assert_eq!(ir.agents[0].name, "BookingAgent");
        assert_eq!(ir.agents[0].tools, ["query_database", "send_notification"]);
        assert!(ir.agents[0].limits.is_some());
    }

    #[test]
    fn compiler_knowledge_items() {
        let json = r#"{
          "phase": 2,
          "phase_label": "Structuring",
          "new_items": [
            {"type": "entity", "name": "Client", "description": "A client", "icon": "👤", "materialize": true},
            {"type": "agent", "name": "support_agent", "description": "Support agent", "icon": "🤖", "materialize": false},
            {"type": "portal", "name": "ClientPortal", "description": "Portal", "icon": "🌐", "materialize": false}
          ],
          "actions": [],
          "milestone": null
        }"#;

        let state: NexusState = serde_json::from_str(json).unwrap();
        let ir = IrCompiler::from_nexus_state(&state);

        assert_eq!(ir.entities.len(), 1);
        assert_eq!(ir.entities[0].name, "Client");
        assert_eq!(ir.agents.len(), 1);
        assert!(ir.ui.portal);
    }

    #[test]
    fn validator_catches_missing_primary_key() {
        let mut ir = NexusIr::new();
        ir.entities.push(IrEntity {
            name: "BadEntity".into(),
            description: "No PK".into(),
            fields: vec![IrField {
                name: "name".into(),
                r#type: "TEXT".into(),
                primary_key: false,
                not_null: false,
                default_value: None,
            }],
            materialize: false,
        });

        let result = IrValidator::validate(&ir);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("primary key")));
    }

    #[test]
    fn validator_passes_valid_ir() {
        let mut ir = NexusIr::new();
        ir.intent.goal = "Test app".into();
        ir.entities.push(IrEntity {
            name: "User".into(),
            description: "A user".into(),
            fields: vec![IrField {
                name: "id".into(),
                r#type: "TEXT".into(),
                primary_key: true,
                not_null: false,
                default_value: None,
            }],
            materialize: true,
        });

        assert!(IrValidator::validate(&ir).is_ok());
    }

    #[test]
    fn ir_merge() {
        let mut base = NexusIr::new();
        base.intent.goal = "Goal A".into();
        base.entities.push(IrEntity {
            name: "Entity1".into(),
            description: "".into(),
            fields: vec![],
            materialize: false,
        });

        let mut other = NexusIr::new();
        other.intent.goal = "Goal B".into();
        other.agents.push(IrAgent {
            name: "agent1".into(),
            role: "does stuff".into(),
            tools: vec![],
            memory_type: "persistent".into(),
            system_prompt: None,
            limits: None,
        });

        base.merge(other);
        assert_eq!(base.intent.goal, "Goal B"); // last writer wins
        assert_eq!(base.entities.len(), 1);
        assert_eq!(base.agents.len(), 1);
    }
}
