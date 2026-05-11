//! Unified generation event schema for all generation paths.
//!
//! Both the modern oneshot flow and the legacy pipeline flow emit events
//! through this single enum, enabling a consistent SSE contract for the
//! frontend regardless of which orchestration path is active.

use std::collections::HashMap;

use serde::Serialize;

/// Unified event type emitted during any generation flow.
///
/// Both oneshot and pipeline events are unified under this enum.
/// The frontend can subscribe to a single SSE stream and handle all
/// variants regardless of the generation path that produced them.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GenerationEvent {
    // ---- Progress ----
    /// A named phase of the generation has started.
    PhaseStarted {
        phase: String,
        description: String,
    },

    /// Incremental progress within a phase (0.0 .. 1.0).
    PhaseProgress {
        phase: String,
        progress: f32,
        detail: String,
    },

    /// A phase completed successfully.
    PhaseCompleted {
        phase: String,
        duration_ms: u64,
    },

    // ---- Intelligence ----
    /// Result of deterministic intent analysis.
    IntentAnalyzed {
        app_type: String,
        complexity: String,
        domain: String,
        needs_auth: bool,
        needs_database: bool,
    },

    /// Architecture decisions (frontend stack, DB, auth strategy, learning overrides).
    DecisionsMade {
        frontend: String,
        database: String,
        auth: String,
        learning_overrides: Vec<String>,
    },

    /// Product brief generated for the application.
    BriefGenerated {
        domain: String,
        hero_headline: String,
        personas: usize,
        features: usize,
    },

    // ---- Generation ----
    /// A file has been proposed (before commit to disk).
    FileProposed {
        path: String,
        size: usize,
        skeleton: bool,
    },

    /// A file has been fully generated and committed.
    FileGenerated {
        path: String,
        lines: usize,
    },

    /// The application spec has been generated.
    SpecGenerated {
        page_count: usize,
        entity_count: usize,
    },

    // ---- Quality ----
    /// Taste engine scored the generated output.
    TasteScored {
        score: f32,
        axes: HashMap<String, f32>,
        redesign_triggered: bool,
    },

    /// A redesign pass was applied to improve taste score.
    RedesignApplied {
        mutations_applied: usize,
        score_before: f32,
        score_after: f32,
    },

    /// Outcome guarantee evaluation result.
    GuaranteeResult {
        passed: bool,
        certificate_id: Option<String>,
        cycles_used: u32,
    },

    // ---- Thinking / UX ----
    /// Real-time thinking message for the frontend progress display.
    Thinking {
        message: String,
        detail: Option<String>,
        icon: String,
        progress: u32,
    },

    /// Explanation of an architectural or design decision.
    Explanation {
        decision: String,
        reason: String,
        confidence: f64,
        alternatives: Vec<String>,
    },

    /// Skeleton file for optimistic/instant preview rendering.
    Skeleton {
        path: String,
        content: String,
        skeleton_type: String,
    },

    /// Estimated total generation time.
    Estimate {
        total_estimated_ms: u64,
        confidence: f64,
    },

    /// Periodic heartbeat so the frontend knows we are alive.
    Heartbeat {
        phase: String,
        elapsed_ms: u64,
        message: String,
    },

    // ---- Validation ----
    /// Result of a validation layer (static, llm, policy, runtime, content).
    ValidationResult {
        layer: String,
        passed: bool,
        issues: Vec<String>,
    },

    // ---- Completion ----
    /// Generation completed successfully.
    Completed {
        project_id: String,
        project_name: String,
        taste_score: f32,
        files_count: usize,
        duration_ms: u64,
        app_url: Option<String>,
    },

    /// An error occurred during generation.
    Error {
        phase: String,
        message: String,
        recoverable: bool,
    },
}

/// Map a [`GenerationEvent`] to its SSE event type name string.
///
/// This is used when constructing SSE `Event` frames so the frontend
/// can dispatch on `event.type` without parsing the JSON body first.
pub fn event_type_name(event: &GenerationEvent) -> &'static str {
    match event {
        GenerationEvent::PhaseStarted { .. } => "phase_started",
        GenerationEvent::PhaseProgress { .. } => "phase_progress",
        GenerationEvent::PhaseCompleted { .. } => "phase_completed",
        GenerationEvent::IntentAnalyzed { .. } => "intent_analyzed",
        GenerationEvent::DecisionsMade { .. } => "decisions_made",
        GenerationEvent::BriefGenerated { .. } => "brief_generated",
        GenerationEvent::FileProposed { .. } => "file_proposed",
        GenerationEvent::FileGenerated { .. } => "file_generated",
        GenerationEvent::SpecGenerated { .. } => "spec_generated",
        GenerationEvent::TasteScored { .. } => "taste_scored",
        GenerationEvent::RedesignApplied { .. } => "redesign_applied",
        GenerationEvent::GuaranteeResult { .. } => "guarantee_result",
        GenerationEvent::Thinking { .. } => "thinking",
        GenerationEvent::Explanation { .. } => "explanation",
        GenerationEvent::Skeleton { .. } => "skeleton",
        GenerationEvent::Estimate { .. } => "estimate",
        GenerationEvent::Heartbeat { .. } => "heartbeat",
        GenerationEvent::ValidationResult { .. } => "validation",
        GenerationEvent::Completed { .. } => "complete",
        GenerationEvent::Error { .. } => "error",
    }
}

#[cfg(test)]
#[path = "generation_event_tests.rs"]
mod tests;
