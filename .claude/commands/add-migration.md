Add a new SQLite migration to nexus-store.

Read the skill at `.claude/skills/add-migration/SKILL.md` first, then:

1. Check the current highest migration: `ls crates/nexus-store/migrations/ | sort | tail -3`
2. Create `crates/nexus-store/migrations/00N_<descriptive_name>.sql` with the next number (3-digit padded).
3. Add `const MIGRATION_<NAME>: &str = include_str!("../migrations/00N_<name>.sql");` near the other constants in `crates/nexus-store/src/db.rs`.
4. Add `apply(N, MIGRATION_<NAME>)?;` at the end of the apply chain inside `run_migrations` — never reorder existing applies.
5. Add store functions / types in `crates/nexus-store/src/<module>.rs` (or new module).
6. Export new public types/functions from `crates/nexus-store/src/lib.rs`.
7. Run `cargo build -p nexus-store` and fix any errors.
8. Run `cargo test -p nexus-store` to verify the migration applies cleanly on a fresh DB.

Rules:
- Never renumber existing migrations.
- Never use bare `CREATE TABLE` — always `CREATE TABLE IF NOT EXISTS`.
- Migrations must be backward-compatible (additive); never drop columns/tables in place.
- For tenant-scoped tables, add `project_id TEXT NOT NULL` + an index — see `003_tenant_isolation.sql` for the pattern.
- For new tables, add an index on every FK column.
