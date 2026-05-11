-- Baseline schema: squashed from migrations 001-028.

-- schema_migrations is created in db.rs before this batch runs.

CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY NOT NULL,
    created_at TEXT NOT NULL,
    title TEXT
);

CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS decisions (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    summary TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS artifacts (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    path TEXT,
    content TEXT,
    status TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    phase INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
, llm_provider TEXT, llm_model TEXT, tenant_id TEXT NOT NULL DEFAULT 'default');

CREATE TABLE IF NOT EXISTS conversations (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL
, phase_state_json TEXT DEFAULT NULL);

CREATE TABLE IF NOT EXISTS nexus_messages (
    id TEXT PRIMARY KEY NOT NULL,
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK(role IN ('user', 'assistant', 'system')),
    content TEXT NOT NULL,
    -- Stores the original AI response including <nexus_state> block for replay
    raw_content TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS knowledge_items (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    -- entity | relationship | rule | agent | integration | portal | database
    item_type TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    icon TEXT,
    -- JSON blob for arbitrary extra metadata
    metadata TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS agent_definitions (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    role TEXT NOT NULL,
    -- JSON array of tool names
    tools TEXT NOT NULL DEFAULT '[]',
    memory_type TEXT NOT NULL DEFAULT 'persistent',
    provider TEXT NOT NULL DEFAULT 'anthropic',
    model TEXT NOT NULL,
    system_prompt TEXT NOT NULL,
    -- defined | running | stopped | error
    status TEXT NOT NULL DEFAULT 'defined',
    zeroclaw_pid INTEGER,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
, zeroclaw_port INTEGER, persona TEXT, custom_rules TEXT DEFAULT '[]', temperature REAL DEFAULT 0.7, max_tokens INTEGER DEFAULT 4096);

CREATE TABLE IF NOT EXISTS materialized_tables (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    table_name TEXT NOT NULL,
    -- Full JSON schema as returned by EntitySchema
    schema_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS workflow_runs (
    id          TEXT PRIMARY KEY NOT NULL,
    project_id  TEXT REFERENCES projects(id) ON DELETE CASCADE,
    pipeline    TEXT NOT NULL,
    -- running | completed | failed
    status      TEXT NOT NULL DEFAULT 'running',
    input_json  TEXT,
    output_json TEXT,
    error       TEXT,
    started_at  TEXT NOT NULL,
    finished_at TEXT
);

CREATE TABLE IF NOT EXISTS portals (
    id           TEXT PRIMARY KEY NOT NULL,
    project_id   TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    slug         TEXT UNIQUE NOT NULL,
    project_name TEXT NOT NULL,
    agent_name   TEXT NOT NULL,
    published_at TEXT,
    created_at   TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS vault_entries (
    id         TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    key        TEXT NOT NULL,
    -- stored base64-encoded; callers are responsible for encryption
    value      TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(project_id, key)
);

CREATE TABLE IF NOT EXISTS step_metrics (
    id            TEXT PRIMARY KEY NOT NULL,
    run_id        TEXT NOT NULL,
    step_id       TEXT NOT NULL,
    agent         TEXT NOT NULL,
    duration_ms   INTEGER NOT NULL DEFAULT 0,
    retries_used  INTEGER NOT NULL DEFAULT 0,
    skipped       INTEGER NOT NULL DEFAULT 0,
    output_length INTEGER NOT NULL DEFAULT 0,
    -- success | failure | skipped
    status        TEXT NOT NULL DEFAULT 'success',
    error         TEXT,
    created_at    TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS run_metrics (
    id                TEXT PRIMARY KEY NOT NULL,
    project_id        TEXT REFERENCES projects(id) ON DELETE CASCADE,
    pipeline          TEXT NOT NULL,
    total_duration_ms INTEGER NOT NULL DEFAULT 0,
    steps_total       INTEGER NOT NULL DEFAULT 0,
    steps_succeeded   INTEGER NOT NULL DEFAULT 0,
    steps_failed      INTEGER NOT NULL DEFAULT 0,
    steps_skipped     INTEGER NOT NULL DEFAULT 0,
    total_retries     INTEGER NOT NULL DEFAULT 0,
    -- success | partial | failure
    status            TEXT NOT NULL DEFAULT 'success',
    created_at        TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS feedback (
    id          TEXT PRIMARY KEY NOT NULL,
    project_id  TEXT REFERENCES projects(id) ON DELETE CASCADE,
    run_id      TEXT,
    step_id     TEXT,
    -- 0 = not rated, 1-5 star rating
    rating      INTEGER NOT NULL DEFAULT 0,
    comment     TEXT,
    -- positive | negative | correction | neutral
    signal      TEXT NOT NULL DEFAULT 'neutral',
    created_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS app_instances (
    id         TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    port       INTEGER NOT NULL,
    pid        INTEGER,
    status     TEXT NOT NULL DEFAULT 'installing',
    output_dir TEXT NOT NULL,
    error      TEXT,
    started_at TEXT NOT NULL,
    stopped_at TEXT
, sandbox INTEGER NOT NULL DEFAULT 0, container_id TEXT, logs TEXT NOT NULL DEFAULT '', health_check_url TEXT, health_status TEXT DEFAULT 'unknown', restart_count INTEGER DEFAULT 0, auto_restart INTEGER DEFAULT 1, crash_reason TEXT, build_started_at TEXT, build_completed_at TEXT);

CREATE TABLE IF NOT EXISTS codegen_manifest (
    project_id TEXT NOT NULL,
    path       TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    generated_at TEXT NOT NULL,
    PRIMARY KEY(project_id, path)
);

CREATE TABLE IF NOT EXISTS schema_versions (
    project_id    TEXT NOT NULL,
    table_name    TEXT NOT NULL,
    version       INTEGER NOT NULL,
    fields_json   TEXT NOT NULL,
    migration_sql TEXT,
    created_at    TEXT NOT NULL,
    PRIMARY KEY(project_id, table_name, version)
);

CREATE TABLE IF NOT EXISTS coding_tasks (
    id            TEXT PRIMARY KEY NOT NULL,
    project_id    TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    -- What the user asked for
    title         TEXT NOT NULL,
    description   TEXT NOT NULL,
    -- Which agent is assigned (nova, orion, luna, etc.)
    agent         TEXT NOT NULL DEFAULT 'nova',
    -- pending | planning | coding | validating | reviewing | completed | failed
    status        TEXT NOT NULL DEFAULT 'pending',
    -- Current phase detail shown to user
    status_detail TEXT,
    -- Priority: 1=critical, 2=high, 3=normal, 4=low
    priority      INTEGER NOT NULL DEFAULT 3,
    -- The plan produced by the agent before coding
    plan_json     TEXT,
    -- Files the agent intends to create/modify
    files_json    TEXT,
    -- The generated code output
    output        TEXT,
    -- Validation/review results
    review_json   TEXT,
    -- Error if failed
    error         TEXT,
    -- Timing
    started_at    TEXT,
    completed_at  TEXT,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS app_build_steps (
    id TEXT PRIMARY KEY,
    instance_id TEXT NOT NULL REFERENCES app_instances(id),
    step_name TEXT NOT NULL,
    label TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    started_at TEXT,
    completed_at TEXT,
    duration_ms INTEGER,
    output TEXT DEFAULT '',
    error TEXT
);

CREATE TABLE IF NOT EXISTS app_env_vars (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(project_id, key)
);

CREATE TABLE IF NOT EXISTS agent_skills (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT,  -- NULL = global preset, non-NULL = project-specific custom skill
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    icon TEXT NOT NULL DEFAULT '⚡',
    category TEXT NOT NULL DEFAULT 'general',  -- coding, research, writing, data, security, devops, support, custom
    system_prompt TEXT NOT NULL,
    tools TEXT NOT NULL DEFAULT '[]',  -- JSON array
    rules TEXT NOT NULL DEFAULT '[]',  -- JSON array of behavioral rules
    examples TEXT NOT NULL DEFAULT '[]', -- JSON array of example interactions
    temperature REAL NOT NULL DEFAULT 0.7,
    max_tokens INTEGER NOT NULL DEFAULT 4096,
    is_builtin INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS agent_skill_assignments (
    id TEXT PRIMARY KEY NOT NULL,
    agent_id TEXT NOT NULL,
    skill_id TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0,  -- higher = applied first in prompt composition
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(agent_id, skill_id)
);

CREATE TABLE IF NOT EXISTS agent_traces (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    task TEXT NOT NULL,
    mode TEXT NOT NULL DEFAULT 'auto',       -- auto | pipeline
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'running',   -- running | completed | failed
    total_iterations INTEGER DEFAULT 0,
    total_tool_calls INTEGER DEFAULT 0,
    total_tokens_in INTEGER DEFAULT 0,
    total_tokens_out INTEGER DEFAULT 0,
    estimated_cost_usd REAL DEFAULT 0.0,
    duration_ms INTEGER DEFAULT 0,
    error TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT
);

CREATE TABLE IF NOT EXISTS agent_tool_logs (
    id TEXT PRIMARY KEY NOT NULL,
    trace_id TEXT NOT NULL REFERENCES agent_traces(id),
    iteration INTEGER NOT NULL,
    tool_name TEXT NOT NULL,
    arguments TEXT NOT NULL DEFAULT '{}',
    output_preview TEXT,          -- first 500 chars of output
    success INTEGER NOT NULL DEFAULT 1,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS healing_events (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    instance_id TEXT NOT NULL,
    trigger TEXT NOT NULL,
    error_log TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'diagnosing',
    fix_summary TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS agent_evolution (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    agent_name TEXT NOT NULL,
    skill_id TEXT,
    task_type TEXT NOT NULL,       -- coding, review, test, fix, etc.
    success INTEGER NOT NULL,      -- 1 or 0
    iterations_used INTEGER,
    tools_used TEXT DEFAULT '[]',  -- JSON array
    feedback_score INTEGER,        -- 1-5 user rating (optional)
    notes TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS audit_log (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    actor TEXT NOT NULL,           -- "user", "agent:nova", "system", etc.
    action TEXT NOT NULL,          -- "file_write", "bash", "app_start", "deploy", etc.
    target TEXT,                   -- file path, command, etc.
    details TEXT,                  -- JSON with extra context
    risk_level TEXT DEFAULT 'low', -- low, medium, high, critical
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS global_memory (
    id TEXT PRIMARY KEY NOT NULL,
    category TEXT NOT NULL,     -- "preference", "pattern", "convention", "decision", "feedback"
    key TEXT NOT NULL UNIQUE,
    value TEXT NOT NULL,
    source TEXT,               -- which project/interaction it came from
    confidence REAL DEFAULT 1.0,  -- 0-1, decreases if contradicted
    times_applied INTEGER DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS background_agents (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    agent_type TEXT NOT NULL,      -- "watcher", "optimizer", "monitor", "refactor"
    config TEXT NOT NULL DEFAULT '{}',  -- JSON config
    status TEXT NOT NULL DEFAULT 'stopped',  -- running, stopped, paused
    last_run TEXT,
    run_count INTEGER DEFAULT 0,
    interval_secs INTEGER DEFAULT 3600,  -- how often to run (default: 1 hour)
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS marketplace_items (
    id TEXT PRIMARY KEY NOT NULL,
    item_type TEXT NOT NULL,       -- "agent", "skill", "workflow", "integration"
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    author TEXT NOT NULL DEFAULT 'community',
    version TEXT NOT NULL DEFAULT '1.0.0',
    icon TEXT DEFAULT '🧩',
    tags TEXT DEFAULT '[]',        -- JSON array
    config TEXT NOT NULL DEFAULT '{}',  -- JSON (the actual plugin data)
    downloads INTEGER DEFAULT 0,
    rating REAL DEFAULT 0.0,
    is_official INTEGER DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS collab_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    created_by TEXT NOT NULL DEFAULT 'user',
    status TEXT NOT NULL DEFAULT 'active',  -- active, ended
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    ended_at TEXT
);

CREATE TABLE IF NOT EXISTS collab_participants (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL REFERENCES collab_sessions(id),
    name TEXT NOT NULL,
    role TEXT NOT NULL,            -- "user", "agent:nova", "agent:orion", etc.
    participant_type TEXT NOT NULL, -- "human", "agent"
    status TEXT NOT NULL DEFAULT 'active',
    joined_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS collab_activity (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL REFERENCES collab_sessions(id),
    participant_id TEXT NOT NULL,
    activity_type TEXT NOT NULL,    -- "editing", "reviewing", "testing", "chatting", "idle"
    target TEXT,                    -- file path, component name, etc.
    detail TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS llm_cost_records (
    id          TEXT PRIMARY KEY,
    project_id  TEXT,
    model       TEXT NOT NULL,
    provider    TEXT NOT NULL,
    purpose     TEXT NOT NULL DEFAULT 'general',
    input_tokens  INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    cost_usd    REAL NOT NULL DEFAULT 0.0,
    latency_ms  INTEGER NOT NULL DEFAULT 0,
    cached      INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS cost_budgets (
    id              TEXT PRIMARY KEY,
    scope           TEXT NOT NULL DEFAULT 'global',
    scope_id        TEXT,
    daily_limit_usd REAL NOT NULL DEFAULT 50.0,
    warn_threshold  REAL NOT NULL DEFAULT 0.8,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS background_agent_runs (
    id          TEXT PRIMARY KEY,
    agent_id    TEXT NOT NULL REFERENCES background_agents(id) ON DELETE CASCADE,
    status      TEXT NOT NULL DEFAULT 'running',
    output      TEXT,
    error       TEXT,
    started_at  TEXT NOT NULL DEFAULT (datetime('now')),
    finished_at TEXT,
    duration_ms INTEGER
);

CREATE TABLE IF NOT EXISTS decision_feedback (
    id              TEXT PRIMARY KEY,
    project_id      TEXT NOT NULL,
    area            TEXT NOT NULL,           -- frontend, backend, database, auth, hosting, ui_library
    chosen_value    TEXT NOT NULL,           -- what was decided
    outcome         TEXT NOT NULL DEFAULT 'pending',  -- success, override, failure
    user_override   TEXT,                    -- what user changed it to (NULL if kept)
    build_success   INTEGER,                -- 1 = build passed, 0 = failed
    taste_score     INTEGER,                -- taste score after generation
    feedback_signal TEXT,                    -- user feedback text
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS decision_weights (
    area            TEXT NOT NULL,
    value           TEXT NOT NULL,
    success_count   INTEGER NOT NULL DEFAULT 0,
    failure_count   INTEGER NOT NULL DEFAULT 0,
    override_count  INTEGER NOT NULL DEFAULT 0,
    weight_adj      REAL NOT NULL DEFAULT 0.0,  -- positive = prefer, negative = avoid
    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (area, value)
);

CREATE TABLE IF NOT EXISTS user_preferences (
    id              TEXT PRIMARY KEY,
    category        TEXT NOT NULL,           -- ui_style, tech_stack, product_pattern, copy_tone
    key             TEXT NOT NULL,
    value           TEXT NOT NULL,
    confidence      REAL NOT NULL DEFAULT 0.5,  -- 0.0-1.0
    times_observed  INTEGER NOT NULL DEFAULT 1,
    source          TEXT NOT NULL DEFAULT 'inferred',  -- inferred, explicit, override
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(category, key)
);

CREATE TABLE IF NOT EXISTS taste_history (
    id              TEXT PRIMARY KEY,
    project_id      TEXT NOT NULL,
    overall         INTEGER NOT NULL,
    visual          INTEGER NOT NULL DEFAULT 0,
    ux              INTEGER NOT NULL DEFAULT 0,
    conversion      INTEGER NOT NULL DEFAULT 0,
    modern_ui       INTEGER NOT NULL DEFAULT 0,
    accessibility   INTEGER NOT NULL DEFAULT 0,
    responsiveness  INTEGER NOT NULL DEFAULT 0,
    improvements_applied INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS agent_versions (
    id            TEXT PRIMARY KEY,
    agent_role    TEXT NOT NULL,       -- 'architect', 'coder', 'reviewer', etc.
    version       INTEGER NOT NULL DEFAULT 1,
    parent_id     TEXT,                -- NULL for v1, points to previous version
    system_prompt TEXT NOT NULL,
    tools         TEXT NOT NULL DEFAULT '[]',   -- JSON array
    config        TEXT NOT NULL DEFAULT '{}',   -- JSON: max_iterations, temperature, etc.
    mutation_type TEXT,                -- 'prompt_tweak', 'tool_add', 'iteration_adj', 'model_change'
    mutation_desc TEXT,                -- human-readable description of what changed
    status        TEXT NOT NULL DEFAULT 'candidate', -- 'candidate', 'active', 'champion', 'retired'
    -- Performance metrics (updated after each use)
    total_uses    INTEGER NOT NULL DEFAULT 0,
    successes     INTEGER NOT NULL DEFAULT 0,
    failures      INTEGER NOT NULL DEFAULT 0,
    avg_iterations REAL NOT NULL DEFAULT 0.0,
    avg_duration_ms REAL NOT NULL DEFAULT 0.0,
    fitness_score REAL NOT NULL DEFAULT 0.0,   -- composite: success_rate * speed * quality
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at    TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS skill_dna (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    description   TEXT NOT NULL,
    -- DNA genes
    intent        TEXT NOT NULL,        -- what problem this skill solves
    tools         TEXT NOT NULL DEFAULT '[]',    -- JSON: which tools it uses
    patterns      TEXT NOT NULL DEFAULT '[]',    -- JSON: code/workflow patterns
    examples      TEXT NOT NULL DEFAULT '[]',    -- JSON: input/output examples
    constraints   TEXT NOT NULL DEFAULT '[]',    -- JSON: rules/invariants
    prompt_fragment TEXT NOT NULL DEFAULT '',     -- injectable prompt text
    -- Lineage
    source_type   TEXT NOT NULL DEFAULT 'auto',  -- 'auto', 'manual', 'merged'
    source_executions TEXT NOT NULL DEFAULT '[]', -- JSON: execution IDs it was extracted from
    parent_ids    TEXT NOT NULL DEFAULT '[]',     -- JSON: parent skill IDs (if merged)
    generation    INTEGER NOT NULL DEFAULT 0,
    -- Performance
    total_uses    INTEGER NOT NULL DEFAULT 0,
    successes     INTEGER NOT NULL DEFAULT 0,
    failures      INTEGER NOT NULL DEFAULT 0,
    confidence    REAL NOT NULL DEFAULT 0.5,
    status        TEXT NOT NULL DEFAULT 'draft',  -- 'draft', 'validated', 'active', 'archived'
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at    TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS agent_skill_dna (
    agent_role    TEXT NOT NULL,
    skill_id      TEXT NOT NULL REFERENCES skill_dna(id) ON DELETE CASCADE,
    priority      INTEGER NOT NULL DEFAULT 0,
    assigned_at   TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (agent_role, skill_id)
);

CREATE TABLE IF NOT EXISTS scheduled_tasks (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    task_type     TEXT NOT NULL,        -- 'evolution_cycle', 'skill_extraction', 'agent_mutation', 'quality_audit', 'metric_cleanup'
    config        TEXT NOT NULL DEFAULT '{}',  -- JSON: task-specific config
    interval_secs INTEGER NOT NULL DEFAULT 300, -- run every N seconds
    priority      INTEGER NOT NULL DEFAULT 5,   -- 1=highest, 10=lowest
    conflict_group TEXT,                -- tasks in same group don't run concurrently
    status        TEXT NOT NULL DEFAULT 'active', -- 'active', 'paused', 'disabled'
    last_run      TEXT,
    last_result   TEXT,                 -- 'success', 'failure', 'skipped'
    last_error    TEXT,
    run_count     INTEGER NOT NULL DEFAULT 0,
    fail_count    INTEGER NOT NULL DEFAULT 0,
    next_run      TEXT,                 -- precomputed next eligible run time
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at    TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS quality_gate_log (
    id            TEXT PRIMARY KEY,
    project_id    TEXT NOT NULL,
    attempt       INTEGER NOT NULL DEFAULT 1,
    taste_score   INTEGER,
    build_passed  INTEGER,             -- 0/1
    invariant_violations TEXT DEFAULT '[]',  -- JSON array
    gate_passed   INTEGER NOT NULL DEFAULT 0, -- 0/1
    improvements  TEXT DEFAULT '[]',    -- JSON: what was auto-fixed
    duration_ms   INTEGER,
    created_at    TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS cross_project_intelligence (
    id TEXT PRIMARY KEY NOT NULL,
    app_type TEXT NOT NULL,
    domain TEXT NOT NULL,
    feature_pattern TEXT NOT NULL,
    decision_area TEXT NOT NULL,
    decision_value TEXT NOT NULL,
    outcome TEXT NOT NULL CHECK(outcome IN ('success', 'failure', 'override')),
    taste_score INTEGER,
    build_success INTEGER,
    performance_data TEXT,
    project_count INTEGER NOT NULL DEFAULT 1,
    confidence REAL NOT NULL DEFAULT 0.5,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS runtime_feedback (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    feedback_type TEXT NOT NULL CHECK(feedback_type IN ('api_latency', 'error_rate', 'user_action', 'crash', 'performance')),
    endpoint TEXT,
    value REAL NOT NULL,
    metadata TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS generation_variants (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    variant_index INTEGER NOT NULL,
    variant_type TEXT NOT NULL CHECK(variant_type IN ('ui', 'api', 'full')),
    description TEXT NOT NULL,
    taste_score INTEGER,
    heuristic_score REAL,
    selected INTEGER NOT NULL DEFAULT 0,
    files_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS intelligence_aggregates (
    id TEXT PRIMARY KEY NOT NULL,
    aggregate_type TEXT NOT NULL CHECK(aggregate_type IN ('app_type_best', 'domain_best', 'feature_best', 'stack_combo')),
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    score REAL NOT NULL DEFAULT 0.0,
    sample_count INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(aggregate_type, key)
);

CREATE TABLE IF NOT EXISTS business_team_instances (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id),
    template_name TEXT NOT NULL,
    display_name TEXT NOT NULL,
    autonomy_level TEXT NOT NULL DEFAULT 'supervised',
    control_mode TEXT NOT NULL DEFAULT 'assisted',
    status TEXT NOT NULL DEFAULT 'active',
    config_json TEXT DEFAULT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS business_events (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id),
    team_id TEXT REFERENCES business_team_instances(id),
    event_type TEXT NOT NULL,
    payload_json TEXT NOT NULL DEFAULT '{}',
    severity TEXT NOT NULL DEFAULT 'info',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS credit_accounts (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL DEFAULT 'default',
    plan TEXT NOT NULL DEFAULT 'free',
    credits_total INTEGER NOT NULL DEFAULT 50,
    credits_used INTEGER NOT NULL DEFAULT 0,
    credits_reset_at TEXT NOT NULL DEFAULT (datetime('now', '+30 days')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS credit_usage (
    id TEXT PRIMARY KEY NOT NULL,
    account_id TEXT NOT NULL REFERENCES credit_accounts(id),
    credits_spent INTEGER NOT NULL DEFAULT 1,
    action_type TEXT NOT NULL,
    description TEXT,
    project_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS starter_templates (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    category TEXT NOT NULL,
    icon TEXT NOT NULL DEFAULT 'sparkles',
    preview_prompt TEXT NOT NULL,
    features TEXT NOT NULL DEFAULT '[]',
    estimated_credits INTEGER NOT NULL DEFAULT 10,
    popularity INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS active_generations (
    project_id  TEXT PRIMARY KEY NOT NULL,
    started_at  TEXT NOT NULL DEFAULT (datetime('now')),
    description TEXT
);

CREATE TABLE IF NOT EXISTS audit_entries (
    id         TEXT PRIMARY KEY,
    project_id TEXT,
    action     TEXT NOT NULL,
    actor      TEXT,
    detail     TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS teams (
    id               TEXT PRIMARY KEY,
    name             TEXT NOT NULL,
    definition_json  TEXT NOT NULL,
    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at       TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS team_runs (
    id              TEXT PRIMARY KEY,
    team_id         TEXT NOT NULL,
    task            TEXT NOT NULL,
    status          TEXT NOT NULL CHECK(status IN ('running','completed','failed','cancelled')),
    started_at      TEXT NOT NULL,
    completed_at    TEXT,
    duration_secs   INTEGER NOT NULL DEFAULT 0,
    total_cost_usd  REAL    NOT NULL DEFAULT 0,
    message_count   INTEGER NOT NULL DEFAULT 0,
    artifact_count  INTEGER NOT NULL DEFAULT 0,
    summary         TEXT    NOT NULL DEFAULT '',
    -- Final snapshot written on completion:
    --   { messages: [...], artifacts: [...], cost_breakdown: [...] }
    result_json     TEXT
);

CREATE TABLE IF NOT EXISTS user_profile_prefs (
    tenant_id             TEXT PRIMARY KEY NOT NULL,
    display_mode          TEXT NOT NULL DEFAULT 'consumer',
    onboarding_completed  INTEGER NOT NULL DEFAULT 0,
    onboarding_step       INTEGER NOT NULL DEFAULT 0,
    theme                 TEXT NOT NULL DEFAULT 'dark',
    show_cost_estimates   INTEGER NOT NULL DEFAULT 1,
    preferred_quality     TEXT NOT NULL DEFAULT 'balanced',
    created_at            TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at            TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id);

CREATE INDEX IF NOT EXISTS idx_decisions_session ON decisions(session_id);

CREATE INDEX IF NOT EXISTS idx_artifacts_session ON artifacts(session_id);

CREATE INDEX IF NOT EXISTS idx_conversations_project ON conversations(project_id);

CREATE INDEX IF NOT EXISTS idx_nexus_messages_conversation ON nexus_messages(conversation_id);

CREATE INDEX IF NOT EXISTS idx_knowledge_items_project ON knowledge_items(project_id);

CREATE INDEX IF NOT EXISTS idx_agent_definitions_project ON agent_definitions(project_id);

CREATE INDEX IF NOT EXISTS idx_materialized_tables_project ON materialized_tables(project_id);

CREATE INDEX IF NOT EXISTS idx_workflow_runs_project ON workflow_runs(project_id);

CREATE INDEX IF NOT EXISTS idx_portals_project       ON portals(project_id);

CREATE INDEX IF NOT EXISTS idx_vault_entries_project ON vault_entries(project_id);

CREATE INDEX IF NOT EXISTS idx_step_metrics_run    ON step_metrics(run_id);

CREATE INDEX IF NOT EXISTS idx_step_metrics_agent  ON step_metrics(agent);

CREATE INDEX IF NOT EXISTS idx_run_metrics_project ON run_metrics(project_id);

CREATE INDEX IF NOT EXISTS idx_run_metrics_pipeline ON run_metrics(pipeline);

CREATE INDEX IF NOT EXISTS idx_feedback_project    ON feedback(project_id);

CREATE INDEX IF NOT EXISTS idx_feedback_run        ON feedback(run_id);

CREATE INDEX IF NOT EXISTS idx_app_instances_project ON app_instances(project_id);

CREATE INDEX IF NOT EXISTS idx_app_instances_status ON app_instances(status);

CREATE INDEX IF NOT EXISTS idx_codegen_manifest_project ON codegen_manifest(project_id);

CREATE INDEX IF NOT EXISTS idx_schema_versions_project ON schema_versions(project_id);

CREATE INDEX IF NOT EXISTS idx_coding_tasks_project ON coding_tasks(project_id);

CREATE INDEX IF NOT EXISTS idx_coding_tasks_status  ON coding_tasks(status);

CREATE INDEX IF NOT EXISTS idx_agent_skills_project ON agent_skills(project_id);

CREATE INDEX IF NOT EXISTS idx_agent_skills_category ON agent_skills(category);

CREATE INDEX IF NOT EXISTS idx_agent_traces_project ON agent_traces(project_id);

CREATE INDEX IF NOT EXISTS idx_agent_tool_logs_trace ON agent_tool_logs(trace_id);

CREATE INDEX IF NOT EXISTS idx_agent_evolution_project ON agent_evolution(project_id);

CREATE INDEX IF NOT EXISTS idx_audit_log_project ON audit_log(project_id);

CREATE INDEX IF NOT EXISTS idx_audit_log_action ON audit_log(action);

CREATE INDEX IF NOT EXISTS idx_global_memory_category ON global_memory(category);

CREATE INDEX IF NOT EXISTS idx_background_agents_project ON background_agents(project_id);

CREATE INDEX IF NOT EXISTS idx_marketplace_type ON marketplace_items(item_type);

CREATE INDEX IF NOT EXISTS idx_collab_activity_session ON collab_activity(session_id);

CREATE INDEX IF NOT EXISTS idx_cost_records_project ON llm_cost_records(project_id);

CREATE INDEX IF NOT EXISTS idx_cost_records_date ON llm_cost_records(created_at);

CREATE INDEX IF NOT EXISTS idx_cost_records_model ON llm_cost_records(model);

CREATE INDEX IF NOT EXISTS idx_bg_agent_runs_agent ON background_agent_runs(agent_id);

CREATE INDEX IF NOT EXISTS idx_decision_fb_area ON decision_feedback(area, chosen_value);

CREATE INDEX IF NOT EXISTS idx_taste_history_project ON taste_history(project_id, created_at);

CREATE INDEX IF NOT EXISTS idx_agent_versions_role ON agent_versions(agent_role, status);

CREATE INDEX IF NOT EXISTS idx_agent_versions_fitness ON agent_versions(agent_role, fitness_score DESC);

CREATE INDEX IF NOT EXISTS idx_agent_versions_parent ON agent_versions(parent_id);

CREATE INDEX IF NOT EXISTS idx_skill_dna_status ON skill_dna(status);

CREATE INDEX IF NOT EXISTS idx_skill_dna_confidence ON skill_dna(confidence DESC);

CREATE INDEX IF NOT EXISTS idx_scheduled_tasks_status ON scheduled_tasks(status, next_run);

CREATE INDEX IF NOT EXISTS idx_scheduled_tasks_type ON scheduled_tasks(task_type);

CREATE INDEX IF NOT EXISTS idx_quality_gate_project ON quality_gate_log(project_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_background_agents_status ON background_agents(status);

CREATE INDEX IF NOT EXISTS idx_app_instances_project_status ON app_instances(project_id, status);

CREATE INDEX IF NOT EXISTS idx_cpi_app_type ON cross_project_intelligence(app_type);

CREATE INDEX IF NOT EXISTS idx_cpi_domain ON cross_project_intelligence(domain);

CREATE INDEX IF NOT EXISTS idx_cpi_feature ON cross_project_intelligence(feature_pattern);

CREATE INDEX IF NOT EXISTS idx_cpi_decision ON cross_project_intelligence(decision_area, decision_value);

CREATE INDEX IF NOT EXISTS idx_rf_project ON runtime_feedback(project_id);

CREATE INDEX IF NOT EXISTS idx_rf_type ON runtime_feedback(feedback_type);

CREATE INDEX IF NOT EXISTS idx_rf_created ON runtime_feedback(created_at);

CREATE INDEX IF NOT EXISTS idx_gv_project ON generation_variants(project_id);

CREATE INDEX IF NOT EXISTS idx_projects_tenant ON projects(tenant_id);

CREATE INDEX IF NOT EXISTS idx_bti_project ON business_team_instances(project_id);

CREATE INDEX IF NOT EXISTS idx_bti_status ON business_team_instances(status);

CREATE INDEX IF NOT EXISTS idx_be_project ON business_events(project_id);

CREATE INDEX IF NOT EXISTS idx_be_team ON business_events(team_id);

CREATE INDEX IF NOT EXISTS idx_be_type ON business_events(event_type);

CREATE INDEX IF NOT EXISTS idx_be_created ON business_events(created_at);

CREATE UNIQUE INDEX IF NOT EXISTS idx_credit_tenant ON credit_accounts(tenant_id);

CREATE INDEX IF NOT EXISTS idx_cu_account ON credit_usage(account_id);

CREATE INDEX IF NOT EXISTS idx_cu_created ON credit_usage(created_at);

CREATE INDEX IF NOT EXISTS idx_audit_entries_project    ON audit_entries(project_id);

CREATE INDEX IF NOT EXISTS idx_audit_entries_action     ON audit_entries(action);

CREATE INDEX IF NOT EXISTS idx_audit_entries_created_at ON audit_entries(created_at);

CREATE INDEX IF NOT EXISTS idx_teams_name ON teams(name);

CREATE INDEX IF NOT EXISTS idx_team_runs_team_id    ON team_runs(team_id);

CREATE INDEX IF NOT EXISTS idx_team_runs_started_at ON team_runs(started_at DESC);

CREATE INDEX IF NOT EXISTS idx_team_runs_status     ON team_runs(status);
