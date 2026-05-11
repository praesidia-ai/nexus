//! `nexus-core` — the single embed surface for Nexus.
//!
//! Import this one crate to get everything: agent orchestration,
//! workflow pipelines, Praesidia policy + publishing, and SQLite persistence.
//!
//! # Architecture
//!
//! ```text
//! nexus-core
//! ├── nexus-zeroclaw  → Agent layer (AgentPool, AgentName, NexusAgent, …)
//! ├── workflow_dag    → Workflow DAG engine + predefined pipelines (embedded)
//! ├── nexus-store     → SQLite persistence (projects, knowledge, settings)
//! └── praesidia-nexus → Policy engine + Praesidia cloud client
//! ```

pub mod app_state;
pub mod ir;
pub mod nexus_state;
pub mod prompt;
pub mod workflow_dag;

// Top-level re-exports — everything a Tauri host or CLI needs:

pub use app_state::NexusState;

// Agent layer
pub use nexus_zeroclaw::{
    AgentMessage, AgentName, AgentPool, AgentReply, AgentRole, ChatMessage, ContextSnippet,
    MessageKind, NexusAgent, RosterConfig,
};

// Workflow layer (DAG-based pipelines, embedded from the former nexus-workflow crate)
pub use workflow_dag::{
    app_build_dag, code_review_dag, deploy_dag, feature_dag, pipeline_by_name, security_audit_dag,
    StepId, StepOutput, WorkflowContext, WorkflowDag, WorkflowResult, WorkflowRunner, WorkflowStep,
    PIPELINE_NAMES,
};

// Praesidia layer
pub use praesidia_nexus::{
    clear_runtime, eval_egress, eval_ingress, eval_tool,
    get_agent_url, list_registered_agents, load_runtime, load_runtime_at,
    parse_policy, ping_agent, runtime_file_at_base, save_runtime, sha256_hex,
    HealthResponse, PraesidiaClient, PraesidiaPolicy, PraesidiaPolicyHook,
    PublishProjectRequest, PublishResult, RegisterAgentRequest,
    ZeroClawAgentEntry, ZeroClawRegistration,
};

// Store layer
pub use nexus_store::{
    AgentBuilder, AgentDefinition, AgentDefinitionInput, AppInstance, AppRunnerService,
    Conversation, EntitySchema, FieldDef, KnowledgeItem, KnowledgeItemType, KnowledgeService,
    MaterializedTable, NexusMessage, NexusSessionService, NewKnowledgeItem,
    PortalBuilder, Project, ProjectService, SchemaBuilder, SessionState,
};

// Metrics + self-improvement layer
pub use nexus_store::{
    AnalyticsService, Feedback, FeedbackService, ImprovementSignal, MetricsService,
    NewFeedback, RunMetric, StepMetric,
};

// Code generation materializer
pub use nexus_store::{
    build_generation_prompt, detect_tech_stack, is_web_stack, parse_file_blocks,
    AppContext, CodeGenMaterializer, CodeGenPlan, CodeGenResult, PlannedAgent, PlannedField,
    PlannedFile, PlannedTable, ValidationResult,
};

// Codegen manifest (incremental codegen + schema migrations)
pub use nexus_store::{CodegenManifest, FileManifestEntry, SchemaVersion};

// Nexus-state JSON blocks (used by Tauri UI layer)
pub use nexus_state::{
    parse_nexus_state, strip_nexus_state, CreateAgentPayload, CreateTablePayload,
    FieldSpec, NexusAction, NexusItem, NexusState as NexusStateBlock,
};

pub use prompt::NEXUS_SYSTEM_PROMPT;

// IR layer — the system brain
pub use ir::{
    IrAgent, IrAgentLimits, IrApi, IrCode, IrCompiler, IrEntity, IrField, IrFile,
    IrIntent, IrMeta, IrPage, IrUi, IrValidationError, IrValidator, IrWorkflow,
    IrWorkflowStep, NexusIr,
};
