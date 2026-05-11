//! Explainability Engine — every major decision produces a traceable explanation.
//!
//! Attaches to:
//! - `decision_engine` (architecture choices)
//! - `execution_pipeline` (step progression)
//! - agent outputs (coding agents, wave pipeline)
//!
//! Every `Explanation` contains: decision, reason, confidence, alternatives.
//! Explanations are collected during a pipeline run and streamed via SSE.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::decision_engine::ArchitectureDecision;
use crate::intent_engine::FlatIntent;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// A single explainable decision made by Nexus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Explanation {
    /// Machine-readable decision identifier (e.g., "frontend_choice").
    pub decision_id: String,
    /// Human-readable category (e.g., "Architecture", "Design", "Agent").
    pub category: ExplainCategory,
    /// What was decided.
    pub decision: String,
    /// Why it was decided — plain language, no jargon.
    pub reason: String,
    /// 0.0–1.0 confidence in this decision.
    pub confidence: f64,
    /// What else could have been chosen (with reasons why not).
    pub alternatives: Vec<Alternative>,
    /// Which pipeline stage produced this.
    pub source: String,
    /// Timestamp (ms since pipeline start).
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExplainCategory {
    Architecture,
    Design,
    Agent,
    Content,
    Validation,
    Optimization,
    Improvement,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alternative {
    pub choice: String,
    pub reason_rejected: String,
}

/// A full explanation trace for one pipeline run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplainTrace {
    pub project_id: String,
    pub explanations: Vec<Explanation>,
    pub summary: String,
    pub total_decisions: usize,
    pub avg_confidence: f64,
}

// ---------------------------------------------------------------------------
// Collector — accumulates explanations during a pipeline run
// ---------------------------------------------------------------------------

/// Thread-safe collector that lives for the duration of one pipeline execution.
#[derive(Clone)]
pub struct ExplainCollector {
    inner: Arc<Mutex<CollectorInner>>,
}

struct CollectorInner {
    project_id: String,
    explanations: Vec<Explanation>,
    start: std::time::Instant,
}

impl ExplainCollector {
    pub fn new(project_id: &str) -> Self {
        Self {
            inner: Arc::new(Mutex::new(CollectorInner {
                project_id: project_id.to_string(),
                explanations: Vec::new(),
                start: std::time::Instant::now(),
            })),
        }
    }

    /// Record a decision with full context.
    #[allow(clippy::too_many_arguments)]
    pub async fn record(
        &self,
        decision_id: &str,
        category: ExplainCategory,
        decision: &str,
        reason: &str,
        confidence: f64,
        alternatives: Vec<Alternative>,
        source: &str,
    ) {
        let mut inner = self.inner.lock().await;
        let elapsed = inner.start.elapsed().as_millis() as u64;
        inner.explanations.push(Explanation {
            decision_id: decision_id.to_string(),
            category,
            decision: decision.to_string(),
            reason: reason.to_string(),
            confidence,
            alternatives,
            source: source.to_string(),
            elapsed_ms: elapsed,
        });
    }

    /// Convenience: record a simple decision (no alternatives).
    pub async fn record_simple(
        &self,
        decision_id: &str,
        category: ExplainCategory,
        decision: &str,
        reason: &str,
        confidence: f64,
        source: &str,
    ) {
        self.record(decision_id, category, decision, reason, confidence, vec![], source)
            .await;
    }

    /// Finalize and produce the complete trace.
    pub async fn finalize(&self) -> ExplainTrace {
        let inner = self.inner.lock().await;
        let total = inner.explanations.len();
        let avg_conf = if total > 0 {
            inner.explanations.iter().map(|e| e.confidence).sum::<f64>() / total as f64
        } else {
            0.0
        };

        let summary = build_summary(&inner.explanations, avg_conf);

        ExplainTrace {
            project_id: inner.project_id.clone(),
            explanations: inner.explanations.clone(),
            summary,
            total_decisions: total,
            avg_confidence: avg_conf,
        }
    }

    /// Get current explanation count.
    pub async fn count(&self) -> usize {
        self.inner.lock().await.explanations.len()
    }
}

// ---------------------------------------------------------------------------
// Architecture decision → explanations
// ---------------------------------------------------------------------------

/// Convert an `ArchitectureDecision` into a set of `Explanation`s.
pub fn explain_architecture(
    decision: &ArchitectureDecision,
    intent: &FlatIntent,
) -> Vec<Explanation> {
    let mut explanations = Vec::new();

    // Frontend
    explanations.push(Explanation {
        decision_id: "frontend_choice".into(),
        category: ExplainCategory::Architecture,
        decision: format!("Using {:?} for the frontend", decision.frontend),
        reason: decision
            .rationale
            .iter()
            .find(|r| r.area == "frontend")
            .map(|r| r.reason.clone())
            .unwrap_or_else(|| "Best fit for this app type".into()),
        confidence: 0.9,
        alternatives: frontend_alternatives(decision),
        source: "decision_engine".into(),
        elapsed_ms: 0,
    });

    // Backend
    explanations.push(Explanation {
        decision_id: "backend_choice".into(),
        category: ExplainCategory::Architecture,
        decision: format!("Using {:?} for the backend", decision.backend),
        reason: decision
            .rationale
            .iter()
            .find(|r| r.area == "backend")
            .map(|r| r.reason.clone())
            .unwrap_or_else(|| "Optimal for the complexity level".into()),
        confidence: 0.85,
        alternatives: backend_alternatives(decision),
        source: "decision_engine".into(),
        elapsed_ms: 0,
    });

    // Database
    explanations.push(Explanation {
        decision_id: "database_choice".into(),
        category: ExplainCategory::Architecture,
        decision: format!("Using {:?} for data storage", decision.database),
        reason: decision
            .rationale
            .iter()
            .find(|r| r.area == "database")
            .map(|r| r.reason.clone())
            .unwrap_or_else(|| "Right-sized for this application".into()),
        confidence: if intent.needs_database { 0.9 } else { 0.95 },
        alternatives: database_alternatives(decision, intent),
        source: "decision_engine".into(),
        elapsed_ms: 0,
    });

    // Auth
    if intent.needs_auth {
        explanations.push(Explanation {
            decision_id: "auth_choice".into(),
            category: ExplainCategory::Architecture,
            decision: format!("Using {:?} for authentication", decision.auth),
            reason: decision
                .rationale
                .iter()
                .find(|r| r.area == "auth")
                .map(|r| r.reason.clone())
                .unwrap_or_else(|| "Matches the security requirements".into()),
            confidence: 0.85,
            alternatives: vec![],
            source: "decision_engine".into(),
            elapsed_ms: 0,
        });
    }

    // UI Library
    explanations.push(Explanation {
        decision_id: "ui_library_choice".into(),
        category: ExplainCategory::Design,
        decision: format!("Using {:?} for UI components", decision.ui_library),
        reason: decision
            .rationale
            .iter()
            .find(|r| r.area == "ui_library")
            .map(|r| r.reason.clone())
            .unwrap_or_else(|| "Best design quality for this type".into()),
        confidence: 0.8,
        alternatives: vec![],
        source: "decision_engine".into(),
        elapsed_ms: 0,
    });

    explanations
}

// ---------------------------------------------------------------------------
// Helpers — generate alternatives
// ---------------------------------------------------------------------------

fn frontend_alternatives(decision: &ArchitectureDecision) -> Vec<Alternative> {
    use crate::decision_engine::FrontendChoice;
    let mut alts = Vec::new();
    if decision.frontend != FrontendChoice::NextjsAppRouter {
        alts.push(Alternative {
            choice: "Next.js App Router".into(),
            reason_rejected: "Over-engineered for this use case".into(),
        });
    }
    if decision.frontend != FrontendChoice::StaticHtml {
        alts.push(Alternative {
            choice: "Static HTML".into(),
            reason_rejected: "Insufficient for interactive features".into(),
        });
    }
    if decision.frontend != FrontendChoice::ReactSpa {
        alts.push(Alternative {
            choice: "React SPA".into(),
            reason_rejected: "Loses SEO and server-rendering benefits".into(),
        });
    }
    alts
}

fn backend_alternatives(decision: &ArchitectureDecision) -> Vec<Alternative> {
    use crate::decision_engine::BackendChoice;
    let mut alts = Vec::new();
    if decision.backend != BackendChoice::None {
        alts.push(Alternative {
            choice: "No backend".into(),
            reason_rejected: "App requires server-side logic or data persistence".into(),
        });
    }
    if decision.backend != BackendChoice::StandaloneExpress {
        alts.push(Alternative {
            choice: "Standalone Express server".into(),
            reason_rejected: "Adds deployment complexity without clear benefit".into(),
        });
    }
    alts
}

fn database_alternatives(
    decision: &ArchitectureDecision,
    intent: &FlatIntent,
) -> Vec<Alternative> {
    use crate::decision_engine::DatabaseChoice;
    let mut alts = Vec::new();
    if intent.needs_database {
        if decision.database != DatabaseChoice::Sqlite {
            alts.push(Alternative {
                choice: "SQLite".into(),
                reason_rejected: "Too limited for multi-user or complex data needs".into(),
            });
        }
        if decision.database != DatabaseChoice::Postgres {
            alts.push(Alternative {
                choice: "PostgreSQL".into(),
                reason_rejected: "Over-provisioned for this complexity level".into(),
            });
        }
    }
    alts
}

// ---------------------------------------------------------------------------
// Summary builder
// ---------------------------------------------------------------------------

fn build_summary(explanations: &[Explanation], avg_conf: f64) -> String {
    if explanations.is_empty() {
        return "No decisions recorded.".into();
    }

    let arch_count = explanations
        .iter()
        .filter(|e| matches!(e.category, ExplainCategory::Architecture))
        .count();
    let design_count = explanations
        .iter()
        .filter(|e| matches!(e.category, ExplainCategory::Design))
        .count();
    let agent_count = explanations
        .iter()
        .filter(|e| matches!(e.category, ExplainCategory::Agent))
        .count();

    format!(
        "Made {} decisions ({} architecture, {} design, {} agent) with {:.0}% average confidence.",
        explanations.len(),
        arch_count,
        design_count,
        agent_count,
        avg_conf * 100.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decision_engine::decide;
    use crate::intent_engine::analyze_flat;

    #[test]
    fn explains_architecture_decisions() {
        let intent = analyze_flat("Build a CRM with auth and billing");
        let decision = decide(&intent);
        let explanations = explain_architecture(&decision, &intent);
        assert!(!explanations.is_empty());
        assert!(explanations.iter().all(|e| e.confidence > 0.0));
        assert!(explanations.iter().all(|e| !e.decision.is_empty()));
    }

    #[tokio::test]
    async fn collector_tracks_decisions() {
        let collector = ExplainCollector::new("test-project");
        collector
            .record_simple(
                "test_decision",
                ExplainCategory::Architecture,
                "Use Rust",
                "Performance matters",
                0.95,
                "test",
            )
            .await;

        let trace = collector.finalize().await;
        assert_eq!(trace.total_decisions, 1);
        assert!((trace.avg_confidence - 0.95).abs() < f64::EPSILON);
    }
}
