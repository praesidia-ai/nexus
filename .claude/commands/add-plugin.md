Add a plugin, plugin hook, or new pipeline interception point.

Read `.claude/skills/add-plugin/SKILL.md` first, then determine what you need:

**Call an existing hook from new code** (most common):
1. Import `fire_hook`, `HookContext`, `HookPoint` from `crate::plugin_hooks`
2. Build `HookContext` with whatever pipeline data is available at that point
3. Call `fire_hook(&state, HookPoint::X, &mut ctx).await`
4. Apply `hook_result.decision_overrides` and `hook_result.injected_context` to your pipeline
5. Check that the same hook is called in BOTH oneshot and pipeline paths

**Add a new HookPoint** (new interception location):
1. Add variant to `HookPoint` enum in `plugin_hooks.rs` + `as_str()` impl
2. Add the same variant to `HookPoint` in `plugin_system.rs` + `as_key()` impl
3. Add handling arm in `fire_hook` match block
4. Call it from the handler/engine at the right stage

**Write a plugin manifest** (new plugin file):
1. Create `~/.nexus/plugins/<id>/manifest.json` following the schema in the skill
2. Choose capabilities and hook declarations
3. Run `cargo run --bin nexus-server` to reload
4. Verify: `curl http://localhost:8080/plugins | jq '.'`

After any Rust changes: `cargo build -p nexus-http && cargo clippy -p nexus-http -- -D warnings`
