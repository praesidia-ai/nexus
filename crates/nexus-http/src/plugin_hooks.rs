//! Plugin Hook System — lifecycle hooks that plugins can intercept.
//!
//! Hooks are fired at key points in the generation pipeline:
//! - `on_intent_parsed` — after intent analysis, before decisions
//! - `on_decision_made` — after architecture decisions
//! - `on_before_codegen` — before code generation starts
//! - `on_after_generation` — after code is generated
//! - `on_taste_score` — after taste scoring, before redesign
//! - `on_build_result` — after build success/failure
//!
//! Plugins register hook handlers as prompt templates or JSON transforms.
//! Hooks execute in plugin priority order and can modify the pipeline context.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::info;

use crate::state::AppState;

// ---------------------------------------------------------------------------
// Hook types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookPoint {
    OnIntentParsed,
    OnDecisionMade,
    OnBeforeCodegen,
    OnAfterGeneration,
    OnTasteScore,
    OnBuildResult,
}

impl HookPoint {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OnIntentParsed => "on_intent_parsed",
            Self::OnDecisionMade => "on_decision_made",
            Self::OnBeforeCodegen => "on_before_codegen",
            Self::OnAfterGeneration => "on_after_generation",
            Self::OnTasteScore => "on_taste_score",
            Self::OnBuildResult => "on_build_result",
        }
    }
}

/// Context passed to hooks — mutable so plugins can modify pipeline state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookContext {
    pub hook: String,
    pub project_id: String,
    /// Tenant owning `project_id`. `None` means no tenant filtering is
    /// applied — only globally-scoped plugins (those without an
    /// `allowed_tenants` restriction) will run.
    #[serde(default)]
    pub tenant_id: Option<String>,
    /// Intent analysis result (if available).
    #[serde(default)]
    pub intent: Option<Value>,
    /// Architecture decisions (if available).
    #[serde(default)]
    pub decisions: Option<Value>,
    /// Plan/spec (if available).
    #[serde(default)]
    pub plan: Option<Value>,
    /// Taste scores (if available).
    #[serde(default)]
    pub taste_score: Option<Value>,
    /// Build result (if available).
    #[serde(default)]
    pub build_success: Option<bool>,
    /// Extra context (plugins can add arbitrary data).
    #[serde(default)]
    pub extra: Value,
    /// Modifications requested by plugins.
    #[serde(default)]
    pub modifications: Vec<HookModification>,
}

/// A modification requested by a plugin hook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookModification {
    pub plugin_id: String,
    pub action: HookAction,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HookAction {
    /// Override a decision.
    #[serde(rename = "override_decision")]
    OverrideDecision { area: String, value: String },
    /// Inject additional context into the LLM prompt.
    #[serde(rename = "inject_context")]
    InjectContext { context: String },
    /// Add a pipeline step.
    #[serde(rename = "add_step")]
    AddStep { step_id: String, position: String },
    /// Skip a pipeline step.
    #[serde(rename = "skip_step")]
    SkipStep { step_id: String },
    /// Log a message (observability only, no mutation).
    #[serde(rename = "log")]
    Log { message: String },
}

/// Result of running all hooks at a given point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookResult {
    pub hook: String,
    pub plugins_executed: usize,
    pub modifications: Vec<HookModification>,
    /// Injected context strings to add to LLM prompts.
    pub injected_context: Vec<String>,
    /// Decision overrides to apply.
    pub decision_overrides: Vec<(String, String)>,
    /// Steps to skip.
    pub steps_to_skip: Vec<String>,
}

// ---------------------------------------------------------------------------
// Hook execution
// ---------------------------------------------------------------------------

/// Fire a hook at a given lifecycle point. Executes all matching plugin hooks.
pub async fn fire_hook(
    app: &Arc<AppState>,
    hook_point: HookPoint,
    context: &mut HookContext,
) -> HookResult {
    let registry = app.legacy_plugin_registry.read().await;
    // Filter plugins by the caller's tenant so a plugin installed for
    // tenant A cannot inject hooks into tenant B's pipeline unless the
    // plugin manifest opts in via empty `allowed_tenants` (global plugin).
    let tenant = context.tenant_id.as_deref();
    let decision_rules = registry.decision_rules_for_tenant(tenant);
    let pipeline_steps = registry.pipeline_steps_for_tenant(tenant);
    let registered_hooks = registry.hooks_for_tenant(tenant);

    let mut result = HookResult {
        hook: hook_point.as_str().to_string(),
        plugins_executed: 0,
        modifications: Vec::new(),
        injected_context: Vec::new(),
        decision_overrides: Vec::new(),
        steps_to_skip: Vec::new(),
    };

    match hook_point {
        HookPoint::OnDecisionMade => {
            // Apply decision rule overrides from plugins
            for (plugin_id, rule) in &decision_rules {
                let description_lower = context
                    .extra
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("")
                    .to_lowercase();

                let matches = rule
                    .when
                    .iter()
                    .any(|k| description_lower.contains(&k.to_lowercase()));

                if matches {
                    let modification = HookModification {
                        plugin_id: plugin_id.to_string(),
                        action: HookAction::OverrideDecision {
                            area: rule.area.clone(),
                            value: rule.then.clone(),
                        },
                        description: rule.description.clone(),
                    };

                    result.decision_overrides.push((rule.area.clone(), rule.then.clone()));
                    result.modifications.push(modification.clone());
                    context.modifications.push(modification);
                    result.plugins_executed += 1;

                    info!(
                        plugin = %plugin_id,
                        area = %rule.area,
                        value = %rule.then,
                        "Plugin decision override applied"
                    );
                }
            }
        }
        HookPoint::OnBeforeCodegen => {
            // Apply pipeline step modifications from plugins
            for (plugin_id, step) in &pipeline_steps {
                if let Some(ref skip_cond) = step.skip_condition {
                    let description_lower = context
                        .extra
                        .get("description")
                        .and_then(|d| d.as_str())
                        .unwrap_or("")
                        .to_lowercase();

                    if description_lower.contains(&skip_cond.to_lowercase()) {
                        result.steps_to_skip.push(step.id.clone());
                        continue;
                    }
                }

                // Inject pipeline step context
                if !step.prompt_template.is_empty() {
                    result.injected_context.push(format!(
                        "## Plugin Step: {} (from {})\n{}",
                        step.name, plugin_id, step.description
                    ));
                    result.plugins_executed += 1;
                }
            }
        }
        HookPoint::OnIntentParsed => {
            // Plugins can inject additional context hints based on the parsed intent.
            // Check design systems and inject relevant context.
            let design_systems = registry.all_design_systems();
            for (plugin_id, ds) in &design_systems {
                let description_lower = context
                    .extra
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("")
                    .to_lowercase();

                // Auto-suggest design system if keywords match the description
                let ds_lower = ds.name.to_lowercase();
                let matches = ds_lower.split_whitespace().any(|word| description_lower.contains(word))
                    || description_lower.contains(&ds.id.to_lowercase());

                if matches {
                    result.injected_context.push(format!(
                        "## Plugin Design System: {} (from {})\n{}\nApply this design system CSS to globals.css.",
                        ds.name, plugin_id, ds.description
                    ));
                    result.plugins_executed += 1;

                    info!(
                        plugin = %plugin_id,
                        design_system = %ds.name,
                        "Plugin design system injected for matching intent"
                    );
                }
            }
        }

        HookPoint::OnAfterGeneration => {
            // Inject post-generation validation hints from plugins.
            // Each enabled plugin contributes its skill rules as validation context.
            let skills = registry.all_skills();
            let mut injected_rules: Vec<String> = Vec::new();

            for (_plugin_id, skill) in &skills {
                for rule in &skill.rules {
                    if !injected_rules.contains(rule) {
                        injected_rules.push(rule.clone());
                    }
                }
            }

            if !injected_rules.is_empty() {
                result.injected_context.push(format!(
                    "## Plugin Validation Rules\nVerify the generated code follows these rules:\n{}",
                    injected_rules.iter().map(|r| format!("- {}", r)).collect::<Vec<_>>().join("\n")
                ));
                result.plugins_executed += injected_rules.len();
            }
        }

        HookPoint::OnTasteScore => {
            // When taste score is low, plugins can contribute improvement strategies.
            if let Some(score_val) = context.taste_score.as_ref() {
                let overall = score_val.get("overall").and_then(|v| v.as_u64()).unwrap_or(100);

                if overall < 70 {
                    let design_systems = registry.all_design_systems();
                    for (plugin_id, ds) in &design_systems {
                        if !ds.css.is_empty() {
                            let modification = HookModification {
                                plugin_id: plugin_id.to_string(),
                                action: HookAction::InjectContext {
                                    context: format!(
                                        "Apply {} design system to improve visual quality score. CSS: {}",
                                        ds.name,
                                        &ds.css[..ds.css.len().min(200)]
                                    ),
                                },
                                description: format!(
                                    "Taste score {} is below 70 — inject {} design system",
                                    overall, ds.name
                                ),
                            };

                            result.injected_context.push(format!(
                                "Design system '{}' from plugin '{}' available to improve taste.",
                                ds.name, plugin_id
                            ));
                            result.modifications.push(modification.clone());
                            context.modifications.push(modification);
                            result.plugins_executed += 1;

                            info!(
                                plugin = %plugin_id,
                                taste_score = overall,
                                "Plugin design system suggested for low taste score"
                            );
                        }
                    }
                }
            }
        }

        HookPoint::OnBuildResult => {
            // Record build result for the decision learning system.
            if let Some(build_ok) = context.build_success {
                let description = context
                    .extra
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("")
                    .to_string();

                result.injected_context.push(format!(
                    "Build {}: project_id={}, description_hint={}",
                    if build_ok { "SUCCEEDED" } else { "FAILED" },
                    context.project_id,
                    &description[..description.len().min(80)]
                ));

                // Self-healing hint injection on failure
                if !build_ok {
                    let agents = registry.all_agents();
                    for (plugin_id, agent) in agents.iter().filter(|(_, a)| {
                        a.role.to_lowercase().contains("debug")
                            || a.role.to_lowercase().contains("fix")
                            || a.role.to_lowercase().contains("repair")
                    }) {
                        result.injected_context.push(format!(
                            "Repair agent '{}' (plugin: {}) available. Role: {}",
                            agent.name, plugin_id, agent.role
                        ));
                        result.plugins_executed += 1;
                    }
                }
            }
        }
    }

    // Process explicitly registered plugin hooks for this hook point
    let hook_name = hook_point.as_str();
    let description_lower = context
        .extra
        .get("description")
        .and_then(|d| d.as_str())
        .unwrap_or("")
        .to_lowercase();

    for (plugin_id, hook_reg) in &registered_hooks {
        if hook_reg.hook != hook_name {
            continue;
        }

        // Check `when` condition
        let condition_met = hook_reg.when.is_empty()
            || hook_reg.when.iter().any(|k| description_lower.contains(&k.to_lowercase()));

        if !condition_met {
            continue;
        }

        // Inject context
        if let Some(ref ctx_str) = hook_reg.inject_context {
            result.injected_context.push(format!(
                "## Plugin Hook Context ({})\n{}",
                plugin_id, ctx_str
            ));
        }

        // Apply decision override
        if let Some(ref override_rule) = hook_reg.override_decision {
            result.decision_overrides.push((override_rule.area.clone(), override_rule.value.clone()));
            result.modifications.push(HookModification {
                plugin_id: plugin_id.to_string(),
                action: HookAction::OverrideDecision {
                    area: override_rule.area.clone(),
                    value: override_rule.value.clone(),
                },
                description: format!(
                    "Plugin hook override: {} = {}",
                    override_rule.area, override_rule.value
                ),
            });
        }

        result.plugins_executed += 1;
        info!(
            plugin = %plugin_id,
            hook = hook_name,
            "Registered plugin hook executed"
        );
    }

    drop(registry);
    result
}

/// Build a `HookContext` for a given lifecycle point with standard fields
/// populated. The tenant is unset — callers that know the tenant should use
/// [`build_context_for_tenant`] so tenant-scoped plugin filtering works.
pub fn build_context(
    project_id: &str,
    hook: HookPoint,
    description: Option<&str>,
) -> HookContext {
    build_context_for_tenant(project_id, None, hook, description)
}

/// Tenant-aware variant of [`build_context`]. When `tenant_id` is `Some`,
/// only plugins whose manifest lists this tenant in `allowed_tenants`
/// (or omits the list entirely, i.e. "global") will be fired.
pub fn build_context_for_tenant(
    project_id: &str,
    tenant_id: Option<&str>,
    hook: HookPoint,
    description: Option<&str>,
) -> HookContext {
    let mut extra = serde_json::Map::new();
    if let Some(desc) = description {
        extra.insert("description".into(), serde_json::Value::String(desc.to_string()));
    }

    HookContext {
        hook: hook.as_str().to_string(),
        project_id: project_id.to_string(),
        tenant_id: tenant_id.map(str::to_string),
        intent: None,
        decisions: None,
        plan: None,
        taste_score: None,
        build_success: None,
        extra: serde_json::Value::Object(extra),
        modifications: Vec::new(),
    }
}
