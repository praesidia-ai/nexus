//! Pipeline Engine -- internal generation engine composable by orchestration flows.
//!
//! Extracted from `execution_pipeline.rs`. This is **not** an HTTP handler.
//! It provides structured, step-by-step generation that the oneshot handler
//! (or any future orchestration path) can compose as needed.
//!
//! Each public method corresponds to a logical phase of app generation and
//! emits [`GenerationEvent`] variants through a channel so callers can
//! stream real-time progress to the frontend.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use serde_json::{json, Value};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::execution_core::{ExecutionCore, Operation, Proposal};
use crate::generation_event::GenerationEvent;
use crate::intent_engine::FlatIntent;
use crate::product_engine::ProductBrief;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Pipeline Result
// ---------------------------------------------------------------------------

/// Summary returned after a full pipeline execution.
#[derive(Debug, Clone)]
pub struct PipelineResult {
    /// The unique project identifier.
    pub project_id: String,
    /// Number of files written to disk.
    pub files_written: usize,
    /// Number of AI agents created in the generated app.
    pub agents_created: usize,
    /// Total wall-clock duration in milliseconds.
    pub duration_ms: u64,
    /// The generated application spec (if available).
    pub spec: Option<Value>,
}

// ---------------------------------------------------------------------------
// Pipeline Engine
// ---------------------------------------------------------------------------

/// The internal generation engine. Composes spec generation, file generation,
/// validation, and installation into reusable steps.
///
/// Each method emits [`GenerationEvent`] variants through the `tx` channel
/// so that callers (e.g. the oneshot handler) can stream progress to the
/// frontend via SSE.
pub struct PipelineEngine {
    app: Arc<AppState>,
    tx: mpsc::Sender<GenerationEvent>,
    project_id: String,
    project_dir: PathBuf,
    intent: FlatIntent,
    product_brief: ProductBrief,
    description: String,
    proposed_files: Vec<(String, String)>,
    /// After `generate_config`, scaffold manifests must not be replaced by LLM file blocks.
    lock_scaffold_paths: bool,
    spec: Option<Value>,
}

impl PipelineEngine {
    /// Create a new pipeline engine instance.
    ///
    /// The `product_brief` is provided externally so callers can inject
    /// learning overrides or customised briefs.
    pub fn new(
        app: Arc<AppState>,
        tx: mpsc::Sender<GenerationEvent>,
        project_id: String,
        project_dir: PathBuf,
        intent: FlatIntent,
        product_brief: ProductBrief,
        description: &str,
    ) -> Self {
        Self {
            app,
            tx,
            project_id,
            project_dir,
            intent,
            product_brief,
            description: description.to_string(),
            proposed_files: Vec::new(),
            lock_scaffold_paths: false,
            spec: None,
        }
    }

    // -----------------------------------------------------------------------
    // Public step methods
    // -----------------------------------------------------------------------

    /// Emit skeleton files for instant preview (< 2ms).
    ///
    /// These lightweight placeholders give the frontend something to render
    /// while the LLM is processing the real content.
    pub async fn emit_skeletons(&self) {
        let app_type = format!("{:?}", self.intent.app_type);
        let pages: Vec<String> = self
            .intent
            .suggested_pages
            .iter()
            .map(|p| {
                let route = p.to_lowercase().replace(' ', "-");
                if route == "home" {
                    "/".to_string()
                } else {
                    format!("/{}", route)
                }
            })
            .collect();
        let skeletons = crate::perceived_speed::generate_skeletons(&app_type, &pages);
        for skeleton in &skeletons {
            let _ = self
                .tx
                .send(GenerationEvent::Skeleton {
                    path: skeleton.file_path.clone(),
                    content: skeleton.content.clone(),
                    skeleton_type: format!("{:?}", skeleton.skeleton_type),
                })
                .await;
            let _ = self
                .tx
                .send(GenerationEvent::FileProposed {
                    path: skeleton.file_path.clone(),
                    size: skeleton.content.len(),
                    skeleton: true,
                })
                .await;
        }
    }

    /// Generate the application specification via LLM.
    ///
    /// Populates `self.spec` on success. The spec is a JSON object describing
    /// pages, entities, agents, and style for the target application.
    pub async fn generate_spec(&mut self) -> Result<(), String> {
        let phase = "generate_spec";
        let start = Instant::now();
        self.emit_phase_started(phase, "Generating application specification").await;

        let brief_context = crate::product_engine::format_brief_for_prompt(&self.product_brief);

        let prompt = format!(
            r##"Generate a JSON specification for this application.

Description: {}

Intent analysis:
- App type: {:?}
- Auth: {}
- Database: {}
- UI style: {:?}
- Suggested pages: {:?}
- Suggested entities: {:?}
- Suggested agents: {}

PRODUCT BRIEF (use this real content in the generated app — do NOT use lorem ipsum):
{}

RULES:
- Use the exact hero headline, subheadline, and CTA from the product brief
- Use the seed content items (names, descriptions, prices) as real data
- Include the trust signals in the footer or below the hero
- Ensure navigation matches the nav items listed
- Every page must have meaningful content, not placeholder text

Respond with ONLY JSON:
{{
  "name": "project name",
  "summary": "one paragraph",
  "pages": [{{"name": "...", "route": "/...", "description": "...", "components": ["..."]}}],
  "entities": [{{"name": "...", "fields": [{{"name": "...", "type": "TEXT|INTEGER|REAL", "required": true}}]}}],
  "agents": [{{"name": "...", "role": "...", "type": "chatbot|recommendation|support|analytics"}}],
  "style": {{"theme": "dark|light", "primary_color": "#hex", "font": "inter|playfair|mono"}}
}}"##,
            self.description,
            self.intent.app_type,
            self.intent.needs_auth,
            self.intent.needs_database,
            self.intent.ui_style,
            self.intent.suggested_pages,
            self.intent.suggested_entities,
            self.intent
                .suggested_agents
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            brief_context,
        );

        let response = crate::pipeline_turbo::call_with_retry(
            &self.app,
            &self.project_id,
            &prompt,
            2,
        )
        .await
        .map_err(|e| format!("Spec generation failed after retries: {}", e))?;

        let spec = parse_json_from_response(&response);
        if spec
            .get("pages")
            .and_then(|p| p.as_array())
            .is_none_or(|a| a.is_empty())
        {
            warn!("LLM spec has no pages — JSON parse likely fell back to empty spec.");
        }

        let page_count = spec
            .get("pages")
            .and_then(|p| p.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        let entity_count = spec
            .get("entities")
            .and_then(|e| e.as_array())
            .map(|a| a.len())
            .unwrap_or(0);

        self.spec = Some(spec);

        let _ = self
            .tx
            .send(GenerationEvent::SpecGenerated {
                page_count,
                entity_count,
            })
            .await;

        self.emit_phase_completed(phase, start).await;
        Ok(())
    }

    /// Validate the generated spec (static + policy layers).
    ///
    /// Must be called after [`generate_spec`].
    pub async fn validate_spec(&mut self) -> Result<(), String> {
        let phase = "validate_spec";
        let start = Instant::now();
        self.emit_phase_started(phase, "Validating specification").await;

        let spec = self.spec.as_ref().ok_or("No spec generated")?;

        // Layer 1: Static validation
        let mut issues = Vec::new();
        if spec
            .get("pages")
            .and_then(|p| p.as_array())
            .map(|a| a.is_empty())
            .unwrap_or(true)
        {
            issues.push("Spec has no pages defined".to_string());
        }
        if spec
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .is_empty()
        {
            issues.push("Spec has no project name".to_string());
        }

        let _ = self
            .tx
            .send(GenerationEvent::ValidationResult {
                layer: "static".into(),
                passed: issues.is_empty(),
                issues: issues.clone(),
            })
            .await;

        // Layer 2: Policy validation
        let mut policy_issues = Vec::new();
        if self.intent.needs_auth
            && spec
                .get("pages")
                .and_then(|p| p.as_array())
                .map(|pages| {
                    !pages.iter().any(|p| {
                        let route = p.get("route").and_then(|r| r.as_str()).unwrap_or("");
                        route.contains("login") || route.contains("auth")
                    })
                })
                .unwrap_or(false)
        {
            policy_issues.push("Auth required but no login page in spec".to_string());
        }

        let _ = self
            .tx
            .send(GenerationEvent::ValidationResult {
                layer: "policy".into(),
                passed: policy_issues.is_empty(),
                issues: policy_issues,
            })
            .await;

        if !issues.is_empty() {
            return Err(format!("Spec validation failed: {}", issues.join(", ")));
        }

        self.emit_phase_completed(phase, start).await;
        Ok(())
    }

    /// Generate all files (schema, config, API, UI, agents, auth) with
    /// parallel API+UI generation where possible.
    ///
    /// Returns the number of files proposed.
    pub async fn generate_files(&mut self) -> Result<usize, String> {
        let phase = "generate_files";
        let start = Instant::now();
        self.emit_phase_started(phase, "Generating application files").await;

        // Schema (deterministic, only if DB needed)
        if self.intent.needs_database {
            self.emit_phase_progress(phase, 0.1, "Generating database schema").await;
            self.step_generate_schema().await?;
        }

        // Config (deterministic)
        self.emit_phase_progress(phase, 0.2, "Generating project configuration").await;
        self.step_generate_config().await?;

        // API + UI in parallel
        self.emit_phase_progress(phase, 0.3, "Generating API routes and UI pages (parallel)").await;
        self.step_generate_api_and_ui_parallel().await?;

        // Agents (if needed)
        if !self.intent.suggested_agents.is_empty() {
            self.emit_phase_progress(phase, 0.8, "Embedding AI agents").await;
            self.step_generate_agents().await?;
        }

        // Auth (if needed)
        if self.intent.needs_auth {
            self.emit_phase_progress(phase, 0.9, "Adding authentication").await;
            self.step_generate_auth().await?;
        }

        self.emit_phase_progress(phase, 1.0, "All files generated").await;
        self.emit_phase_completed(phase, start).await;

        Ok(self.proposed_files.len())
    }

    /// Validate all proposed files and commit them to disk.
    ///
    /// Returns the number of files written.
    pub async fn validate_and_commit(&mut self) -> Result<usize, String> {
        let phase = "validate_and_commit";
        let start = Instant::now();
        self.emit_phase_started(phase, "Validating and committing files").await;

        // Validation
        self.step_validate_all().await?;

        // Commit
        self.step_commit_files().await?;

        let count = self.proposed_files.len();
        self.emit_phase_completed(phase, start).await;
        Ok(count)
    }

    /// Install dependencies and start the dev server.
    ///
    /// Returns the port the app is running on, or `None` if it could not start.
    pub async fn install_and_start(&mut self) -> Result<Option<u16>, String> {
        let phase = "install_and_start";
        let start = Instant::now();
        self.emit_phase_started(phase, "Installing dependencies and starting app").await;

        // npm install
        self.emit_phase_progress(phase, 0.3, "Running npm install").await;
        let dir = self.project_dir.clone();
        let install_result = tokio::task::spawn_blocking(move || {
            nexus_store::app_runner::npm_install(&dir)
        })
        .await
        .map_err(|e| format!("Install task panicked: {}", e))?;

        if let Err(e) = install_result {
            return Err(format!("npm install failed: {}", e));
        }

        // Start dev server
        self.emit_phase_progress(phase, 0.7, "Starting dev server").await;
        let port = {
            let db = self.app.db.lock().await;
            let svc = nexus_store::AppRunnerService::new(&db);
            let port = svc.next_available_port().unwrap_or(4100);
            let _ = svc.create_instance(
                &self.project_id,
                port,
                self.project_dir.to_str().unwrap_or_default(),
                false,
            );
            port
        };

        let dir = self.project_dir.clone();
        let pid_result = tokio::task::spawn_blocking(move || {
            nexus_store::app_runner::spawn_dev_server(&dir, port)
        })
        .await
        .map_err(|e| format!("Dev server task panicked: {}", e))?;

        let result_port = match pid_result {
            Ok(pid) => {
                {
                    let db = self.app.db.lock().await;
                    let svc = nexus_store::AppRunnerService::new(&db);
                    let instances = svc
                        .list_instances(&self.project_id)
                        .unwrap_or_default();
                    if let Some(inst) = instances.first() {
                        let _ = svc.update_status(&inst.id, "running", Some(pid), None);
                    }
                }
                info!(port = port, pid = pid, "App started");

                // Health check
                let healthy = crate::pipeline_turbo::wait_for_health(port.into(), 30).await;
                if healthy {
                    self.emit_phase_progress(phase, 1.0, &format!("App running at http://localhost:{}", port)).await;
                } else {
                    let _ = self
                        .tx
                        .send(GenerationEvent::Error {
                            phase: "health_check".into(),
                            message: "App started but health check timed out — it may still be compiling".into(),
                            recoverable: true,
                        })
                        .await;
                }
                Some(port)
            }
            Err(e) => {
                warn!(error = %e, "Dev server failed to start — app is on disk but not running");
                let _ = self
                    .tx
                    .send(GenerationEvent::Error {
                        phase: "install_and_start".into(),
                        message: format!(
                            "Dev server failed to start: {}. App files are ready — run `npm run dev` manually.",
                            e
                        ),
                        recoverable: true,
                    })
                    .await;
                None
            }
        };

        self.emit_phase_completed(phase, start).await;
        Ok(result_port)
    }

    /// Get the generated spec (available after [`generate_spec`]).
    pub fn spec(&self) -> Option<&Value> {
        self.spec.as_ref()
    }

    /// Get the number of proposed files.
    pub fn proposed_file_count(&self) -> usize {
        self.proposed_files.len()
    }

    /// Get the intent used by this engine.
    pub fn intent(&self) -> &FlatIntent {
        &self.intent
    }

    /// Get the product brief used by this engine.
    pub fn product_brief(&self) -> &ProductBrief {
        &self.product_brief
    }

    /// Get the project ID.
    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    // -----------------------------------------------------------------------
    // Internal step implementations (ported from execution_pipeline.rs)
    // -----------------------------------------------------------------------

    /// Generate database schema from spec entities (deterministic).
    async fn step_generate_schema(&mut self) -> Result<(), String> {
        let spec = self.spec.as_ref().ok_or("No spec")?;
        let entities = spec
            .get("entities")
            .and_then(|e| e.as_array())
            .cloned()
            .unwrap_or_default();

        if entities.is_empty() {
            return Ok(());
        }

        let mut schema_sql = String::from("-- Auto-generated schema\n\n");
        for entity in &entities {
            let name = entity
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("unknown");
            let fields = entity
                .get("fields")
                .and_then(|f| f.as_array())
                .cloned()
                .unwrap_or_default();

            schema_sql.push_str(&format!("CREATE TABLE IF NOT EXISTS {} (\n", name));
            schema_sql.push_str("  id TEXT PRIMARY KEY NOT NULL,\n");
            for field in &fields {
                let fname = field
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("col");
                if fname == "id" {
                    continue;
                }
                let ftype = field
                    .get("type")
                    .and_then(|t| t.as_str())
                    .unwrap_or("TEXT");
                let required = field
                    .get("required")
                    .and_then(|r| r.as_bool())
                    .unwrap_or(false);
                schema_sql.push_str(&format!(
                    "  {} {}{},\n",
                    fname,
                    ftype,
                    if required { " NOT NULL" } else { "" }
                ));
            }
            schema_sql.push_str("  created_at TEXT NOT NULL DEFAULT (datetime('now'))\n");
            schema_sql.push_str(");\n\n");
        }

        self.propose_file("src/db/schema.sql", &schema_sql, true).await;

        let db_helper = r#"import Database from 'better-sqlite3';
import path from 'path';
import fs from 'fs';

const DB_PATH = path.join(process.cwd(), 'data.db');
const SCHEMA_PATH = path.join(process.cwd(), 'src', 'db', 'schema.sql');

let db: Database.Database | null = null;

export function getDb(): Database.Database {
  if (!db) {
    db = new Database(DB_PATH);
    db.pragma('journal_mode = WAL');
    if (fs.existsSync(SCHEMA_PATH)) {
      const schema = fs.readFileSync(SCHEMA_PATH, 'utf-8');
      db.exec(schema);
    }
  }
  return db;
}
"#;
        self.propose_file("src/db/index.ts", db_helper, true).await;

        let seed_sql = crate::pipeline_turbo::generate_seed_sql(
            &self.product_brief,
            self.spec.as_ref().unwrap_or(&json!({})),
        );
        let seed_script = crate::pipeline_turbo::generate_seed_script(&seed_sql);
        if !seed_script.is_empty() {
            self.propose_file("src/db/seed.ts", &seed_script, true).await;
        }

        Ok(())
    }

    /// Generate project configuration files (deterministic).
    async fn step_generate_config(&mut self) -> Result<(), String> {
        let spec = self.spec.as_ref().ok_or("No spec")?;
        let name = spec
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("nexus-app");
        let safe_name = name
            .to_lowercase()
            .replace(' ', "-")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-')
            .collect::<String>();

        let mut deps = json!({
            "next": "15.1.0",
            "react": "^19.0.0",
            "react-dom": "^19.0.0"
        });
        let mut dev_deps = json!({
            "typescript": "^5.7.0",
            "@types/node": "^22.0.0",
            "@types/react": "^19.0.0",
            "tailwindcss": "^4.0.0",
            "@tailwindcss/postcss": "^4.0.0",
            "postcss": "^8.4.0"
        });

        if self.intent.needs_database {
            deps["better-sqlite3"] = json!("^11.0.0");
            dev_deps["@types/better-sqlite3"] = json!("^7.6.0");
        }

        let pkg = json!({
            "name": safe_name,
            "version": "1.0.0",
            "private": true,
            "scripts": {
                "dev": "next dev",
                "build": "next build",
                "start": "next start"
            },
            "dependencies": deps,
            "devDependencies": dev_deps
        });

        self.propose_file(
            "package.json",
            &serde_json::to_string_pretty(&pkg).unwrap_or_default(),
            true,
        )
        .await;

        let tsconfig = json!({
            "compilerOptions": {
                "target": "ES2017",
                "lib": ["dom", "dom.iterable", "esnext"],
                "allowJs": true,
                "skipLibCheck": true,
                "strict": true,
                "noEmit": true,
                "esModuleInterop": true,
                "module": "esnext",
                "moduleResolution": "bundler",
                "resolveJsonModule": true,
                "isolatedModules": true,
                "jsx": "preserve",
                "incremental": true,
                "plugins": [{"name": "next"}],
                "paths": {"@/*": ["./src/*"]}
            },
            "include": ["next-env.d.ts", "**/*.ts", "**/*.tsx"],
            "exclude": ["node_modules"]
        });
        self.propose_file(
            "tsconfig.json",
            &serde_json::to_string_pretty(&tsconfig).unwrap_or_default(),
            true,
        )
        .await;

        self.propose_file(
            "next.config.ts",
            "import type { NextConfig } from 'next';\n\nconst nextConfig: NextConfig = {};\n\nexport default nextConfig;\n",
            true,
        )
        .await;

        self.propose_file(
            "postcss.config.mjs",
            "/** @type {import('postcss-load-config').Config} */\nconst config = {\n  plugins: {\n    '@tailwindcss/postcss': {},\n  },\n};\n\nexport default config;\n",
            true,
        )
        .await;

        let app_name = self
            .spec
            .as_ref()
            .and_then(|s| s.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("Nexus App");
        let globals_css =
            crate::design_system::generate_globals_css(&self.intent.ui_style, app_name);
        self.propose_file("src/app/globals.css", &globals_css, true).await;

        if !self.intent.suggested_agents.is_empty() {
            let env_local = format!(
                "OLLAMA_URL=http://localhost:11434\nAI_MODEL={}\n",
                crate::llm_model_defaults::OPENAI_DEFAULT_MODEL
            );
            self.propose_file(".env.local", &env_local, true).await;
        }

        self.lock_scaffold_paths = true;

        Ok(())
    }

    /// Generate API routes via LLM.
    ///
    /// Kept as an individual entry point for tests and direct composition.
    /// The parallel variant [`step_generate_api_and_ui_parallel`] is preferred.
    #[allow(dead_code)]
    async fn step_generate_api(&mut self) -> Result<(), String> {
        let prompt = self.build_api_prompt()?;
        let response = crate::handlers::chat::call_llm_simple_for_project(
            &self.app,
            &prompt,
            Some(&self.project_id),
        )
        .await
        .map_err(|e| format!("API generation failed: {}", e))?;

        let files = nexus_store::parse_file_blocks(&response);
        for (path, content) in files {
            self.propose_file(&path, &content, false).await;
        }

        Ok(())
    }

    /// Generate UI pages via LLM.
    ///
    /// Kept as an individual entry point for tests and direct composition.
    /// The parallel variant [`step_generate_api_and_ui_parallel`] is preferred.
    #[allow(dead_code)]
    async fn step_generate_ui(&mut self) -> Result<(), String> {
        let prompt = self.build_ui_prompt()?;
        let response = crate::handlers::chat::call_llm_simple_for_project(
            &self.app,
            &prompt,
            Some(&self.project_id),
        )
        .await
        .map_err(|e| format!("UI generation failed: {}", e))?;

        let files = nexus_store::parse_file_blocks(&response);
        for (path, content) in files {
            self.propose_file(&path, &content, false).await;
        }

        Ok(())
    }

    /// Run API + UI generation in parallel -- cuts this phase from ~2x to ~1x LLM latency.
    async fn step_generate_api_and_ui_parallel(&mut self) -> Result<(), String> {
        let api_prompt = self.build_api_prompt()?;
        let ui_prompt = self.build_ui_prompt()?;

        let app = self.app.clone();
        let pid = self.project_id.clone();

        let api_future =
            crate::handlers::chat::call_llm_simple_for_project(&app, &api_prompt, Some(&pid));
        let ui_future =
            crate::handlers::chat::call_llm_simple_for_project(&app, &ui_prompt, Some(&pid));

        let (api_result, ui_result) = tokio::join!(api_future, ui_future);

        match api_result {
            Ok(response) => {
                for (path, content) in nexus_store::parse_file_blocks(&response) {
                    self.propose_file(&path, &content, false).await;
                }
            }
            Err(e) => {
                warn!("API generation failed (non-fatal): {}", e);
                let _ = self
                    .tx
                    .send(GenerationEvent::Error {
                        phase: "generate_api".into(),
                        message: e.to_string(),
                        recoverable: true,
                    })
                    .await;
            }
        }

        match ui_result {
            Ok(response) => {
                for (path, content) in nexus_store::parse_file_blocks(&response) {
                    self.propose_file(&path, &content, false).await;
                }
            }
            Err(e) => {
                return Err(format!("UI generation failed: {}", e));
            }
        }

        Ok(())
    }

    /// Generate AI agent API route and chat widget (deterministic template).
    async fn step_generate_agents(&mut self) -> Result<(), String> {
        if self.intent.suggested_agents.is_empty() {
            return Ok(());
        }

        let mut agent_prompts = String::new();
        for agent in &self.intent.suggested_agents {
            let key = agent.name.to_lowercase().replace(' ', "_");
            agent_prompts.push_str(&format!(
                "  '{}': `{}`,\n",
                key,
                agent.system_prompt.replace('`', "'")
            ));
        }

        let api_route = format!(
            r#"import {{ NextRequest }} from 'next/server';

const AGENT_PROMPTS: Record<string, string> = {{
{agent_prompts}}};

export async function POST(req: NextRequest) {{
  const {{ message, agent_name }} = await req.json();
  const systemPrompt = AGENT_PROMPTS[agent_name] || 'You are a helpful assistant.';

  const apiBase = process.env.OLLAMA_URL || 'http://localhost:11434';
  const model = process.env.AI_MODEL || '{default_model}';

  try {{
    const resp = await fetch(`${{apiBase}}/api/chat`, {{
      method: 'POST',
      headers: {{ 'Content-Type': 'application/json' }},
      body: JSON.stringify({{
        model,
        messages: [
          {{ role: 'system', content: systemPrompt }},
          {{ role: 'user', content: message }},
        ],
        stream: false,
      }}),
    }});

    if (!resp.ok) {{
      return Response.json({{ response: 'AI service unavailable. Please ensure Ollama is running.' }}, {{ status: 200 }});
    }}

    const data = await resp.json();
    return Response.json({{ response: data.message?.content || 'No response' }});
  }} catch {{
    return Response.json({{ response: 'AI service unavailable. Start Ollama to enable AI features.' }}, {{ status: 200 }});
  }}
}}"#
            ,
            agent_prompts = agent_prompts,
            default_model = crate::llm_model_defaults::OPENAI_DEFAULT_MODEL,
        );

        self.propose_file("src/app/api/agent/chat/route.ts", &api_route, true)
            .await;

        let chat_widget = r#"'use client';

import { useState, useRef, useEffect } from 'react';

interface Message {
  role: 'user' | 'assistant';
  content: string;
}

export function ChatWidget({ agentName, title, placeholder }: { agentName: string; title: string; placeholder?: string }) {
  const [open, setOpen] = useState(false);
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState('');
  const [loading, setLoading] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    scrollRef.current?.scrollTo(0, scrollRef.current.scrollHeight);
  }, [messages]);

  async function send() {
    if (!input.trim() || loading) return;
    const userMsg = input.trim();
    setInput('');
    setMessages(prev => [...prev, { role: 'user', content: userMsg }]);
    setLoading(true);
    try {
      const res = await fetch('/api/agent/chat', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ message: userMsg, agent_name: agentName }),
      });
      const data = await res.json();
      setMessages(prev => [...prev, { role: 'assistant', content: data.response }]);
    } catch {
      setMessages(prev => [...prev, { role: 'assistant', content: 'Sorry, I could not process that.' }]);
    }
    setLoading(false);
  }

  if (!open) {
    return (
      <button
        onClick={() => setOpen(true)}
        className="fixed bottom-6 right-6 z-50 w-14 h-14 bg-gradient-to-br from-violet-600 to-blue-600 rounded-full shadow-xl flex items-center justify-center hover:scale-105 transition-transform"
        aria-label="Open AI chat"
      >
        <svg className="w-6 h-6 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 10h.01M12 10h.01M16 10h.01M9 16H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-5l-5 5v-5z" />
        </svg>
      </button>
    );
  }

  return (
    <div className="fixed bottom-6 right-6 z-50 w-96 h-[500px] bg-gray-900 border border-gray-700 rounded-2xl shadow-2xl flex flex-col overflow-hidden">
      <div className="flex items-center justify-between px-4 py-3 bg-gradient-to-r from-violet-600/20 to-blue-600/20 border-b border-gray-700">
        <div className="flex items-center gap-2">
          <div className="w-2 h-2 rounded-full bg-green-400 animate-pulse" />
          <span className="text-sm font-semibold text-white">{title}</span>
        </div>
        <button onClick={() => setOpen(false)} className="text-gray-400 hover:text-white">
          <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>
      <div ref={scrollRef} className="flex-1 overflow-y-auto p-4 space-y-3">
        {messages.length === 0 && (
          <p className="text-gray-500 text-sm text-center mt-8">Ask me anything!</p>
        )}
        {messages.map((msg, i) => (
          <div key={i} className={`flex ${msg.role === 'user' ? 'justify-end' : 'justify-start'}`}>
            <div className={`max-w-[80%] rounded-xl px-3 py-2 text-sm ${
              msg.role === 'user'
                ? 'bg-violet-600 text-white'
                : 'bg-gray-800 text-gray-200'
            }`}>
              {msg.content}
            </div>
          </div>
        ))}
        {loading && (
          <div className="flex justify-start">
            <div className="bg-gray-800 rounded-xl px-3 py-2 text-sm text-gray-400">Thinking...</div>
          </div>
        )}
      </div>
      <div className="p-3 border-t border-gray-700">
        <div className="flex gap-2">
          <input
            value={input}
            onChange={e => setInput(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && send()}
            placeholder={placeholder || 'Type a message...'}
            className="flex-1 bg-gray-800 border border-gray-700 rounded-lg px-3 py-2 text-sm text-white placeholder-gray-500 focus:outline-none focus:ring-1 focus:ring-violet-500"
          />
          <button onClick={send} disabled={loading} className="bg-violet-600 hover:bg-violet-700 text-white rounded-lg px-3 py-2 text-sm disabled:opacity-50">
            Send
          </button>
        </div>
      </div>
    </div>
  );
}
"#;

        self.propose_file("src/components/chat-widget.tsx", chat_widget, true)
            .await;

        Ok(())
    }

    /// Generate authentication files via LLM.
    async fn step_generate_auth(&mut self) -> Result<(), String> {
        if !self.intent.needs_auth {
            return Ok(());
        }

        let prompt = r#"Generate a simple NextAuth.js email+password authentication for Next.js 15 App Router.

Generate:
1. src/app/api/auth/[...nextauth]/route.ts -- NextAuth handler with credentials provider
2. src/app/login/page.tsx -- login form
3. src/app/register/page.tsx -- registration form
4. src/lib/auth.ts -- auth config and session helpers

Use Tailwind CSS for styling. Make forms clean and modern.
Keep it simple -- email + password only, no OAuth.

=== FILE: path ===
(content)
=== END FILE ==="#;

        let response = crate::pipeline_turbo::call_with_retry(
            &self.app,
            &self.project_id,
            prompt,
            2,
        )
        .await
        .map_err(|e| format!("Auth generation failed after retries: {}", e))?;

        let files = nexus_store::parse_file_blocks(&response);
        for (path, content) in files {
            self.propose_file(&path, &content, false).await;
        }

        Ok(())
    }

    /// Run 4-layer validation gate on all proposed files.
    async fn step_validate_all(&mut self) -> Result<(), String> {
        let core = ExecutionCore::new(self.project_dir.clone());

        let operations: Vec<Operation> = self
            .proposed_files
            .iter()
            .map(|(path, content)| Operation::FileWrite {
                path: path.clone(),
                content: content.clone(),
            })
            .collect();

        let proposal = Proposal {
            id: uuid::Uuid::new_v4().to_string(),
            intent: "Pipeline code generation".into(),
            operations,
            constraints: vec![],
            rollback_plan: vec![],
        };

        // Layer 1: Static validation
        let validation = core.validate(&proposal);
        let _ = self
            .tx
            .send(GenerationEvent::ValidationResult {
                layer: "static".into(),
                passed: validation.valid,
                issues: validation
                    .errors
                    .iter()
                    .map(|e| e.message.clone())
                    .collect(),
            })
            .await;

        if !validation.valid {
            let error_indices: HashSet<usize> =
                validation.errors.iter().map(|e| e.operation_index).collect();
            self.proposed_files = self
                .proposed_files
                .iter()
                .enumerate()
                .filter(|(i, _)| !error_indices.contains(i))
                .map(|(_, f)| f.clone())
                .collect();

            let _ = self
                .tx
                .send(GenerationEvent::ValidationResult {
                    layer: "static".into(),
                    passed: true,
                    issues: vec![format!(
                        "Removed {} invalid files",
                        error_indices.len()
                    )],
                })
                .await;
        }

        // Layer 2: Content validation
        let mut empty_files = Vec::new();
        let mut placeholder_files = Vec::new();
        for (path, content) in &self.proposed_files {
            if content.is_empty() {
                empty_files.push(format!("{}: empty file", path));
            } else if content.contains("TODO")
                || content.contains("FIXME")
                || content.contains("implement here")
            {
                placeholder_files.push(path.clone());
            }
        }

        if !placeholder_files.is_empty() {
            warn!(
                files = ?placeholder_files,
                "Generated files contain placeholder text — quality may be degraded"
            );
        }

        let _ = self
            .tx
            .send(GenerationEvent::ValidationResult {
                layer: "content".into(),
                passed: empty_files.is_empty(),
                issues: {
                    let mut all = empty_files.clone();
                    for p in &placeholder_files {
                        all.push(format!("{}: contains placeholder/TODO text", p));
                    }
                    all
                },
            })
            .await;

        if !empty_files.is_empty() {
            let empty_paths: HashSet<&str> = empty_files
                .iter()
                .map(|e| e.split(':').next().unwrap_or(""))
                .collect();
            self.proposed_files
                .retain(|(path, _)| !empty_paths.contains(path.as_str()));
            warn!(removed = empty_files.len(), "Removed empty files from proposal");
        }

        Ok(())
    }

    /// Commit all proposed files to disk with atomic backup/rollback.
    async fn step_commit_files(&mut self) -> Result<(), String> {
        let _ = std::fs::create_dir_all(&self.project_dir);

        let backup_dir = self
            .app
            .data_dir
            .join("projects")
            .join(&self.project_id)
            .join(".backup");
        if self.project_dir.exists() {
            let _ = std::fs::remove_dir_all(&backup_dir);
            if let Err(e) = copy_dir_recursive(&self.project_dir, &backup_dir) {
                warn!(error = %e, "Could not create project backup — proceeding without rollback safety");
            }
        }

        let commit_result: Result<(), String> = (|| {
            for (path, content) in &self.proposed_files {
                let full_path = self.project_dir.join(path);
                if let Some(parent) = full_path.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("mkdir: {}", e))?;
                }
                std::fs::write(&full_path, content)
                    .map_err(|e| format!("write {}: {}", path, e))?;
            }
            Ok(())
        })();

        if let Err(e) = &commit_result {
            warn!(error = %e, "File commit failed — attempting rollback from backup");
            if backup_dir.exists() {
                let _ = std::fs::remove_dir_all(&self.project_dir);
                let _ = std::fs::rename(&backup_dir, &self.project_dir);
            }
            return Err(e.clone());
        }

        let _ = std::fs::remove_dir_all(&backup_dir);

        for (path, content) in &self.proposed_files {
            let _ = self
                .tx
                .send(GenerationEvent::FileGenerated {
                    path: path.clone(),
                    lines: content.lines().count(),
                })
                .await;
        }

        info!(
            project = %self.project_id,
            files = self.proposed_files.len(),
            "All files committed to disk"
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Prompt builders
    // -----------------------------------------------------------------------

    /// Build the LLM prompt for API route generation.
    fn build_api_prompt(&self) -> Result<String, String> {
        let spec = self.spec.as_ref().ok_or("No spec")?;
        let entities = spec
            .get("entities")
            .and_then(|e| e.as_array())
            .cloned()
            .unwrap_or_default();

        Ok(format!(
            r#"Generate Next.js 15 App Router API routes for these entities: {}

For each entity, generate:
1. src/app/api/{{entity}}/route.ts -- GET (list all) + POST (create)
2. src/app/api/{{entity}}/[id]/route.ts -- GET (by id) + DELETE

Use this DB pattern:
```
import {{ getDb }} from '@/db';
```

Tables already exist with columns matching the entity fields. Always include `id` (TEXT, uuid) and `created_at`.

Generate ONLY the file blocks, no explanation:
=== FILE: path ===
(content)
=== END FILE ==="#,
            entities
                .iter()
                .map(|e| {
                    let name = e
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("item");
                    let fields: Vec<String> = e
                        .get("fields")
                        .and_then(|f| f.as_array())
                        .unwrap_or(&vec![])
                        .iter()
                        .filter_map(|f| f.get("name").and_then(|n| n.as_str()).map(String::from))
                        .collect();
                    format!("{} (fields: {})", name, fields.join(", "))
                })
                .collect::<Vec<_>>()
                .join("; ")
        ))
    }

    /// Build the LLM prompt for UI page generation.
    fn build_ui_prompt(&self) -> Result<String, String> {
        let spec = self.spec.as_ref().ok_or("No spec")?;
        let pages = spec
            .get("pages")
            .and_then(|p| p.as_array())
            .cloned()
            .unwrap_or_default();
        let style = spec.get("style").cloned().unwrap_or(json!({}));

        let pages_desc = pages
            .iter()
            .map(|p| {
                let name = p.get("name").and_then(|n| n.as_str()).unwrap_or("Page");
                let route = p.get("route").and_then(|r| r.as_str()).unwrap_or("/");
                let desc = p
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("");
                let comps = p
                    .get("components")
                    .and_then(|c| c.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|c| c.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                format!(
                    "- {} (route: {}): {} [components: {}]",
                    name, route, desc, comps
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let style_guide = format!(
            "UI style: {:?}. Theme: {}. Font: {}.",
            self.intent.ui_style,
            style
                .get("theme")
                .and_then(|t| t.as_str())
                .unwrap_or("dark"),
            style
                .get("font")
                .and_then(|f| f.as_str())
                .unwrap_or("inter"),
        );

        let font_link = crate::design_system::font_imports(&self.intent.ui_style);
        let description = &self.description;

        Ok(format!(
            r#"Generate Next.js 15 App Router pages with Tailwind CSS.

App description: {description}

Pages to generate:
{pages_desc}

{style_guide}

DESIGN SYSTEM (already in globals.css):
- Use CSS classes: .btn-primary, .card, .input, .nav, .nav-link, .hero, .section, .section-title, .section-subtitle, .grid-3
- Use CSS variables: hsl(var(--primary)), hsl(var(--foreground)), hsl(var(--muted-foreground)), hsl(var(--background)), hsl(var(--border))
- Font link for layout.tsx <head>: {font_link}
- Hero sections: use .hero class with h1 + p + .btn-primary
- Cards: use .card class
- Buttons: use .btn-primary class OR Tailwind bg-[hsl(var(--primary))]
- Navigation: use .nav + .nav-link classes

REAL CONTENT TO USE (do NOT use lorem ipsum — use this exact content):
{brief_content}

RULES:
- layout.tsx MUST include the font link in <head>
- Home page MUST have a .hero section with the EXACT headline, subheadline, CTA from above
- Home page MUST show the seed content items (with real names, descriptions, prices)
- Home page MUST include trust signal badges below the hero
- Every page should look polished — real spacing, readable typography, visual hierarchy
- Use .animate-fade-in on main content for entrance animation
- Cards should use: .card class + {card_hover} for hover effect
- Buttons should use: .btn-primary class + {button_hover}
- Use Tailwind for layout (flex, grid, padding, margin) and the CSS classes for styled components
- 'use client' only where needed (forms, state)
- For items with image_emoji, display the emoji as a large visual element (text-4xl)

Generate src/app/layout.tsx and a page.tsx for each route.

Generate ONLY file blocks:
=== FILE: path ===
(content)
=== END FILE ==="#,
            font_link = font_link,
            brief_content = crate::product_engine::format_brief_for_prompt(&self.product_brief),
            card_hover = self.product_brief.delight_classes.card_hover,
            button_hover = self.product_brief.delight_classes.button_hover,
        ))
    }

    // -----------------------------------------------------------------------
    // Event helpers
    // -----------------------------------------------------------------------

    /// Emit a PhaseStarted event.
    async fn emit_phase_started(&self, phase: &str, description: &str) {
        let _ = self
            .tx
            .send(GenerationEvent::PhaseStarted {
                phase: phase.to_string(),
                description: description.to_string(),
            })
            .await;
    }

    /// Emit a PhaseCompleted event with elapsed duration from `start`.
    async fn emit_phase_completed(&self, phase: &str, start: Instant) {
        let _ = self
            .tx
            .send(GenerationEvent::PhaseCompleted {
                phase: phase.to_string(),
                duration_ms: start.elapsed().as_millis() as u64,
            })
            .await;
    }

    /// Emit a PhaseProgress event.
    async fn emit_phase_progress(&self, phase: &str, progress: f32, detail: &str) {
        let _ = self
            .tx
            .send(GenerationEvent::PhaseProgress {
                phase: phase.to_string(),
                progress,
                detail: detail.to_string(),
            })
            .await;
    }

    /// Propose a file (tracks it internally and emits a FileProposed event).
    async fn propose_file(&mut self, path: &str, content: &str, allow_override_locked_scaffold: bool) {
        if !allow_override_locked_scaffold && self.lock_scaffold_paths {
            const PROTECTED: &[&str] = &[
                "package.json",
                "tsconfig.json",
                "postcss.config.mjs",
                "next.config.ts",
                "src/app/globals.css",
            ];
            if PROTECTED.contains(&path) {
                return;
            }
        }

        let _ = self
            .tx
            .send(GenerationEvent::FileProposed {
                path: path.to_string(),
                size: content.len(),
                skeleton: false,
            })
            .await;

        self.proposed_files
            .push((path.to_string(), content.to_string()));
    }
}

// ---------------------------------------------------------------------------
// Helpers (shared with execution_pipeline.rs)
// ---------------------------------------------------------------------------

/// Recursively copy a directory, skipping `node_modules` and `.next`.
fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let dest = dst.join(entry.file_name());
        if ft.is_dir() {
            if entry.file_name() == "node_modules" || entry.file_name() == ".next" {
                continue;
            }
            copy_dir_recursive(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), &dest)?;
        }
    }
    Ok(())
}

/// Parse JSON from an LLM response, handling markdown code fences.
fn parse_json_from_response(text: &str) -> Value {
    let trimmed = text
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    serde_json::from_str(trimmed).unwrap_or_else(|_| {
        if let Some(start) = trimmed.find('{') {
            if let Some(end) = trimmed.rfind('}') {
                if let Ok(v) = serde_json::from_str(&trimmed[start..=end]) {
                    return v;
                }
            }
        }
        warn!(
            text_len = text.len(),
            preview = &text[..text.len().min(120)],
            "parse_json_from_response: failed to parse JSON, falling back to empty spec"
        );
        json!({"summary": text, "pages": [], "entities": [], "agents": []})
    })
}
