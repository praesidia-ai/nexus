Add a new Axum HTTP handler to nexus-http.

Read the skill at `.claude/skills/add-handler/SKILL.md` first, then follow these steps:

1. Ask (if not already specified): handler name, HTTP method(s), route path, project-scoped or global, does it need SSE streaming?
2. Create `crates/nexus-http/src/handlers/<name>_handler.rs` following the skill pattern.
3. If the route is project-scoped (operates on `:id`), use the `ProjectAccess` extractor — never raw `Path<String>` for project IDs.
4. If the handler calls an LLM, acquire a `rate_limiter` slot and emit a `cost_tracker` record.
5. Add `pub mod <name>_handler;` to `crates/nexus-http/src/handlers/mod.rs` in alphabetical order.
6. Wire the route in `crates/nexus-http/src/server.rs` — group it with semantically related routes; place it inside the auth-required router group unless intentionally public.
7. If SSE is needed, also read `.claude/skills/sse-streaming/SKILL.md`.
8. Validate request bodies against `crate::input_limits::*` for size/length caps before any DB/LLM work.
9. Run `cargo build -p nexus-http` and fix any errors.
10. Run `cargo clippy -p nexus-http -- -D warnings` and fix all warnings.
