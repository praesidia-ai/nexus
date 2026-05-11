---
name: add-plugin
description: Add a plugin, plugin hook, or new hook point to nexus-http. Use when extending the plugin system, adding pipeline interception points, or writing a plugin manifest.
---

# Plugin System in nexus-http

## Two things to distinguish

| Task | Where |
|------|-------|
| **Adding a hook point** (new interception point in the pipeline) | `plugin_hooks.rs` |
| **Adding a new plugin capability type** | `plugin_system.rs` → `PluginCapability` enum |
| **Writing a plugin manifest** (for a plugin that ships as a JSON file) | `~/.nexus/plugins/<id>/manifest.json` |
| **Calling a hook from a handler or engine** | Any handler/engine file |

---

## A. Calling an existing hook from a new code path

Import and call `fire_hook` at key points in any handler or pipeline stage:

```rust
use crate::plugin_hooks::{fire_hook, HookContext, HookPoint};

// Build the mutable context — pass whatever is available at this point
let mut hook_ctx = HookContext {
    hook: HookPoint::PostDecision.as_key(),
    project_id: project_id.clone(),
    intent: Some(serde_json::to_value(&intent_result)?),
    decisions: Some(serde_json::to_value(&decisions)?),
    plan: None,
    taste_score: None,
    build_success: None,
    extra: serde_json::json!({}),
    modifications: vec![],
};

let hook_result = fire_hook(&state, HookPoint::PostDecision, &mut hook_ctx).await;

// Apply decision overrides from plugins
for (area, value) in &hook_result.decision_overrides {
    tracing::info!(area, value, "Plugin override applied");
    // apply to your decision struct
}

// Inject plugin context into the next LLM prompt
let injected = hook_result.injected_context.join("\n");
let full_prompt = format!("{base_prompt}\n\n{injected}");

// Check which steps to skip
let skip = &hook_result.steps_to_skip;
```

**Invariant**: hooks MUST be called in both the oneshot path (`handlers/oneshot.rs`) AND the pipeline path (`execution_pipeline.rs`) if the stage exists in both. Never add a hook call in one and forget the other.

---

## B. Adding a new hook point

Open `crates/nexus-http/src/plugin_hooks.rs`:

### 1. Add variant to `HookPoint`

```rust
pub enum HookPoint {
    // ... existing ...
    OnMyNewEvent,  // <-- add here
}

impl HookPoint {
    pub fn as_str(&self) -> &'static str {
        match self {
            // ... existing ...
            Self::OnMyNewEvent => "on_my_new_event",
        }
    }
}
```

### 2. Add handling in `fire_hook`

In the `match hook_point { ... }` inside `fire_hook`, add a new arm:

```rust
HookPoint::OnMyNewEvent => {
    // Apply any plugin modifications specific to this hook
    for (plugin_id, step) in &pipeline_steps {
        if step.trigger == "on_my_new_event" {
            result.modifications.push(HookModification {
                plugin_id: plugin_id.clone(),
                action: HookAction::Log { message: "OnMyNewEvent fired".into() },
                description: "Plugin observed my new event".into(),
            });
            result.plugins_executed += 1;
        }
    }
}
```

### 3. Also add to `plugin_system.rs` `HookPoint` enum (they are separate)

`plugin_system.rs` has its own `HookPoint` enum for manifest declarations. Add the same variant there and in the `as_key()` impl.

---

## C. Adding a new PluginCapability type

Open `crates/nexus-http/src/plugin_system.rs` and add a variant to `PluginCapability`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PluginCapability {
    // ... existing ...
    /// My new capability type.
    MyCapability {
        /// What this capability configures.
        setting: String,
        /// Additional parameters.
        params: Vec<String>,
    },
}
```

Then handle the new variant wherever capabilities are consumed (search for `PluginCapability::` in the codebase).

---

## D. Writing a plugin manifest (JSON)

Plugins are installed as directories at `~/.nexus/plugins/<plugin-id>/`:

```
~/.nexus/plugins/my-design-system/
  manifest.json
  styles.css          (optional, for DesignSystem capability)
  components/         (optional)
```

`manifest.json` structure:

```json
{
  "id": "my-design-system",
  "name": "My Design System",
  "version": "1.0.0",
  "description": "Custom Tailwind-based design system",
  "author": "Your Name",
  "capabilities": [
    {
      "type": "design_system",
      "css": "/* your CSS here */",
      "components": ["Button", "Card", "Modal", "Input"]
    },
    {
      "type": "prompt_enhancement",
      "phase": "codegen",
      "template": "Always use the MyDS component library. Import from '@my-ds/core'."
    }
  ],
  "hooks": [
    {
      "hook_point": { "point": "pre_generation" },
      "priority": 10,
      "condition": "saas",
      "timeout_ms": 3000,
      "blocking": true,
      "requires": []
    },
    {
      "hook_point": { "point": "during_generation", "phase": "layout" },
      "priority": 0,
      "timeout_ms": 5000,
      "blocking": false,
      "requires": []
    }
  ],
  "compatibility": {
    "min_version": "0.5.0",
    "required_plugins": [],
    "conflicts_with": ["other-design-system"]
  }
}
```

### Available HookPoint values for manifests

| `"point"` value | Fires when |
|----------------|-----------|
| `"pre_intent_analysis"` | Before intent engine runs |
| `"post_intent_analysis"` | After intent parsed, before decisions |
| `"pre_decision"` | Before architecture decisions |
| `"post_decision"` | After decisions, before generation |
| `"pre_generation"` | Before code generation starts |
| `"during_generation"` | During a named generation phase (add `"phase": "..."`) |
| `"post_generation"` | After code is generated |
| `"pre_taste"` | Before taste scoring |
| `"post_taste"` | After taste scoring |
| `"pre_guarantee"` | Before outcome guarantee loop |
| `"post_guarantee"` | After outcome guarantee loop |
| `"on_error"` | On a named error class (add `"error_class": "..."`) |

### Available PluginCapability types for manifests

| `"type"` value | Purpose |
|---------------|---------|
| `"design_system"` | Inject CSS + component list into codegen |
| `"domain_pack"` | Add domain-specific entities |
| `"decision_override"` | Force an architecture decision (e.g. `"database": "postgres"`) |
| `"custom_tool"` | Register a tool agents can call |
| `"quality_rule"` | Add a custom build/taste quality rule |
| `"prompt_enhancement"` | Inject text into LLM prompts at a pipeline phase |

---

## E. Installing and testing a plugin

```bash
# Copy manifest to the plugins directory
mkdir -p ~/.nexus/plugins/my-plugin
cp manifest.json ~/.nexus/plugins/my-plugin/

# Plugins are loaded at server startup — restart the server
cargo run --bin nexus-server

# Verify the plugin loaded
curl http://localhost:8080/plugins | jq '.[] | select(.id == "my-plugin")'

# Dry-run validation (checks conflicts, compatibility, manifest validity)
curl -X POST http://localhost:8080/plugins/validate \
  -H 'Content-Type: application/json' \
  -d '{"manifest": {...}}'
```
