//! Workflow DAG — directed acyclic graph of agent tasks.
//!
//! A `WorkflowDag` is a description of work: which agents to invoke, in what
//! order, and how outputs from earlier steps feed into later ones.
//!
//! Steps are connected by dependency edges; the `WorkflowRunner` resolves
//! them into execution layers (groups of steps with all deps satisfied) and
//! runs each layer.

use serde::{Deserialize, Serialize};

use nexus_zeroclaw::AgentName;

// ---------------------------------------------------------------------------
// StepId
// ---------------------------------------------------------------------------

pub type StepId = String;

// ---------------------------------------------------------------------------
// WorkflowStep
// ---------------------------------------------------------------------------

/// A single unit of work in a `WorkflowDag`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    /// Unique identifier within the DAG (e.g. `"architecture"`, `"implement"`).
    pub id: StepId,

    /// Which specialist agent performs this step.
    pub agent: AgentName,

    /// Prompt template.  Variables are interpolated with `{key}` syntax:
    ///
    /// - `{<step_id>}` — the output of a previous step with that id.
    /// - Any key from `WorkflowContext.inputs` is also available.
    pub prompt_template: String,

    /// Step IDs that must complete before this step can start.
    #[serde(default)]
    pub depends_on: Vec<StepId>,

    /// Human-readable description shown in progress output.
    #[serde(default)]
    pub description: String,

    /// Optional condition expression.  If set, the step only runs when the
    /// condition evaluates to true.  Syntax: `{step_id}` to check if a
    /// previous step's output is non-empty, or `{step_id} contains <text>`
    /// for substring matching.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,

    /// Maximum number of retry attempts on failure (0 = no retries).
    #[serde(default)]
    pub max_retries: u32,

    /// Base delay in milliseconds between retries (exponential backoff).
    #[serde(default = "default_retry_delay_ms")]
    pub retry_delay_ms: u64,

    /// Number of times this step should loop (1 = run once, N = repeat N times).
    /// Each iteration's output is available as `{step_id}_iter_{n}`.
    #[serde(default = "default_loop_count")]
    pub loop_count: u32,
}

fn default_retry_delay_ms() -> u64 {
    1000
}

fn default_loop_count() -> u32 {
    1
}

impl WorkflowStep {
    /// Render the prompt by substituting `{key}` placeholders with values
    /// from `vars`.  Unknown keys are left as-is (no error).
    pub fn render_prompt(&self, vars: &std::collections::HashMap<String, String>) -> String {
        let mut out = self.prompt_template.clone();
        for (k, v) in vars {
            out = out.replace(&format!("{{{}}}", k), v);
        }
        out
    }

    /// Evaluate whether this step's condition is met.
    ///
    /// Returns `true` if there is no condition, or if the condition passes.
    ///
    /// Supported expressions:
    /// - `{step_id}` — true if the referenced step produced non-empty output
    /// - `{step_id} contains <text>` — true if the output contains the text
    /// - `{step_id} not_empty` — explicit non-empty check
    /// - `not {step_id}` — true if the referenced step output is empty or missing
    pub fn evaluate_condition(&self, vars: &std::collections::HashMap<String, String>) -> bool {
        let cond = match &self.condition {
            Some(c) => c.trim(),
            None => return true,
        };

        if cond.is_empty() {
            return true;
        }

        // `not {step_id}` — negate
        if let Some(rest) = cond.strip_prefix("not ") {
            let key = rest.trim().trim_matches(|c| c == '{' || c == '}');
            return vars.get(key).is_none_or(|v| v.trim().is_empty());
        }

        // `{step_id} contains <text>`
        if cond.contains(" contains ") {
            let parts: Vec<&str> = cond.splitn(2, " contains ").collect();
            if parts.len() == 2 {
                let key = parts[0].trim().trim_matches(|c| c == '{' || c == '}');
                let needle = parts[1].trim();
                return vars.get(key).is_some_and(|v| v.contains(needle));
            }
        }

        // `{step_id} not_empty` or just `{step_id}`
        let key = cond
            .trim_end_matches(" not_empty")
            .trim()
            .trim_matches(|c| c == '{' || c == '}');
        vars.get(key).is_some_and(|v| !v.trim().is_empty())
    }
}

// ---------------------------------------------------------------------------
// WorkflowDag
// ---------------------------------------------------------------------------

/// A named, directed acyclic graph of `WorkflowStep`s.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDag {
    /// Short machine-friendly name (e.g. `"app-build"`, `"code-review"`).
    pub name: String,

    /// One-line human-readable description.
    pub description: String,

    /// All steps in the DAG (order within the vec doesn't matter — the runner
    /// uses dependency edges to determine execution order).
    pub steps: Vec<WorkflowStep>,
}

impl WorkflowDag {
    /// Resolve steps into execution layers: each layer contains steps whose
    /// `depends_on` are all satisfied by previous layers.
    ///
    /// Returns an error if there are unknown dependency IDs or a cycle.
    pub fn execution_layers(&self) -> anyhow::Result<Vec<Vec<&WorkflowStep>>> {
        let step_ids: std::collections::HashSet<&str> =
            self.steps.iter().map(|s| s.id.as_str()).collect();

        // Validate: every dependency must refer to a known step
        for step in &self.steps {
            for dep in &step.depends_on {
                if !step_ids.contains(dep.as_str()) {
                    anyhow::bail!(
                        "Step '{}' depends on unknown step '{}'",
                        step.id, dep
                    );
                }
            }
        }

        // Kahn's algorithm for topological layering
        let mut in_degree: std::collections::HashMap<&str, usize> =
            self.steps.iter().map(|s| (s.id.as_str(), s.depends_on.len())).collect();

        let mut dependents: std::collections::HashMap<&str, Vec<&str>> =
            self.steps.iter().map(|s| (s.id.as_str(), vec![])).collect();

        for step in &self.steps {
            for dep in &step.depends_on {
                dependents.entry(dep.as_str()).or_default().push(step.id.as_str());
            }
        }

        let step_map: std::collections::HashMap<&str, &WorkflowStep> =
            self.steps.iter().map(|s| (s.id.as_str(), s)).collect();

        let mut layers: Vec<Vec<&WorkflowStep>> = vec![];
        let mut ready: Vec<&str> = in_degree
            .iter()
            .filter(|(_, &d)| d == 0)
            .map(|(&id, _)| id)
            .collect();
        ready.sort(); // deterministic order

        while !ready.is_empty() {
            let layer: Vec<&WorkflowStep> = ready
                .iter()
                .filter_map(|id| step_map.get(id).copied())
                .collect();
            layers.push(layer);

            let mut next_ready = vec![];
            for id in &ready {
                for &dep_on_us in dependents.get(id).map(Vec::as_slice).unwrap_or(&[]) {
                    let cnt = in_degree.get_mut(dep_on_us).unwrap();
                    *cnt -= 1;
                    if *cnt == 0 {
                        next_ready.push(dep_on_us);
                    }
                }
            }
            next_ready.sort();
            ready = next_ready;
        }

        let scheduled: usize = layers.iter().map(|l| l.len()).sum();
        if scheduled != self.steps.len() {
            anyhow::bail!("Cycle detected in workflow DAG '{}'", self.name);
        }

        Ok(layers)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn step(id: &str, agent: AgentName, deps: &[&str]) -> WorkflowStep {
        WorkflowStep {
            id: id.into(),
            agent,
            prompt_template: format!("Do {{task}} as {}", agent),
            depends_on: deps.iter().map(|s| s.to_string()).collect(),
            description: String::new(),
            condition: None,
            max_retries: 0,
            retry_delay_ms: 1000,
            loop_count: 1,
        }
    }

    #[test]
    fn simple_chain_layers() {
        let dag = WorkflowDag {
            name: "test".into(),
            description: "".into(),
            steps: vec![
                step("a", AgentName::Kai, &[]),
                step("b", AgentName::Nova, &["a"]),
                step("c", AgentName::Rex, &["b"]),
            ],
        };
        let layers = dag.execution_layers().unwrap();
        assert_eq!(layers.len(), 3);
        assert_eq!(layers[0][0].id, "a");
        assert_eq!(layers[1][0].id, "b");
        assert_eq!(layers[2][0].id, "c");
    }

    #[test]
    fn parallel_steps_same_layer() {
        let dag = WorkflowDag {
            name: "test".into(),
            description: "".into(),
            steps: vec![
                step("research", AgentName::Kai, &[]),
                step("design",   AgentName::Leo, &[]),
                step("implement", AgentName::Nova, &["research", "design"]),
            ],
        };
        let layers = dag.execution_layers().unwrap();
        assert_eq!(layers.len(), 2);
        assert_eq!(layers[0].len(), 2); // research + design in parallel
        assert_eq!(layers[1][0].id, "implement");
    }

    #[test]
    fn unknown_dep_errors() {
        let dag = WorkflowDag {
            name: "bad".into(),
            description: "".into(),
            steps: vec![step("a", AgentName::Nova, &["nonexistent"])],
        };
        assert!(dag.execution_layers().is_err());
    }

    #[test]
    fn prompt_template_interpolation() {
        let s = step("x", AgentName::Nova, &[]);
        let mut vars = std::collections::HashMap::new();
        vars.insert("task".into(), "build an API".into());
        let rendered = s.render_prompt(&vars);
        assert!(rendered.contains("build an API"));
    }

    #[test]
    fn condition_none_passes() {
        let s = step("a", AgentName::Nova, &[]);
        let vars = std::collections::HashMap::new();
        assert!(s.evaluate_condition(&vars));
    }

    #[test]
    fn condition_var_present() {
        let mut s = step("a", AgentName::Nova, &[]);
        s.condition = Some("{research}".into());
        let mut vars = std::collections::HashMap::new();
        assert!(!s.evaluate_condition(&vars)); // missing
        vars.insert("research".into(), "some output".into());
        assert!(s.evaluate_condition(&vars)); // present
    }

    #[test]
    fn condition_contains() {
        let mut s = step("a", AgentName::Nova, &[]);
        s.condition = Some("{research} contains vulnerability".into());
        let mut vars = std::collections::HashMap::new();
        vars.insert("research".into(), "Found a vulnerability in auth".into());
        assert!(s.evaluate_condition(&vars));
        vars.insert("research".into(), "All looks good".into());
        assert!(!s.evaluate_condition(&vars));
    }

    #[test]
    fn condition_not() {
        let mut s = step("a", AgentName::Nova, &[]);
        s.condition = Some("not {errors}".into());
        let mut vars = std::collections::HashMap::new();
        assert!(s.evaluate_condition(&vars)); // missing = empty = true
        vars.insert("errors".into(), "".into());
        assert!(s.evaluate_condition(&vars)); // empty = true
        vars.insert("errors".into(), "some error".into());
        assert!(!s.evaluate_condition(&vars)); // non-empty = false
    }
}
