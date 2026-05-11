Add a new engine to nexus-http following the deterministic-first, LLM-fallback pattern.

Read `.claude/skills/add-engine/SKILL.md` first, then:

1. Clarify: what is the engine's input? what does it classify/score/decide?
2. Create `crates/nexus-http/src/<name>_engine.rs` with:
   - Deterministic layer (NO LLM calls, keyword/rule heuristics only)
   - Optional semantic LLM layer (only called when deterministic confidence < 0.6)
   - Public `analyze(input, state) -> EngineResult` async entry point
3. Add `pub mod <name>_engine;` to `crates/nexus-http/src/lib.rs`
4. If this engine should be called from the oneshot pipeline, wire it into `handlers/oneshot.rs`
   - Add it to the appropriate phase
   - Emit a `Phase` SSE event with the result
5. If plugins should observe it, add a `HookPoint` variant and call `plugin_hooks::run_hook`
6. Run `cargo build -p nexus-http` and fix errors
7. Run `cargo clippy -p nexus-http -- -D warnings`
8. Add at least one unit test covering the deterministic path (no LLM mock needed)

Core invariant: the deterministic analysis function must take only `&str` input — no `state` parameter.
