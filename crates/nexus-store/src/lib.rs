pub mod agent_skills;
pub mod generation_lock;
pub mod app_runner;
pub mod background_agents;
pub mod business;
pub mod cache;
pub mod codegen;
pub mod crypto;
pub mod collaboration;
pub mod codegen_manifest;
pub mod coding_tasks;
pub mod credits;
mod db;
mod error;
pub mod global_memory;
pub mod knowledge;
pub mod marketplace;
pub mod materialization;
pub mod metrics;
pub mod observability;
pub mod pool;
pub mod portal;
pub mod projects;
mod service;
pub mod team_events;
pub mod templates;
pub mod user_prefs;
pub mod cost_records;

pub use app_runner::{AppInstance, AppRunnerService, BuildStep, EnvVar};
pub use coding_tasks::{CodingTask, CodingTaskService, NewCodingTask};
pub use db::{open_connection, run_migrations};
pub use pool::{ConnectionPool, ReaderGuard};
pub use error::{Result, StoreError};
pub use knowledge::{KnowledgeItem, KnowledgeItemType, KnowledgeService, NewKnowledgeItem};
pub use materialization::{
    AgentBuilder, AgentDefinition, AgentDefinitionInput, EntitySchema, FieldDef,
    MaterializedTable, PortalBuilder, SchemaBuilder,
};
pub use codegen::{
    build_enhanced_generation_prompt, build_generation_prompt, detect_tech_stack, is_web_stack,
    parse_file_blocks, AppContext, CodeGenMaterializer, CodeGenPlan, CodeGenResult, PlannedAgent,
    PlannedField, PlannedFile, PlannedTable, ValidationResult,
};
pub use codegen_manifest::{CodegenManifest, FileManifestEntry, SchemaVersion};
pub use metrics::{
    AnalyticsService, Feedback, FeedbackService, ImprovementSignal, MetricsService, NewFeedback,
    RunMetric, StepMetric,
};
pub use portal::{Portal, PortalService, VaultEntry, VaultService, WorkflowRun, WorkflowRunService};
pub use projects::{Conversation, NexusMessage, Project, ProjectService};
pub use agent_skills::{AgentSkill, AgentSkillService, NewAgentSkill, SkillAssignment, SkillExample};
pub use observability::{ObservabilityService, AgentTrace, ToolLog, AuditEntry, EvolutionEntry, HealingEvent};
pub use global_memory::{GlobalMemoryService, MemoryEntry};
pub use background_agents::{BackgroundAgentService, BackgroundAgent};
pub use generation_lock::GenerationLockService;
pub use marketplace::{MarketplaceService, MarketplaceItem};
pub use business::{BusinessService, BusinessTeamInstance, BusinessEvent};
pub use collaboration::{CollabService, CollabSession, Participant, Activity};
pub use credits::{CreditAccount, CreditService, CreditUsage};
pub use templates::{StarterTemplate, TemplateService};
pub use user_prefs::{UserPreferences, UserPrefsService};
pub use service::{
    ArtifactRow, DecisionRow, MaterializeResult, MessageRow, NexusSessionService, SessionState,
};

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::tempdir;

    use super::*;
    use crate::knowledge::KnowledgeService;
    use crate::materialization::{AgentBuilder, AgentDefinitionInput, EntitySchema, FieldDef,
        PortalBuilder, SchemaBuilder};
    use crate::projects::ProjectService;

    fn open_db(dir: &tempfile::TempDir) -> rusqlite::Connection {
        let conn = open_connection(&dir.path().join("n.db")).unwrap();
        conn
    }

    // -----------------------------------------------------------------------
    // Legacy session service
    // -----------------------------------------------------------------------

    #[test]
    fn materialize_writes_yaml_files() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("n.db");
        let proj = dir.path().join("project");
        let svc = NexusSessionService::open(&db_path).unwrap();
        let sid = svc.ensure_session(None, Some("Test")).unwrap();
        svc.append_message(&sid, "user", "I want a booking app for salons")
            .unwrap();
        let r = svc
            .materialize_agent_artifacts(&sid, &proj, "salon-booking", Some("Salon bookings MVP"))
            .unwrap();
        assert!(PathBuf::from(&r.agent_yaml_path).is_file());
        assert!(PathBuf::from(&r.policy_yaml_path).is_file());
        let state = svc.get_session_state(&sid).unwrap();
        assert!(state.artifacts.iter().any(|a| a.kind == "agent_yaml"));
    }

    #[test]
    fn export_project_copies_committed_files() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("n.db");
        let proj = dir.path().join("project");
        let exp = dir.path().join("export_out");
        let svc = NexusSessionService::open(&db_path).unwrap();
        let sid = svc.ensure_session(None, None).unwrap();
        svc.materialize_agent_artifacts(&sid, &proj, "x", None)
            .unwrap();
        let paths = svc.export_project(&sid, &exp).unwrap();
        assert!(!paths.is_empty());
        assert!(exp.join("neuro.agent.yaml").is_file());
    }

    // -----------------------------------------------------------------------
    // ProjectService
    // -----------------------------------------------------------------------

    #[test]
    fn project_create_and_list() {
        let dir = tempdir().unwrap();
        let conn = open_db(&dir);
        let svc = ProjectService::new(&conn);

        let p = svc.create_project("Freelance Tracker", Some("Track clients and invoices"), "default").unwrap();
        assert_eq!(p.name, "Freelance Tracker");
        assert_eq!(p.phase, 1);

        let list = svc.list_projects().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, p.id);
    }

    #[test]
    fn project_phase_update() {
        let dir = tempdir().unwrap();
        let conn = open_db(&dir);
        let svc = ProjectService::new(&conn);
        let p = svc.create_project("Test", None, "default").unwrap();
        svc.update_project_phase(&p.id, 3).unwrap();
        let got = svc.get_project(&p.id).unwrap().unwrap();
        assert_eq!(got.phase, 3);
    }

    #[test]
    fn conversation_and_messages() {
        let dir = tempdir().unwrap();
        let conn = open_db(&dir);
        let svc = ProjectService::new(&conn);
        let p = svc.create_project("P", None, "default").unwrap();
        let conv = svc.create_conversation(&p.id).unwrap();

        svc.append_nexus_message(&conv.id, "user", "Hello", None).unwrap();
        svc.append_nexus_message(
            &conv.id,
            "assistant",
            "Let me help you",
            Some("Let me help you<nexus_state>{}</nexus_state>"),
        ).unwrap();

        let msgs = svc.list_messages(&conv.id).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[1].role, "assistant");
        assert!(msgs[1].raw_content.as_deref().unwrap().contains("<nexus_state>"));
    }

    #[test]
    fn settings_get_set() {
        let dir = tempdir().unwrap();
        let conn = open_db(&dir);
        let svc = ProjectService::new(&conn);

        assert!(svc.get_setting("llm.provider").unwrap().is_none());
        svc.set_setting("llm.provider", "anthropic").unwrap();
        assert_eq!(svc.get_setting("llm.provider").unwrap().as_deref(), Some("anthropic"));

        // Upsert
        svc.set_setting("llm.provider", "openai").unwrap();
        assert_eq!(svc.get_setting("llm.provider").unwrap().as_deref(), Some("openai"));
    }

    // -----------------------------------------------------------------------
    // KnowledgeService
    // -----------------------------------------------------------------------

    #[test]
    fn knowledge_add_and_list() {
        let dir = tempdir().unwrap();
        let conn = open_db(&dir);
        let psvc = ProjectService::new(&conn);
        let p = psvc.create_project("K", None, "default").unwrap();

        let ksvc = KnowledgeService::new(&conn);
        let item = ksvc.add_item(&p.id, &crate::knowledge::NewKnowledgeItem {
            item_type: "entity".into(),
            name: "Client".into(),
            description: Some("A paying customer".into()),
            icon: Some("👤".into()),
            metadata: None,
        }).unwrap();

        assert_eq!(item.item_type, "entity");

        let list = ksvc.list_items(&p.id).unwrap();
        assert_eq!(list.len(), 1);

        let by_type = ksvc.list_by_type(&p.id, "agent").unwrap();
        assert!(by_type.is_empty());
    }

    #[test]
    fn knowledge_bulk_insert() {
        let dir = tempdir().unwrap();
        let conn = open_db(&dir);
        let psvc = ProjectService::new(&conn);
        let p = psvc.create_project("Bulk", None, "default").unwrap();
        let ksvc = KnowledgeService::new(&conn);

        let items = vec![
            crate::knowledge::NewKnowledgeItem { item_type: "entity".into(), name: "Invoice".into(), description: None, icon: None, metadata: None },
            crate::knowledge::NewKnowledgeItem { item_type: "agent".into(), name: "Billing Agent".into(), description: None, icon: Some("🤖".into()), metadata: None },
        ];
        let inserted = ksvc.add_items(&p.id, &items).unwrap();
        assert_eq!(inserted.len(), 2);
        assert_eq!(ksvc.list_items(&p.id).unwrap().len(), 2);
    }

    // -----------------------------------------------------------------------
    // SchemaBuilder
    // -----------------------------------------------------------------------

    #[test]
    fn schema_builder_creates_table_in_project_db() {
        let dir = tempdir().unwrap();
        let conn = open_db(&dir);
        let psvc = ProjectService::new(&conn);
        let p = psvc.create_project("SchemaTest", None, "default").unwrap();

        let project_db = dir.path().join("projects").join(&p.id).join("data.db");
        let sb = SchemaBuilder::new(&conn);

        let schema = EntitySchema {
            entity_name: "Invoice".into(),
            fields: vec![
                FieldDef { name: "id".into(), r#type: "TEXT".into(), primary_key: true, not_null: false },
                FieldDef { name: "amount".into(), r#type: "REAL".into(), primary_key: false, not_null: true },
            ],
        };

        let mt = sb.materialize_table(&p.id, &project_db, &schema).unwrap();
        assert_eq!(mt.table_name, "Invoice");
        assert!(project_db.is_file());

        let tables = sb.list_tables(&p.id).unwrap();
        assert_eq!(tables.len(), 1);
    }

    #[test]
    fn schema_builder_sanitizes_bad_identifier() {
        let dir = tempdir().unwrap();
        let conn = open_db(&dir);
        let psvc = ProjectService::new(&conn);
        let p = psvc.create_project("S2", None, "default").unwrap();

        let project_db = dir.path().join("projects").join(&p.id).join("data.db");
        let sb = SchemaBuilder::new(&conn);

        // Entity names with spaces and dashes should be sanitised
        let schema = EntitySchema {
            entity_name: "My Entity-Table!".into(),
            fields: vec![
                FieldDef { name: "id".into(), r#type: "TEXT".into(), primary_key: true, not_null: false },
            ],
        };
        let mt = sb.materialize_table(&p.id, &project_db, &schema).unwrap();
        assert!(!mt.table_name.contains(' '));
        assert!(!mt.table_name.contains('-'));
    }

    // -----------------------------------------------------------------------
    // AgentBuilder
    // -----------------------------------------------------------------------

    #[test]
    fn agent_builder_writes_yaml() {
        let dir = tempdir().unwrap();
        let conn = open_db(&dir);
        let psvc = ProjectService::new(&conn);
        let p = psvc.create_project("AgentTest", None, "default").unwrap();

        let agents_dir = dir.path().join("agents");
        let ab = AgentBuilder::new(&conn);

        let def = AgentDefinitionInput {
            name: "Intake Agent".into(),
            role: "Handles new client onboarding".into(),
            tools: vec!["query_database".into(), "send_email".into()],
            memory_type: "persistent".into(),
            provider: "anthropic".into(),
            model: "claude-sonnet-4-6".into(),
            system_prompt: "You are a friendly intake agent.".into(),
        };

        let agent = ab.materialize_agent(&p.id, &agents_dir, &def).unwrap();
        assert_eq!(agent.status, "defined");

        // YAML file was written
        let yaml_path = agents_dir.join(format!("{}.yaml", agent.id));
        assert!(yaml_path.is_file());
        let yaml_content = std::fs::read_to_string(&yaml_path).unwrap();
        assert!(yaml_content.contains("Intake Agent"));
        assert!(yaml_content.contains("query_database"));

        // Listed back from DB
        let list = ab.list_agents(&p.id).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].tools, vec!["query_database", "send_email"]);
    }

    // -----------------------------------------------------------------------
    // PortalBuilder
    // -----------------------------------------------------------------------

    #[test]
    fn portal_builder_scaffolds_files() {
        let dir = tempdir().unwrap();
        let portal_dir = dir.path().join("portal");
        PortalBuilder::scaffold(&portal_dir, "My Biz", "Support Agent").unwrap();
        assert!(portal_dir.join("index.html").is_file());
        assert!(portal_dir.join("style.css").is_file());
        assert!(portal_dir.join("app.js").is_file());

        let html = std::fs::read_to_string(portal_dir.join("index.html")).unwrap();
        assert!(html.contains("My Biz"));
    }

    // -----------------------------------------------------------------------
    // Migration idempotency
    // -----------------------------------------------------------------------

    #[test]
    fn migrations_are_idempotent() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("idem.db");
        // Run twice — should not error or create duplicates
        let c1 = open_connection(&path).unwrap();
        drop(c1);
        let c2 = open_connection(&path).unwrap();
        let v: i64 = c2.query_row(
            "SELECT MAX(version) FROM schema_migrations",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(v, 9);
    }
}
