//! `NexusState` — the shared application state for Tauri and other Rust hosts.
//!
//! Holds everything needed to run Nexus:
//! - The ZeroClaw `AgentPool` (lazily-initialised specialist agents)
//! - The `WorkflowRunner` for executing named pipelines
//! - The SQLite database for project + knowledge persistence
//! - An optional `PraesidiaClient` for cloud publishing
//!
//! # Initialisation
//! ```rust,ignore
//! let state = NexusState::init("~/.nexus").await?;
//! ```
//!
//! # Running a workflow
//! ```rust,ignore
//! let result = state.run_pipeline("app-build",
//!     WorkflowContext::new().with("requirements", "...")).await?;
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::info;

use nexus_zeroclaw::{AgentName, AgentPool, RosterConfig};
use crate::workflow_dag::{WorkflowContext, WorkflowResult, WorkflowRunner, pipeline_by_name};
use praesidia_nexus::PraesidiaClient;

// ---------------------------------------------------------------------------
// NexusState
// ---------------------------------------------------------------------------

/// Central shared state.  Wrap in `Arc` and pass to every handler.
pub struct NexusState {
    /// ZeroClaw agent pool.  Agents are created lazily on first use.
    pub pool: Arc<AgentPool>,

    /// Workflow runner — executes DAG pipelines using `pool`.
    pub workflow: WorkflowRunner,

    /// SQLite connection for project + knowledge persistence.
    pub db: Arc<tokio::sync::Mutex<rusqlite::Connection>>,

    /// Optional Praesidia cloud client (`None` until the user connects).
    pub praesidia: Arc<RwLock<Option<PraesidiaClient>>>,

    /// Root data directory (e.g. `~/.nexus/`).
    pub data_dir: PathBuf,

    /// Active roster configuration (provider, keys, workspace path).
    pub roster_config: RosterConfig,
}

impl NexusState {
    /// Initialise Nexus.
    ///
    /// - Creates `data_dir` if it does not exist.
    /// - Opens the SQLite database and runs migrations.
    /// - Builds the `AgentPool` and `WorkflowRunner` from environment variables.
    pub async fn init(data_dir: impl AsRef<Path>) -> anyhow::Result<Arc<Self>> {
        use nexus_store::{open_connection, run_migrations};

        let data_dir = data_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&data_dir)?;

        // SQLite
        let db_path = data_dir.join("nexus.db");
        let conn = open_connection(&db_path)?;
        run_migrations(&conn)?;
        let db = Arc::new(tokio::sync::Mutex::new(conn));

        // Roster config from environment
        let mut roster_config = RosterConfig::from_env();
        roster_config.workspace_dir = data_dir.join("agents");

        // Agent pool + workflow runner
        let pool = Arc::new(AgentPool::new(roster_config.clone()));
        let workflow = WorkflowRunner::new(pool.clone());

        info!(
            data_dir  = %data_dir.display(),
            provider  = %roster_config.provider,
            "Nexus initialised"
        );

        Ok(Arc::new(Self {
            pool,
            workflow,
            db,
            praesidia: Arc::new(RwLock::new(None)),
            data_dir,
            roster_config,
        }))
    }

    // -----------------------------------------------------------------------
    // Workflow helpers
    // -----------------------------------------------------------------------

    /// Run a named built-in pipeline (e.g. `"app-build"`, `"code-review"`).
    ///
    /// Returns `Err` if the name is not recognised.
    pub async fn run_pipeline(
        &self,
        name: &str,
        ctx: WorkflowContext,
    ) -> anyhow::Result<WorkflowResult> {
        let dag = pipeline_by_name(name)
            .ok_or_else(|| anyhow::anyhow!("Unknown pipeline: '{}'. Available: {}", name,
                crate::workflow_dag::PIPELINE_NAMES.join(", ")))?;
        self.workflow.run(&dag, ctx).await
    }

    /// Send a single task message directly to a named agent.
    pub async fn ask_agent(&self, agent: AgentName, task: &str) -> anyhow::Result<String> {
        self.pool.task(agent, task).await
    }

    // -----------------------------------------------------------------------
    // Praesidia helpers
    // -----------------------------------------------------------------------

    /// Connect (or reconnect) to the Praesidia platform.
    pub async fn connect_praesidia(&self, url: String, api_key: String) {
        *self.praesidia.write().await = Some(PraesidiaClient::new(url, api_key));
    }

    /// Publish a project to Praesidia.
    pub async fn publish_project(
        &self,
        name: &str,
        description: Option<&str>,
    ) -> anyhow::Result<praesidia_nexus::PublishResult> {
        let client = self.praesidia.read().await;
        let client = client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Not connected to Praesidia — call connect_praesidia first"))?;

        client
            .publish_project(&praesidia_nexus::PublishProjectRequest {
                name: name.to_string(),
                description: description.map(String::from),
                agent_yaml: None,
                policy_yaml: None,
            })
            .await
    }

    // -----------------------------------------------------------------------
    // Path helpers
    // -----------------------------------------------------------------------

    /// `{data_dir}/projects/{id}/data.db`
    pub fn project_data_db(&self, project_id: &str) -> PathBuf {
        self.data_dir.join("projects").join(project_id).join("data.db")
    }

    /// `{data_dir}/projects/{id}/agents/`
    pub fn project_agents_dir(&self, project_id: &str) -> PathBuf {
        self.data_dir.join("projects").join(project_id).join("agents")
    }

    /// `{data_dir}/projects/{id}/portal/`
    pub fn project_portal_dir(&self, project_id: &str) -> PathBuf {
        self.data_dir.join("projects").join(project_id).join("portal")
    }
}

impl std::fmt::Debug for NexusState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NexusState")
            .field("data_dir", &self.data_dir)
            .field("provider", &self.roster_config.provider)
            .finish_non_exhaustive()
    }
}
