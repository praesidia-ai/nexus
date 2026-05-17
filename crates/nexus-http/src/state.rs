//! Shared application state injected into every Axum handler via `State<Arc<AppState>>`.

use std::path::PathBuf;
use std::sync::Arc;

use nexus_a2a::{A2aServer, AgentCardRegistry, FederationManager, NexusTaskHandler};
use nexus_kernel::{
    AgentScheduler, AuditKeypair, AuditLog, EventBus, EventBusConfig, ReactiveAgentManager,
    SchedulerConfig,
};
use nexus_store::{open_connection, run_migrations, ConnectionPool, ReaderGuard};
use tokio::sync::OwnedMutexGuard;
use tracing::info;

pub struct AppState {
    /// Main nexus.db connection — holds projects, conversations, knowledge, agents, etc.
    ///
    /// Legacy single-writer mutex. Every site that hasn't yet migrated to the
    /// pool still goes through here. New code on hot read paths should call
    /// [`AppState::acquire_reader`] instead, which dispatches across N reader
    /// connections in parallel.
    pub db: Arc<tokio::sync::Mutex<rusqlite::Connection>>,
    /// SQLite connection pool — 1 writer + N readers, all opened on the same
    /// nexus.db file. Readers run with `PRAGMA query_only=ON` so they cannot
    /// accidentally block on the writer mutex. Migrate read-only handlers
    /// here to unblock parallel-read throughput.
    pub pool: Arc<ConnectionPool>,
    /// Root data directory (e.g. `~/.nexus`).
    pub data_dir: PathBuf,
    /// Server startup timestamp (for uptime calculation).
    pub started_at: std::time::Instant,
    /// OpenAI API key (read from `OPENAI_API_KEY` env var at startup).
    pub openai_api_key: String,
    /// Anthropic API key (optional, read from `ANTHROPIC_API_KEY`).
    pub anthropic_api_key: Option<String>,
    /// LLM model to use (default: [`crate::llm_model_defaults::OPENAI_DEFAULT_MODEL`]).
    pub model: String,
    /// Shared HTTP client — connection pooling across all LLM streaming calls.
    /// Long timeout (300 s) tuned for slow streaming model responses; do NOT
    /// use for short health probes / plugin webhooks — those go to
    /// [`http_client_short`] so a frozen plugin can't hang a request slot for
    /// 5 minutes.
    pub http_client: reqwest::Client,
    /// Short-timeout HTTP client for non-streaming calls — health probes,
    /// MCP/plugin webhooks, A2A peer discovery, federation pings, etc.
    pub http_client_short: reqwest::Client,
    /// Super Agents metrics bus — all subsystems push metrics here.
    pub metrics_bus: Arc<crate::super_agents::metrics_bus::MetricsBus>,
    /// Performance Orchestrator handle (populated after `start_super_agents`).
    pub orchestrator:
        tokio::sync::OnceCell<Arc<crate::super_agents::orchestrator::PerformanceOrchestrator>>,
    /// Legacy plugin registry — loaded from ~/.nexus/plugins/ at startup.
    pub legacy_plugin_registry: tokio::sync::RwLock<crate::plugin_system::LegacyPluginRegistry>,
    /// Live build event bus — per-project broadcast channels for SSE consumers.
    pub build_event_bus: crate::handlers::live_build_handler::BuildEventBus,
    /// LLM response cache — avoids duplicate calls for identical prompts.
    pub llm_cache: crate::cache::LlmCache,
    /// Rate limiter — concurrency slots + per-IP request limits.
    pub rate_limiter: crate::rate_limiter::RateLimiter,
    /// Per-tenant token-bucket rate limiter — enforces fair-share across
    /// generation/chat/agent/read endpoint categories. Independent of the
    /// IP-based limiter; applies even when many tenants share an egress IP.
    pub tenant_rate_limiter: Arc<crate::security::rate_limit::TenantRateLimiter>,
    /// Cost tracker — records all LLM calls with token counts and cost estimates.
    pub cost_tracker: crate::cost_intelligence::CostTracker,
    /// Predictive preprocessor — shared cache for "mind reading" while user types.
    pub predictor: crate::predictive_preprocess::PredictivePreprocessor,
    /// Plugin registry — unified, composable, observable plugin system.
    pub plugin_registry: tokio::sync::RwLock<crate::plugin_system::PluginRegistry>,
    /// LLM provider registry — built from the user's saved Settings keys per
    /// ADR-003 §5. Mutable: a settings reload calls `register_builtins` again
    /// to swap providers without dropping references; existing `Arc<dyn
    /// LlmProvider>` clones held elsewhere keep working.
    pub provider_registry: tokio::sync::RwLock<nexus_providers::ProviderRegistry>,
    /// MCP server registry — manages connections to external MCP servers.
    pub mcp_registry: tokio::sync::RwLock<nexus_mcp::registry::McpServerRegistry>,
    /// In-memory store for evaluation results.
    pub eval_results: tokio::sync::Mutex<Vec<nexus_eval::suite::SuiteResult>>,
    /// Webhook service — manages registered webhooks and fires events.
    pub webhook_service: tokio::sync::Mutex<crate::webhooks::WebhookService>,
    /// Kernel process scheduler — manages all agent process lifecycle.
    pub scheduler: Arc<AgentScheduler>,
    /// Kernel event bus — unified pub/sub for all system events.
    pub event_bus: Arc<EventBus>,
    /// Kernel reactive agent manager — trigger-based agent scheduling.
    pub reactive_manager: Arc<ReactiveAgentManager>,
    /// In-flight team orchestrators keyed by run_id — enables HITL message injection
    /// and live SSE subscription for the team execution UI. Keyed by run_id so
    /// concurrent runs of the same team can coexist.
    pub team_run_registry: tokio::sync::RwLock<
        std::collections::HashMap<String, Arc<crate::team_orchestrator::TeamOrchestrator>>,
    >,
    /// A2A protocol: Agent Card registry for local + remote agents.
    pub a2a_registry: AgentCardRegistry,
    /// A2A protocol: server that dispatches incoming tasks to local agents.
    pub a2a_server: Arc<A2aServer>,
    /// A2A federation manager — routes tasks to remote Nexus peer instances.
    pub federation: Arc<FederationManager>,
    /// Cryptographic audit log — tamper-evident Merkle chain of all agent actions.
    pub audit_log: Arc<AuditLog>,
    /// Ed25519 key pair for signing audit entries.
    pub audit_keypair: Arc<AuditKeypair>,
    /// Governance policy engine — enforces per-agent policies and kill switch.
    pub policy_engine: Arc<crate::governance::PolicyEngine>,
    /// App-as-Agent registry — bindings between generated apps and their business agents.
    pub app_agent_registry: Arc<crate::app_agent::AppAgentRegistry>,
    /// Sandbox ownership registry: `sandbox_id -> tenant_id`. Persisted only in
    /// memory because sandboxes are inherently ephemeral (destroyed on process
    /// restart). Without this map, any admin in any tenant could replay
    /// another tenant's sandbox ID, since the handlers reconstruct the
    /// instance handle from the URL path with no provenance check.
    pub sandbox_owners: tokio::sync::RwLock<std::collections::HashMap<String, String>>,
}

impl AppState {
    /// Acquire a read-only SQLite connection from the pool. Many readers run
    /// concurrently (default 4); each runs with `PRAGMA query_only=ON`. Use
    /// this for any handler that does not write.
    ///
    /// The returned guard implements `Deref<Target = Connection>` and releases
    /// the slot back to the pool when dropped.
    pub async fn acquire_reader(&self) -> ReaderGuard {
        self.pool.acquire_reader().await
    }

    /// Acquire the writer SQLite connection. Use only for handlers that
    /// actually write (INSERT/UPDATE/DELETE/DDL). Read-only handlers should
    /// call [`Self::acquire_reader`] instead — the writer mutex is the
    /// system-wide bottleneck.
    pub async fn acquire_writer(&self) -> OwnedMutexGuard<rusqlite::Connection> {
        self.pool.acquire_writer().await
    }
}

impl AppState {
    pub async fn init(data_dir: impl Into<PathBuf>) -> anyhow::Result<Arc<Self>> {
        let data_dir = data_dir.into();
        std::fs::create_dir_all(&data_dir)?;

        // Auto-provision NEXUS_JWT_SECRET + NEXUS_ENCRYPTION_KEY on first boot.
        // This keeps `docker compose up` (and `cargo run -p nexus-http`) a
        // zero-config experience — operators only need to set the env vars if
        // they want to override the stored secrets.
        ensure_auto_secrets(&data_dir);

        let db_path = data_dir.join("nexus.db");
        let conn = open_connection(&db_path)?;
        run_migrations(&conn)?;

        // Open a parallel-read pool on the same DB file. Migrations have
        // already been applied via `open_connection`, and `ConnectionPool::new`
        // re-runs them as a no-op via the `schema_migrations` version table.
        // WAL mode allows multiple connections to share the same nexus.db.
        let pool = Arc::new(
            ConnectionPool::new(&db_path, None)
                .map_err(|e| anyhow::anyhow!("failed to open connection pool: {e}"))?,
        );

        let openai_api_key = std::env::var("OPENAI_API_KEY")
            .ok()
            .filter(|k| !k.is_empty())
            .unwrap_or_default();
        let anthropic_api_key = std::env::var("ANTHROPIC_API_KEY")
            .ok()
            .filter(|k| !k.is_empty());

        if openai_api_key.is_empty() && anthropic_api_key.is_none() {
            tracing::warn!(
                "No LLM provider key set (OPENAI_API_KEY / ANTHROPIC_API_KEY). \
                 LLM-backed features will fail at call time. \
                 Set a key, or run a local provider (e.g. Ollama) and configure NEXUS_MODEL."
            );
        }
        let model = std::env::var("NEXUS_MODEL")
            .unwrap_or_else(|_| crate::llm_model_defaults::OPENAI_DEFAULT_MODEL.to_string());

        // Reset stale "running" agents — their processes died when the server stopped.
        {
            let stale_count = conn.execute(
                "UPDATE agent_definitions SET status = 'stopped', zeroclaw_pid = NULL, zeroclaw_port = NULL, updated_at = datetime('now')
                 WHERE status = 'running'",
                [],
            ).unwrap_or(0);
            if stale_count > 0 {
                info!(count = stale_count, "Reset stale running agents to stopped");
            }
        }

        // Seed built-in agent skills (idempotent — only seeds if none exist).
        {
            let skill_svc = nexus_store::AgentSkillService::new(&conn);
            if let Err(e) = skill_svc.seed_builtin_skills() {
                tracing::warn!(error = %e, "Failed to seed built-in skills");
            }
        }

        // Seed official marketplace items (idempotent).
        {
            let mp_svc = nexus_store::MarketplaceService::new(&conn);
            if let Err(e) = mp_svc.seed_official() {
                tracing::warn!(error = %e, "Failed to seed marketplace");
            }
        }

        // Seed starter templates (idempotent).
        {
            let tmpl_svc = nexus_store::TemplateService::new(&conn);
            if let Err(e) = tmpl_svc.seed_defaults() {
                tracing::warn!(error = %e, "Failed to seed starter templates");
            }
        }

        // Reap any orphaned dev-server / container children BEFORE resetting
        // their rows. Without this step, killing the nexus server leaves
        // `next dev` / `node` / `python -m http.server` processes running
        // forever — they keep their ports and eventually we run out.
        {
            let mut stmt = conn
                .prepare(
                    "SELECT id, pid FROM app_instances
                     WHERE status IN ('running', 'starting', 'installing')
                       AND pid IS NOT NULL",
                )
                .ok();
            if let Some(ref mut s) = stmt {
                let rows = s
                    .query_map([], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?))
                    })
                    .ok();
                if let Some(rows) = rows {
                    let mut killed = 0usize;
                    for r in rows.flatten() {
                        let (_id, pid_opt) = r;
                        if let Some(pid) = pid_opt {
                            // PID reuse is a real risk on busy hosts: between
                            // restart and reap, the OS could have recycled the
                            // PID for an unrelated process (an editor, sibling
                            // docker, etc.). Verify the PID still belongs to a
                            // known dev-server binary before signalling.
                            if !pid_looks_like_dev_server(pid) {
                                tracing::debug!(
                                    pid,
                                    "skipping reap: PID not a known dev-server binary (likely recycled)"
                                );
                                continue;
                            }
                            // Best-effort SIGTERM; process may already be gone.
                            #[cfg(unix)]
                            {
                                let rc = unsafe { libc::kill(pid as i32, libc::SIGTERM) };
                                if rc == 0 {
                                    killed += 1;
                                }
                            }
                            #[cfg(not(unix))]
                            {
                                // Windows: no direct signal; best-effort via taskkill.
                                let _ = std::process::Command::new("taskkill")
                                    .args(["/F", "/PID", &pid.to_string()])
                                    .output();
                                killed += 1;
                            }
                        }
                    }
                    if killed > 0 {
                        info!(
                            count = killed,
                            "Reaped orphan dev-server processes from previous run"
                        );
                    }
                }
            }
        }

        // Reset stale app instances — their dev servers / containers died with the server.
        {
            let stale_count = conn.execute(
                "UPDATE app_instances SET status = 'stopped', pid = NULL, stopped_at = datetime('now')
                 WHERE status IN ('running', 'starting', 'installing')",
                [],
            ).unwrap_or(0);
            if stale_count > 0 {
                info!(count = stale_count, "Reset stale app instances to stopped");
            }
        }

        // Release any generation locks left from a previous server run.
        {
            match nexus_store::GenerationLockService::cleanup_stale(&conn) {
                Ok(n) if n > 0 => info!(count = n, "Released stale generation locks"),
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "Failed to clean up generation locks"),
            }
        }

        // Load persisted cost budget overrides, if any. Operators set these
        // via POST /costs/budget and they survive restart.
        let cost_budget = {
            let svc = nexus_store::ProjectService::new(&conn);
            let mut b = crate::cost_intelligence::CostBudget::default();
            if let Ok(Some(v)) = svc.get_setting("budget.daily_project_limit") {
                if let Ok(parsed) = v.parse::<f64>() {
                    b.daily_project_limit = parsed;
                }
            }
            if let Ok(Some(v)) = svc.get_setting("budget.daily_global_limit") {
                if let Ok(parsed) = v.parse::<f64>() {
                    b.daily_global_limit = parsed;
                }
            }
            if let Ok(Some(v)) = svc.get_setting("budget.max_tokens_per_call") {
                if let Ok(parsed) = v.parse::<u64>() {
                    b.max_tokens_per_call = parsed;
                }
            }
            b
        };

        // Initialise security tables (api_keys, audit_log) — idempotent.
        if let Err(e) = crate::security::init_security_tables(&conn) {
            tracing::warn!(error = %e, "Failed to init security tables");
        }

        // Warn operators if the at-rest encryption key isn't set. We still
        // encrypt (via a dev-only fallback key) so the on-disk format is
        // stable across restarts, but anyone who copies nexus.db + the
        // binary can decrypt. Prod deployments MUST set the env var.
        if !nexus_store::crypto::is_production_key_configured() {
            tracing::warn!(
                "NEXUS_ENCRYPTION_KEY is not set. Vault values and LLM API keys \
                 are encrypted with a deterministic dev-only key. \
                 Set NEXUS_ENCRYPTION_KEY to a 32+ character random string \
                 before deploying to production."
            );
        }

        // Load legacy plugin registry from ~/.nexus/plugins/
        let legacy_plugin_registry = crate::plugin_system::LegacyPluginRegistry::load(&data_dir);
        let plugin_summary = legacy_plugin_registry.summary();

        info!(
            data_dir = %data_dir.display(),
            model = %model,
            plugins = plugin_summary.total_plugins,
            enabled_plugins = plugin_summary.enabled_plugins,
            "Nexus HTTP server state initialised"
        );

        // Shared HTTP client with connection pooling and sensible timeouts.
        // A `.build()` failure here is extremely rare (e.g. TLS init error,
        // file-descriptor exhaustion) but if it happens we must surface it
        // with a log line and a propagated error — an `.expect()` panic at
        // startup produces a crash loop with no diagnostic trail.
        let http_client = reqwest::Client::builder()
            .pool_max_idle_per_host(10)
            .timeout(std::time::Duration::from_secs(300))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| {
                tracing::error!(error = %e, "failed to build shared HTTP client");
                anyhow::anyhow!("failed to build shared HTTP client: {e}")
            })?;

        // Short-timeout sibling for non-streaming use-cases (plugin/MCP/A2A
        // probes). 15s is well above any healthy round trip and well below
        // the threshold at which a hung peer becomes a denial-of-service
        // against our request slots.
        let http_client_short = reqwest::Client::builder()
            .pool_max_idle_per_host(10)
            .timeout(std::time::Duration::from_secs(15))
            .connect_timeout(std::time::Duration::from_secs(5))
            .build()
            .map_err(|e| {
                tracing::error!(error = %e, "failed to build short-timeout HTTP client");
                anyhow::anyhow!("failed to build short-timeout HTTP client: {e}")
            })?;

        let metrics_bus = crate::super_agents::metrics_bus::MetricsBus::new(2000);

        // Kernel subsystems
        let scheduler = Arc::new(AgentScheduler::new(SchedulerConfig::default()));
        let event_bus = Arc::new(EventBus::new(EventBusConfig::default()));
        let reactive_manager = Arc::new(ReactiveAgentManager::new(
            scheduler.clone(),
            event_bus.clone(),
        ));

        // Start kernel background tasks
        reactive_manager.start_process_event_listener();
        info!("Kernel subsystems initialised (scheduler + event bus + reactive manager)");

        // Cryptographic audit log — both signing key and the chain itself
        // persist under data_dir so a restart cannot orphan the prior chain
        // or silently regenerate the keypair (which would invalidate every
        // earlier signature).
        let audit_keypair = Arc::new(
            AuditKeypair::load_or_create(&data_dir.join("audit.key"))
                .await
                .map_err(|e| anyhow::anyhow!("audit keypair init failed: {e}"))?,
        );
        let audit_log = Arc::new(
            AuditLog::open(&data_dir.join("audit_chain.jsonl"))
                .await
                .map_err(|e| anyhow::anyhow!("audit log open failed: {e}"))?,
        );
        info!("Cryptographic audit log initialised (Ed25519 + Merkle chain, persisted)");

        // Governance policy engine
        let policy_engine = Arc::new(crate::governance::PolicyEngine::new());
        info!("Governance policy engine initialised");

        // A2A protocol
        let a2a_registry = AgentCardRegistry::new();
        let a2a_handler = Arc::new(NexusTaskHandler::new(
            openai_api_key.clone(),
            std::env::var("NEXUS_MODEL")
                .unwrap_or_else(|_| crate::llm_model_defaults::OPENAI_DEFAULT_MODEL.to_string()),
        ));
        let a2a_server = Arc::new(A2aServer::new(a2a_handler));
        // Populate local agent card after construction (via set_local in serve())
        info!("A2A server initialised");

        // Federation manager — routes tasks across Nexus peer instances
        let federation = Arc::new(FederationManager::new(a2a_registry.clone()));
        info!("A2A federation manager initialised");

        // ADR-003 §5: build the LLM provider registry. Per project memory
        // operators configure keys via the Settings UI (stored in the
        // `settings` table), so on boot we hydrate from there first and
        // fall back to env. This way a server restart picks up keys saved
        // through the UI without the operator re-typing them.
        let provider_registry = {
            let mut reg = nexus_providers::ProviderRegistry::new();
            let svc = nexus_store::ProjectService::new(&conn);
            let read = |provider: &str| -> Option<String> {
                let key = format!("llm.{provider}.api_key");
                match svc.get_setting(&key) {
                    Ok(Some(v)) if !v.is_empty() => Some(v),
                    _ => None,
                }
            };
            let openai_from_settings = read("openai");
            let anthropic_from_settings = read("anthropic");
            let cfg = nexus_providers::BuiltinProviderConfig {
                openai_api_key: openai_from_settings.or_else(|| {
                    if openai_api_key.is_empty() || openai_api_key == "sk-placeholder" {
                        None
                    } else {
                        Some(openai_api_key.clone())
                    }
                }),
                openai_base_url: None,
                anthropic_api_key: anthropic_from_settings.or_else(|| anthropic_api_key.clone()),
                ollama_base_url: None,
            };
            nexus_providers::register_builtins(&mut reg, &cfg);
            info!(
                providers = reg.len(),
                "LLM provider registry initialised (settings + env)"
            );
            reg
        };

        let state = Arc::new(Self {
            db: Arc::new(tokio::sync::Mutex::new(conn)),
            pool,
            data_dir,
            started_at: std::time::Instant::now(),
            openai_api_key,
            anthropic_api_key,
            model,
            http_client,
            http_client_short,
            metrics_bus,
            orchestrator: tokio::sync::OnceCell::new(),
            legacy_plugin_registry: tokio::sync::RwLock::new(legacy_plugin_registry),
            build_event_bus: crate::handlers::live_build_handler::create_event_bus(),
            // Cache LLM responses for 10 minutes (avoids re-running identical prompts)
            llm_cache: crate::cache::LlmCache::new(600),
            rate_limiter: crate::rate_limiter::RateLimiter::new(),
            tenant_rate_limiter: Arc::new(
                crate::security::rate_limit::TenantRateLimiter::with_defaults(),
            ),
            cost_tracker: crate::cost_intelligence::CostTracker::new(cost_budget),
            predictor: crate::predictive_preprocess::PredictivePreprocessor::new(),
            plugin_registry: tokio::sync::RwLock::new(crate::plugin_system::PluginRegistry::new()),
            provider_registry: tokio::sync::RwLock::new(provider_registry),
            mcp_registry: tokio::sync::RwLock::new(nexus_mcp::registry::McpServerRegistry::new()),
            eval_results: tokio::sync::Mutex::new(Vec::new()),
            webhook_service: tokio::sync::Mutex::new(crate::webhooks::WebhookService::new()),
            scheduler,
            event_bus,
            reactive_manager,
            team_run_registry: tokio::sync::RwLock::new(std::collections::HashMap::new()),
            a2a_registry,
            a2a_server,
            federation,
            audit_log,
            audit_keypair,
            policy_engine,
            app_agent_registry: Arc::new(crate::app_agent::AppAgentRegistry::new()),
            sandbox_owners: tokio::sync::RwLock::new(std::collections::HashMap::new()),
        });

        // ADR-002 §6: any team_run row marked `running` whose last event is
        // older than 60s is rewritten to `paused` with reason `server_restart`.
        // Cleared in the background so a SQLite hiccup at boot doesn't block.
        {
            let db_for_reconcile = state.db.clone();
            tokio::spawn(async move {
                crate::team_event_sink::reconcile_orphans_on_boot(
                    &db_for_reconcile,
                    /* stale_after_ms */ 60_000,
                )
                .await;
            });
        }

        Ok(state)
    }

    // -----------------------------------------------------------------------
    // Memory system
    // -----------------------------------------------------------------------

    /// Look up an LLM provider by id from the registry. Returns `None` if no
    /// provider with that id is registered (e.g. operator hasn't supplied a
    /// key for it yet via the Settings UI).
    ///
    /// Per ADR-003 §5: this is the canonical way for handlers to obtain an
    /// LLM provider. Direct `reqwest::post(api.openai.com/...)` is forbidden
    /// (clippy.toml lint will flag it once the bypass call sites are gone).
    pub async fn llm_provider(
        &self,
        id: &str,
    ) -> Option<std::sync::Arc<dyn nexus_providers::LlmProvider>> {
        self.provider_registry.read().await.get(id)
    }

    /// Re-register the built-in providers from updated Settings. Called when
    /// the operator changes their saved API keys via the Settings UI so
    /// providers swap in place without a server restart.
    pub async fn reload_provider_registry(&self, cfg: &nexus_providers::BuiltinProviderConfig) {
        let mut reg = self.provider_registry.write().await;
        // Keep a fresh registry so old (revoked) keys don't linger.
        *reg = nexus_providers::ProviderRegistry::new();
        nexus_providers::register_builtins(&mut reg, cfg);
        tracing::info!(
            providers = reg.len(),
            "LLM provider registry reloaded from settings"
        );
    }

    /// Get the persistent memory system (episodic + semantic).
    ///
    /// Creates a new `MemorySystem` instance each time, pointing at the
    /// `memory/` subdirectory of `data_dir`. The SQLite databases are
    /// created/opened on demand; table init is idempotent.
    pub fn memory_system(&self) -> Result<nexus_memory::MemorySystem, crate::error::ApiError> {
        let mem_dir = self.data_dir.join("memory");
        std::fs::create_dir_all(&mem_dir).map_err(|e| {
            crate::error::ApiError::Internal(format!("Failed to create memory dir: {e}"))
        })?;
        nexus_memory::MemorySystem::new(&mem_dir)
            .map_err(|e| crate::error::ApiError::Internal(e.to_string()))
    }

    // -----------------------------------------------------------------------
    // Path helpers (mirror NexusState path helpers)
    // -----------------------------------------------------------------------

    pub fn project_data_db(&self, project_id: &str) -> PathBuf {
        self.data_dir
            .join("projects")
            .join(project_id)
            .join("data.db")
    }

    pub fn project_agents_dir(&self, project_id: &str) -> PathBuf {
        self.data_dir
            .join("projects")
            .join(project_id)
            .join("agents")
    }

    pub fn project_portal_dir(&self, project_id: &str) -> PathBuf {
        self.data_dir
            .join("projects")
            .join(project_id)
            .join("portal")
    }

    /// Tenant-scoped project directory.
    ///
    /// Prefer this over direct `data_dir.join("projects").join(id)` in new
    /// code — the legacy layout is kept for backwards-compat with existing
    /// deployments (project IDs are UUIDv4 so collisions are statistically
    /// impossible), but tenant-scoped paths are the correct long-term layout.
    ///
    /// Returns the legacy path when the project's tenant is `"default"` OR
    /// when the tenant is unresolvable, so running deployments keep working.
    /// Non-default tenants get the tenant-scoped path:
    /// `<data_dir>/tenants/<tenant_id>/projects/<project_id>`.
    pub async fn resolve_project_dir(&self, project_id: &str) -> PathBuf {
        let tenant_id: Option<String> = {
            let db = self.db.lock().await;
            db.prepare("SELECT tenant_id FROM projects WHERE id = ?1")
                .ok()
                .and_then(|mut stmt| {
                    stmt.query_row([project_id], |row| row.get::<_, Option<String>>(0))
                        .ok()
                        .flatten()
                })
        };
        match tenant_id.as_deref() {
            Some(t) if t != "default" => {
                crate::security::tenant::tenant_project_dir(&self.data_dir, t, project_id)
            }
            _ => self.data_dir.join("projects").join(project_id),
        }
    }
}

// ---------------------------------------------------------------------------
// PID reuse safety
// ---------------------------------------------------------------------------

/// Best-effort check that `pid` still points at a process whose binary name
/// looks like a dev-server we spawned (`node`, `next`, `python`, `bun`, `npm`,
/// `pnpm`, `yarn`, `deno`). If the PID has been recycled into something else,
/// we skip the kill — better to leave one orphan than to SIGTERM the user's
/// editor.
#[allow(unused_variables)]
fn pid_looks_like_dev_server(pid: i64) -> bool {
    const KNOWN: &[&str] = &[
        "node",
        "next",
        "next-server",
        "next-router-worker",
        "python",
        "python3",
        "uvicorn",
        "bun",
        "deno",
        "npm",
        "pnpm",
        "yarn",
    ];
    #[cfg(target_os = "linux")]
    {
        let path = format!("/proc/{pid}/comm");
        match std::fs::read_to_string(&path) {
            Ok(name) => {
                let trimmed = name.trim();
                KNOWN.iter().any(|k| trimmed == *k)
            }
            Err(_) => {
                // /proc entry missing = process is already gone, safe to skip.
                false
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        // `ps -o comm= -p <pid>` is the simplest portable lookup on macOS.
        let out = std::process::Command::new("ps")
            .args(["-o", "comm=", "-p", &pid.to_string()])
            .output();
        match out {
            Ok(o) if o.status.success() => {
                let s = String::from_utf8_lossy(&o.stdout);
                let last = s.trim().rsplit('/').next().unwrap_or("").trim();
                KNOWN.iter().any(|k| last.starts_with(k))
            }
            _ => false,
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        // On Windows / other Unixes we don't have a cheap, dependency-free
        // way to read the process image name. Default to allowing the kill;
        // the previous behaviour was to always kill.
        true
    }
}

// ---------------------------------------------------------------------------
// Auto-secrets: persist generated JWT + encryption secrets on first boot.
// ---------------------------------------------------------------------------

/// Marker substrings we treat as "the user did not set a real secret".
/// Matches the placeholders historically baked into docker-compose.yml.
const INSECURE_MARKERS: &[&str] = &[
    "dev-only-change-me",
    "nexus-dev-secret-do-not-use-in-prod",
    "change-me",
];

/// Ensure `NEXUS_JWT_SECRET` and `NEXUS_ENCRYPTION_KEY` are both set to a
/// strong random value. Behaviour:
///
/// - If the env var is set to a non-empty, non-insecure value, leave it.
/// - Otherwise, try to read the value from `<data_dir>/secrets.toml`.
/// - If that's missing or the field is absent, generate a fresh 32-byte
///   random string, persist it to disk, and set the env var for the rest
///   of the process lifetime.
///
/// The persisted file is `chmod 600` on Unix. On all platforms we log the
/// path so operators know where the secret lives and how to override it.
///
/// Never fails — secret generation is best-effort. If writing the file
/// fails (read-only FS, etc.) we still set the env var for this run so
/// the server boots; a warning is logged.
pub(crate) fn ensure_auto_secrets(data_dir: &std::path::Path) {
    let secrets_path = data_dir.join("secrets.toml");

    // Read any existing persisted secrets up front — they're our fallback
    // whenever an env var is missing or contains an insecure placeholder.
    let stored = load_stored_secrets(&secrets_path);

    let mut out = PersistedSecrets {
        jwt_secret: stored.jwt_secret.clone(),
        encryption_key: stored.encryption_key.clone(),
        encryption_salt: stored.encryption_salt.clone(),
    };
    let mut generated_any = false;

    for (env_name, slot) in [
        ("NEXUS_JWT_SECRET", &mut out.jwt_secret),
        ("NEXUS_ENCRYPTION_KEY", &mut out.encryption_key),
        ("NEXUS_ENCRYPTION_SALT", &mut out.encryption_salt),
    ] {
        let env_val = std::env::var(env_name).ok().filter(|v| !v.is_empty());
        let env_is_insecure = env_val.as_deref().map(is_insecure_value).unwrap_or(true);

        if let Some(v) = env_val.as_ref() {
            if !env_is_insecure {
                // Honour operator-provided env var — don't persist it.
                *slot = Some(v.clone());
                continue;
            }
        }

        // Env var missing or insecure — prefer the persisted secret.
        if let Some(existing) = slot.as_ref() {
            if !is_insecure_value(existing) {
                std::env::set_var(env_name, existing);
                continue;
            }
        }

        // Generate fresh — 32 random bytes, hex-encoded → 64-char string.
        let fresh = generate_secret();
        std::env::set_var(env_name, &fresh);
        *slot = Some(fresh);
        generated_any = true;
    }

    if generated_any {
        match save_secrets(&secrets_path, &out) {
            Ok(()) => {
                tracing::info!(
                    path = %secrets_path.display(),
                    "Generated secrets at {}; set NEXUS_JWT_SECRET/NEXUS_ENCRYPTION_KEY to override.",
                    secrets_path.display()
                );
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    path = %secrets_path.display(),
                    "Generated in-memory secrets but failed to persist to {} — they will be regenerated on next boot. Set NEXUS_JWT_SECRET/NEXUS_ENCRYPTION_KEY to override.",
                    secrets_path.display()
                );

                // Per ADR-001, gate the strict refusal on NEXUS_PRODUCTION=1
                // rather than build profile so the same release binary can
                // run dev (auto-generate ephemeral) and prod (refuse to boot
                // without a stable key).
                let is_production = matches!(
                    std::env::var("NEXUS_PRODUCTION").ok().as_deref(),
                    Some("1") | Some("true") | Some("TRUE") | Some("True")
                );
                if is_production && !nexus_store::crypto::is_production_key_configured() {
                    panic!(
                        "NEXUS_ENCRYPTION_KEY is not set and secrets.toml at {} could \
                         not be written ({}). Refusing to boot with an ephemeral or dev \
                         fallback encryption key when NEXUS_PRODUCTION=1.",
                        secrets_path.display(),
                        e
                    );
                }
            }
        }
    }
}

#[derive(Default, Clone)]
struct PersistedSecrets {
    jwt_secret: Option<String>,
    encryption_key: Option<String>,
    /// Per-install hex-encoded salt for Argon2id (crypto v3). Distinct
    /// installations must use distinct salts so a single rainbow table can't
    /// be shared across deployments. 32 bytes hex => 64 chars.
    encryption_salt: Option<String>,
}

fn is_insecure_value(v: &str) -> bool {
    let lower = v.to_ascii_lowercase();
    INSECURE_MARKERS.iter().any(|m| lower.contains(m))
}

fn generate_secret() -> String {
    use rand::RngCore;
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    hex::encode(buf)
}

fn load_stored_secrets(path: &std::path::Path) -> PersistedSecrets {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return PersistedSecrets::default();
    };
    // Tolerant parse: a malformed file should not crash the server.
    let parsed: Result<toml::Value, _> = raw.parse();
    let Ok(val) = parsed else {
        tracing::warn!(path = %path.display(), "secrets.toml is not valid TOML; ignoring");
        return PersistedSecrets::default();
    };
    let pick = |k: &str| -> Option<String> {
        val.get(k)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
    };
    PersistedSecrets {
        jwt_secret: pick("jwt_secret"),
        encryption_key: pick("encryption_key"),
        encryption_salt: pick("encryption_salt"),
    }
}

fn save_secrets(path: &std::path::Path, s: &PersistedSecrets) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = format!(
        "# Auto-generated by nexus-http on first boot.\n\
         # Override by setting NEXUS_JWT_SECRET / NEXUS_ENCRYPTION_KEY /\n\
         # NEXUS_ENCRYPTION_SALT env vars.\n\
         # Delete this file to regenerate fresh secrets on next boot.\n\
         # NOTE: rotating encryption_salt invalidates every existing enc:v3\n\
         # ciphertext. Only rotate during a planned migration.\n\
         jwt_secret = \"{}\"\n\
         encryption_key = \"{}\"\n\
         encryption_salt = \"{}\"\n",
        s.jwt_secret.as_deref().unwrap_or(""),
        s.encryption_key.as_deref().unwrap_or(""),
        s.encryption_salt.as_deref().unwrap_or(""),
    );

    // Atomically create the file at mode 0600 BEFORE writing any bytes.
    // Previously we wrote first and chmod'd second, leaving a window where the
    // file existed at the umask default (often 0644) — readable by other users
    // on the host until the chmod landed.
    use std::io::Write as _;
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        f.write_all(body.as_bytes())?;
        // Re-apply mode after open in case the file already existed with
        // looser permissions (OpenOptions::mode only takes effect on create).
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        f.write_all(body.as_bytes())?;
    }

    Ok(())
}
