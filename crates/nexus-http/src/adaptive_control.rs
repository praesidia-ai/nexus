//! Adaptive Control Mode — adjusts pipeline behavior based on user preference.
//!
//! Three execution modes:
//! - **Fast**: Minimum agents, skip validation, optimistic writes
//! - **Balanced**: Default — right-sized agents, standard validation
//! - **Perfect**: Maximum agents, strict validation, post-build analysis
//!
//! The mode is wired into the pipeline at runtime and affects:
//! - How many coding agents run in parallel
//! - How many retry attempts per step
//! - How strict the validation gate is
//! - Whether post-build intelligence runs automatically

use serde::{Deserialize, Serialize};

use crate::intent_engine::Complexity;

// ---------------------------------------------------------------------------
// Execution Mode
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ExecutionMode {
    /// Fastest possible generation. Skip non-essential validation.
    Fast,
    /// Default balance of speed and quality.
    #[default]
    Balanced,
    /// Maximum quality — extra validation, post-build analysis, auto-improvements.
    Perfect,
}


// ---------------------------------------------------------------------------
// Pipeline Parameters — derived from mode
// ---------------------------------------------------------------------------

/// Concrete pipeline parameters computed from mode + intent complexity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineParams {
    /// How many coding agents can run in parallel.
    pub max_parallel_agents: usize,
    /// Maximum retries per pipeline step on failure.
    pub max_retries: u32,
    /// Whether to run the spec validation step.
    pub validate_spec: bool,
    /// Whether to run the full 4-layer validation gate.
    pub validate_all: bool,
    /// Whether to run taste scoring after generation.
    pub run_taste_scoring: bool,
    /// Whether to run post-build intelligence analysis.
    pub run_post_build_intel: bool,
    /// Whether to auto-apply safe improvements.
    pub auto_improve: bool,
    /// LLM temperature for generation steps (lower = more deterministic).
    pub llm_temperature: f32,
    /// Whether to generate skeleton previews for perceived speed.
    pub emit_skeletons: bool,
    /// Description of the mode for UI display.
    pub mode_description: String,
}

/// Compute pipeline parameters from mode and complexity.
pub fn compute_params(mode: ExecutionMode, complexity: &Complexity) -> PipelineParams {
    match mode {
        ExecutionMode::Fast => PipelineParams {
            max_parallel_agents: match complexity {
                Complexity::Simple => 1,
                Complexity::Medium => 2,
                Complexity::Complex => 3,
            },
            max_retries: 0,
            validate_spec: false,
            validate_all: false,
            run_taste_scoring: false,
            run_post_build_intel: false,
            auto_improve: false,
            llm_temperature: 0.3,
            emit_skeletons: true,
            mode_description: "Fast mode — prioritizing speed over polish".into(),
        },

        ExecutionMode::Balanced => PipelineParams {
            max_parallel_agents: match complexity {
                Complexity::Simple => 2,
                Complexity::Medium => 4,
                Complexity::Complex => 6,
            },
            max_retries: 1,
            validate_spec: true,
            validate_all: true,
            run_taste_scoring: true,
            run_post_build_intel: false,
            auto_improve: false,
            llm_temperature: 0.2,
            emit_skeletons: true,
            mode_description: "Balanced mode — good speed with quality checks".into(),
        },

        ExecutionMode::Perfect => PipelineParams {
            max_parallel_agents: match complexity {
                Complexity::Simple => 3,
                Complexity::Medium => 6,
                Complexity::Complex => 10,
            },
            max_retries: 2,
            validate_spec: true,
            validate_all: true,
            run_taste_scoring: true,
            run_post_build_intel: true,
            auto_improve: true,
            llm_temperature: 0.1,
            emit_skeletons: true,
            mode_description: "Perfect mode — maximum quality, thorough analysis".into(),
        },
    }
}

/// Suggest the best execution mode based on the user's description.
pub fn suggest_mode(description: &str, complexity: &Complexity) -> ExecutionMode {
    let lower = description.to_lowercase();

    // Explicit speed signals
    if lower.contains("quick") || lower.contains("fast") || lower.contains("prototype")
        || lower.contains("mvp") || lower.contains("rough")
    {
        return ExecutionMode::Fast;
    }

    // Explicit quality signals
    if lower.contains("production") || lower.contains("perfect") || lower.contains("polished")
        || lower.contains("enterprise") || lower.contains("client")
        || lower.contains("high quality") || lower.contains("premium")
    {
        return ExecutionMode::Perfect;
    }

    // Default: scale with complexity
    match complexity {
        Complexity::Simple | Complexity::Medium => ExecutionMode::Balanced,
        Complexity::Complex => ExecutionMode::Perfect,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_mode_skips_validation() {
        let params = compute_params(ExecutionMode::Fast, &Complexity::Medium);
        assert!(!params.validate_spec);
        assert!(!params.validate_all);
        assert!(!params.auto_improve);
        assert_eq!(params.max_retries, 0);
    }

    #[test]
    fn perfect_mode_enables_everything() {
        let params = compute_params(ExecutionMode::Perfect, &Complexity::Complex);
        assert!(params.validate_spec);
        assert!(params.validate_all);
        assert!(params.run_post_build_intel);
        assert!(params.auto_improve);
        assert_eq!(params.max_parallel_agents, 10);
    }

    #[test]
    fn suggest_mode_detects_speed_signals() {
        assert_eq!(
            suggest_mode("quick prototype of a todo app", &Complexity::Simple),
            ExecutionMode::Fast
        );
        assert_eq!(
            suggest_mode("production-ready CRM", &Complexity::Complex),
            ExecutionMode::Perfect
        );
    }
}
