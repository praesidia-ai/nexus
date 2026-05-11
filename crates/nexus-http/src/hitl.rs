//! Human-in-the-Loop Control System — approval gates, intervention points,
//! and user override mechanisms for the execution pipeline.
//!
//! Provides:
//! - Approval gates that pause execution until user confirms
//! - Risk-based auto-approve (low risk) / require-approve (high risk)
//! - Intervention points where user can modify agent output before commit
//! - Rollback to any checkpoint
//! - Audit trail of all approvals/rejections

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot, Mutex};
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalGate {
    pub id: String,
    pub gate_type: GateType,
    pub description: String,
    pub risk_level: String,
    pub context: GateContext,
    pub status: GateStatus,
    pub created_at: String,
    pub resolved_at: Option<String>,
    pub resolved_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateType {
    /// Approve files before writing to disk.
    FileWrite,
    /// Approve bash commands before execution.
    BashCommand,
    /// Approve deployment action.
    Deploy,
    /// Approve destructive operation (delete, overwrite).
    Destructive,
    /// Custom gate defined by pipeline.
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateContext {
    /// Files that will be affected.
    pub files: Vec<String>,
    /// Commands that will run.
    pub commands: Vec<String>,
    /// Agent output to review.
    pub agent_output: Option<String>,
    /// Diff preview.
    pub diff_preview: Option<String>,
    /// Additional metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateStatus {
    Pending,
    Approved,
    Rejected,
    Modified,
    AutoApproved,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalResponse {
    pub gate_id: String,
    pub action: ApprovalAction,
    pub modifications: Option<Vec<FileModification>>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalAction {
    Approve,
    Reject,
    Modify,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileModification {
    pub path: String,
    pub content: String,
}

/// HITL configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HitlConfig {
    /// Risk levels that require manual approval.
    pub require_approval: Vec<String>,
    /// Risk levels that are auto-approved.
    pub auto_approve: Vec<String>,
    /// Gate timeout in seconds (after which gate expires).
    pub gate_timeout_secs: u64,
    /// Whether to pause on all file writes.
    pub pause_on_file_write: bool,
    /// Whether to pause on all bash commands.
    pub pause_on_bash: bool,
    /// Whether to pause on deploy actions.
    pub pause_on_deploy: bool,
}

impl Default for HitlConfig {
    fn default() -> Self {
        Self {
            require_approval: vec!["high".into(), "critical".into()],
            auto_approve: vec!["safe".into(), "low".into()],
            gate_timeout_secs: 600, // 10 minutes
            pause_on_file_write: false,
            pause_on_bash: true,
            pause_on_deploy: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HitlEvent {
    #[serde(rename = "gate_created")]
    GateCreated { gate: Box<ApprovalGate> },
    #[serde(rename = "gate_resolved")]
    GateResolved { gate_id: String, status: GateStatus },
    #[serde(rename = "auto_approved")]
    AutoApproved { gate_id: String, reason: String },
    #[serde(rename = "intervention")]
    Intervention { gate_id: String, modifications: usize },
}

// ---------------------------------------------------------------------------
// Controller
// ---------------------------------------------------------------------------

pub struct HitlController {
    config: HitlConfig,
    gates: Arc<Mutex<HashMap<String, ApprovalGate>>>,
    /// Channels for pending gates waiting for approval.
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<ApprovalResponse>>>>,
    event_tx: Option<mpsc::Sender<HitlEvent>>,
}

impl HitlController {
    pub fn new(config: HitlConfig, event_tx: Option<mpsc::Sender<HitlEvent>>) -> Self {
        Self {
            config,
            gates: Arc::new(Mutex::new(HashMap::new())),
            pending: Arc::new(Mutex::new(HashMap::new())),
            event_tx,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(HitlConfig::default(), None)
    }

    /// Create an approval gate and wait for resolution.
    /// Returns the approval response (approved/rejected/modified).
    pub async fn request_approval(
        &self,
        gate_type: GateType,
        description: String,
        risk_level: String,
        context: GateContext,
    ) -> Result<ApprovalResponse, String> {
        let gate_id = uuid::Uuid::new_v4().to_string();

        // Check if auto-approve applies
        if self.config.auto_approve.contains(&risk_level) {
            let gate = ApprovalGate {
                id: gate_id.clone(),
                gate_type,
                description,
                risk_level: risk_level.clone(),
                context,
                status: GateStatus::AutoApproved,
                created_at: Utc::now().to_rfc3339(),
                resolved_at: Some(Utc::now().to_rfc3339()),
                resolved_by: Some("auto".into()),
            };

            self.gates
                .lock()
                .await
                .insert(gate_id.clone(), gate.clone());

            if let Some(ref tx) = self.event_tx {
                let _ = tx
                    .send(HitlEvent::AutoApproved {
                        gate_id: gate_id.clone(),
                        reason: format!("Risk level '{}' is auto-approved", risk_level),
                    })
                    .await;
            }

            info!(gate_id = %gate_id, risk = %risk_level, "Auto-approved");

            return Ok(ApprovalResponse {
                gate_id,
                action: ApprovalAction::Approve,
                modifications: None,
                reason: Some("Auto-approved".into()),
            });
        }

        // Create pending gate
        let gate = ApprovalGate {
            id: gate_id.clone(),
            gate_type,
            description,
            risk_level,
            context,
            status: GateStatus::Pending,
            created_at: Utc::now().to_rfc3339(),
            resolved_at: None,
            resolved_by: None,
        };

        {
            let mut gates = self.gates.lock().await;
            // Evict old resolved gates to prevent unbounded growth (keep last 1000)
            const MAX_GATES: usize = 1000;
            if gates.len() >= MAX_GATES {
                let mut resolved: Vec<(String, String)> = gates
                    .iter()
                    .filter(|(_, g)| g.status != GateStatus::Pending)
                    .map(|(id, g)| (id.clone(), g.created_at.clone()))
                    .collect();
                resolved.sort_by(|a, b| a.1.cmp(&b.1));
                let to_remove = resolved.len().min(gates.len() - MAX_GATES / 2);
                for (id, _) in resolved.into_iter().take(to_remove) {
                    gates.remove(&id);
                }
            }
            gates.insert(gate_id.clone(), gate.clone());
        }

        if let Some(ref tx) = self.event_tx {
            let _ = tx
                .send(HitlEvent::GateCreated { gate: Box::new(gate.clone()) })
                .await;
        }

        // Create oneshot channel and wait for response. If the caller's future
        // is dropped before timeout (e.g. HTTP client disconnects mid-SSE),
        // the entry would otherwise leak — so cap the map and evict the oldest
        // entries by flushing dead senders when we hit the ceiling.
        const MAX_PENDING: usize = 1_000;
        let (resp_tx, resp_rx) = oneshot::channel::<ApprovalResponse>();
        {
            let mut pending = self.pending.lock().await;
            if pending.len() >= MAX_PENDING {
                // Drop any senders whose receivers are already gone.
                pending.retain(|_, tx| !tx.is_closed());
                // If still over the limit, drop the map wholesale — these
                // gates are orphaned and the callers will observe a timeout.
                if pending.len() >= MAX_PENDING {
                    pending.clear();
                }
            }
            pending.insert(gate_id.clone(), resp_tx);
        }

        // Wait with timeout
        let timeout = std::time::Duration::from_secs(self.config.gate_timeout_secs);
        match tokio::time::timeout(timeout, resp_rx).await {
            Ok(Ok(response)) => {
                // Update gate status
                let status = match response.action {
                    ApprovalAction::Approve => GateStatus::Approved,
                    ApprovalAction::Reject => GateStatus::Rejected,
                    ApprovalAction::Modify => GateStatus::Modified,
                };

                if let Some(gate) = self.gates.lock().await.get_mut(&gate_id) {
                    gate.status = status;
                    gate.resolved_at = Some(Utc::now().to_rfc3339());
                    gate.resolved_by = Some("user".into());
                }

                if let Some(ref tx) = self.event_tx {
                    let _ = tx
                        .send(HitlEvent::GateResolved {
                            gate_id: gate_id.clone(),
                            status,
                        })
                        .await;
                }

                info!(gate_id = %gate_id, action = ?response.action, "Gate resolved");
                Ok(response)
            }
            Ok(Err(_)) => {
                // Channel closed (sender dropped)
                self.expire_gate(&gate_id).await;
                Err("Gate channel closed".into())
            }
            Err(_) => {
                // Timeout
                self.expire_gate(&gate_id).await;
                warn!(gate_id = %gate_id, "Gate timed out");
                Err(format!(
                    "Approval gate timed out after {}s",
                    self.config.gate_timeout_secs
                ))
            }
        }
    }

    /// Resolve a pending gate (called by the user via API).
    pub async fn resolve_gate(&self, response: ApprovalResponse) -> Result<(), String> {
        let mut pending = self.pending.lock().await;
        if let Some(tx) = pending.remove(&response.gate_id) {
            tx.send(response)
                .map_err(|_| "Gate already expired".to_string())?;
            Ok(())
        } else {
            Err(format!("No pending gate with id {}", response.gate_id))
        }
    }

    /// List all gates (active and historical).
    pub async fn list_gates(&self) -> Vec<ApprovalGate> {
        let gates = self.gates.lock().await;
        gates.values().cloned().collect()
    }

    /// List only pending gates.
    pub async fn pending_gates(&self) -> Vec<ApprovalGate> {
        let gates = self.gates.lock().await;
        gates
            .values()
            .filter(|g| g.status == GateStatus::Pending)
            .cloned()
            .collect()
    }

    /// Check if a specific action type needs approval based on config.
    pub fn needs_approval(&self, gate_type: &GateType, risk_level: &str) -> bool {
        // Destructive operations always require approval regardless of risk level
        if matches!(gate_type, GateType::Destructive) {
            return true;
        }

        if self.config.auto_approve.contains(&risk_level.to_string()) {
            return false;
        }

        match gate_type {
            GateType::FileWrite => self.config.pause_on_file_write,
            GateType::BashCommand => self.config.pause_on_bash,
            GateType::Deploy => self.config.pause_on_deploy,
            GateType::Destructive => unreachable!(),
            GateType::Custom => self.config.require_approval.contains(&risk_level.to_string()),
        }
    }

    async fn expire_gate(&self, gate_id: &str) {
        self.pending.lock().await.remove(gate_id);
        if let Some(gate) = self.gates.lock().await.get_mut(gate_id) {
            gate.status = GateStatus::Expired;
            gate.resolved_at = Some(Utc::now().to_rfc3339());
        }
    }
}

/// Store gate in the audit trail.
pub async fn store_gate_audit(
    db: &tokio::sync::Mutex<rusqlite::Connection>,
    project_id: &str,
    gate: &ApprovalGate,
) {
    let conn = db.lock().await;
    let id = uuid::Uuid::new_v4().to_string();
    let detail = serde_json::to_string(gate).unwrap_or_default();

    let _ = conn.execute(
        "INSERT INTO audit_entries (id, project_id, action, actor, detail, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            id,
            project_id,
            format!("hitl_{:?}", gate.gate_type).to_lowercase(),
            gate.resolved_by.as_deref().unwrap_or("pending"),
            detail,
            gate.created_at,
        ],
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn auto_approves_low_risk() {
        let controller = HitlController::with_defaults();
        let result = controller
            .request_approval(
                GateType::FileWrite,
                "Write index.ts".into(),
                "low".into(),
                GateContext {
                    files: vec!["index.ts".into()],
                    commands: vec![],
                    agent_output: None,
                    diff_preview: None,
                    metadata: HashMap::new(),
                },
            )
            .await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(matches!(response.action, ApprovalAction::Approve));
    }

    #[tokio::test]
    async fn requires_approval_for_destructive() {
        let controller = HitlController::with_defaults();
        assert!(controller.needs_approval(&GateType::Destructive, "high"));
        assert!(controller.needs_approval(&GateType::Destructive, "low"));
    }

    #[tokio::test]
    async fn resolve_pending_gate() {
        let (tx, mut rx) = mpsc::channel(10);
        let controller = Arc::new(HitlController::new(
            HitlConfig {
                auto_approve: vec![], // Nothing auto-approved
                gate_timeout_secs: 5,
                ..Default::default()
            },
            Some(tx),
        ));

        let controller_inner = controller.clone();

        // Spawn gate request
        let handle = tokio::spawn(async move {
            controller_inner
                .request_approval(
                    GateType::FileWrite,
                    "test".into(),
                    "high".into(),
                    GateContext {
                        files: vec!["test.ts".into()],
                        commands: vec![],
                        agent_output: None,
                        diff_preview: None,
                        metadata: HashMap::new(),
                    },
                )
                .await
        });

        // Wait for gate created event
        if let Some(HitlEvent::GateCreated { gate }) = rx.recv().await {
            // Resolve it
            controller
                .resolve_gate(ApprovalResponse {
                    gate_id: gate.id,
                    action: ApprovalAction::Approve,
                    modifications: None,
                    reason: Some("Looks good".into()),
                })
                .await
                .unwrap();
        }

        let result = handle.await.unwrap();
        assert!(result.is_ok());
        assert!(matches!(result.unwrap().action, ApprovalAction::Approve));
    }
}
