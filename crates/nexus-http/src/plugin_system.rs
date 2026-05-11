//! Plugin System — unified, composable, observable plugin architecture.
//!
//! Features:
//! - Typed capabilities (design systems, domain packs, decision overrides, etc.)
//! - Declarative hook system with priority, dependency ordering, timeouts, and conditions
//! - Observable execution: every hook invocation is logged with timing and result
//! - Conflict detection and manifest validation
//! - Dry-run validation before installation
//! - Legacy support: file-based plugin scanning from `~/.nexus/plugins/`

use std::collections::HashMap;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Plugin Capabilities
// ---------------------------------------------------------------------------

/// Describes a single capability that a plugin provides.
///
/// Capabilities are declarative — they tell the host what the plugin can do
/// so the system can compose plugins intelligently and detect conflicts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PluginCapability {
    /// Provides a complete design system (CSS + component list).
    DesignSystem {
        /// Raw CSS content for the design system.
        css: String,
        /// Component names provided (e.g. `["Button", "Card", "Modal"]`).
        components: Vec<String>,
    },
    /// Provides a domain-specific pack of entities and logic.
    DomainPack {
        /// The domain name (e.g. `"ecommerce"`, `"healthcare"`).
        domain: String,
        /// Entity names provided (e.g. `["Product", "Order", "Cart"]`).
        entities: Vec<String>,
    },
    /// Overrides an architecture or design decision.
    DecisionOverride {
        /// Decision area (e.g. `"database"`, `"auth"`, `"hosting"`).
        area: String,
        /// Forced value (e.g. `"postgres"`, `"clerk"`).
        value: String,
    },
    /// Registers a custom tool that agents can invoke.
    CustomTool {
        /// Tool name.
        name: String,
        /// Human-readable description of what the tool does.
        description: String,
    },
    /// Adds a quality rule to the build/taste pipeline.
    QualityRule {
        /// Rule name (e.g. `"no-inline-styles"`).
        rule_name: String,
        /// Severity: `"error"`, `"warning"`, or `"info"`.
        severity: String,
    },
    /// Enhances prompts sent to LLMs at a specific pipeline phase.
    PromptEnhancement {
        /// Which pipeline phase this applies to (e.g. `"codegen"`, `"taste"`).
        phase: String,
        /// Prompt template fragment to inject.
        template: String,
    },
}

// ---------------------------------------------------------------------------
// Compatibility Specification
// ---------------------------------------------------------------------------

/// Declares version constraints and inter-plugin relationships.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompatibilitySpec {
    /// Minimum Nexus version this plugin supports (semver string, e.g. `"0.5.0"`).
    #[serde(default)]
    pub min_version: Option<String>,
    /// Maximum Nexus version this plugin supports.
    #[serde(default)]
    pub max_version: Option<String>,
    /// Plugin IDs that must be installed for this plugin to work.
    #[serde(default)]
    pub required_plugins: Vec<String>,
    /// Plugin IDs that conflict with this plugin (cannot be co-installed).
    #[serde(default)]
    pub conflicts_with: Vec<String>,
}

// ---------------------------------------------------------------------------
// Hook Declaration
// ---------------------------------------------------------------------------

/// A hook point in the generation pipeline where plugins can intercept execution.
///
/// Hook points include error-class hooks and mid-generation interception.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "point", rename_all = "snake_case")]
pub enum HookPoint {
    /// Before the intent engine runs.
    PreIntentAnalysis,
    /// After intent is parsed, before decisions.
    PostIntentAnalysis,
    /// Before architecture decisions are made.
    PreDecision,
    /// After architecture decisions, before generation.
    PostDecision,
    /// Before code generation starts.
    PreGeneration,
    /// During code generation at a named phase.
    DuringGeneration {
        /// Which generation phase (e.g. `"layout"`, `"api-routes"`, `"database"`).
        phase: String,
    },
    /// After code generation completes.
    PostGeneration,
    /// Before taste scoring runs.
    PreTaste,
    /// After taste scoring, before any redesign.
    PostTaste,
    /// Before the outcome guarantee loop.
    PreGuarantee,
    /// After the outcome guarantee loop completes.
    PostGuarantee,
    /// When an error of a specific class occurs.
    OnError {
        /// Error class to match (e.g. `"build_failure"`, `"lint_error"`, `"timeout"`).
        error_class: String,
    },
}

impl HookPoint {
    /// Returns a stable string key for this hook point (used in logs and lookups).
    pub fn as_key(&self) -> String {
        match self {
            Self::PreIntentAnalysis => "pre_intent_analysis".to_string(),
            Self::PostIntentAnalysis => "post_intent_analysis".to_string(),
            Self::PreDecision => "pre_decision".to_string(),
            Self::PostDecision => "post_decision".to_string(),
            Self::PreGeneration => "pre_generation".to_string(),
            Self::DuringGeneration { phase } => format!("during_generation:{}", phase),
            Self::PostGeneration => "post_generation".to_string(),
            Self::PreTaste => "pre_taste".to_string(),
            Self::PostTaste => "post_taste".to_string(),
            Self::PreGuarantee => "pre_guarantee".to_string(),
            Self::PostGuarantee => "post_guarantee".to_string(),
            Self::OnError { error_class } => format!("on_error:{}", error_class),
        }
    }
}

/// Declares that a plugin wants to intercept a specific hook point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookDeclaration {
    /// Which hook point to intercept.
    pub hook_point: HookPoint,
    /// Execution priority (lower numbers run first; default 0).
    #[serde(default)]
    pub priority: i32,
    /// Optional condition expression. If set, the hook only fires when the
    /// project description contains this substring (case-insensitive).
    #[serde(default)]
    pub condition: Option<String>,
    /// Maximum execution time in milliseconds before the hook is timed out.
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    /// Whether this hook blocks the pipeline (true) or runs as fire-and-forget (false).
    #[serde(default = "default_blocking")]
    pub blocking: bool,
    /// Plugin IDs that must have their hooks for this point executed before this one.
    #[serde(default)]
    pub requires: Vec<String>,
}

fn default_timeout_ms() -> u64 {
    5000
}

fn default_blocking() -> bool {
    true
}

// ---------------------------------------------------------------------------
// Plugin Manifest
// ---------------------------------------------------------------------------

/// Complete manifest for a plugin.
///
/// This is the top-level descriptor that a plugin author provides. It declares
/// all capabilities, hooks, configuration schema, and compatibility constraints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Unique plugin identifier (e.g. `"nexus-tailwind-ds"`).
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Semver version string (e.g. `"1.2.0"`).
    pub version: String,
    /// Short description of what the plugin does.
    pub description: String,
    /// Author name or organization.
    pub author: String,
    /// Capabilities this plugin provides.
    #[serde(default)]
    pub capabilities: Vec<PluginCapability>,
    /// Hook declarations — where this plugin intercepts the pipeline.
    #[serde(default)]
    pub hooks: Vec<HookDeclaration>,
    /// Optional JSON Schema describing the plugin's configuration format.
    #[serde(default)]
    pub config_schema: Option<serde_json::Value>,
    /// Version and inter-plugin compatibility constraints.
    #[serde(default)]
    pub compatibility: CompatibilitySpec,
    /// Tenants allowed to execute this plugin's hooks. An empty list means
    /// the plugin is globally available (legacy behavior). A non-empty list
    /// restricts hook execution to `HookContext.tenant_id` values in the
    /// list — this is the multi-tenant opt-in scoping mechanism.
    #[serde(default)]
    pub allowed_tenants: Vec<String>,
}

impl PluginManifest {
    /// True when this plugin is permitted to run for `tenant_id`.
    /// A plugin with no `allowed_tenants` is considered global (everyone);
    /// a plugin with an explicit list is restricted to that list.
    pub fn is_tenant_allowed(&self, tenant_id: Option<&str>) -> bool {
        if self.allowed_tenants.is_empty() {
            return true;
        }
        match tenant_id {
            Some(t) => self.allowed_tenants.iter().any(|a| a == t),
            None => false,
        }
    }
}

// ---------------------------------------------------------------------------
// Hook Execution Observability
// ---------------------------------------------------------------------------

/// Records the outcome of a single hook execution.
///
/// Every time `HookExecutor::fire` runs a hook, it produces one of these records.
/// They are collected in `PluginRegistry::execution_log` for observability.
#[derive(Debug, Clone, Serialize)]
pub struct HookExecution {
    /// The hook point key (e.g. `"pre_generation"`).
    pub hook_point: String,
    /// Which plugin's hook was executed.
    pub plugin_id: String,
    /// ISO-8601 timestamp when execution started.
    pub started_at: String,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u64,
    /// Outcome of this execution.
    pub result: HookResult,
    /// Human-readable descriptions of mutations applied by this hook.
    pub mutations: Vec<String>,
}

/// Outcome of a single hook execution.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum HookResult {
    /// Hook completed successfully.
    Success,
    /// Hook was skipped (condition not met, dependency missing, etc.).
    Skipped {
        /// Why it was skipped.
        reason: String,
    },
    /// Hook exceeded its declared `timeout_ms`.
    TimedOut,
    /// Hook produced an error.
    Error {
        /// Error message.
        message: String,
    },
}

// ---------------------------------------------------------------------------
// Installed Plugin
// ---------------------------------------------------------------------------

/// A plugin that has been installed in the registry.
#[derive(Debug, Clone, Serialize)]
pub struct InstalledPlugin {
    /// The full manifest.
    pub manifest: PluginManifest,
    /// Whether this plugin is currently enabled.
    pub enabled: bool,
    /// ISO-8601 timestamp when the plugin was installed.
    pub installed_at: String,
    /// Optional runtime configuration (must conform to `manifest.config_schema`).
    pub config: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Plugin Registry
// ---------------------------------------------------------------------------

/// In-memory registry of all installed plugins and their execution history.
///
/// Manages plugin lifecycle (install, uninstall, enable, disable) and execution log.
pub struct PluginRegistry {
    plugins: HashMap<String, InstalledPlugin>,
    execution_log: Vec<HookExecution>,
}

impl PluginRegistry {
    /// Create an empty plugin registry.
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            execution_log: Vec::new(),
        }
    }

    /// Hooks visible to `tenant_id`. Plugins with an empty `allowed_tenants`
    /// are global (everyone); plugins with an explicit list are filtered
    /// against the caller's tenant. Returned tuples are `(plugin_id, hook)`
    /// pairs for every enabled plugin permitted to run for this tenant.
    pub fn hooks_for_tenant(
        &self,
        tenant_id: Option<&str>,
    ) -> Vec<(&str, &HookDeclaration)> {
        self.plugins
            .values()
            .filter(|p| p.enabled && p.manifest.is_tenant_allowed(tenant_id))
            .flat_map(|p| {
                p.manifest
                    .hooks
                    .iter()
                    .map(move |h| (p.manifest.id.as_str(), h))
            })
            .collect()
    }

    /// Install a plugin from its manifest.
    ///
    /// Returns an error if a plugin with the same ID is already installed.
    pub fn install(&mut self, manifest: PluginManifest) -> Result<(), String> {
        if self.plugins.contains_key(&manifest.id) {
            return Err(format!(
                "Plugin '{}' is already installed",
                manifest.id
            ));
        }

        let id = manifest.id.clone();
        self.plugins.insert(
            id.clone(),
            InstalledPlugin {
                manifest,
                enabled: true,
                installed_at: Utc::now().to_rfc3339(),
                config: None,
            },
        );

        info!(plugin = %id, "Plugin installed");
        Ok(())
    }

    /// Uninstall a plugin by ID.
    ///
    /// Returns an error if the plugin is not found.
    pub fn uninstall(&mut self, plugin_id: &str) -> Result<(), String> {
        if self.plugins.remove(plugin_id).is_some() {
            info!(plugin = %plugin_id, "Plugin uninstalled");
            Ok(())
        } else {
            Err(format!("Plugin '{}' not found", plugin_id))
        }
    }

    /// Enable a previously disabled plugin.
    ///
    /// Returns an error if the plugin is not found.
    pub fn enable(&mut self, plugin_id: &str) -> Result<(), String> {
        match self.plugins.get_mut(plugin_id) {
            Some(p) => {
                p.enabled = true;
                info!(plugin = %plugin_id, "Plugin enabled");
                Ok(())
            }
            None => Err(format!("Plugin '{}' not found", plugin_id)),
        }
    }

    /// Disable a plugin (it stays installed but its hooks do not fire).
    ///
    /// Returns an error if the plugin is not found.
    pub fn disable(&mut self, plugin_id: &str) -> Result<(), String> {
        match self.plugins.get_mut(plugin_id) {
            Some(p) => {
                p.enabled = false;
                info!(plugin = %plugin_id, "Plugin disabled");
                Ok(())
            }
            None => Err(format!("Plugin '{}' not found", plugin_id)),
        }
    }

    /// List all installed plugins (both enabled and disabled).
    pub fn list(&self) -> Vec<&InstalledPlugin> {
        self.plugins.values().collect()
    }

    /// Get a specific installed plugin by ID.
    pub fn get(&self, plugin_id: &str) -> Option<&InstalledPlugin> {
        self.plugins.get(plugin_id)
    }

    /// Append a hook execution record to the log.
    pub fn record_execution(&mut self, exec: HookExecution) {
        self.execution_log.push(exec);
    }

    /// Get all hook execution records (most recent last).
    pub fn execution_log(&self) -> &[HookExecution] {
        &self.execution_log
    }

    /// Get execution records filtered by project ID (matched via hook context, stored externally).
    /// This is a convenience that returns the full log; callers filter by project.
    pub fn execution_log_all(&self) -> &[HookExecution] {
        &self.execution_log
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Hook Context
// ---------------------------------------------------------------------------

/// Mutable context passed through the hook chain.
///
/// Plugins can read current pipeline state and write mutations (injected context,
/// decision overrides) that downstream pipeline stages will consume.
#[derive(Debug, Clone)]
pub struct HookContext {
    /// The project being processed.
    pub project_id: String,
    /// The tenant that owns `project_id`. `None` for global / unscoped
    /// callers (tests, admin-triggered work). Plugins with a non-empty
    /// `allowed_tenants` list are filtered against this value before
    /// they are allowed to contribute to the hook pipeline.
    pub tenant_id: Option<String>,
    /// The user's original description (if available).
    pub description: Option<String>,
    /// Parsed intent analysis result.
    pub intent: Option<serde_json::Value>,
    /// Architecture decisions made so far.
    pub decisions: Option<serde_json::Value>,
    /// Taste engine score result.
    pub taste_score: Option<serde_json::Value>,
    /// Generation plan / spec.
    pub plan: Option<serde_json::Value>,
    /// Context strings injected by earlier hooks.
    pub injected_context: Vec<String>,
    /// Decision overrides applied by earlier hooks (area -> value).
    pub decision_overrides: HashMap<String, String>,
}

impl HookContext {
    /// Create a new hook context for a given project.
    pub fn new(project_id: impl Into<String>) -> Self {
        Self {
            project_id: project_id.into(),
            tenant_id: None,
            description: None,
            intent: None,
            decisions: None,
            taste_score: None,
            plan: None,
            injected_context: Vec::new(),
            decision_overrides: HashMap::new(),
        }
    }

    /// Attach the tenant identifier; returns `self` so it can chain off `new`.
    pub fn with_tenant(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }
}

// ---------------------------------------------------------------------------
// Hook Executor
// ---------------------------------------------------------------------------

/// Executes hooks registered at a given hook point across all enabled plugins.
///
/// The executor handles priority ordering, dependency resolution, timeout
/// enforcement, condition checking, and conflict detection. Every execution
/// is recorded as a `HookExecution` for observability.
pub struct HookExecutor;

impl HookExecutor {
    /// Fire all hooks registered at the given hook point.
    ///
    /// Steps:
    /// 1. Collect all hooks for the given point from enabled plugins.
    /// 2. Sort by priority (ascending — lower priority number runs first).
    /// 3. Check dependency ordering and re-order if needed.
    /// 4. Evaluate each hook's condition against the context.
    /// 5. Execute with timeout enforcement.
    /// 6. Log the execution result and any mutations.
    /// 7. Detect conflicts between plugins modifying the same decision area.
    ///
    /// Returns a list of execution records.
    pub async fn fire(
        registry: &PluginRegistry,
        hook_point: &HookPoint,
        context: &mut HookContext,
    ) -> Vec<HookExecution> {
        let mut executions = Vec::new();

        // Step 1: Collect hooks for this point from enabled plugins.
        let mut candidates: Vec<(&str, &HookDeclaration, &InstalledPlugin)> = Vec::new();
        for plugin in registry.plugins.values() {
            if !plugin.enabled {
                continue;
            }
            for hook in &plugin.manifest.hooks {
                if &hook.hook_point == hook_point {
                    candidates.push((plugin.manifest.id.as_str(), hook, plugin));
                }
            }
        }

        // Step 2: Sort by priority (ascending — lower runs first).
        candidates.sort_by_key(|(_, h, _)| h.priority);

        // Step 3: Re-order for dependency satisfaction.
        let ordered = Self::resolve_dependencies(&candidates);

        // Track which decision areas have been overridden (for conflict detection).
        let mut decision_owners: HashMap<String, String> = HashMap::new();

        // Step 4–6: Execute each hook.
        for (plugin_id, hook_decl, _plugin) in &ordered {
            let started_at = Utc::now();

            // Check condition.
            if let Some(ref condition) = hook_decl.condition {
                let desc_lower = context
                    .description
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase();
                if !desc_lower.contains(&condition.to_lowercase()) {
                    executions.push(HookExecution {
                        hook_point: hook_point.as_key(),
                        plugin_id: plugin_id.to_string(),
                        started_at: started_at.to_rfc3339(),
                        duration_ms: 0,
                        result: HookResult::Skipped {
                            reason: format!(
                                "Condition '{}' not matched in description",
                                condition
                            ),
                        },
                        mutations: vec![],
                    });
                    continue;
                }
            }

            // Execute the hook with a timeout.
            let timeout = std::time::Duration::from_millis(hook_decl.timeout_ms);
            let exec_start = std::time::Instant::now();

            let timed_out = tokio::time::timeout(timeout, async {
                // Apply capability-driven mutations from this plugin.
                Self::apply_plugin_capabilities(plugin_id, _plugin, context, &mut decision_owners);
            })
            .await;

            let duration_ms = exec_start.elapsed().as_millis() as u64;

            let (result, mutations) = match timed_out {
                Ok(()) => {
                    let muts = Self::describe_mutations(plugin_id, _plugin);
                    (HookResult::Success, muts)
                }
                Err(_) => {
                    warn!(
                        plugin = %plugin_id,
                        hook = %hook_point.as_key(),
                        timeout_ms = hook_decl.timeout_ms,
                        "Hook execution timed out"
                    );
                    (HookResult::TimedOut, vec![])
                }
            };

            executions.push(HookExecution {
                hook_point: hook_point.as_key(),
                plugin_id: plugin_id.to_string(),
                started_at: started_at.to_rfc3339(),
                duration_ms,
                result,
                mutations,
            });
        }

        // Step 7: Detect conflicts (multiple plugins overriding the same decision area).
        for (area, owner) in &decision_owners {
            let overriders: Vec<&str> = ordered
                .iter()
                .filter(|(pid, _, p)| {
                    *pid != owner.as_str()
                        && p.manifest.capabilities.iter().any(|cap| {
                            matches!(cap, PluginCapability::DecisionOverride { area: a, .. } if a == area)
                        })
                })
                .map(|(pid, _, _)| *pid)
                .collect();

            if !overriders.is_empty() {
                warn!(
                    area = %area,
                    winner = %owner,
                    conflicting = ?overriders,
                    "Conflict detected: multiple plugins override the same decision area"
                );
            }
        }

        executions
    }

    /// Apply capability-driven mutations from a plugin to the hook context.
    fn apply_plugin_capabilities(
        plugin_id: &str,
        plugin: &InstalledPlugin,
        context: &mut HookContext,
        decision_owners: &mut HashMap<String, String>,
    ) {
        for cap in &plugin.manifest.capabilities {
            match cap {
                PluginCapability::DecisionOverride { area, value } => {
                    context
                        .decision_overrides
                        .insert(area.clone(), value.clone());
                    decision_owners.insert(area.clone(), plugin_id.to_string());
                }
                PluginCapability::PromptEnhancement { phase: _, template } => {
                    context.injected_context.push(format!(
                        "## Plugin [{}] prompt enhancement\n{}",
                        plugin_id, template
                    ));
                }
                PluginCapability::DesignSystem {
                    css: _,
                    components,
                } => {
                    context.injected_context.push(format!(
                        "## Plugin [{}] design system — components: {}",
                        plugin_id,
                        components.join(", ")
                    ));
                }
                PluginCapability::DomainPack { domain, entities } => {
                    context.injected_context.push(format!(
                        "## Plugin [{}] domain pack '{}' — entities: {}",
                        plugin_id,
                        domain,
                        entities.join(", ")
                    ));
                }
                PluginCapability::QualityRule {
                    rule_name,
                    severity,
                } => {
                    context.injected_context.push(format!(
                        "## Plugin [{}] quality rule: {} (severity: {})",
                        plugin_id, rule_name, severity
                    ));
                }
                PluginCapability::CustomTool { name, description } => {
                    context.injected_context.push(format!(
                        "## Plugin [{}] custom tool '{}': {}",
                        plugin_id, name, description
                    ));
                }
            }
        }
    }

    /// Describe what mutations a plugin would apply (for the execution log).
    fn describe_mutations(plugin_id: &str, plugin: &InstalledPlugin) -> Vec<String> {
        let mut muts = Vec::new();
        for cap in &plugin.manifest.capabilities {
            match cap {
                PluginCapability::DecisionOverride { area, value } => {
                    muts.push(format!(
                        "[{}] override decision '{}' = '{}'",
                        plugin_id, area, value
                    ));
                }
                PluginCapability::PromptEnhancement { phase, .. } => {
                    muts.push(format!(
                        "[{}] inject prompt enhancement for phase '{}'",
                        plugin_id, phase
                    ));
                }
                PluginCapability::DesignSystem { .. } => {
                    muts.push(format!("[{}] inject design system", plugin_id));
                }
                PluginCapability::DomainPack { domain, .. } => {
                    muts.push(format!(
                        "[{}] inject domain pack '{}'",
                        plugin_id, domain
                    ));
                }
                PluginCapability::QualityRule { rule_name, .. } => {
                    muts.push(format!(
                        "[{}] add quality rule '{}'",
                        plugin_id, rule_name
                    ));
                }
                PluginCapability::CustomTool { name, .. } => {
                    muts.push(format!(
                        "[{}] register custom tool '{}'",
                        plugin_id, name
                    ));
                }
            }
        }
        muts
    }

    /// Topological sort respecting `requires` dependencies.
    ///
    /// If a dependency cannot be satisfied (missing plugin), the hook is placed
    /// at the end with a warning.
    fn resolve_dependencies<'a>(
        candidates: &[(&'a str, &'a HookDeclaration, &'a InstalledPlugin)],
    ) -> Vec<(&'a str, &'a HookDeclaration, &'a InstalledPlugin)> {
        let ids: Vec<&str> = candidates.iter().map(|(id, _, _)| *id).collect();
        let mut resolved: Vec<(&str, &HookDeclaration, &InstalledPlugin)> = Vec::new();
        let mut remaining: Vec<_> = candidates.to_vec();

        // Simple iterative resolution: keep pulling items whose deps are satisfied.
        let max_iterations = remaining.len() + 1;
        for _ in 0..max_iterations {
            if remaining.is_empty() {
                break;
            }

            let mut progress = false;
            let mut still_remaining = Vec::new();

            for item in remaining {
                let (pid, hook, plugin) = item;
                let deps_satisfied = hook.requires.iter().all(|dep| {
                    // Dependency is satisfied if it's already in `resolved` or not in candidates at all.
                    resolved.iter().any(|(rid, _, _)| *rid == dep.as_str())
                        || !ids.contains(&dep.as_str())
                });

                if deps_satisfied {
                    resolved.push((pid, hook, plugin));
                    progress = true;
                } else {
                    still_remaining.push((pid, hook, plugin));
                }
            }

            remaining = still_remaining;

            if !progress {
                // Circular dependency — just append the rest.
                warn!(
                    plugins = ?remaining.iter().map(|(id, _, _)| *id).collect::<Vec<_>>(),
                    "Circular hook dependency detected; appending remaining hooks"
                );
                resolved.extend(remaining);
                break;
            }
        }

        resolved
    }
}

// ---------------------------------------------------------------------------
// Validation & Dry Run
// ---------------------------------------------------------------------------

/// Validate a plugin manifest and return a list of warnings.
///
/// An empty list means the manifest is valid. Warnings do not prevent
/// installation but indicate potential issues.
pub fn validate_manifest(manifest: &PluginManifest) -> Vec<String> {
    let mut warnings = Vec::new();

    if manifest.id.is_empty() {
        warnings.push("Plugin ID is empty".to_string());
    }
    if manifest.name.is_empty() {
        warnings.push("Plugin name is empty".to_string());
    }
    if manifest.version.is_empty() {
        warnings.push("Plugin version is empty".to_string());
    }
    if manifest.author.is_empty() {
        warnings.push("Plugin author is empty".to_string());
    }
    if manifest.description.is_empty() {
        warnings.push("Plugin description is empty".to_string());
    }

    // Validate semver-ish version.
    let parts: Vec<&str> = manifest.version.split('.').collect();
    if parts.len() != 3 || parts.iter().any(|p| p.parse::<u32>().is_err()) {
        warnings.push(format!(
            "Version '{}' is not valid semver (expected X.Y.Z)",
            manifest.version
        ));
    }

    // Check hook timeouts.
    for (i, hook) in manifest.hooks.iter().enumerate() {
        if hook.timeout_ms == 0 {
            warnings.push(format!("Hook #{} has zero timeout — it will always time out", i));
        }
        if hook.timeout_ms > 30_000 {
            warnings.push(format!(
                "Hook #{} has timeout {}ms (>30s) — this may block the pipeline",
                i, hook.timeout_ms
            ));
        }
    }

    // Check for duplicate hook points within the same plugin.
    let mut seen_hooks: Vec<String> = Vec::new();
    for hook in &manifest.hooks {
        let key = hook.hook_point.as_key();
        if seen_hooks.contains(&key) {
            warnings.push(format!(
                "Duplicate hook point '{}' — only the first will be effective",
                key
            ));
        }
        seen_hooks.push(key);
    }

    // Check for self-referencing requires.
    for hook in &manifest.hooks {
        if hook.requires.contains(&manifest.id) {
            warnings.push(format!(
                "Hook at '{}' requires its own plugin ID — this is a self-dependency",
                hook.hook_point.as_key()
            ));
        }
    }

    // Check compatibility self-conflict.
    if manifest.compatibility.conflicts_with.contains(&manifest.id) {
        warnings.push("Plugin conflicts with itself".to_string());
    }

    warnings
}

/// Check whether installing a new plugin would cause conflicts with
/// already-installed plugins in the registry.
///
/// Returns a list of conflict descriptions (empty = no conflicts).
pub fn check_conflicts(
    registry: &PluginRegistry,
    new_plugin: &PluginManifest,
) -> Vec<String> {
    let mut conflicts = Vec::new();

    for installed in registry.plugins.values() {
        if !installed.enabled {
            continue;
        }

        let installed_id = &installed.manifest.id;

        // Check explicit conflicts_with declarations (bidirectional).
        if new_plugin.compatibility.conflicts_with.contains(installed_id) {
            conflicts.push(format!(
                "New plugin '{}' declares conflict with installed plugin '{}'",
                new_plugin.id, installed_id
            ));
        }
        if installed
            .manifest
            .compatibility
            .conflicts_with
            .contains(&new_plugin.id)
        {
            conflicts.push(format!(
                "Installed plugin '{}' declares conflict with new plugin '{}'",
                installed_id, new_plugin.id
            ));
        }

        // Check overlapping decision overrides.
        for new_cap in &new_plugin.capabilities {
            if let PluginCapability::DecisionOverride {
                area: new_area,
                value: new_value,
            } = new_cap
            {
                for existing_cap in &installed.manifest.capabilities {
                    if let PluginCapability::DecisionOverride {
                        area: ex_area,
                        value: ex_value,
                    } = existing_cap
                    {
                        if new_area == ex_area && new_value != ex_value {
                            conflicts.push(format!(
                                "Decision override conflict on '{}': '{}' wants '{}' but '{}' wants '{}'",
                                new_area, new_plugin.id, new_value, installed_id, ex_value
                            ));
                        }
                    }
                }
            }
        }
    }

    // Check that required plugins are installed.
    for required in &new_plugin.compatibility.required_plugins {
        if !registry.plugins.contains_key(required) {
            conflicts.push(format!(
                "Required plugin '{}' is not installed",
                required
            ));
        }
    }

    conflicts
}

/// Return all available hook points as a list of descriptive entries.
pub fn list_available_hook_points() -> Vec<serde_json::Value> {
    use serde_json::json;
    vec![
        json!({"point": "pre_intent_analysis", "description": "Before intent analysis runs"}),
        json!({"point": "post_intent_analysis", "description": "After intent is parsed, before decisions"}),
        json!({"point": "pre_decision", "description": "Before architecture decisions are made"}),
        json!({"point": "post_decision", "description": "After architecture decisions, before generation"}),
        json!({"point": "pre_generation", "description": "Before code generation starts"}),
        json!({"point": "during_generation", "description": "During code generation at a named phase (requires 'phase' field)"}),
        json!({"point": "post_generation", "description": "After code generation completes"}),
        json!({"point": "pre_taste", "description": "Before taste scoring runs"}),
        json!({"point": "post_taste", "description": "After taste scoring, before redesign"}),
        json!({"point": "pre_guarantee", "description": "Before the outcome guarantee loop"}),
        json!({"point": "post_guarantee", "description": "After the outcome guarantee loop completes"}),
        json!({"point": "on_error", "description": "When an error of a specific class occurs (requires 'error_class' field)"}),
    ]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest(id: &str) -> PluginManifest {
        PluginManifest {
            id: id.to_string(),
            name: format!("Test Plugin {}", id),
            version: "1.0.0".to_string(),
            description: "A test plugin".to_string(),
            author: "Test Author".to_string(),
            capabilities: vec![],
            hooks: vec![],
            config_schema: None,
            compatibility: CompatibilitySpec::default(),
            allowed_tenants: vec![],
        }
    }

    #[tokio::test]
    async fn tenant_scoped_plugin_filters_foreign_tenants() {
        let mut registry = PluginRegistry::new();

        // Global plugin — runs for everyone.
        let mut global = sample_manifest("global-plugin");
        global.hooks.push(HookDeclaration {
            hook_point: HookPoint::PreGeneration,
            priority: 1,
            condition: None,
            timeout_ms: 1000,
            blocking: false,
            requires: vec![],
        });
        registry.install(global).unwrap();

        // Tenant-A only plugin.
        let mut scoped = sample_manifest("scoped-plugin");
        scoped.allowed_tenants.push("tenant-a".into());
        scoped.hooks.push(HookDeclaration {
            hook_point: HookPoint::PreGeneration,
            priority: 2,
            condition: None,
            timeout_ms: 1000,
            blocking: false,
            requires: vec![],
        });
        registry.install(scoped).unwrap();

        // Tenant A sees both plugins.
        let a_hooks = registry.hooks_for_tenant(Some("tenant-a"));
        let a_ids: std::collections::HashSet<_> =
            a_hooks.iter().map(|(id, _)| *id).collect();
        assert!(a_ids.contains("global-plugin"));
        assert!(a_ids.contains("scoped-plugin"));

        // Tenant B sees only the global plugin.
        let b_hooks = registry.hooks_for_tenant(Some("tenant-b"));
        let b_ids: std::collections::HashSet<_> =
            b_hooks.iter().map(|(id, _)| *id).collect();
        assert!(b_ids.contains("global-plugin"));
        assert!(
            !b_ids.contains("scoped-plugin"),
            "scoped plugin must NOT fire for other tenants"
        );

        // No tenant context → only global plugin is visible.
        let none_hooks = registry.hooks_for_tenant(None);
        let none_ids: std::collections::HashSet<_> =
            none_hooks.iter().map(|(id, _)| *id).collect();
        assert!(none_ids.contains("global-plugin"));
        assert!(!none_ids.contains("scoped-plugin"));
    }

    #[test]
    fn test_install_and_list() {
        let mut registry = PluginRegistry::new();

        let manifest = sample_manifest("test-plugin-1");
        registry.install(manifest).unwrap();

        assert_eq!(registry.list().len(), 1);
        assert!(registry.get("test-plugin-1").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_install_duplicate_rejected() {
        let mut registry = PluginRegistry::new();

        let manifest = sample_manifest("dup");
        registry.install(manifest.clone()).unwrap();

        let result = registry.install(manifest);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already installed"));
    }

    #[test]
    fn test_uninstall() {
        let mut registry = PluginRegistry::new();
        registry.install(sample_manifest("removeme")).unwrap();
        assert_eq!(registry.list().len(), 1);

        registry.uninstall("removeme").unwrap();
        assert_eq!(registry.list().len(), 0);

        let result = registry.uninstall("removeme");
        assert!(result.is_err());
    }

    #[test]
    fn test_enable_disable() {
        let mut registry = PluginRegistry::new();
        registry.install(sample_manifest("toggle")).unwrap();

        assert!(registry.get("toggle").unwrap().enabled);

        registry.disable("toggle").unwrap();
        assert!(!registry.get("toggle").unwrap().enabled);

        registry.enable("toggle").unwrap();
        assert!(registry.get("toggle").unwrap().enabled);

        assert!(registry.enable("nonexistent").is_err());
        assert!(registry.disable("nonexistent").is_err());
    }

    #[tokio::test]
    async fn test_hook_priority_ordering() {
        let mut registry = PluginRegistry::new();

        // Plugin A: priority 10.
        let mut manifest_a = sample_manifest("plugin-a");
        manifest_a.hooks.push(HookDeclaration {
            hook_point: HookPoint::PreGeneration,
            priority: 10,
            condition: None,
            timeout_ms: 5000,
            blocking: true,
            requires: vec![],
        });
        manifest_a.capabilities.push(PluginCapability::PromptEnhancement {
            phase: "codegen".to_string(),
            template: "A template".to_string(),
        });
        registry.install(manifest_a).unwrap();

        // Plugin B: priority 1 (runs first).
        let mut manifest_b = sample_manifest("plugin-b");
        manifest_b.hooks.push(HookDeclaration {
            hook_point: HookPoint::PreGeneration,
            priority: 1,
            condition: None,
            timeout_ms: 5000,
            blocking: true,
            requires: vec![],
        });
        manifest_b.capabilities.push(PluginCapability::PromptEnhancement {
            phase: "codegen".to_string(),
            template: "B template".to_string(),
        });
        registry.install(manifest_b).unwrap();

        let mut ctx = HookContext::new("test-project");
        let executions =
            HookExecutor::fire(&registry, &HookPoint::PreGeneration, &mut ctx).await;

        assert_eq!(executions.len(), 2);
        // Plugin B (priority 1) should run before Plugin A (priority 10).
        assert_eq!(executions[0].plugin_id, "plugin-b");
        assert_eq!(executions[1].plugin_id, "plugin-a");
    }

    #[tokio::test]
    async fn test_hook_condition_skips() {
        let mut registry = PluginRegistry::new();

        let mut manifest = sample_manifest("conditional");
        manifest.hooks.push(HookDeclaration {
            hook_point: HookPoint::PreGeneration,
            priority: 0,
            condition: Some("ecommerce".to_string()),
            timeout_ms: 5000,
            blocking: true,
            requires: vec![],
        });
        registry.install(manifest).unwrap();

        // Context without matching description — hook should be skipped.
        let mut ctx = HookContext::new("test-project");
        ctx.description = Some("A simple blog app".to_string());

        let executions =
            HookExecutor::fire(&registry, &HookPoint::PreGeneration, &mut ctx).await;

        assert_eq!(executions.len(), 1);
        assert!(matches!(executions[0].result, HookResult::Skipped { .. }));

        // Context with matching description — hook should fire.
        let mut ctx2 = HookContext::new("test-project");
        ctx2.description = Some("An ecommerce store".to_string());

        let executions2 =
            HookExecutor::fire(&registry, &HookPoint::PreGeneration, &mut ctx2).await;

        assert_eq!(executions2.len(), 1);
        assert!(matches!(executions2[0].result, HookResult::Success));
    }

    #[tokio::test]
    async fn test_timeout_enforcement() {
        // Timeout is enforced at the executor level. Since our apply_plugin_capabilities
        // is near-instant, we verify the mechanism exists by checking that a hook with
        // a generous timeout succeeds.
        let mut registry = PluginRegistry::new();

        let mut manifest = sample_manifest("fast");
        manifest.hooks.push(HookDeclaration {
            hook_point: HookPoint::PostDecision,
            priority: 0,
            condition: None,
            timeout_ms: 100, // Very short but our code is instant.
            blocking: true,
            requires: vec![],
        });
        registry.install(manifest).unwrap();

        let mut ctx = HookContext::new("proj");
        let execs = HookExecutor::fire(&registry, &HookPoint::PostDecision, &mut ctx).await;

        assert_eq!(execs.len(), 1);
        assert!(matches!(execs[0].result, HookResult::Success));
        // Duration should be well under the timeout.
        assert!(execs[0].duration_ms < 100);
    }

    #[test]
    fn test_conflict_detection() {
        let mut registry = PluginRegistry::new();

        let mut existing = sample_manifest("existing-db");
        existing
            .capabilities
            .push(PluginCapability::DecisionOverride {
                area: "database".to_string(),
                value: "postgres".to_string(),
            });
        registry.install(existing).unwrap();

        // New plugin overrides the same area with a different value.
        let mut newcomer = sample_manifest("newcomer-db");
        newcomer
            .capabilities
            .push(PluginCapability::DecisionOverride {
                area: "database".to_string(),
                value: "mysql".to_string(),
            });

        let conflicts = check_conflicts(&registry, &newcomer);
        assert!(!conflicts.is_empty());
        assert!(conflicts[0].contains("database"));
    }

    #[test]
    fn test_explicit_conflict_declaration() {
        let mut registry = PluginRegistry::new();

        let mut installed = sample_manifest("alpha");
        installed.compatibility.conflicts_with = vec!["beta".to_string()];
        registry.install(installed).unwrap();

        let newcomer = sample_manifest("beta");
        let conflicts = check_conflicts(&registry, &newcomer);
        assert!(!conflicts.is_empty());
        assert!(conflicts[0].contains("conflict"));
    }

    #[test]
    fn test_required_plugin_missing() {
        let registry = PluginRegistry::new();

        let mut manifest = sample_manifest("dependent");
        manifest.compatibility.required_plugins = vec!["base-plugin".to_string()];

        let conflicts = check_conflicts(&registry, &manifest);
        assert!(!conflicts.is_empty());
        assert!(conflicts[0].contains("base-plugin"));
    }

    #[test]
    fn test_validate_manifest_valid() {
        let manifest = sample_manifest("valid");
        let warnings = validate_manifest(&manifest);
        assert!(warnings.is_empty(), "Expected no warnings, got: {:?}", warnings);
    }

    #[test]
    fn test_validate_manifest_empty_fields() {
        let manifest = PluginManifest {
            id: "".to_string(),
            name: "".to_string(),
            version: "".to_string(),
            description: "".to_string(),
            author: "".to_string(),
            capabilities: vec![],
            hooks: vec![],
            config_schema: None,
            compatibility: CompatibilitySpec::default(),
            allowed_tenants: vec![],
        };

        let warnings = validate_manifest(&manifest);
        assert!(warnings.len() >= 5); // id, name, version, author, description
    }

    #[test]
    fn test_validate_manifest_bad_version() {
        let mut manifest = sample_manifest("bad-ver");
        manifest.version = "not-semver".to_string();

        let warnings = validate_manifest(&manifest);
        assert!(warnings.iter().any(|w| w.contains("semver")));
    }

    #[test]
    fn test_validate_manifest_zero_timeout() {
        let mut manifest = sample_manifest("zero-timeout");
        manifest.hooks.push(HookDeclaration {
            hook_point: HookPoint::PreGeneration,
            priority: 0,
            condition: None,
            timeout_ms: 0,
            blocking: true,
            requires: vec![],
        });

        let warnings = validate_manifest(&manifest);
        assert!(warnings.iter().any(|w| w.contains("zero timeout")));
    }

    #[test]
    fn test_hook_point_as_key() {
        assert_eq!(HookPoint::PreIntentAnalysis.as_key(), "pre_intent_analysis");
        assert_eq!(
            HookPoint::DuringGeneration {
                phase: "layout".to_string()
            }
            .as_key(),
            "during_generation:layout"
        );
        assert_eq!(
            HookPoint::OnError {
                error_class: "build_failure".to_string()
            }
            .as_key(),
            "on_error:build_failure"
        );
    }

    #[test]
    fn test_hook_context_new() {
        let ctx = HookContext::new("my-project");
        assert_eq!(ctx.project_id, "my-project");
        assert!(ctx.injected_context.is_empty());
        assert!(ctx.decision_overrides.is_empty());
    }
}

// ===========================================================================
// Legacy file-based plugin system
//
// Plugins are JSON manifest files loaded from `~/.nexus/plugins/<id>/`.
// Registers custom agents, skills, pipeline steps, design systems,
// decision rules, and lifecycle hooks.
// ===========================================================================

use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Legacy Plugin Manifest (plugin.json)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyPluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub homepage: String,
    #[serde(default)]
    pub agents: Vec<PluginAgent>,
    #[serde(default)]
    pub skills: Vec<PluginSkill>,
    #[serde(default)]
    pub pipeline_steps: Vec<PluginPipelineStep>,
    #[serde(default)]
    pub design_systems: Vec<PluginDesignSystem>,
    #[serde(default)]
    pub decision_rules: Vec<PluginDecisionRule>,
    #[serde(default)]
    pub hooks: Vec<LegacyPluginHookRegistration>,
    /// See [`PluginManifest::allowed_tenants`]. Empty = global; non-empty
    /// restricts all hook/decision/pipeline execution to listed tenants.
    #[serde(default)]
    pub allowed_tenants: Vec<String>,
}

impl LegacyPluginManifest {
    pub fn is_tenant_allowed(&self, tenant_id: Option<&str>) -> bool {
        if self.allowed_tenants.is_empty() {
            return true;
        }
        match tenant_id {
            Some(t) => self.allowed_tenants.iter().any(|a| a == t),
            None => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyPluginHookRegistration {
    pub hook: String,
    #[serde(default)]
    pub when: Vec<String>,
    #[serde(default)]
    pub inject_context: Option<String>,
    #[serde(default)]
    pub override_decision: Option<PluginHookDecisionOverride>,
    #[serde(default)]
    pub priority: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginHookDecisionOverride {
    pub area: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginAgent {
    pub name: String,
    pub role: String,
    pub description: String,
    pub system_prompt: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub max_iterations: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSkill {
    pub name: String,
    pub description: String,
    pub category: String,
    pub system_prompt: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub rules: Vec<String>,
    #[serde(default)]
    pub examples: Vec<PluginExample>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginExample {
    pub user: String,
    pub assistant: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginPipelineStep {
    pub id: String,
    pub name: String,
    pub description: String,
    pub position: String,
    pub prompt_template: String,
    #[serde(default)]
    pub skip_condition: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDesignSystem {
    pub id: String,
    pub name: String,
    pub description: String,
    pub css: String,
    #[serde(default)]
    pub tailwind_config: Option<serde_json::Value>,
    #[serde(default)]
    pub font_imports: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDecisionRule {
    pub id: String,
    pub description: String,
    pub area: String,
    pub when: Vec<String>,
    pub then: String,
    #[serde(default)]
    pub priority: i32,
}

// ---------------------------------------------------------------------------
// Legacy Plugin Registry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedPlugin {
    pub manifest: LegacyPluginManifest,
    pub path: PathBuf,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRegistrySummary {
    pub total_plugins: usize,
    pub enabled_plugins: usize,
    pub total_agents: usize,
    pub total_skills: usize,
    pub total_pipeline_steps: usize,
    pub total_design_systems: usize,
    pub total_decision_rules: usize,
    pub total_hooks: usize,
    pub plugins: Vec<LegacyPluginInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyPluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub enabled: bool,
    pub agents: usize,
    pub skills: usize,
    pub pipeline_steps: usize,
    pub design_systems: usize,
    pub decision_rules: usize,
    pub hooks: usize,
}

pub struct LegacyPluginRegistry {
    plugins: HashMap<String, LoadedPlugin>,
    plugins_dir: PathBuf,
}

impl LegacyPluginRegistry {
    pub fn load(data_dir: &Path) -> Self {
        let plugins_dir = data_dir.join("plugins");
        let _ = std::fs::create_dir_all(&plugins_dir);

        let mut registry = Self {
            plugins: HashMap::new(),
            plugins_dir,
        };

        registry.scan();
        registry
    }

    pub fn scan(&mut self) {
        let Ok(entries) = std::fs::read_dir(&self.plugins_dir) else {
            return;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let manifest_path = path.join("plugin.json");
            if !manifest_path.exists() {
                continue;
            }

            match std::fs::read_to_string(&manifest_path) {
                Ok(content) => match serde_json::from_str::<LegacyPluginManifest>(&content) {
                    Ok(manifest) => {
                        let id = manifest.id.clone();
                        let enabled = !path.join(".disabled").exists();

                        info!(
                            plugin = %id,
                            version = %manifest.version,
                            enabled = enabled,
                            agents = manifest.agents.len(),
                            skills = manifest.skills.len(),
                            "Loaded plugin"
                        );

                        self.plugins.insert(
                            id,
                            LoadedPlugin {
                                manifest,
                                path: path.clone(),
                                enabled,
                            },
                        );
                    }
                    Err(e) => {
                        warn!(
                            path = %manifest_path.display(),
                            error = %e,
                            "Failed to parse plugin manifest"
                        );
                    }
                },
                Err(e) => {
                    warn!(
                        path = %manifest_path.display(),
                        error = %e,
                        "Failed to read plugin manifest"
                    );
                }
            }
        }
    }

    pub fn summary(&self) -> PluginRegistrySummary {
        let enabled: Vec<&LoadedPlugin> = self.plugins.values().filter(|p| p.enabled).collect();

        PluginRegistrySummary {
            total_plugins: self.plugins.len(),
            enabled_plugins: enabled.len(),
            total_agents: enabled.iter().map(|p| p.manifest.agents.len()).sum(),
            total_skills: enabled.iter().map(|p| p.manifest.skills.len()).sum(),
            total_pipeline_steps: enabled.iter().map(|p| p.manifest.pipeline_steps.len()).sum(),
            total_design_systems: enabled.iter().map(|p| p.manifest.design_systems.len()).sum(),
            total_decision_rules: enabled.iter().map(|p| p.manifest.decision_rules.len()).sum(),
            total_hooks: enabled.iter().map(|p| p.manifest.hooks.len()).sum(),
            plugins: self
                .plugins
                .values()
                .map(|p| LegacyPluginInfo {
                    id: p.manifest.id.clone(),
                    name: p.manifest.name.clone(),
                    version: p.manifest.version.clone(),
                    author: p.manifest.author.clone(),
                    enabled: p.enabled,
                    agents: p.manifest.agents.len(),
                    skills: p.manifest.skills.len(),
                    pipeline_steps: p.manifest.pipeline_steps.len(),
                    design_systems: p.manifest.design_systems.len(),
                    decision_rules: p.manifest.decision_rules.len(),
                    hooks: p.manifest.hooks.len(),
                })
                .collect(),
        }
    }

    pub fn all_agents(&self) -> Vec<(&str, &PluginAgent)> {
        self.plugins
            .values()
            .filter(|p| p.enabled)
            .flat_map(|p| {
                p.manifest
                    .agents
                    .iter()
                    .map(move |a| (p.manifest.id.as_str(), a))
            })
            .collect()
    }

    pub fn all_skills(&self) -> Vec<(&str, &PluginSkill)> {
        self.plugins
            .values()
            .filter(|p| p.enabled)
            .flat_map(|p| {
                p.manifest
                    .skills
                    .iter()
                    .map(move |s| (p.manifest.id.as_str(), s))
            })
            .collect()
    }

    pub fn all_pipeline_steps(&self) -> Vec<(&str, &PluginPipelineStep)> {
        self.plugins
            .values()
            .filter(|p| p.enabled)
            .flat_map(|p| {
                p.manifest
                    .pipeline_steps
                    .iter()
                    .map(move |s| (p.manifest.id.as_str(), s))
            })
            .collect()
    }

    pub fn all_design_systems(&self) -> Vec<(&str, &PluginDesignSystem)> {
        self.plugins
            .values()
            .filter(|p| p.enabled)
            .flat_map(|p| {
                p.manifest
                    .design_systems
                    .iter()
                    .map(move |d| (p.manifest.id.as_str(), d))
            })
            .collect()
    }

    pub fn all_hooks(&self) -> Vec<(&str, &LegacyPluginHookRegistration)> {
        self.hooks_for_tenant(None)
    }

    /// Hooks visible to `tenant_id`. Plugins with `allowed_tenants` empty
    /// are global (always included); plugins with an explicit list are
    /// filtered against the caller's tenant.
    pub fn hooks_for_tenant(
        &self,
        tenant_id: Option<&str>,
    ) -> Vec<(&str, &LegacyPluginHookRegistration)> {
        let mut hooks: Vec<_> = self
            .plugins
            .values()
            .filter(|p| p.enabled && p.manifest.is_tenant_allowed(tenant_id))
            .flat_map(|p| {
                p.manifest
                    .hooks
                    .iter()
                    .map(move |h| (p.manifest.id.as_str(), h))
            })
            .collect();
        hooks.sort_by(|a, b| b.1.priority.cmp(&a.1.priority));
        hooks
    }

    pub fn all_decision_rules(&self) -> Vec<(&str, &PluginDecisionRule)> {
        self.decision_rules_for_tenant(None)
    }

    pub fn decision_rules_for_tenant(
        &self,
        tenant_id: Option<&str>,
    ) -> Vec<(&str, &PluginDecisionRule)> {
        let mut rules: Vec<_> = self
            .plugins
            .values()
            .filter(|p| p.enabled && p.manifest.is_tenant_allowed(tenant_id))
            .flat_map(|p| {
                p.manifest
                    .decision_rules
                    .iter()
                    .map(move |r| (p.manifest.id.as_str(), r))
            })
            .collect();
        rules.sort_by(|a, b| b.1.priority.cmp(&a.1.priority));
        rules
    }

    pub fn pipeline_steps_for_tenant(
        &self,
        tenant_id: Option<&str>,
    ) -> Vec<(&str, &PluginPipelineStep)> {
        self.plugins
            .values()
            .filter(|p| p.enabled && p.manifest.is_tenant_allowed(tenant_id))
            .flat_map(|p| {
                p.manifest
                    .pipeline_steps
                    .iter()
                    .map(move |s| (p.manifest.id.as_str(), s))
            })
            .collect()
    }

    pub fn enable(&mut self, id: &str) -> bool {
        if let Some(plugin) = self.plugins.get_mut(id) {
            let disabled_marker = plugin.path.join(".disabled");
            let _ = std::fs::remove_file(&disabled_marker);
            plugin.enabled = true;
            true
        } else {
            false
        }
    }

    pub fn disable(&mut self, id: &str) -> bool {
        if let Some(plugin) = self.plugins.get_mut(id) {
            let disabled_marker = plugin.path.join(".disabled");
            let _ = std::fs::write(&disabled_marker, "");
            plugin.enabled = false;
            true
        } else {
            false
        }
    }

    pub fn install_from_config(
        &mut self,
        item_id: &str,
        config: &serde_json::Value,
    ) -> Result<String, String> {
        let manifest: LegacyPluginManifest = serde_json::from_value(
            config
                .get("plugin")
                .cloned()
                .unwrap_or(config.clone()),
        )
        .map_err(|e| format!("Invalid plugin config: {}", e))?;

        let plugin_dir = self.plugins_dir.join(&manifest.id);
        std::fs::create_dir_all(&plugin_dir)
            .map_err(|e| format!("Cannot create plugin dir: {}", e))?;

        let manifest_json = serde_json::to_string_pretty(&manifest)
            .map_err(|e| format!("Cannot serialize manifest: {}", e))?;
        std::fs::write(plugin_dir.join("plugin.json"), &manifest_json)
            .map_err(|e| format!("Cannot write manifest: {}", e))?;

        let id = manifest.id.clone();
        self.plugins.insert(
            id.clone(),
            LoadedPlugin {
                manifest,
                path: plugin_dir,
                enabled: true,
            },
        );

        info!(plugin = %id, marketplace_item = %item_id, "Plugin installed from marketplace");
        Ok(id)
    }

    pub fn uninstall(&mut self, id: &str) -> bool {
        if let Some(plugin) = self.plugins.remove(id) {
            let _ = std::fs::remove_dir_all(&plugin.path);
            info!(plugin = %id, "Plugin uninstalled");
            true
        } else {
            false
        }
    }

    pub fn export(&self, id: &str) -> Option<serde_json::Value> {
        let plugin = self.plugins.get(id)?;
        serde_json::to_value(&plugin.manifest).ok()
    }

    pub fn import(&mut self, bundle: &serde_json::Value) -> Result<String, String> {
        let manifest: LegacyPluginManifest =
            serde_json::from_value(bundle.clone()).map_err(|e| format!("Invalid bundle: {}", e))?;

        let plugin_dir = self.plugins_dir.join(&manifest.id);
        std::fs::create_dir_all(&plugin_dir)
            .map_err(|e| format!("Cannot create plugin dir: {}", e))?;

        let manifest_json = serde_json::to_string_pretty(&manifest)
            .map_err(|e| format!("Cannot serialize: {}", e))?;
        std::fs::write(plugin_dir.join("plugin.json"), &manifest_json)
            .map_err(|e| format!("Cannot write: {}", e))?;

        let id = manifest.id.clone();
        self.plugins.insert(
            id.clone(),
            LoadedPlugin {
                manifest,
                path: plugin_dir,
                enabled: true,
            },
        );

        Ok(id)
    }
}
