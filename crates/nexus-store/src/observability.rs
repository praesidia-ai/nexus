//! Observability — agent traces, tool logs, audit log, evolution metrics, healing events.

use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;

// ---- Agent Traces ----

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTrace {
    pub id: String,
    pub project_id: String,
    pub task: String,
    pub mode: String,
    pub provider: String,
    pub model: String,
    pub status: String,
    pub total_iterations: i32,
    pub total_tool_calls: i32,
    pub total_tokens_in: i64,
    pub total_tokens_out: i64,
    pub estimated_cost_usd: f64,
    pub duration_ms: i64,
    pub error: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

// ---- Tool Logs ----

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolLog {
    pub id: String,
    pub trace_id: String,
    pub iteration: i32,
    pub tool_name: String,
    pub arguments: String,
    pub output_preview: Option<String>,
    pub success: bool,
    pub duration_ms: i64,
    pub created_at: String,
}

// ---- Audit Log ----

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    pub project_id: String,
    pub actor: String,
    pub action: String,
    pub target: Option<String>,
    pub details: Option<String>,
    pub risk_level: String,
    pub created_at: String,
}

// ---- Agent Evolution ----

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionEntry {
    pub id: String,
    pub project_id: String,
    pub agent_name: String,
    pub skill_id: Option<String>,
    pub task_type: String,
    pub success: bool,
    pub iterations_used: Option<i32>,
    pub tools_used: Vec<String>,
    pub feedback_score: Option<i32>,
    pub notes: Option<String>,
    pub created_at: String,
}

// ---- Healing Event ----

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealingEvent {
    pub id: String,
    pub project_id: String,
    pub instance_id: String,
    pub trigger: String,
    pub error_log: String,
    pub status: String,
    pub fix_summary: Option<String>,
    pub created_at: String,
}

pub struct ObservabilityService<'c> {
    conn: &'c Connection,
}

impl<'c> ObservabilityService<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    // ---- Traces ----

    pub fn create_trace(
        &self,
        project_id: &str,
        task: &str,
        mode: &str,
        provider: &str,
        model: &str,
    ) -> Result<AgentTrace> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO agent_traces (id, project_id, task, mode, provider, model, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'running', ?7)",
            params![id, project_id, task, mode, provider, model, now],
        )?;
        Ok(AgentTrace {
            id,
            project_id: project_id.to_string(),
            task: task.to_string(),
            mode: mode.to_string(),
            provider: provider.to_string(),
            model: model.to_string(),
            status: "running".to_string(),
            total_iterations: 0,
            total_tool_calls: 0,
            total_tokens_in: 0,
            total_tokens_out: 0,
            estimated_cost_usd: 0.0,
            duration_ms: 0,
            error: None,
            created_at: now,
            completed_at: None,
        })
    }

    pub fn complete_trace(
        &self,
        trace_id: &str,
        status: &str,
        iterations: i32,
        tool_calls: i32,
        duration_ms: i64,
        error: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE agent_traces SET status=?1, total_iterations=?2, total_tool_calls=?3, duration_ms=?4, error=?5, completed_at=?6 WHERE id=?7",
            params![status, iterations, tool_calls, duration_ms, error, now, trace_id],
        )?;
        Ok(())
    }

    pub fn update_trace_tokens(
        &self,
        trace_id: &str,
        tokens_in: i64,
        tokens_out: i64,
        cost_usd: f64,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE agent_traces SET total_tokens_in = total_tokens_in + ?1, total_tokens_out = total_tokens_out + ?2, estimated_cost_usd = estimated_cost_usd + ?3 WHERE id = ?4",
            params![tokens_in, tokens_out, cost_usd, trace_id],
        )?;
        Ok(())
    }

    pub fn list_traces(&self, project_id: &str, limit: i64) -> Result<Vec<AgentTrace>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, task, mode, provider, model, status, total_iterations, total_tool_calls,
                    total_tokens_in, total_tokens_out, estimated_cost_usd, duration_ms, error, created_at, completed_at
             FROM agent_traces WHERE project_id=?1 ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![project_id, limit], |r| {
            Ok(AgentTrace {
                id: r.get(0)?,
                project_id: r.get(1)?,
                task: r.get(2)?,
                mode: r.get(3)?,
                provider: r.get(4)?,
                model: r.get(5)?,
                status: r.get(6)?,
                total_iterations: r.get(7)?,
                total_tool_calls: r.get(8)?,
                total_tokens_in: r.get(9)?,
                total_tokens_out: r.get(10)?,
                estimated_cost_usd: r.get(11)?,
                duration_ms: r.get(12)?,
                error: r.get(13)?,
                created_at: r.get(14)?,
                completed_at: r.get(15)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    // ---- Tool Logs ----

    #[allow(clippy::too_many_arguments)]
    pub fn log_tool_call(
        &self,
        trace_id: &str,
        iteration: i32,
        tool_name: &str,
        arguments: &str,
        output_preview: Option<&str>,
        success: bool,
        duration_ms: i64,
    ) -> Result<()> {
        let id = Uuid::new_v4().to_string();
        self.conn.execute(
            "INSERT INTO agent_tool_logs (id, trace_id, iteration, tool_name, arguments, output_preview, success, duration_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                trace_id,
                iteration,
                tool_name,
                arguments,
                output_preview,
                success as i64,
                duration_ms
            ],
        )?;
        Ok(())
    }

    /// Fetch the `project_id` that owns a trace, so callers can tenant-validate
    /// before exposing logs.
    pub fn get_trace_project_id(&self, trace_id: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT project_id FROM agent_traces WHERE id = ?1 LIMIT 1")?;
        let mut rows = stmt.query(params![trace_id])?;
        if let Some(row) = rows.next()? {
            let pid: String = row.get(0)?;
            return Ok(Some(pid));
        }
        Ok(None)
    }

    pub fn list_tool_logs(&self, trace_id: &str) -> Result<Vec<ToolLog>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, trace_id, iteration, tool_name, arguments, output_preview, success, duration_ms, created_at
             FROM agent_tool_logs WHERE trace_id=?1 ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(params![trace_id], |r| {
            Ok(ToolLog {
                id: r.get(0)?,
                trace_id: r.get(1)?,
                iteration: r.get(2)?,
                tool_name: r.get(3)?,
                arguments: r.get(4)?,
                output_preview: r.get(5)?,
                success: r.get::<_, i64>(6)? != 0,
                duration_ms: r.get(7)?,
                created_at: r.get(8)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    // ---- Audit Log ----

    pub fn audit(
        &self,
        project_id: &str,
        actor: &str,
        action: &str,
        target: Option<&str>,
        details: Option<&str>,
        risk_level: &str,
    ) -> Result<()> {
        let id = Uuid::new_v4().to_string();
        self.conn.execute(
            "INSERT INTO audit_log (id, project_id, actor, action, target, details, risk_level)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, project_id, actor, action, target, details, risk_level],
        )?;
        Ok(())
    }

    pub fn list_audit(&self, project_id: &str, limit: i64) -> Result<Vec<AuditEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, actor, action, target, details, risk_level, created_at
             FROM audit_log WHERE project_id=?1 ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![project_id, limit], |r| {
            Ok(AuditEntry {
                id: r.get(0)?,
                project_id: r.get(1)?,
                actor: r.get(2)?,
                action: r.get(3)?,
                target: r.get(4)?,
                details: r.get(5)?,
                risk_level: r.get(6)?,
                created_at: r.get(7)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    // ---- Evolution ----

    #[allow(clippy::too_many_arguments)]
    pub fn record_evolution(
        &self,
        project_id: &str,
        agent_name: &str,
        skill_id: Option<&str>,
        task_type: &str,
        success: bool,
        iterations: Option<i32>,
        tools: &[String],
        score: Option<i32>,
    ) -> Result<()> {
        let id = Uuid::new_v4().to_string();
        let tools_json = serde_json::to_string(tools).unwrap_or_else(|_| "[]".to_string());
        self.conn.execute(
            "INSERT INTO agent_evolution (id, project_id, agent_name, skill_id, task_type, success, iterations_used, tools_used, feedback_score)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                id,
                project_id,
                agent_name,
                skill_id,
                task_type,
                success as i64,
                iterations,
                tools_json,
                score
            ],
        )?;
        Ok(())
    }

    pub fn get_agent_stats(
        &self,
        project_id: &str,
        agent_name: &str,
    ) -> Result<serde_json::Value> {
        let total: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM agent_evolution WHERE project_id=?1 AND agent_name=?2",
                params![project_id, agent_name],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let successes: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM agent_evolution WHERE project_id=?1 AND agent_name=?2 AND success=1",
                params![project_id, agent_name],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let avg_iterations: f64 = self
            .conn
            .query_row(
                "SELECT COALESCE(AVG(iterations_used), 0.0) FROM agent_evolution WHERE project_id=?1 AND agent_name=?2 AND iterations_used IS NOT NULL",
                params![project_id, agent_name],
                |r| r.get(0),
            )
            .unwrap_or(0.0);

        Ok(serde_json::json!({
            "total_tasks": total,
            "successes": successes,
            "failures": total - successes,
            "success_rate": if total > 0 { (successes as f64 / total as f64 * 100.0).round() } else { 0.0 },
            "avg_iterations": (avg_iterations * 10.0).round() / 10.0,
        }))
    }

    // ---- Healing Events ----

    pub fn list_healing_events(
        &self,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<HealingEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, instance_id, trigger, error_log, status, fix_summary, created_at
             FROM healing_events WHERE project_id=?1 ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![project_id, limit], |r| {
            Ok(HealingEvent {
                id: r.get(0)?,
                project_id: r.get(1)?,
                instance_id: r.get(2)?,
                trigger: r.get(3)?,
                error_log: r.get(4)?,
                status: r.get(5)?,
                fix_summary: r.get(6)?,
                created_at: r.get(7)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }
}
