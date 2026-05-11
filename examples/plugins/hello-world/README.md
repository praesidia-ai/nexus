# Hello World Plugin

A minimal example plugin for the Nexus V2 plugin system.

## What it does

This plugin registers a single non-blocking hook at the `PostGeneration` hook point.
When code generation completes, the plugin's hook fires and the system logs that
the hello-world plugin was executed. Because `blocking` is `false`, this hook does
not delay the pipeline.

## Installation

### Manual installation

1. Copy this directory into your Nexus plugins folder:

```bash
cp -r examples/plugins/hello-world ~/.nexus/plugins/hello-world
```

2. Restart Nexus (or call the plugin reload endpoint).

3. Verify the plugin was loaded by checking the plugin list:

```bash
curl http://localhost:8080/plugins
```

### V1 plugin system

For the V1 plugin system, rename `manifest.json` to `plugin.json` and adjust the
hook format to use the V1 `PluginHookRegistration` structure:

```json
{
    "hook": "on_after_generation",
    "when": [],
    "inject_context": "Hello from the hello-world plugin!",
    "override_decision": null,
    "priority": 100
}
```

## Disabling

To temporarily disable the plugin without uninstalling it, create a `.disabled`
marker file:

```bash
touch ~/.nexus/plugins/hello-world/.disabled
```

Remove the file to re-enable it.

## Extending this example

To make the plugin do something useful, add capabilities to the manifest:

- **PromptEnhancement** -- inject extra instructions into the LLM prompt at a
  specific pipeline phase.
- **DecisionOverride** -- force a particular architecture decision (e.g.
  always use PostgreSQL).
- **DesignSystem** -- provide CSS and component definitions.
- **DomainPack** -- provide domain-specific entities and logic.
- **QualityRule** -- add a build/taste quality check.
- **CustomTool** -- register a tool that agents can invoke.

See `crates/nexus-http/src/plugin_system_v2.rs` for the full capability and
hook point reference.
