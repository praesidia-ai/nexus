//! `WorkflowRunner` — executes a `WorkflowDag` using an `AgentPool`.
//!
//! The runner resolves the DAG into execution layers, runs each layer's steps
//! **in parallel** (each step locks only its own agent), accumulates their
//! outputs, and feeds them as context into later steps.
//!
//! Steps support:
//! - **Conditional execution** via `step.condition`
//! - **Retry with exponential backoff** via `step.max_retries`
//! - **Loop iterations** via `step.loop_count`

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::task::JoinSet;
use tracing::{info, instrument, warn};

use nexus_zeroclaw::{AgentMessage, AgentName, AgentPool, ContextSnippet, MessageKind};

use super::dag::{StepId, WorkflowDag, WorkflowStep};

// ---------------------------------------------------------------------------
// WorkflowContext
// ---------------------------------------------------------------------------

/// User-provided inputs that seed the workflow.
///
/// Any key/value pair here is available for `{key}` interpolation in every
/// step's prompt template.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct WorkflowContext {
    pub inputs: HashMap<String, String>,
}

impl WorkflowContext {
    pub fn new() -> Self { Self::default() }

    pub fn with(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.inputs.insert(key.into(), value.into());
        self
    }
}

// ---------------------------------------------------------------------------
// StepOutput
// ---------------------------------------------------------------------------

/// The result of executing a single workflow step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepOutput {
    pub step_id: StepId,
    pub agent: AgentName,
    pub output: String,
    /// Number of retries that were needed (0 = succeeded first try).
    #[serde(default)]
    pub retries_used: u32,
    /// Whether the step was skipped due to its condition evaluating to false.
    #[serde(default)]
    pub skipped: bool,
    /// Execution duration in milliseconds.
    #[serde(default)]
    pub duration_ms: u64,
}

// ---------------------------------------------------------------------------
// WorkflowResult
// ---------------------------------------------------------------------------

/// The aggregated result of a completed workflow run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowResult {
    pub dag_name: String,
    /// Outputs for every step, in execution order.
    pub steps: Vec<StepOutput>,
    /// The output of the last non-skipped step (the typical "final answer").
    pub final_output: String,
    /// Total execution time in milliseconds.
    #[serde(default)]
    pub total_duration_ms: u64,
}

impl WorkflowResult {
    /// Look up the output of a specific step by its ID.
    pub fn step_output(&self, step_id: &str) -> Option<&str> {
        self.steps
            .iter()
            .find(|s| s.step_id == step_id)
            .map(|s| s.output.as_str())
    }
}

// ---------------------------------------------------------------------------
// WorkflowRunner
// ---------------------------------------------------------------------------

/// Executes `WorkflowDag`s using a shared `AgentPool`.
///
/// Steps within the same layer are executed **in parallel** via `tokio::spawn`.
/// Each step locks only its own agent in the pool, so different agents can
/// work simultaneously.
///
/// # Example
/// ```rust,ignore
/// let pool = Arc::new(AgentPool::new(RosterConfig::from_env()));
/// let runner = WorkflowRunner::new(pool);
///
/// let ctx = WorkflowContext::new()
///     .with("requirements", "Build a SaaS invoicing app in Rust + Next.js");
///
/// let result = runner.run(&pipelines::app_build_dag(), ctx).await?;
/// println!("{}", result.final_output);
/// ```
pub struct WorkflowRunner {
    pool: Arc<AgentPool>,
}

impl WorkflowRunner {
    pub fn new(pool: Arc<AgentPool>) -> Self {
        Self { pool }
    }

    /// Execute the DAG and return the collected results.
    ///
    /// Steps within the same layer run in parallel. Conditions, retries, and
    /// loop iterations are handled per-step.
    #[instrument(skip(self, dag, ctx), fields(dag = %dag.name))]
    pub async fn run(
        &self,
        dag: &WorkflowDag,
        ctx: WorkflowContext,
    ) -> anyhow::Result<WorkflowResult> {
        let run_start = std::time::Instant::now();
        info!(steps = dag.steps.len(), "Starting workflow");

        let layers = dag.execution_layers()?;

        // Accumulated variables: initial inputs + step outputs
        let mut vars: HashMap<String, String> = ctx.inputs.clone();
        let mut all_outputs: Vec<StepOutput> = Vec::new();
        let mut final_output = String::new();

        for (layer_idx, layer) in layers.iter().enumerate() {
            info!(layer = layer_idx, count = layer.len(), "Executing layer");

            if layer.len() == 1 {
                // Single step — no need for spawn overhead
                let step = layer[0];
                let step_out = self.execute_step(step, &vars).await?;
                if !step_out.skipped {
                    vars.insert(step.id.clone(), step_out.output.clone());
                    final_output = step_out.output.clone();
                }
                all_outputs.push(step_out);
            } else {
                // Multiple steps — run in parallel
                let layer_outputs = self.execute_layer_parallel(layer, &vars).await?;
                for step_out in &layer_outputs {
                    if !step_out.skipped {
                        vars.insert(step_out.step_id.clone(), step_out.output.clone());
                        final_output = step_out.output.clone();
                    }
                }
                all_outputs.extend(layer_outputs);
            }
        }

        let total_duration_ms = run_start.elapsed().as_millis() as u64;

        Ok(WorkflowResult {
            dag_name: dag.name.clone(),
            steps: all_outputs,
            final_output,
            total_duration_ms,
        })
    }

    /// Execute all steps in a layer concurrently using `JoinSet`.
    async fn execute_layer_parallel(
        &self,
        layer: &[&WorkflowStep],
        vars: &HashMap<String, String>,
    ) -> anyhow::Result<Vec<StepOutput>> {
        let mut join_set: JoinSet<anyhow::Result<StepOutput>> = JoinSet::new();

        for &step in layer {
            let pool = self.pool.clone();
            let vars = vars.clone();
            let step = step.clone();

            join_set.spawn(async move {
                execute_step_with_pool(&pool, &step, &vars).await
            });
        }

        let mut outputs = Vec::with_capacity(layer.len());
        while let Some(result) = join_set.join_next().await {
            outputs.push(result??);
        }

        // Sort by step_id for deterministic ordering
        outputs.sort_by(|a, b| a.step_id.cmp(&b.step_id));
        Ok(outputs)
    }

    /// Execute a single step (used for single-step layers to avoid spawn).
    async fn execute_step(
        &self,
        step: &WorkflowStep,
        vars: &HashMap<String, String>,
    ) -> anyhow::Result<StepOutput> {
        execute_step_with_pool(&self.pool, step, vars).await
    }
}

/// Core step execution logic — extracted so it can be used from both
/// the sequential and parallel paths.
async fn execute_step_with_pool(
    pool: &AgentPool,
    step: &WorkflowStep,
    vars: &HashMap<String, String>,
) -> anyhow::Result<StepOutput> {
    let step_start = std::time::Instant::now();

    // Check condition
    if !step.evaluate_condition(vars) {
        info!(step = %step.id, "Step skipped (condition not met)");
        return Ok(StepOutput {
            step_id: step.id.clone(),
            agent: step.agent,
            output: String::new(),
            retries_used: 0,
            skipped: true,
            duration_ms: step_start.elapsed().as_millis() as u64,
        });
    }

    // Build context snippets from dependency outputs
    let context_snippets: Vec<ContextSnippet> = step
        .depends_on
        .iter()
        .filter_map(|dep_id| {
            vars.get(dep_id).map(|content| {
                ContextSnippet::new(
                    format!("Output of step '{}'", dep_id),
                    content.clone(),
                )
            })
        })
        .collect();

    let mut final_output = String::new();
    let mut retries_used = 0;

    // Loop iterations
    for iteration in 0..step.loop_count.max(1) {
        let mut attempt_vars = vars.clone();
        if iteration > 0 {
            attempt_vars.insert(
                format!("{}_iter_{}", step.id, iteration - 1),
                final_output.clone(),
            );
        }

        let prompt = step.render_prompt(&attempt_vars);

        let msg = AgentMessage {
            from: None,
            to: step.agent,
            content: prompt,
            kind: MessageKind::Task,
            context: context_snippets.clone(),
        };

        // Retry loop with exponential backoff
        let mut last_error: Option<anyhow::Error> = None;
        let max_attempts = step.max_retries + 1;

        for attempt in 0..max_attempts {
            match pool.send(msg.clone()).await {
                Ok(reply) => {
                    final_output = reply.content;
                    last_error = None;
                    if attempt > 0 {
                        retries_used += attempt;
                    }
                    break;
                }
                Err(e) => {
                    last_error = Some(e);
                    if attempt + 1 < max_attempts {
                        let delay = step.retry_delay_ms * 2u64.pow(attempt);
                        warn!(
                            step = %step.id,
                            attempt = attempt + 1,
                            max = max_attempts,
                            delay_ms = delay,
                            "Step failed, retrying"
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                    }
                }
            }
        }

        if let Some(e) = last_error {
            return Err(e);
        }

        info!(
            step = %step.id,
            iteration = iteration,
            len = final_output.len(),
            "Step iteration completed"
        );
    }

    Ok(StepOutput {
        step_id: step.id.clone(),
        agent: step.agent,
        output: final_output,
        retries_used,
        skipped: false,
        duration_ms: step_start.elapsed().as_millis() as u64,
    })
}
