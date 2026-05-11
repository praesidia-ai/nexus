//! Materialization builders: turn AI-generated intent into real artifacts.
//!
//! Three builders are provided:
//! - `SchemaBuilder` — creates SQLite tables in a project's `data.db`
//! - `AgentBuilder`  — persists agent definitions + writes `.neurogent.yaml`
//! - `PortalBuilder` — scaffolds a minimal client-portal HTML/JS bundle

use std::path::Path;

use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;

// ---------------------------------------------------------------------------
// Shared types
// ---------------------------------------------------------------------------

/// A single field in an entity's schema, as emitted in `nexus_state.actions`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDef {
    pub name: String,
    /// SQLite column type: TEXT | INTEGER | REAL | BLOB
    pub r#type: String,
    #[serde(default)]
    pub primary_key: bool,
    #[serde(default)]
    pub not_null: bool,
}

/// Full entity schema emitted by the AI in a `create_table` action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitySchema {
    pub entity_name: String,
    pub fields: Vec<FieldDef>,
}

/// Persisted record for a materialized table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterializedTable {
    pub id: String,
    pub project_id: String,
    pub table_name: String,
    pub schema_json: serde_json::Value,
    pub created_at: String,
}

/// Input for creating an agent from a `create_agent` nexus_state action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinitionInput {
    pub name: String,
    pub role: String,
    pub tools: Vec<String>,
    #[serde(default = "default_memory_type")]
    pub memory_type: String,
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_model")]
    pub model: String,
    pub system_prompt: String,
}

fn default_memory_type() -> String { "persistent".to_string() }
fn default_provider() -> String { "anthropic".to_string() }
fn default_model() -> String { "claude-sonnet-4-6".to_string() }

/// Persisted agent definition row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub role: String,
    pub tools: Vec<String>,
    pub memory_type: String,
    pub provider: String,
    pub model: String,
    pub system_prompt: String,
    pub status: String,
    pub zeroclaw_pid: Option<i64>,
    pub zeroclaw_port: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

// ---------------------------------------------------------------------------
// SchemaBuilder — creates SQLite tables in the project data.db
// ---------------------------------------------------------------------------

pub struct SchemaBuilder<'c> {
    /// Main nexus DB connection (for `materialized_tables` registry).
    nexus_conn: &'c Connection,
}

impl<'c> SchemaBuilder<'c> {
    pub fn new(nexus_conn: &'c Connection) -> Self {
        Self { nexus_conn }
    }

    /// Creates the table in the project's `data.db` and records it in the
    /// main nexus DB. `project_data_db` is the path to `data.db`.
    pub fn materialize_table(
        &self,
        project_id: &str,
        project_data_db: &Path,
        schema: &EntitySchema,
    ) -> Result<MaterializedTable> {
        // Sanitize table name
        let table_name = sanitize_identifier(&schema.entity_name);

        // Build CREATE TABLE SQL
        let col_defs: Vec<String> = schema
            .fields
            .iter()
            .map(|f| {
                let mut col = format!(
                    "{} {}",
                    sanitize_identifier(&f.name),
                    sanitize_type(&f.r#type)
                );
                if f.primary_key {
                    col.push_str(" PRIMARY KEY");
                }
                if f.not_null && !f.primary_key {
                    col.push_str(" NOT NULL");
                }
                col
            })
            .collect();

        let ddl = format!(
            "CREATE TABLE IF NOT EXISTS {} ({})",
            table_name,
            col_defs.join(", ")
        );

        // Open project data.db and run DDL
        if let Some(parent) = project_data_db.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data_conn = Connection::open(project_data_db)?;
        data_conn.execute_batch(&ddl)?;

        // Record in nexus_db
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let schema_json = serde_json::to_string(schema)?;

        self.nexus_conn.execute(
            "INSERT INTO materialized_tables (id, project_id, table_name, schema_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, project_id, table_name, schema_json, now],
        )?;

        Ok(MaterializedTable {
            id,
            project_id: project_id.to_string(),
            table_name,
            schema_json: serde_json::to_value(schema)?,
            created_at: now,
        })
    }

    pub fn list_tables(&self, project_id: &str) -> Result<Vec<MaterializedTable>> {
        let mut stmt = self.nexus_conn.prepare(
            "SELECT id, project_id, table_name, schema_json, created_at
             FROM materialized_tables WHERE project_id = ?1 ORDER BY created_at",
        )?;
        let rows = stmt.query_map(params![project_id], |row| {
            let schema_str: String = row.get(3)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                schema_str,
                row.get::<_, String>(4)?,
            ))
        })?;
        let mut result = Vec::new();
        for row in rows {
            let (id, pid, table_name, schema_str, created_at) = row?;
            let schema_json =
                serde_json::from_str(&schema_str).unwrap_or(serde_json::Value::Null);
            result.push(MaterializedTable {
                id,
                project_id: pid,
                table_name,
                schema_json,
                created_at,
            });
        }
        Ok(result)
    }
}

// ---------------------------------------------------------------------------
// AgentBuilder — persists agent definitions and writes .neurogent.yaml
// ---------------------------------------------------------------------------

pub struct AgentBuilder<'c> {
    nexus_conn: &'c Connection,
}

impl<'c> AgentBuilder<'c> {
    pub fn new(nexus_conn: &'c Connection) -> Self {
        Self { nexus_conn }
    }

    /// Persists the agent definition and writes a `.neurogent.yaml` to
    /// `agents_dir/{agent_id}.yaml`.
    pub fn materialize_agent(
        &self,
        project_id: &str,
        agents_dir: &Path,
        input: &AgentDefinitionInput,
    ) -> Result<AgentDefinition> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let tools_json = serde_json::to_string(&input.tools)?;

        self.nexus_conn.execute(
            "INSERT INTO agent_definitions
             (id, project_id, name, role, tools, memory_type, provider, model, system_prompt, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'defined', ?10, ?10)",
            params![
                id,
                project_id,
                input.name,
                input.role,
                tools_json,
                input.memory_type,
                input.provider,
                input.model,
                input.system_prompt,
                now
            ],
        )?;

        // Write .neurogent.yaml
        std::fs::create_dir_all(agents_dir)?;

        let yaml_content = build_neurogent_yaml(&id, input);
        let yaml_path = agents_dir.join(format!("{}.yaml", id));
        std::fs::write(&yaml_path, &yaml_content)?;

        Ok(AgentDefinition {
            id,
            project_id: project_id.to_string(),
            name: input.name.clone(),
            role: input.role.clone(),
            tools: input.tools.clone(),
            memory_type: input.memory_type.clone(),
            provider: input.provider.clone(),
            model: input.model.clone(),
            system_prompt: input.system_prompt.clone(),
            status: "defined".to_string(),
            zeroclaw_pid: None,
            zeroclaw_port: None,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    /// Register an agent in the Nexus DB without writing any YAML file to
    /// the generated project directory. Use this when the codegen LLM owns
    /// the full project layout and is expected to emit its own agent config.
    pub fn register_agent_db_only(
        &self,
        project_id: &str,
        input: &AgentDefinitionInput,
    ) -> Result<AgentDefinition> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let tools_json = serde_json::to_string(&input.tools)?;

        self.nexus_conn.execute(
            "INSERT INTO agent_definitions
             (id, project_id, name, role, tools, memory_type, provider, model, system_prompt, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'defined', ?10, ?10)",
            params![
                id,
                project_id,
                input.name,
                input.role,
                tools_json,
                input.memory_type,
                input.provider,
                input.model,
                input.system_prompt,
                now
            ],
        )?;

        Ok(AgentDefinition {
            id,
            project_id: project_id.to_string(),
            name: input.name.clone(),
            role: input.role.clone(),
            tools: input.tools.clone(),
            memory_type: input.memory_type.clone(),
            provider: input.provider.clone(),
            model: input.model.clone(),
            system_prompt: input.system_prompt.clone(),
            status: "defined".to_string(),
            zeroclaw_pid: None,
            zeroclaw_port: None,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    pub fn list_agents(&self, project_id: &str) -> Result<Vec<AgentDefinition>> {
        let mut stmt = self.nexus_conn.prepare(
            "SELECT id, project_id, name, role, tools, memory_type, provider, model,
                    system_prompt, status, zeroclaw_pid, zeroclaw_port, created_at, updated_at
             FROM agent_definitions WHERE project_id = ?1 ORDER BY created_at",
        )?;
        let rows = stmt.query_map(params![project_id], |row| {
            let tools_str: String = row.get(4)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                tools_str,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, Option<i64>>(10)?,
                row.get::<_, Option<i64>>(11)?,
                row.get::<_, String>(12)?,
                row.get::<_, String>(13)?,
            ))
        })?;

        let mut result = Vec::new();
        for row in rows {
            let (
                id, pid, name, role, tools_str, memory_type, provider, model,
                system_prompt, status, zeroclaw_pid, zeroclaw_port, created_at, updated_at,
            ) = row?;
            let tools: Vec<String> =
                serde_json::from_str(&tools_str).unwrap_or_default();
            result.push(AgentDefinition {
                id,
                project_id: pid,
                name,
                role,
                tools,
                memory_type,
                provider,
                model,
                system_prompt,
                status,
                zeroclaw_pid,
                zeroclaw_port,
                created_at,
                updated_at,
            });
        }
        Ok(result)
    }

    pub fn update_agent_status(
        &self,
        agent_id: &str,
        status: &str,
        pid: Option<i64>,
    ) -> Result<()> {
        self.update_agent_status_with_port(agent_id, status, pid, None)
    }

    pub fn update_agent_status_with_port(
        &self,
        agent_id: &str,
        status: &str,
        pid: Option<i64>,
        port: Option<i64>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.nexus_conn.execute(
            "UPDATE agent_definitions SET status = ?1, zeroclaw_pid = ?2, zeroclaw_port = ?3, updated_at = ?4
             WHERE id = ?5",
            params![status, pid, port, now, agent_id],
        )?;
        Ok(())
    }

    /// Get the highest allocated zeroclaw port across all agents.
    pub fn max_allocated_port(&self) -> Result<Option<i64>> {
        let port: Option<i64> = self.nexus_conn.query_row(
            "SELECT MAX(zeroclaw_port) FROM agent_definitions WHERE status = 'running'",
            [],
            |row| row.get(0),
        )?;
        Ok(port)
    }
}

// ---------------------------------------------------------------------------
// PortalBuilder — scaffold a minimal client portal
// ---------------------------------------------------------------------------

pub struct PortalBuilder;

impl PortalBuilder {
    /// Write a minimal HTML+JS portal scaffold to `portal_dir/`.
    /// The scaffold includes a basic chat interface ready to be customised.
    pub fn scaffold(portal_dir: &Path, project_name: &str, agent_name: &str) -> Result<()> {
        std::fs::create_dir_all(portal_dir)?;

        let html = build_portal_html(project_name, agent_name);
        std::fs::write(portal_dir.join("index.html"), html)?;

        let css = build_portal_css();
        std::fs::write(portal_dir.join("style.css"), css)?;

        let js = build_portal_js(agent_name);
        std::fs::write(portal_dir.join("app.js"), js)?;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn sanitize_identifier(s: &str) -> String {
    let clean: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect();
    // SQLite identifiers cannot start with a digit
    if clean.starts_with(|c: char| c.is_ascii_digit()) {
        format!("_{}", clean)
    } else {
        clean
    }
}

fn sanitize_type(t: &str) -> &'static str {
    if t.eq_ignore_ascii_case("INTEGER") {
        "INTEGER"
    } else if t.eq_ignore_ascii_case("REAL") {
        "REAL"
    } else if t.eq_ignore_ascii_case("BLOB") {
        "BLOB"
    } else if t.eq_ignore_ascii_case("NUMERIC") {
        "NUMERIC"
    } else {
        "TEXT"
    }
}

fn build_neurogent_yaml(id: &str, input: &AgentDefinitionInput) -> String {
    let tools_yaml: String = if input.tools.is_empty() {
        "tools: []".to_string()
    } else {
        let items: Vec<String> = input
            .tools
            .iter()
            .map(|t| format!("  - name: {}\n    description: \"{}\"", t, t))
            .collect();
        format!("tools:\n{}", items.join("\n"))
    };

    format!(
        r#"id: {id}
name: {name}
version: 1.0.0
description: |
  {role}
author: Nexus
license: MIT
capabilities:
  - chat
model:
  provider: {provider}
  name: {model}
  temperature: 0.7
  max_tokens: 4096
memory:
  type: conversation-buffer
  max_messages: 50
  persistent: true
  storage: ./memory
sandbox:
  allow_network: false
  allow_fs: true
  allowed_paths:
    - ./data
deployment:
  - mode: personal
  - mode: server
{tools}
endpoint: /a2a
system_prompt: |
{system}
"#,
        id = id,
        name = input.name,
        role = input.role.replace('\n', "\n  "),
        provider = input.provider,
        model = input.model,
        tools = tools_yaml,
        system = input
            .system_prompt
            .lines()
            .map(|l| format!("  {}", l))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

fn build_portal_html(project_name: &str, agent_name: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>{project_name} Portal</title>
  <link rel="stylesheet" href="style.css" />
</head>
<body>
  <header>
    <h1>{project_name}</h1>
    <span class="agent-badge">Powered by {agent_name}</span>
  </header>

  <main>
    <div id="messages" class="messages"></div>
    <form id="chat-form">
      <textarea id="user-input" placeholder="Type your message..." rows="3"></textarea>
      <button type="submit">Send</button>
    </form>
  </main>

  <script src="app.js"></script>
</body>
</html>
"#,
        project_name = project_name,
        agent_name = agent_name,
    )
}

fn build_portal_css() -> String {
    r#"/* Nexus client portal — generated scaffold */
* { box-sizing: border-box; margin: 0; padding: 0; }

body {
  font-family: system-ui, sans-serif;
  background: #09090b;
  color: #e2e8f0;
  display: flex;
  flex-direction: column;
  height: 100vh;
}

header {
  padding: 1rem 1.5rem;
  border-bottom: 1px solid #27272a;
  display: flex;
  align-items: center;
  gap: 1rem;
}

header h1 { font-size: 1.25rem; font-weight: 600; }

.agent-badge {
  font-size: 0.75rem;
  background: #1c1400;
  color: #f59e0b;
  padding: 2px 8px;
  border-radius: 9999px;
}

main {
  flex: 1;
  display: flex;
  flex-direction: column;
  max-width: 800px;
  width: 100%;
  margin: 0 auto;
  padding: 1rem;
  gap: 1rem;
}

.messages {
  flex: 1;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.message { padding: 0.75rem 1rem; border-radius: 0.5rem; max-width: 85%; }
.message.user { background: #1e1b4b; align-self: flex-end; }
.message.assistant { background: #141418; align-self: flex-start; border: 1px solid #27272a; }

#chat-form { display: flex; gap: 0.5rem; }

textarea {
  flex: 1;
  background: #141418;
  border: 1px solid #27272a;
  color: #e2e8f0;
  border-radius: 0.5rem;
  padding: 0.75rem;
  resize: none;
  font-family: inherit;
}

button {
  background: #f59e0b;
  color: #09090b;
  border: none;
  border-radius: 0.5rem;
  padding: 0.75rem 1.25rem;
  font-weight: 600;
  cursor: pointer;
}
button:hover { background: #d97706; }
"#
    .to_string()
}

fn build_portal_js(agent_name: &str) -> String {
    format!(
        r#"// Nexus client portal — generated scaffold
// Agent endpoint — reads from NEXUS_AGENT_ENDPOINT env var at deploy time,
// or falls back to the default A2A message path on the same origin.
const AGENT_ENDPOINT = window.__NEXUS_AGENT_ENDPOINT__ || '/a2a/message';
const AGENT_NAME = '{agent_name}';

const messagesEl = document.getElementById('messages');
const form = document.getElementById('chat-form');
const input = document.getElementById('user-input');

function addMessage(role, content) {{
  const div = document.createElement('div');
  div.className = 'message ' + role;
  div.textContent = content;
  messagesEl.appendChild(div);
  messagesEl.scrollTop = messagesEl.scrollHeight;
}}

form.addEventListener('submit', async (e) => {{
  e.preventDefault();
  const text = input.value.trim();
  if (!text) return;
  input.value = '';
  addMessage('user', text);

  try {{
    const res = await fetch(AGENT_ENDPOINT, {{
      method: 'POST',
      headers: {{ 'Content-Type': 'application/json' }},
      body: JSON.stringify({{ message: {{ role: 'user', content: text }} }}),
    }});
    const data = await res.json();
    const reply = data?.result || data?.message?.content || '[No response]';
    addMessage('assistant', reply);
  }} catch (err) {{
    addMessage('assistant', 'Error: ' + err.message);
  }}
}});

// Welcome message
addMessage('assistant', 'Hello! I am ' + AGENT_NAME + '. How can I help you today?');
"#,
        agent_name = agent_name,
    )
}
