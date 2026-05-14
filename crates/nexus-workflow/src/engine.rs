//! Workflow execution engine — orchestrates step execution with dependency
//! resolution, parallelism, retry, human approval, and crash recovery.
//!
//! The engine does **not** execute step actions itself (it has no LLM client or
//! agent pool). Instead it manages lifecycle and state transitions. The caller
//! is expected to integrate action execution via callbacks or by polling step
//! status and providing results through the API.
//!
//! For steps that the engine *can* handle natively (e.g. `Command`), it does so.
//! For all others, it transitions the step to `Running` and waits for the caller
//! to call [`WorkflowEngine::complete_step`] or [`WorkflowEngine::fail_step`].

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio::task::JoinSet;
use tracing::{debug, info, warn};

use crate::definition::{
    ErrorPolicy, StepAction, StepCondition, WorkflowDefinition, WorkflowStep,
};
use crate::error::WorkflowError;
use crate::store::WorkflowStore;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Status of an entire workflow instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStatus {
    /// Currently executing steps.
    Running,
    /// Paused (e.g. waiting for human approval).
    Paused,
    /// Blocked on a `HumanApproval` step.
    WaitingForApproval,
    /// All steps completed successfully.
    Completed,
    /// One or more steps failed and the error policy stopped execution.
    Failed,
    /// Cancelled by the operator.
    Cancelled,
}

impl WorkflowStatus {
    /// Convert to a string representation for storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Paused => "paused",
            Self::WaitingForApproval => "waiting_for_approval",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    /// Parse from a stored string.
    pub fn parse(s: &str) -> Self {
        match s {
            "running" => Self::Running,
            "paused" => Self::Paused,
            "waiting_for_approval" => Self::WaitingForApproval,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            _ => Self::Failed,
        }
    }
}

/// Status of an individual step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    /// Not yet started.
    Pending,
    /// Currently executing.
    Running,
    /// Finished successfully.
    Completed,
    /// Failed (after retries exhausted).
    Failed,
    /// Skipped (condition evaluated to false, or workflow cancelled).
    Skipped,
    /// Waiting for human approval.
    WaitingApproval,
    /// Timed out.
    TimedOut,
}

impl StepStatus {
    /// Convert to a string representation for storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
            Self::WaitingApproval => "waiting_approval",
            Self::TimedOut => "timed_out",
        }
    }

    /// Parse from a stored string.
    pub fn parse(s: &str) -> Self {
        match s {
            "pending" => Self::Pending,
            "running" => Self::Running,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "skipped" => Self::Skipped,
            "waiting_approval" => Self::WaitingApproval,
            "timed_out" => Self::TimedOut,
            _ => Self::Failed,
        }
    }
}

/// State of a single step within a workflow instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepState {
    /// Current status.
    pub status: StepStatus,
    /// When the step started executing.
    pub started_at: Option<String>,
    /// When the step finished (success or failure).
    pub completed_at: Option<String>,
    /// Result value (set on success).
    pub result: Option<serde_json::Value>,
    /// Error message (set on failure).
    pub error: Option<String>,
    /// Number of retry attempts so far.
    pub retries: u32,
    /// Number of times this step has been executed (for loop detection).
    #[serde(default)]
    pub execution_count: u32,
}

/// A running (or completed) workflow instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowInstance {
    /// Unique instance ID.
    pub id: String,
    /// ID of the definition this instance was created from.
    pub definition_id: String,
    /// Snapshot of the definition at creation time.
    pub definition: WorkflowDefinition,
    /// Current status.
    pub status: WorkflowStatus,
    /// Per-step state, keyed by step ID.
    pub step_states: HashMap<String, StepState>,
    /// Shared context — steps can read/write values here.
    pub context: serde_json::Value,
    /// When the instance was created.
    pub started_at: String,
    /// Last time any state changed.
    pub updated_at: String,
}

// ---------------------------------------------------------------------------
// Template registry
// ---------------------------------------------------------------------------

/// In-memory template store for workflow definitions.
struct TemplateRegistry {
    templates: HashMap<String, WorkflowDefinition>,
}

impl TemplateRegistry {
    fn new() -> Self {
        Self {
            templates: HashMap::new(),
        }
    }

    fn register(&mut self, def: WorkflowDefinition) {
        self.templates.insert(def.id.clone(), def);
    }

    fn get(&self, id: &str) -> Option<&WorkflowDefinition> {
        self.templates.get(id)
    }
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

/// The workflow execution engine. Thread-safe and cheap to clone (wraps an
/// `Arc` internally).
#[derive(Clone)]
pub struct WorkflowEngine {
    store: Arc<WorkflowStore>,
    /// Guards concurrent mutations to the same instance.
    locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
    /// In-memory template registry.
    templates: Arc<Mutex<TemplateRegistry>>,
}

impl WorkflowEngine {
    /// Create a new engine persisting state under `data_dir`.
    ///
    /// Creates the directory and initialises SQLite tables if needed.
    pub fn new(data_dir: &Path) -> Result<Self, WorkflowError> {
        std::fs::create_dir_all(data_dir)
            .map_err(|e| WorkflowError::Storage(format!("cannot create data dir: {e}")))?;
        let store = WorkflowStore::new(data_dir);
        store.init_tables()?;
        info!(data_dir = %data_dir.display(), "Workflow engine initialised");
        Ok(Self {
            store: Arc::new(store),
            locks: Arc::new(Mutex::new(HashMap::new())),
            templates: Arc::new(Mutex::new(TemplateRegistry::new())),
        })
    }

    // -----------------------------------------------------------------------
    // Validation
    // -----------------------------------------------------------------------

    /// Validate a workflow definition before execution.
    ///
    /// Checks for:
    /// - Duplicate step IDs
    /// - Missing dependency references
    /// - Circular dependencies (cycle detection)
    /// - Valid condition references
    pub fn validate(definition: &WorkflowDefinition) -> Result<(), WorkflowError> {
        let step_ids: HashSet<&str> = definition.steps.iter().map(|s| s.id.as_str()).collect();

        // Check for duplicate step IDs
        if step_ids.len() != definition.steps.len() {
            return Err(WorkflowError::ValidationError(
                "duplicate step IDs found in workflow definition".to_string(),
            ));
        }

        // Check that all dependencies reference existing steps
        for step in &definition.steps {
            for dep in &step.depends_on {
                if !step_ids.contains(dep.as_str()) {
                    return Err(WorkflowError::ValidationError(format!(
                        "step '{}' depends on unknown step '{dep}'",
                        step.id
                    )));
                }
            }

            // Validate condition references
            if let Some(condition) = &step.condition {
                match condition {
                    StepCondition::OnSuccess { step_id }
                    | StepCondition::OnFailure { step_id } => {
                        if !step_ids.contains(step_id.as_str()) {
                            return Err(WorkflowError::ValidationError(format!(
                                "step '{}' condition references unknown step '{step_id}'",
                                step.id
                            )));
                        }
                    }
                    StepCondition::Always | StepCondition::Expression { .. } => {}
                }
            }
        }

        // Cycle detection via topological sort
        detect_cycles(&definition.steps)?;

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Templates & versioning
    // -----------------------------------------------------------------------

    /// Register a workflow definition as a reusable template.
    pub async fn register_template(
        &self,
        definition: WorkflowDefinition,
    ) -> Result<(), WorkflowError> {
        if !definition.template {
            return Err(WorkflowError::ValidationError(
                "definition must have template=true to be registered as a template".to_string(),
            ));
        }
        Self::validate(&definition)?;
        let mut registry = self.templates.lock().await;
        registry.register(definition);
        Ok(())
    }

    /// Instantiate a workflow from a registered template, substituting
    /// parameters into step prompts and configuration.
    ///
    /// Parameter substitution replaces `{{key}}` placeholders in string fields
    /// with values from `params`.
    pub async fn instantiate(
        &self,
        template_id: &str,
        params: serde_json::Value,
    ) -> Result<WorkflowDefinition, WorkflowError> {
        let registry = self.templates.lock().await;
        let template = registry.get(template_id).ok_or_else(|| {
            WorkflowError::NotFound(format!("template '{template_id}' not found"))
        })?;

        let mut def = template.clone();
        def.template = false;
        def.id = format!("{}-{}", template_id, uuid::Uuid::new_v4());

        // Perform parameter substitution in the JSON representation
        let mut json = serde_json::to_value(&def)?;
        substitute_params(&mut json, &params);
        def = serde_json::from_value(json)
            .map_err(|e| WorkflowError::Storage(format!("template substitution error: {e}")))?;

        Ok(def)
    }

    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// Start a new workflow instance from a definition.
    ///
    /// Validates the definition (cycle detection, etc.), initialises all step
    /// states to `Pending`, persists the instance, and begins executing the
    /// first layer of steps (those with no dependencies).
    pub async fn start(
        &self,
        definition: &WorkflowDefinition,
        context: serde_json::Value,
    ) -> Result<WorkflowInstance, WorkflowError> {
        // Validate before starting
        Self::validate(definition)?;

        let now = chrono::Utc::now().to_rfc3339();
        let instance_id = uuid::Uuid::new_v4().to_string();

        let mut step_states = HashMap::new();
        for step in &definition.steps {
            step_states.insert(
                step.id.clone(),
                StepState {
                    status: StepStatus::Pending,
                    started_at: None,
                    completed_at: None,
                    result: None,
                    error: None,
                    retries: 0,
                    execution_count: 0,
                },
            );
            // Also handle Parallel sub-steps
            if let StepAction::Parallel { steps } = &step.action {
                for sub in steps {
                    step_states.insert(
                        sub.id.clone(),
                        StepState {
                            status: StepStatus::Pending,
                            started_at: None,
                            completed_at: None,
                            result: None,
                            error: None,
                            retries: 0,
                            execution_count: 0,
                        },
                    );
                }
            }
        }

        let instance = WorkflowInstance {
            id: instance_id.clone(),
            definition_id: definition.id.clone(),
            definition: definition.clone(),
            status: WorkflowStatus::Running,
            step_states,
            context,
            started_at: now.clone(),
            updated_at: now,
        };

        self.store.save_instance(&instance)?;
        info!(instance_id = %instance_id, definition = %definition.id, version = definition.version, "Workflow started");

        // Execute the first wave of ready steps
        self.advance(&instance_id).await?;

        // Return fresh state
        self.store.load_instance(&instance_id)
    }

    /// Resume a paused or crashed workflow from its last checkpoint.
    ///
    /// Finds all steps that are `Pending` whose dependencies are `Completed`
    /// and begins executing them.
    pub async fn resume(&self, instance_id: &str) -> Result<WorkflowInstance, WorkflowError> {
        let instance = self.store.load_instance(instance_id)?;

        match instance.status {
            WorkflowStatus::Completed => {
                return Err(WorkflowError::AlreadyRunning(format!(
                    "workflow {instance_id} is already completed"
                )));
            }
            WorkflowStatus::Cancelled => {
                return Err(WorkflowError::Cancelled(format!(
                    "workflow {instance_id} was cancelled"
                )));
            }
            _ => {}
        }

        // Mark as running if it was paused/failed
        self.store
            .update_instance_status(instance_id, &WorkflowStatus::Running, &instance.context)?;

        info!(instance_id = %instance_id, "Workflow resumed");
        self.advance(instance_id).await?;
        self.store.load_instance(instance_id)
    }

    /// Approve a `HumanApproval` step, allowing the workflow to continue.
    pub async fn approve(
        &self,
        instance_id: &str,
        step_id: &str,
    ) -> Result<(), WorkflowError> {
        let instance = self.store.load_instance(instance_id)?;

        let state = instance
            .step_states
            .get(step_id)
            .ok_or_else(|| WorkflowError::NotFound(format!("step {step_id} not found")))?;

        if state.status != StepStatus::WaitingApproval {
            return Err(WorkflowError::StepFailed(format!(
                "step {step_id} is not waiting for approval (status: {:?})",
                state.status
            )));
        }

        let now = chrono::Utc::now().to_rfc3339();
        let updated = StepState {
            status: StepStatus::Completed,
            started_at: state.started_at.clone(),
            completed_at: Some(now),
            result: Some(serde_json::json!({"approved": true})),
            error: None,
            retries: 0,
            execution_count: state.execution_count,
        };
        self.store.update_step(instance_id, step_id, &updated)?;
        self.store
            .update_instance_status(instance_id, &WorkflowStatus::Running, &instance.context)?;

        info!(instance_id = %instance_id, step_id = %step_id, "Step approved");

        // Continue executing
        self.advance(instance_id).await?;
        Ok(())
    }

    /// Cancel a running workflow. All pending steps are marked as `Skipped`.
    pub async fn cancel(&self, instance_id: &str) -> Result<(), WorkflowError> {
        let instance = self.store.load_instance(instance_id)?;

        if instance.status == WorkflowStatus::Completed
            || instance.status == WorkflowStatus::Cancelled
        {
            return Err(WorkflowError::Cancelled(format!(
                "workflow {instance_id} is already {:?}",
                instance.status
            )));
        }

        let now = chrono::Utc::now().to_rfc3339();

        // Skip all pending/waiting steps
        for (step_id, state) in &instance.step_states {
            if matches!(
                state.status,
                StepStatus::Pending | StepStatus::WaitingApproval | StepStatus::Running
            ) {
                let skipped = StepState {
                    status: StepStatus::Skipped,
                    started_at: state.started_at.clone(),
                    completed_at: Some(now.clone()),
                    result: None,
                    error: Some("workflow cancelled".to_string()),
                    retries: state.retries,
                    execution_count: state.execution_count,
                };
                self.store.update_step(instance_id, step_id, &skipped)?;
            }
        }

        self.store
            .update_instance_status(instance_id, &WorkflowStatus::Cancelled, &instance.context)?;

        info!(instance_id = %instance_id, "Workflow cancelled");
        Ok(())
    }

    /// Get the current status of a workflow instance.
    pub fn status(&self, instance_id: &str) -> Result<WorkflowInstance, WorkflowError> {
        self.store.load_instance(instance_id)
    }

    /// List all workflow instances.
    pub fn list(&self) -> Result<Vec<WorkflowInstance>, WorkflowError> {
        self.store.list_instances()
    }

    /// Mark a step as completed with a result. This is the primary integration
    /// point — external systems call this after processing a step's action.
    pub async fn complete_step(
        &self,
        instance_id: &str,
        step_id: &str,
        result: serde_json::Value,
    ) -> Result<(), WorkflowError> {
        // Hold the per-instance lock across load → mutate context → persist.
        // Without this, two concurrent completions race on the shared
        // `context.steps` map and the second write loses the first's entry.
        //
        // The lock MUST be released before calling `self.advance()` —
        // `advance_inner` re-acquires the same per-instance lock and
        // `tokio::sync::Mutex` is non-reentrant, so holding it across the
        // advance call deadlocks every Command-step's workflow transition.
        // This was the real bug behind the previously-flaky test_command_execution.
        {
            let lock = self.instance_lock(instance_id).await;
            let _guard = lock.lock().await;

            let now = chrono::Utc::now().to_rfc3339();
            let instance = self.store.load_instance(instance_id)?;

            let state = instance
                .step_states
                .get(step_id)
                .ok_or_else(|| WorkflowError::NotFound(format!("step {step_id} not found")))?;

            let updated = StepState {
                status: StepStatus::Completed,
                started_at: state.started_at.clone(),
                completed_at: Some(now),
                result: Some(result),
                error: None,
                retries: state.retries,
                execution_count: state.execution_count,
            };
            self.store.update_step(instance_id, step_id, &updated)?;

            // Store step output in context for condition evaluation
            if let Some(ref res) = updated.result {
                let mut ctx = instance.context.clone();
                if let serde_json::Value::Object(ref mut map) = ctx {
                    let steps_key = "steps".to_string();
                    let steps_obj = map
                        .entry(steps_key)
                        .or_insert_with(|| serde_json::json!({}));
                    if let serde_json::Value::Object(ref mut steps_map) = steps_obj {
                        steps_map.insert(
                            step_id.to_string(),
                            serde_json::json!({
                                "status": "success",
                                "output": res,
                            }),
                        );
                    }
                }
                self.store
                    .update_instance_status(instance_id, &instance.status, &ctx)?;
            }
        } // <-- guard dropped here; advance below can now re-acquire it.

        // Continue
        self.advance(instance_id).await?;
        Ok(())
    }

    /// Mark a step as failed. Respects the retry policy: if retries remain,
    /// the step is re-queued; otherwise the step is marked `Failed` and the
    /// workflow error policy is applied.
    pub async fn fail_step(
        &self,
        instance_id: &str,
        step_id: &str,
        error_msg: &str,
    ) -> Result<(), WorkflowError> {
        // Same deadlock-avoidance pattern as `complete_step`: do all the
        // mutations under the per-instance lock, decide what follow-up to
        // run (backoff sleep, advance), then drop the lock before doing it.
        // `tokio::sync::Mutex` is non-reentrant, so holding it across either
        // the backoff sleep or `self.advance()` would block any concurrent
        // call into the same instance.
        enum FollowUp {
            BackoffThenAdvance(u64),
            Advance,
            None,
        }
        let follow_up = {
            let lock = self.instance_lock(instance_id).await;
            let _guard = lock.lock().await;

            let now = chrono::Utc::now().to_rfc3339();
            let instance = self.store.load_instance(instance_id)?;

            let step_def = instance.definition.steps.iter().find(|s| s.id == step_id);

            let state = instance
                .step_states
                .get(step_id)
                .ok_or_else(|| WorkflowError::NotFound(format!("step {step_id} not found")))?;

            let max_retries = step_def.map(|s| s.retry.max_retries).unwrap_or(0);
            let new_retries = state.retries + 1;

            if new_retries <= max_retries {
                let retry_state = StepState {
                    status: StepStatus::Pending,
                    started_at: None,
                    completed_at: None,
                    result: None,
                    error: Some(format!("retry {new_retries}/{max_retries}: {error_msg}")),
                    retries: new_retries,
                    execution_count: state.execution_count,
                };
                self.store
                    .update_step(instance_id, step_id, &retry_state)?;
                debug!(
                    instance_id = %instance_id,
                    step_id = %step_id,
                    attempt = new_retries,
                    max = max_retries,
                    "Step queued for retry"
                );

                let backoff_secs = step_def
                    .map(|s| s.retry.backoff_secs * 2u64.pow(new_retries.saturating_sub(1)))
                    .unwrap_or(2);
                FollowUp::BackoffThenAdvance(backoff_secs)
            } else {
                let failed_state = StepState {
                    status: StepStatus::Failed,
                    started_at: state.started_at.clone(),
                    completed_at: Some(now),
                    result: None,
                    error: Some(error_msg.to_string()),
                    retries: new_retries,
                    execution_count: state.execution_count,
                };
                self.store
                    .update_step(instance_id, step_id, &failed_state)?;

                let mut ctx = instance.context.clone();
                if let serde_json::Value::Object(ref mut map) = ctx {
                    let steps_obj = map
                        .entry("steps".to_string())
                        .or_insert_with(|| serde_json::json!({}));
                    if let serde_json::Value::Object(ref mut steps_map) = steps_obj {
                        steps_map.insert(
                            step_id.to_string(),
                            serde_json::json!({
                                "status": "failed",
                                "error": error_msg,
                            }),
                        );
                    }
                }

                warn!(
                    instance_id = %instance_id,
                    step_id = %step_id,
                    error = %error_msg,
                    "Step permanently failed"
                );

                if instance.definition.on_error == ErrorPolicy::FailFast {
                    self.store.update_instance_status(
                        instance_id,
                        &WorkflowStatus::Failed,
                        &ctx,
                    )?;
                    FollowUp::None
                } else {
                    self.store
                        .update_instance_status(instance_id, &instance.status, &ctx)?;
                    FollowUp::Advance
                }
            }
        }; // lock dropped here

        // Lock-free follow-up actions.
        match follow_up {
            FollowUp::BackoffThenAdvance(secs) => {
                tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
                self.advance(instance_id).await?;
            }
            FollowUp::Advance => {
                self.advance(instance_id).await?;
            }
            FollowUp::None => {}
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Internal: advance execution
    // -----------------------------------------------------------------------

    /// Core scheduling loop. Finds all steps that are ready to run and
    /// transitions them. Also detects workflow completion.
    fn advance<'a>(
        &'a self,
        instance_id: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), WorkflowError>> + Send + 'a>>
    {
        Box::pin(self.advance_inner(instance_id))
    }

    /// Get (or lazily create) the per-instance serialization lock. Exposed
    /// internally so `complete_step` / `fail_step` can hold it across
    /// load → mutate → update_instance_status; without it, two concurrent
    /// step completions can each read a stale `steps.*` map and the second
    /// write clobbers the first.
    async fn instance_lock(&self, instance_id: &str) -> Arc<Mutex<()>> {
        let mut locks = self.locks.lock().await;
        locks
            .entry(instance_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    async fn advance_inner(&self, instance_id: &str) -> Result<(), WorkflowError> {
        // Acquire per-instance lock to prevent concurrent advance calls from
        // creating duplicate work.
        let lock = self.instance_lock(instance_id).await;
        let _guard = lock.lock().await;

        let instance = self.store.load_instance(instance_id)?;

        // Don't advance terminal states
        if matches!(
            instance.status,
            WorkflowStatus::Completed | WorkflowStatus::Cancelled
        ) {
            return Ok(());
        }

        let ready = self.find_ready_steps(&instance);

        if ready.is_empty() {
            // Check if we're done
            let all_terminal = instance.step_states.values().all(|s| {
                matches!(
                    s.status,
                    StepStatus::Completed
                        | StepStatus::Failed
                        | StepStatus::Skipped
                        | StepStatus::TimedOut
                )
            });

            if all_terminal {
                let has_failures = instance
                    .step_states
                    .values()
                    .any(|s| matches!(s.status, StepStatus::Failed | StepStatus::TimedOut));

                let final_status = if has_failures {
                    WorkflowStatus::Failed
                } else {
                    WorkflowStatus::Completed
                };

                self.store.update_instance_status(
                    instance_id,
                    &final_status,
                    &instance.context,
                )?;
                info!(
                    instance_id = %instance_id,
                    status = final_status.as_str(),
                    "Workflow finished"
                );
            }
            // If not all terminal and no ready steps, we're waiting (e.g. for
            // approval or external step completion).
            return Ok(());
        }

        // Group steps by parallel group for concurrent execution
        let (parallel_groups, sequential_steps) = partition_parallel_steps(&ready);

        // Execute ready steps
        let now = chrono::Utc::now().to_rfc3339();
        let mut any_skipped = false;

        // Execute parallel groups concurrently using JoinSet
        for group_steps in parallel_groups.values() {
            let mut join_set: JoinSet<Result<(), WorkflowError>> = JoinSet::new();

            for step in group_steps {
                // Check max iterations
                if let Some(max_iter) = step.max_iterations {
                    let exec_count = instance
                        .step_states
                        .get(&step.id)
                        .map(|s| s.execution_count)
                        .unwrap_or(0);
                    if exec_count >= max_iter {
                        let failed = StepState {
                            status: StepStatus::Failed,
                            started_at: Some(now.clone()),
                            completed_at: Some(now.clone()),
                            result: None,
                            error: Some(format!(
                                "max iterations ({max_iter}) exceeded for step '{}'",
                                step.id
                            )),
                            retries: 0,
                            execution_count: exec_count,
                        };
                        self.store.update_step(instance_id, &step.id, &failed)?;
                        continue;
                    }
                }

                // Check condition
                if let Some(ref condition) = step.condition {
                    if !evaluate_condition(condition, &instance.context, &instance.step_states) {
                        let skipped = StepState {
                            status: StepStatus::Skipped,
                            started_at: Some(now.clone()),
                            completed_at: Some(now.clone()),
                            result: None,
                            error: Some(format!("condition not met: {condition:?}")),
                            retries: 0,
                            execution_count: instance
                                .step_states
                                .get(&step.id)
                                .map(|s| s.execution_count)
                                .unwrap_or(0),
                        };
                        self.store.update_step(instance_id, &step.id, &skipped)?;
                        debug!(step_id = %step.id, "Step skipped (condition false)");
                        any_skipped = true;
                        continue;
                    }
                }

                let engine = self.clone();
                let inst_id = instance_id.to_string();
                let step = step.clone();
                let now_clone = now.clone();
                let current_exec_count = instance
                    .step_states
                    .get(&step.id)
                    .map(|s| s.execution_count)
                    .unwrap_or(0);
                let current_retries = instance
                    .step_states
                    .get(&step.id)
                    .map(|s| s.retries)
                    .unwrap_or(0);

                join_set.spawn(async move {
                    engine
                        .execute_step(
                            &inst_id,
                            &step,
                            &now_clone,
                            current_exec_count,
                            current_retries,
                        )
                        .await
                });
            }

            // Wait for all parallel steps to be dispatched
            while let Some(result) = join_set.join_next().await {
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        warn!(error = %e, "Parallel step dispatch error");
                    }
                    Err(e) => {
                        warn!(error = %e, "Parallel step task panicked");
                    }
                }
            }
        }

        // Execute sequential (non-parallel) steps
        for step in &sequential_steps {
            // Check max iterations
            if let Some(max_iter) = step.max_iterations {
                let exec_count = instance
                    .step_states
                    .get(&step.id)
                    .map(|s| s.execution_count)
                    .unwrap_or(0);
                if exec_count >= max_iter {
                    let failed = StepState {
                        status: StepStatus::Failed,
                        started_at: Some(now.clone()),
                        completed_at: Some(now.clone()),
                        result: None,
                        error: Some(format!(
                            "max iterations ({max_iter}) exceeded for step '{}'",
                            step.id
                        )),
                        retries: 0,
                        execution_count: exec_count,
                    };
                    self.store.update_step(instance_id, &step.id, &failed)?;
                    continue;
                }
            }

            // Check condition
            if let Some(ref condition) = step.condition {
                if !evaluate_condition(condition, &instance.context, &instance.step_states) {
                    let skipped = StepState {
                        status: StepStatus::Skipped,
                        started_at: Some(now.clone()),
                        completed_at: Some(now.clone()),
                        result: None,
                        error: Some(format!("condition not met: {condition:?}")),
                        retries: 0,
                        execution_count: instance
                            .step_states
                            .get(&step.id)
                            .map(|s| s.execution_count)
                            .unwrap_or(0),
                    };
                    self.store.update_step(instance_id, &step.id, &skipped)?;
                    debug!(step_id = %step.id, "Step skipped (condition false)");
                    any_skipped = true;
                    continue;
                }
            }

            let current_exec_count = instance
                .step_states
                .get(&step.id)
                .map(|s| s.execution_count)
                .unwrap_or(0);
            let current_retries = instance
                .step_states
                .get(&step.id)
                .map(|s| s.retries)
                .unwrap_or(0);

            self.execute_step(instance_id, step, &now, current_exec_count, current_retries)
                .await?;
        }

        // If steps were skipped, new steps may have become ready or the
        // workflow may now be complete. Re-check by reloading state.
        if any_skipped {
            let refreshed = self.store.load_instance(instance_id)?;
            let new_ready = self.find_ready_steps(&refreshed);

            if new_ready.is_empty() {
                // Check if fully terminal
                let all_terminal = refreshed.step_states.values().all(|s| {
                    matches!(
                        s.status,
                        StepStatus::Completed
                            | StepStatus::Failed
                            | StepStatus::Skipped
                            | StepStatus::TimedOut
                    )
                });

                if all_terminal {
                    let has_failures = refreshed
                        .step_states
                        .values()
                        .any(|s| matches!(s.status, StepStatus::Failed | StepStatus::TimedOut));

                    let final_status = if has_failures {
                        WorkflowStatus::Failed
                    } else {
                        WorkflowStatus::Completed
                    };

                    self.store.update_instance_status(
                        instance_id,
                        &final_status,
                        &refreshed.context,
                    )?;
                    info!(
                        instance_id = %instance_id,
                        status = final_status.as_str(),
                        "Workflow finished (after skips)"
                    );
                }
            }
            // If there are new ready steps, they'll be picked up by the next
            // advance call (triggered by complete_step or resume).
        }

        Ok(())
    }

    /// Execute a single step — dispatches based on action type.
    async fn execute_step(
        &self,
        instance_id: &str,
        step: &WorkflowStep,
        now: &str,
        current_exec_count: u32,
        current_retries: u32,
    ) -> Result<(), WorkflowError> {
        let timeout_ms = step.timeout_ms.unwrap_or(step.timeout_secs * 1000);

        match &step.action {
            StepAction::HumanApproval { description } => {
                let waiting = StepState {
                    status: StepStatus::WaitingApproval,
                    started_at: Some(now.to_string()),
                    completed_at: None,
                    result: None,
                    error: None,
                    retries: 0,
                    execution_count: current_exec_count + 1,
                };
                self.store.update_step(instance_id, &step.id, &waiting)?;
                self.store.update_instance_status(
                    instance_id,
                    &WorkflowStatus::WaitingForApproval,
                    &serde_json::json!({}),
                )?;
                info!(
                    instance_id = %instance_id,
                    step_id = %step.id,
                    description = %description,
                    "Waiting for human approval"
                );
            }
            StepAction::Command { cmd, cwd } => {
                // Execute command natively with timeout
                let running = StepState {
                    status: StepStatus::Running,
                    started_at: Some(now.to_string()),
                    completed_at: None,
                    result: None,
                    error: None,
                    retries: current_retries,
                    execution_count: current_exec_count + 1,
                };
                self.store.update_step(instance_id, &step.id, &running)?;

                let engine = self.clone();
                let inst_id = instance_id.to_string();
                let step_id = step.id.clone();
                let cmd = cmd.clone();
                let cwd = cwd.clone();

                tokio::spawn(async move {
                    let result = execute_command(&cmd, cwd.as_deref(), timeout_ms).await;
                    match result {
                        Ok(output) => {
                            if let Err(e) =
                                engine.complete_step(&inst_id, &step_id, output).await
                            {
                                warn!(error = %e, "Failed to complete command step");
                            }
                        }
                        Err(e) => {
                            if e.contains("timed out") {
                                // Mark as timed out specifically
                                let now = chrono::Utc::now().to_rfc3339();
                                let timed_out = StepState {
                                    status: StepStatus::TimedOut,
                                    started_at: Some(now.clone()),
                                    completed_at: Some(now),
                                    result: None,
                                    error: Some(e.clone()),
                                    retries: 0,
                                    execution_count: 0,
                                };
                                let _ =
                                    engine.store.update_step(&inst_id, &step_id, &timed_out);
                                // Try to advance past the timeout
                                let _ = engine.advance(&inst_id).await;
                            } else if let Err(err) =
                                engine.fail_step(&inst_id, &step_id, &e).await
                            {
                                warn!(error = %err, "Failed to fail command step");
                            }
                        }
                    }
                });
            }
            _ => {
                // For all other action types, transition to Running and
                // wait for external completion via complete_step/fail_step.
                let running = StepState {
                    status: StepStatus::Running,
                    started_at: Some(now.to_string()),
                    completed_at: None,
                    result: None,
                    error: None,
                    retries: current_retries,
                    execution_count: current_exec_count + 1,
                };
                self.store.update_step(instance_id, &step.id, &running)?;

                // If the step has a timeout, spawn a watchdog
                if timeout_ms > 0 {
                    let engine = self.clone();
                    let inst_id = instance_id.to_string();
                    let step_id = step.id.clone();

                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(timeout_ms)).await;

                        // Check if step is still running
                        if let Ok(inst) = engine.store.load_instance(&inst_id) {
                            if let Some(state) = inst.step_states.get(&step_id) {
                                if state.status == StepStatus::Running {
                                    let now = chrono::Utc::now().to_rfc3339();
                                    let timed_out = StepState {
                                        status: StepStatus::TimedOut,
                                        started_at: state.started_at.clone(),
                                        completed_at: Some(now),
                                        result: None,
                                        error: Some(format!(
                                            "step timed out after {timeout_ms}ms"
                                        )),
                                        retries: state.retries,
                                        execution_count: state.execution_count,
                                    };
                                    let _ = engine
                                        .store
                                        .update_step(&inst_id, &step_id, &timed_out);
                                    warn!(
                                        instance_id = %inst_id,
                                        step_id = %step_id,
                                        timeout_ms = timeout_ms,
                                        "Step timed out"
                                    );
                                    let _ = engine.advance(&inst_id).await;
                                }
                            }
                        }
                    });
                }

                debug!(step_id = %step.id, action = ?std::mem::discriminant(&step.action), "Step started (awaiting external completion)");
            }
        }

        Ok(())
    }

    /// Find steps whose dependencies are all satisfied and that are still
    /// `Pending`.
    fn find_ready_steps(&self, instance: &WorkflowInstance) -> Vec<WorkflowStep> {
        let completed_or_skipped: HashSet<&str> = instance
            .step_states
            .iter()
            .filter(|(_, s)| {
                matches!(
                    s.status,
                    StepStatus::Completed | StepStatus::Skipped | StepStatus::TimedOut
                )
            })
            .map(|(id, _)| id.as_str())
            .collect();

        // Also include Failed steps when using ContinueOnError for dependency
        // resolution (downstream steps with OnFailure conditions may need them)
        let satisfied: HashSet<&str> = instance
            .step_states
            .iter()
            .filter(|(_, s)| {
                matches!(
                    s.status,
                    StepStatus::Completed
                        | StepStatus::Skipped
                        | StepStatus::Failed
                        | StepStatus::TimedOut
                )
            })
            .map(|(id, _)| id.as_str())
            .collect();

        instance
            .definition
            .steps
            .iter()
            .filter(|step| {
                let state = instance.step_states.get(&step.id);
                let is_pending = state
                    .map(|s| s.status == StepStatus::Pending)
                    .unwrap_or(true);

                if !is_pending {
                    return false;
                }

                // For steps with OnFailure condition, dependencies are
                // satisfied when the referenced step has any terminal status
                let has_failure_condition =
                    matches!(step.condition, Some(StepCondition::OnFailure { .. }));

                if has_failure_condition
                    || instance.definition.on_error == ErrorPolicy::ContinueOnError
                {
                    step.depends_on
                        .iter()
                        .all(|dep| satisfied.contains(dep.as_str()))
                } else {
                    // All dependencies must be completed or skipped
                    step.depends_on
                        .iter()
                        .all(|dep| completed_or_skipped.contains(dep.as_str()))
                }
            })
            .cloned()
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Partition steps into parallel groups and sequential steps.
fn partition_parallel_steps(
    steps: &[WorkflowStep],
) -> (HashMap<String, Vec<WorkflowStep>>, Vec<WorkflowStep>) {
    let mut groups: HashMap<String, Vec<WorkflowStep>> = HashMap::new();
    let mut sequential = Vec::new();

    for step in steps {
        if step.parallel {
            let group = step
                .parallel_group
                .clone()
                .unwrap_or_else(|| "__default__".to_string());
            groups.entry(group).or_default().push(step.clone());
        } else {
            sequential.push(step.clone());
        }
    }

    (groups, sequential)
}

/// Evaluate a step condition against the workflow context and step states.
fn evaluate_condition(
    condition: &StepCondition,
    context: &serde_json::Value,
    step_states: &HashMap<String, StepState>,
) -> bool {
    match condition {
        StepCondition::Always => true,
        StepCondition::OnSuccess { step_id } => step_states
            .get(step_id)
            .map(|s| s.status == StepStatus::Completed)
            .unwrap_or(false),
        StepCondition::OnFailure { step_id } => step_states
            .get(step_id)
            .map(|s| matches!(s.status, StepStatus::Failed | StepStatus::TimedOut))
            .unwrap_or(false),
        StepCondition::Expression { expr } => evaluate_expression(expr, context, step_states),
    }
}

/// Evaluate a simple expression string.
///
/// Supported patterns:
/// - `steps.<step_id>.status == "success"` or `"failed"`
/// - `steps.<step_id>.output.contains("<substring>")`
/// - Dot-path into context (e.g. `build.success`) — truthiness check
fn evaluate_expression(
    expr: &str,
    context: &serde_json::Value,
    step_states: &HashMap<String, StepState>,
) -> bool {
    let expr = expr.trim();

    // Pattern: steps.<step_id>.status == "<status>"
    if let Some(rest) = expr.strip_prefix("steps.") {
        if let Some((step_id, remainder)) = rest.split_once('.') {
            let remainder = remainder.trim();

            // status == "success" / "failed"
            if let Some(rhs) = remainder.strip_prefix("status == ") {
                let expected = rhs.trim().trim_matches('"');
                if let Some(state) = step_states.get(step_id) {
                    return match expected {
                        "success" | "completed" => state.status == StepStatus::Completed,
                        "failed" => state.status == StepStatus::Failed,
                        "skipped" => state.status == StepStatus::Skipped,
                        "timed_out" => state.status == StepStatus::TimedOut,
                        "running" => state.status == StepStatus::Running,
                        "pending" => state.status == StepStatus::Pending,
                        _ => false,
                    };
                }
                return false;
            }

            // output.contains("<substring>")
            if let Some(contains_part) = remainder.strip_prefix("output.contains(") {
                let substring = contains_part.trim_end_matches(')').trim_matches('"');
                if let Some(state) = step_states.get(step_id) {
                    if let Some(ref result) = state.result {
                        let result_str = result.to_string();
                        return result_str.contains(substring);
                    }
                }
                return false;
            }
        }
    }

    // Fall back to dot-path truthiness check in context
    evaluate_dot_path(expr, context)
}

/// Evaluate a dot-path against a JSON value for truthiness.
fn evaluate_dot_path(path: &str, context: &serde_json::Value) -> bool {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = context;

    for part in &parts {
        match current.get(part) {
            Some(v) => current = v,
            None => return false,
        }
    }

    match current {
        serde_json::Value::Null => false,
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::String(s) => !s.is_empty(),
        serde_json::Value::Number(_) => true,
        serde_json::Value::Array(a) => !a.is_empty(),
        serde_json::Value::Object(_) => true,
    }
}

/// Detect cycles in the workflow step graph. Returns an error if a cycle is found.
fn detect_cycles(steps: &[WorkflowStep]) -> Result<(), WorkflowError> {
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();

    for step in steps {
        in_degree.entry(&step.id).or_insert(0);
        adjacency.entry(&step.id).or_default();

        for dep in &step.depends_on {
            adjacency.entry(dep.as_str()).or_default().push(&step.id);
            *in_degree.entry(&step.id).or_insert(0) += 1;
        }
    }

    let mut queue: VecDeque<&str> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(&id, _)| id)
        .collect();

    let mut visited = 0usize;
    while let Some(node) = queue.pop_front() {
        visited += 1;
        if let Some(neighbors) = adjacency.get(node) {
            for &neighbor in neighbors {
                if let Some(deg) = in_degree.get_mut(neighbor) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(neighbor);
                    }
                }
            }
        }
    }

    if visited != steps.len() {
        // Find the steps involved in the cycle for a helpful error message
        let cycle_steps: Vec<&str> = in_degree
            .iter()
            .filter(|(_, &deg)| deg > 0)
            .map(|(&id, _)| id)
            .collect();
        return Err(WorkflowError::CycleDetected(format!(
            "circular dependencies detected among steps: {}",
            cycle_steps.join(", ")
        )));
    }

    Ok(())
}

/// Substitute `{{key}}` placeholders in a JSON value tree with values from params.
fn substitute_params(value: &mut serde_json::Value, params: &serde_json::Value) {
    match value {
        serde_json::Value::String(s) => {
            if let serde_json::Value::Object(map) = params {
                for (key, val) in map {
                    let placeholder = format!("{{{{{key}}}}}");
                    if s.contains(&placeholder) {
                        let replacement = match val {
                            serde_json::Value::String(v) => v.clone(),
                            other => other.to_string(),
                        };
                        *s = s.replace(&placeholder, &replacement);
                    }
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                substitute_params(item, params);
            }
        }
        serde_json::Value::Object(map) => {
            for (_, v) in map.iter_mut() {
                substitute_params(v, params);
            }
        }
        _ => {}
    }
}

/// Execute a shell command with a timeout. Returns the stdout as a JSON value
/// on success, or an error string on failure.
async fn execute_command(
    cmd: &str,
    cwd: Option<&str>,
    timeout_ms: u64,
) -> Result<serde_json::Value, String> {
    let mut command = tokio::process::Command::new("sh");
    command.arg("-c").arg(cmd);

    if let Some(dir) = cwd {
        command.current_dir(dir);
    }

    command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let result = tokio::time::timeout(
        std::time::Duration::from_millis(timeout_ms),
        command.output(),
    )
    .await;

    match result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            if output.status.success() {
                Ok(serde_json::json!({
                    "exit_code": 0,
                    "stdout": stdout,
                    "stderr": stderr,
                }))
            } else {
                Err(format!(
                    "command exited with code {}: {}",
                    output.status.code().unwrap_or(-1),
                    stderr
                ))
            }
        }
        Ok(Err(e)) => Err(format!("failed to spawn command: {e}")),
        Err(_) => Err(format!("command timed out after {timeout_ms}ms")),
    }
}

/// Topological sort of workflow steps. Returns step IDs in execution order.
/// Used for validation and debugging.
pub fn topological_sort(steps: &[WorkflowStep]) -> Result<Vec<String>, WorkflowError> {
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();

    for step in steps {
        in_degree.entry(&step.id).or_insert(0);
        adjacency.entry(&step.id).or_default();

        for dep in &step.depends_on {
            adjacency.entry(dep.as_str()).or_default().push(&step.id);
            *in_degree.entry(&step.id).or_insert(0) += 1;
        }
    }

    let mut queue: VecDeque<&str> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(&id, _)| id)
        .collect();

    let mut sorted = Vec::new();
    while let Some(node) = queue.pop_front() {
        sorted.push(node.to_string());
        if let Some(neighbors) = adjacency.get(node) {
            for &neighbor in neighbors {
                if let Some(deg) = in_degree.get_mut(neighbor) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(neighbor);
                    }
                }
            }
        }
    }

    if sorted.len() != steps.len() {
        return Err(WorkflowError::CycleDetected(
            "workflow has cyclic dependencies".to_string(),
        ));
    }

    Ok(sorted)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definition::*;
    use tempfile::TempDir;

    /// Poll `engine.status(instance_id)` until `predicate(state)` returns true
    /// or the deadline expires. Returns the matching state or panics with a
    /// detail of the last observed state.
    ///
    /// Replaces `tokio::time::sleep(N ms); assert!(state == ...)` which races
    /// under parallel test load — the sleep is a guess at how long the
    /// background tokio::spawn'd command needs and is wrong as soon as the
    /// host is busy.
    async fn poll_until<F>(
        engine: &WorkflowEngine,
        instance_id: &str,
        deadline: std::time::Duration,
        what: &str,
        predicate: F,
    ) -> WorkflowInstance
    where
        F: Fn(&WorkflowInstance) -> bool,
    {
        let start = std::time::Instant::now();
        let poll_interval = std::time::Duration::from_millis(10);
        let mut last = engine.status(instance_id).expect("status lookup");
        while !predicate(&last) {
            if start.elapsed() > deadline {
                panic!(
                    "poll_until({what}) deadline exceeded; last state: status={:?} step_states={:?}",
                    last.status, last.step_states
                );
            }
            tokio::time::sleep(poll_interval).await;
            last = engine.status(instance_id).expect("status lookup");
        }
        last
    }

    fn make_step(id: &str, deps: Vec<&str>) -> WorkflowStep {
        WorkflowStep {
            id: id.to_string(),
            name: format!("Step {id}"),
            action: StepAction::Custom {
                handler: "test".to_string(),
                config: serde_json::json!({}),
            },
            depends_on: deps.into_iter().map(String::from).collect(),
            condition: None,
            retry: RetryPolicy::default(),
            timeout_secs: 60,
            timeout_ms: None,
            parallel: false,
            parallel_group: None,
            max_iterations: None,
        }
    }

    fn make_parallel_step(id: &str, group: &str, deps: Vec<&str>) -> WorkflowStep {
        WorkflowStep {
            id: id.to_string(),
            name: format!("Parallel Step {id}"),
            action: StepAction::Custom {
                handler: "test".to_string(),
                config: serde_json::json!({}),
            },
            depends_on: deps.into_iter().map(String::from).collect(),
            condition: None,
            retry: RetryPolicy::default(),
            timeout_secs: 60,
            timeout_ms: None,
            parallel: true,
            parallel_group: Some(group.to_string()),
            max_iterations: None,
        }
    }

    fn make_command_step(id: &str, cmd: &str, deps: Vec<&str>) -> WorkflowStep {
        WorkflowStep {
            id: id.to_string(),
            name: format!("Step {id}"),
            action: StepAction::Command {
                cmd: cmd.to_string(),
                cwd: None,
            },
            depends_on: deps.into_iter().map(String::from).collect(),
            condition: None,
            retry: RetryPolicy::default(),
            timeout_secs: 60,
            timeout_ms: None,
            parallel: false,
            parallel_group: None,
            max_iterations: None,
        }
    }

    fn make_approval_step(id: &str, deps: Vec<&str>) -> WorkflowStep {
        WorkflowStep {
            id: id.to_string(),
            name: format!("Approve {id}"),
            action: StepAction::HumanApproval {
                description: "Please review and approve".to_string(),
            },
            depends_on: deps.into_iter().map(String::from).collect(),
            condition: None,
            retry: RetryPolicy::default(),
            timeout_secs: 0,
            timeout_ms: None,
            parallel: false,
            parallel_group: None,
            max_iterations: None,
        }
    }

    fn make_def(id: &str, steps: Vec<WorkflowStep>) -> WorkflowDefinition {
        WorkflowDefinition {
            id: id.to_string(),
            name: format!("WF {id}"),
            steps,
            on_error: ErrorPolicy::FailFast,
            version: 1,
            template: false,
        }
    }

    // -- Topological sort --

    #[test]
    fn test_topological_sort_linear() {
        let steps = vec![
            make_step("a", vec![]),
            make_step("b", vec!["a"]),
            make_step("c", vec!["b"]),
        ];
        let sorted = topological_sort(&steps).unwrap();
        assert_eq!(sorted, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_topological_sort_parallel() {
        let steps = vec![
            make_step("a", vec![]),
            make_step("b", vec![]),
            make_step("c", vec!["a", "b"]),
        ];
        let sorted = topological_sort(&steps).unwrap();
        assert_eq!(sorted.last().unwrap(), "c");
        assert_eq!(sorted.len(), 3);
    }

    #[test]
    fn test_topological_sort_cycle() {
        let steps = vec![
            make_step("a", vec!["c"]),
            make_step("b", vec!["a"]),
            make_step("c", vec!["b"]),
        ];
        let result = topological_sort(&steps);
        assert!(result.is_err());
    }

    // -- Condition evaluation --

    #[test]
    fn test_evaluate_condition_always() {
        let ctx = serde_json::json!({});
        let states = HashMap::new();
        assert!(evaluate_condition(&StepCondition::Always, &ctx, &states));
    }

    #[test]
    fn test_evaluate_condition_on_success() {
        let ctx = serde_json::json!({});
        let mut states = HashMap::new();
        states.insert(
            "s1".to_string(),
            StepState {
                status: StepStatus::Completed,
                started_at: None,
                completed_at: None,
                result: None,
                error: None,
                retries: 0,
                execution_count: 1,
            },
        );
        assert!(evaluate_condition(
            &StepCondition::OnSuccess {
                step_id: "s1".to_string()
            },
            &ctx,
            &states,
        ));
    }

    #[test]
    fn test_evaluate_condition_on_failure() {
        let ctx = serde_json::json!({});
        let mut states = HashMap::new();
        states.insert(
            "s1".to_string(),
            StepState {
                status: StepStatus::Failed,
                started_at: None,
                completed_at: None,
                result: None,
                error: Some("boom".to_string()),
                retries: 0,
                execution_count: 1,
            },
        );
        assert!(evaluate_condition(
            &StepCondition::OnFailure {
                step_id: "s1".to_string()
            },
            &ctx,
            &states,
        ));
        // Should be false if step succeeded
        states.get_mut("s1").unwrap().status = StepStatus::Completed;
        assert!(!evaluate_condition(
            &StepCondition::OnFailure {
                step_id: "s1".to_string()
            },
            &ctx,
            &states,
        ));
    }

    #[test]
    fn test_evaluate_expression_status() {
        let ctx = serde_json::json!({});
        let mut states = HashMap::new();
        states.insert(
            "build".to_string(),
            StepState {
                status: StepStatus::Completed,
                started_at: None,
                completed_at: None,
                result: Some(serde_json::json!({"output": "all good"})),
                error: None,
                retries: 0,
                execution_count: 1,
            },
        );
        assert!(evaluate_expression(
            "steps.build.status == \"success\"",
            &ctx,
            &states,
        ));
        assert!(!evaluate_expression(
            "steps.build.status == \"failed\"",
            &ctx,
            &states,
        ));
    }

    #[test]
    fn test_evaluate_expression_output_contains() {
        let ctx = serde_json::json!({});
        let mut states = HashMap::new();
        states.insert(
            "step1".to_string(),
            StepState {
                status: StepStatus::Completed,
                started_at: None,
                completed_at: None,
                result: Some(
                    serde_json::json!({"message": "build error: compilation failed"}),
                ),
                error: None,
                retries: 0,
                execution_count: 1,
            },
        );
        assert!(evaluate_expression(
            "steps.step1.output.contains(\"error\")",
            &ctx,
            &states,
        ));
        assert!(!evaluate_expression(
            "steps.step1.output.contains(\"success\")",
            &ctx,
            &states,
        ));
    }

    #[test]
    fn test_evaluate_dot_path_truthiness() {
        let ctx = serde_json::json!({"build": {"success": true}});
        assert!(evaluate_dot_path("build.success", &ctx));

        let ctx2 = serde_json::json!({"build": {"success": false}});
        assert!(!evaluate_dot_path("build.success", &ctx2));

        let ctx3 = serde_json::json!({"build": {}});
        assert!(!evaluate_dot_path("build.success", &ctx3));
    }

    // -- Cycle detection --

    #[test]
    fn test_cycle_detection_simple_cycle() {
        let steps = vec![make_step("a", vec!["b"]), make_step("b", vec!["a"])];
        let result = detect_cycles(&steps);
        assert!(result.is_err());
        match result.unwrap_err() {
            WorkflowError::CycleDetected(msg) => {
                assert!(msg.contains("circular dependencies"));
            }
            other => panic!("expected CycleDetected, got: {other:?}"),
        }
    }

    #[test]
    fn test_cycle_detection_no_cycle() {
        let steps = vec![
            make_step("a", vec![]),
            make_step("b", vec!["a"]),
            make_step("c", vec!["a", "b"]),
        ];
        assert!(detect_cycles(&steps).is_ok());
    }

    #[test]
    fn test_cycle_detection_three_node_cycle() {
        let steps = vec![
            make_step("a", vec!["c"]),
            make_step("b", vec!["a"]),
            make_step("c", vec!["b"]),
        ];
        let result = detect_cycles(&steps);
        assert!(result.is_err());
    }

    // -- Validation --

    #[test]
    fn test_validate_duplicate_step_ids() {
        let def = WorkflowDefinition {
            id: "test".to_string(),
            name: "Test".to_string(),
            steps: vec![make_step("a", vec![]), make_step("a", vec![])],
            on_error: ErrorPolicy::FailFast,
            version: 1,
            template: false,
        };
        let result = WorkflowEngine::validate(&def);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            WorkflowError::ValidationError(_)
        ));
    }

    #[test]
    fn test_validate_missing_dependency() {
        let def = WorkflowDefinition {
            id: "test".to_string(),
            name: "Test".to_string(),
            steps: vec![make_step("a", vec!["nonexistent"])],
            on_error: ErrorPolicy::FailFast,
            version: 1,
            template: false,
        };
        let result = WorkflowEngine::validate(&def);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            WorkflowError::ValidationError(_)
        ));
    }

    #[test]
    fn test_validate_cyclic_dependency() {
        let def = WorkflowDefinition {
            id: "test".to_string(),
            name: "Test".to_string(),
            steps: vec![make_step("a", vec!["b"]), make_step("b", vec!["a"])],
            on_error: ErrorPolicy::FailFast,
            version: 1,
            template: false,
        };
        let result = WorkflowEngine::validate(&def);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            WorkflowError::CycleDetected(_)
        ));
    }

    // -- Engine lifecycle --

    #[tokio::test]
    async fn test_start_and_status() {
        let tmp = TempDir::new().unwrap();
        let engine = WorkflowEngine::new(tmp.path()).unwrap();

        let def = make_def("test", vec![make_step("s1", vec![])]);

        let instance = engine.start(&def, serde_json::json!({})).await.unwrap();
        assert_eq!(instance.definition_id, "test");
        assert_eq!(instance.step_states["s1"].status, StepStatus::Running);

        let status = engine.status(&instance.id).unwrap();
        assert_eq!(status.id, instance.id);
    }

    #[tokio::test]
    async fn test_complete_step_finishes_workflow() {
        let tmp = TempDir::new().unwrap();
        let engine = WorkflowEngine::new(tmp.path()).unwrap();

        let def = make_def("test-complete", vec![make_step("s1", vec![])]);

        let instance = engine.start(&def, serde_json::json!({})).await.unwrap();
        engine
            .complete_step(&instance.id, "s1", serde_json::json!({"ok": true}))
            .await
            .unwrap();

        let final_state = engine.status(&instance.id).unwrap();
        assert_eq!(final_state.status, WorkflowStatus::Completed);
        assert_eq!(final_state.step_states["s1"].status, StepStatus::Completed);
    }

    #[tokio::test]
    async fn test_dependency_ordering() {
        let tmp = TempDir::new().unwrap();
        let engine = WorkflowEngine::new(tmp.path()).unwrap();

        let def = make_def(
            "test-deps",
            vec![make_step("s1", vec![]), make_step("s2", vec!["s1"])],
        );

        let instance = engine.start(&def, serde_json::json!({})).await.unwrap();
        assert_eq!(instance.step_states["s1"].status, StepStatus::Running);
        assert_eq!(instance.step_states["s2"].status, StepStatus::Pending);

        engine
            .complete_step(&instance.id, "s1", serde_json::json!({}))
            .await
            .unwrap();

        let updated = engine.status(&instance.id).unwrap();
        assert_eq!(updated.step_states["s2"].status, StepStatus::Running);
    }

    #[tokio::test]
    async fn test_parallel_steps_run_concurrently() {
        let tmp = TempDir::new().unwrap();
        let engine = WorkflowEngine::new(tmp.path()).unwrap();

        let def = make_def(
            "test-parallel",
            vec![
                make_step("a", vec![]),
                make_step("b", vec![]),
                make_step("c", vec!["a", "b"]),
            ],
        );

        let instance = engine.start(&def, serde_json::json!({})).await.unwrap();
        assert_eq!(instance.step_states["a"].status, StepStatus::Running);
        assert_eq!(instance.step_states["b"].status, StepStatus::Running);
        assert_eq!(instance.step_states["c"].status, StepStatus::Pending);

        engine
            .complete_step(&instance.id, "a", serde_json::json!({}))
            .await
            .unwrap();
        let updated = engine.status(&instance.id).unwrap();
        assert_eq!(updated.step_states["c"].status, StepStatus::Pending);

        engine
            .complete_step(&instance.id, "b", serde_json::json!({}))
            .await
            .unwrap();
        let updated = engine.status(&instance.id).unwrap();
        assert_eq!(updated.step_states["c"].status, StepStatus::Running);
    }

    #[tokio::test]
    async fn test_explicit_parallel_group() {
        let tmp = TempDir::new().unwrap();
        let engine = WorkflowEngine::new(tmp.path()).unwrap();

        let def = make_def(
            "test-par-group",
            vec![
                make_parallel_step("p1", "build", vec![]),
                make_parallel_step("p2", "build", vec![]),
                make_step("final", vec!["p1", "p2"]),
            ],
        );

        let instance = engine.start(&def, serde_json::json!({})).await.unwrap();
        assert_eq!(instance.step_states["p1"].status, StepStatus::Running);
        assert_eq!(instance.step_states["p2"].status, StepStatus::Running);
        assert_eq!(instance.step_states["final"].status, StepStatus::Pending);

        engine
            .complete_step(&instance.id, "p1", serde_json::json!({}))
            .await
            .unwrap();
        engine
            .complete_step(&instance.id, "p2", serde_json::json!({}))
            .await
            .unwrap();

        let final_state = engine.status(&instance.id).unwrap();
        assert_eq!(final_state.step_states["final"].status, StepStatus::Running);
    }

    #[tokio::test]
    async fn test_human_approval_pauses_workflow() {
        let tmp = TempDir::new().unwrap();
        let engine = WorkflowEngine::new(tmp.path()).unwrap();

        let def = make_def(
            "test-approval",
            vec![
                make_approval_step("review", vec![]),
                make_step("deploy", vec!["review"]),
            ],
        );

        let instance = engine.start(&def, serde_json::json!({})).await.unwrap();
        assert_eq!(
            instance.step_states["review"].status,
            StepStatus::WaitingApproval
        );
        assert_eq!(instance.status, WorkflowStatus::WaitingForApproval);
        assert_eq!(instance.step_states["deploy"].status, StepStatus::Pending);

        engine.approve(&instance.id, "review").await.unwrap();

        let updated = engine.status(&instance.id).unwrap();
        assert_eq!(
            updated.step_states["review"].status,
            StepStatus::Completed
        );
        assert_eq!(updated.step_states["deploy"].status, StepStatus::Running);
    }

    #[tokio::test]
    async fn test_cancel() {
        let tmp = TempDir::new().unwrap();
        let engine = WorkflowEngine::new(tmp.path()).unwrap();

        let def = make_def(
            "test-cancel",
            vec![make_step("s1", vec![]), make_step("s2", vec!["s1"])],
        );

        let instance = engine.start(&def, serde_json::json!({})).await.unwrap();
        engine.cancel(&instance.id).await.unwrap();

        let cancelled = engine.status(&instance.id).unwrap();
        assert_eq!(cancelled.status, WorkflowStatus::Cancelled);

        for state in cancelled.step_states.values() {
            assert!(matches!(
                state.status,
                StepStatus::Skipped | StepStatus::Completed
            ));
        }
    }

    #[tokio::test]
    async fn test_retry_logic() {
        let tmp = TempDir::new().unwrap();
        let engine = WorkflowEngine::new(tmp.path()).unwrap();

        let def = WorkflowDefinition {
            id: "test-retry".to_string(),
            name: "Test Retry".to_string(),
            steps: vec![WorkflowStep {
                id: "flaky".to_string(),
                name: "Flaky Step".to_string(),
                action: StepAction::Custom {
                    handler: "test".to_string(),
                    config: serde_json::json!({}),
                },
                depends_on: vec![],
                condition: None,
                retry: RetryPolicy {
                    max_retries: 2,
                    backoff_secs: 0,
                },
                timeout_secs: 60,
                timeout_ms: None,
                parallel: false,
                parallel_group: None,
                max_iterations: None,
            }],
            on_error: ErrorPolicy::FailFast,
            version: 1,
            template: false,
        };

        let instance = engine.start(&def, serde_json::json!({})).await.unwrap();

        engine
            .fail_step(&instance.id, "flaky", "transient error")
            .await
            .unwrap();
        let after_retry1 = engine.status(&instance.id).unwrap();
        assert_eq!(
            after_retry1.step_states["flaky"].status,
            StepStatus::Running
        );

        engine
            .fail_step(&instance.id, "flaky", "transient error again")
            .await
            .unwrap();
        let after_retry2 = engine.status(&instance.id).unwrap();
        assert_eq!(
            after_retry2.step_states["flaky"].status,
            StepStatus::Running
        );

        engine
            .fail_step(&instance.id, "flaky", "permanent error")
            .await
            .unwrap();
        let final_state = engine.status(&instance.id).unwrap();
        assert_eq!(
            final_state.step_states["flaky"].status,
            StepStatus::Failed
        );
        assert_eq!(final_state.status, WorkflowStatus::Failed);
    }

    #[tokio::test]
    async fn test_command_execution() {
        let tmp = TempDir::new().unwrap();
        let engine = WorkflowEngine::new(tmp.path()).unwrap();

        let def = make_def("test-cmd", vec![make_command_step("echo", "echo hello", vec![])]);

        let instance = engine.start(&def, serde_json::json!({})).await.unwrap();

        // Command runs asynchronously via tokio::spawn — poll until the
        // WORKFLOW reaches a terminal state. The step transitions first;
        // there's a brief window where the step is `Completed` but the
        // workflow status is still `Running` while the engine wraps up.
        // Polling on the workflow status covers both transitions.
        let final_state = poll_until(
            &engine,
            &instance.id,
            std::time::Duration::from_secs(10),
            "workflow terminal",
            |s| !matches!(s.status, WorkflowStatus::Running | WorkflowStatus::Paused),
        )
        .await;

        assert_eq!(
            final_state.step_states["echo"].status,
            StepStatus::Completed
        );
        assert_eq!(final_state.status, WorkflowStatus::Completed);

        let result = final_state.step_states["echo"].result.as_ref().unwrap();
        assert_eq!(result["exit_code"], 0);
        assert!(result["stdout"].as_str().unwrap().contains("hello"));
    }

    #[tokio::test]
    async fn test_persistence_across_loads() {
        let tmp = TempDir::new().unwrap();
        let engine = WorkflowEngine::new(tmp.path()).unwrap();

        let def = make_def("test-persist", vec![make_step("s1", vec![])]);

        let instance = engine
            .start(&def, serde_json::json!({"key": "val"}))
            .await
            .unwrap();
        let instance_id = instance.id.clone();

        let engine2 = WorkflowEngine::new(tmp.path()).unwrap();
        let loaded = engine2.status(&instance_id).unwrap();

        assert_eq!(loaded.id, instance_id);
        assert_eq!(loaded.definition_id, "test-persist");
        assert_eq!(loaded.context["key"], "val");
        assert_eq!(loaded.step_states["s1"].status, StepStatus::Running);
    }

    #[tokio::test]
    async fn test_resume_after_crash() {
        let tmp = TempDir::new().unwrap();
        let engine = WorkflowEngine::new(tmp.path()).unwrap();

        let def = make_def(
            "test-resume",
            vec![
                make_step("s1", vec![]),
                make_step("s2", vec!["s1"]),
                make_step("s3", vec!["s2"]),
            ],
        );

        let instance = engine.start(&def, serde_json::json!({})).await.unwrap();
        engine
            .complete_step(&instance.id, "s1", serde_json::json!({}))
            .await
            .unwrap();

        let engine2 = WorkflowEngine::new(tmp.path()).unwrap();
        engine2
            .complete_step(&instance.id, "s2", serde_json::json!({}))
            .await
            .unwrap();

        let resumed = engine2.status(&instance.id).unwrap();
        assert_eq!(resumed.step_states["s3"].status, StepStatus::Running);
    }

    #[tokio::test]
    async fn test_condition_skips_step() {
        let tmp = TempDir::new().unwrap();
        let engine = WorkflowEngine::new(tmp.path()).unwrap();

        let def = WorkflowDefinition {
            id: "test-condition".to_string(),
            name: "Test Condition".to_string(),
            steps: vec![
                make_step("s1", vec![]),
                WorkflowStep {
                    id: "s2".to_string(),
                    name: "Conditional step".to_string(),
                    action: StepAction::Custom {
                        handler: "test".to_string(),
                        config: serde_json::json!({}),
                    },
                    depends_on: vec!["s1".to_string()],
                    condition: Some(StepCondition::Expression {
                        expr: "should_deploy".to_string(),
                    }),
                    retry: RetryPolicy::default(),
                    timeout_secs: 60,
                    timeout_ms: None,
                    parallel: false,
                    parallel_group: None,
                    max_iterations: None,
                },
            ],
            on_error: ErrorPolicy::FailFast,
            version: 1,
            template: false,
        };

        let instance = engine
            .start(&def, serde_json::json!({"should_deploy": false}))
            .await
            .unwrap();

        engine
            .complete_step(&instance.id, "s1", serde_json::json!({}))
            .await
            .unwrap();

        let final_state = engine.status(&instance.id).unwrap();
        assert_eq!(final_state.step_states["s2"].status, StepStatus::Skipped);
        assert_eq!(final_state.status, WorkflowStatus::Completed);
    }

    #[tokio::test]
    async fn test_conditional_on_success() {
        let tmp = TempDir::new().unwrap();
        let engine = WorkflowEngine::new(tmp.path()).unwrap();

        let def = WorkflowDefinition {
            id: "test-on-success".to_string(),
            name: "Test OnSuccess Condition".to_string(),
            steps: vec![
                make_step("build", vec![]),
                WorkflowStep {
                    id: "deploy".to_string(),
                    name: "Deploy".to_string(),
                    action: StepAction::Custom {
                        handler: "test".to_string(),
                        config: serde_json::json!({}),
                    },
                    depends_on: vec!["build".to_string()],
                    condition: Some(StepCondition::OnSuccess {
                        step_id: "build".to_string(),
                    }),
                    retry: RetryPolicy::default(),
                    timeout_secs: 60,
                    timeout_ms: None,
                    parallel: false,
                    parallel_group: None,
                    max_iterations: None,
                },
            ],
            on_error: ErrorPolicy::ContinueOnError,
            version: 1,
            template: false,
        };

        let instance = engine.start(&def, serde_json::json!({})).await.unwrap();

        engine
            .complete_step(&instance.id, "build", serde_json::json!({"ok": true}))
            .await
            .unwrap();

        let state = engine.status(&instance.id).unwrap();
        assert_eq!(state.step_states["deploy"].status, StepStatus::Running);
    }

    #[tokio::test]
    async fn test_conditional_on_failure() {
        let tmp = TempDir::new().unwrap();
        let engine = WorkflowEngine::new(tmp.path()).unwrap();

        let def = WorkflowDefinition {
            id: "test-on-failure".to_string(),
            name: "Test OnFailure Condition".to_string(),
            steps: vec![
                make_step("build", vec![]),
                WorkflowStep {
                    id: "notify_failure".to_string(),
                    name: "Notify Failure".to_string(),
                    action: StepAction::Custom {
                        handler: "test".to_string(),
                        config: serde_json::json!({}),
                    },
                    depends_on: vec!["build".to_string()],
                    condition: Some(StepCondition::OnFailure {
                        step_id: "build".to_string(),
                    }),
                    retry: RetryPolicy::default(),
                    timeout_secs: 60,
                    timeout_ms: None,
                    parallel: false,
                    parallel_group: None,
                    max_iterations: None,
                },
            ],
            on_error: ErrorPolicy::ContinueOnError,
            version: 1,
            template: false,
        };

        let instance = engine.start(&def, serde_json::json!({})).await.unwrap();

        engine
            .fail_step(&instance.id, "build", "compilation error")
            .await
            .unwrap();

        let state = engine.status(&instance.id).unwrap();
        assert_eq!(
            state.step_states["notify_failure"].status,
            StepStatus::Running
        );
    }

    #[tokio::test]
    async fn test_list_workflows() {
        let tmp = TempDir::new().unwrap();
        let engine = WorkflowEngine::new(tmp.path()).unwrap();

        let def = make_def("test-list", vec![make_step("s1", vec![])]);

        engine.start(&def, serde_json::json!({})).await.unwrap();
        engine.start(&def, serde_json::json!({})).await.unwrap();

        let all = engine.list().unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_continue_on_error() {
        let tmp = TempDir::new().unwrap();
        let engine = WorkflowEngine::new(tmp.path()).unwrap();

        let def = WorkflowDefinition {
            id: "test-continue".to_string(),
            name: "Test ContinueOnError".to_string(),
            steps: vec![make_step("a", vec![]), make_step("b", vec![])],
            on_error: ErrorPolicy::ContinueOnError,
            version: 1,
            template: false,
        };

        let instance = engine.start(&def, serde_json::json!({})).await.unwrap();

        engine
            .fail_step(&instance.id, "a", "boom")
            .await
            .unwrap();

        let state = engine.status(&instance.id).unwrap();
        assert_eq!(state.step_states["a"].status, StepStatus::Failed);
        assert_eq!(state.step_states["b"].status, StepStatus::Running);
    }

    // -- Max iterations --

    #[tokio::test]
    async fn test_max_iterations_prevents_runaway() {
        let tmp = TempDir::new().unwrap();
        let engine = WorkflowEngine::new(tmp.path()).unwrap();

        let def = WorkflowDefinition {
            id: "test-max-iter".to_string(),
            name: "Test Max Iterations".to_string(),
            steps: vec![WorkflowStep {
                id: "limited".to_string(),
                name: "Limited Step".to_string(),
                action: StepAction::Custom {
                    handler: "test".to_string(),
                    config: serde_json::json!({}),
                },
                depends_on: vec![],
                condition: None,
                retry: RetryPolicy::default(),
                timeout_secs: 60,
                timeout_ms: None,
                parallel: false,
                parallel_group: None,
                max_iterations: Some(1),
            }],
            on_error: ErrorPolicy::FailFast,
            version: 1,
            template: false,
        };

        let instance = engine.start(&def, serde_json::json!({})).await.unwrap();
        assert_eq!(
            instance.step_states["limited"].status,
            StepStatus::Running
        );
        assert_eq!(instance.step_states["limited"].execution_count, 1);
    }

    // -- Versioning --

    #[tokio::test]
    async fn test_workflow_versioning() {
        let tmp = TempDir::new().unwrap();
        let engine = WorkflowEngine::new(tmp.path()).unwrap();

        let def_v1 = WorkflowDefinition {
            id: "versioned-wf".to_string(),
            name: "Versioned Workflow".to_string(),
            steps: vec![make_step("s1", vec![])],
            on_error: ErrorPolicy::FailFast,
            version: 1,
            template: false,
        };

        let def_v2 = WorkflowDefinition {
            id: "versioned-wf".to_string(),
            name: "Versioned Workflow v2".to_string(),
            steps: vec![make_step("s1", vec![]), make_step("s2", vec!["s1"])],
            on_error: ErrorPolicy::FailFast,
            version: 2,
            template: false,
        };

        let inst1 = engine.start(&def_v1, serde_json::json!({})).await.unwrap();
        let inst2 = engine.start(&def_v2, serde_json::json!({})).await.unwrap();

        assert_eq!(inst1.definition.version, 1);
        assert_eq!(inst1.definition.steps.len(), 1);
        assert_eq!(inst2.definition.version, 2);
        assert_eq!(inst2.definition.steps.len(), 2);
    }

    // -- Templates --

    #[tokio::test]
    async fn test_template_instantiation() {
        let tmp = TempDir::new().unwrap();
        let engine = WorkflowEngine::new(tmp.path()).unwrap();

        let template = WorkflowDefinition {
            id: "deploy-template".to_string(),
            name: "Deploy {{environment}}".to_string(),
            steps: vec![WorkflowStep {
                id: "deploy".to_string(),
                name: "Deploy to {{environment}}".to_string(),
                action: StepAction::Command {
                    cmd: "echo deploying to {{environment}}".to_string(),
                    cwd: None,
                },
                depends_on: vec![],
                condition: None,
                retry: RetryPolicy::default(),
                timeout_secs: 60,
                timeout_ms: None,
                parallel: false,
                parallel_group: None,
                max_iterations: None,
            }],
            on_error: ErrorPolicy::FailFast,
            version: 1,
            template: true,
        };

        engine.register_template(template).await.unwrap();

        let instantiated = engine
            .instantiate(
                "deploy-template",
                serde_json::json!({"environment": "production"}),
            )
            .await
            .unwrap();

        assert!(instantiated.id.starts_with("deploy-template-"));
        assert_eq!(instantiated.name, "Deploy production");
        assert!(!instantiated.template);

        if let StepAction::Command { ref cmd, .. } = instantiated.steps[0].action {
            assert_eq!(cmd, "echo deploying to production");
        } else {
            panic!("expected Command action");
        }
    }

    #[tokio::test]
    async fn test_template_requires_flag() {
        let tmp = TempDir::new().unwrap();
        let engine = WorkflowEngine::new(tmp.path()).unwrap();

        let non_template = make_def("not-a-template", vec![make_step("s1", vec![])]);

        let result = engine.register_template(non_template).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            WorkflowError::ValidationError(_)
        ));
    }

    // -- Step timeout --

    #[tokio::test]
    async fn test_step_timeout_ms() {
        let tmp = TempDir::new().unwrap();
        let engine = WorkflowEngine::new(tmp.path()).unwrap();

        let def = WorkflowDefinition {
            id: "test-timeout".to_string(),
            name: "Test Timeout".to_string(),
            steps: vec![WorkflowStep {
                id: "slow".to_string(),
                name: "Slow Step".to_string(),
                action: StepAction::Custom {
                    handler: "test".to_string(),
                    config: serde_json::json!({}),
                },
                depends_on: vec![],
                condition: None,
                retry: RetryPolicy::default(),
                timeout_secs: 0,
                timeout_ms: Some(200),
                parallel: false,
                parallel_group: None,
                max_iterations: None,
            }],
            on_error: ErrorPolicy::FailFast,
            version: 1,
            template: false,
        };

        let instance = engine.start(&def, serde_json::json!({})).await.unwrap();
        assert_eq!(instance.step_states["slow"].status, StepStatus::Running);

        // Poll until the timeout watchdog fires — fixed sleep raced under
        // parallel CI load.
        let final_state = poll_until(
            &engine,
            &instance.id,
            std::time::Duration::from_secs(5),
            "slow step TimedOut",
            |s| matches!(s.step_states["slow"].status, StepStatus::TimedOut),
        )
        .await;
        assert_eq!(
            final_state.step_states["slow"].status,
            StepStatus::TimedOut
        );
        assert!(final_state.step_states["slow"]
            .error
            .as_ref()
            .unwrap()
            .contains("timed out"));
    }

    #[tokio::test]
    async fn test_command_timeout() {
        let tmp = TempDir::new().unwrap();
        let engine = WorkflowEngine::new(tmp.path()).unwrap();

        let def = WorkflowDefinition {
            id: "test-cmd-timeout".to_string(),
            name: "Test Command Timeout".to_string(),
            steps: vec![WorkflowStep {
                id: "slow_cmd".to_string(),
                name: "Slow Command".to_string(),
                action: StepAction::Command {
                    cmd: "sleep 10".to_string(),
                    cwd: None,
                },
                depends_on: vec![],
                condition: None,
                retry: RetryPolicy::default(),
                timeout_secs: 0,
                timeout_ms: Some(200),
                parallel: false,
                parallel_group: None,
                max_iterations: None,
            }],
            on_error: ErrorPolicy::FailFast,
            version: 1,
            template: false,
        };

        let instance = engine.start(&def, serde_json::json!({})).await.unwrap();

        // Poll until the command-step timeout watchdog fires.
        let final_state = poll_until(
            &engine,
            &instance.id,
            std::time::Duration::from_secs(5),
            "slow_cmd TimedOut",
            |s| matches!(s.step_states["slow_cmd"].status, StepStatus::TimedOut),
        )
        .await;
        assert_eq!(
            final_state.step_states["slow_cmd"].status,
            StepStatus::TimedOut
        );
    }

    // -- Start rejects cyclic workflows --

    #[tokio::test]
    async fn test_start_rejects_cyclic_workflow() {
        let tmp = TempDir::new().unwrap();
        let engine = WorkflowEngine::new(tmp.path()).unwrap();

        let def = WorkflowDefinition {
            id: "cyclic".to_string(),
            name: "Cyclic Workflow".to_string(),
            steps: vec![make_step("a", vec!["b"]), make_step("b", vec!["a"])],
            on_error: ErrorPolicy::FailFast,
            version: 1,
            template: false,
        };

        let result = engine.start(&def, serde_json::json!({})).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            WorkflowError::CycleDetected(_)
        ));
    }
}
