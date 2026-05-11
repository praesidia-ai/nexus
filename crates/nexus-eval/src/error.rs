//! Error types for the evaluation framework.

/// All errors that can occur during evaluation.
#[derive(Debug, thiserror::Error)]
pub enum EvalError {
    /// A test case exceeded its allowed timeout.
    #[error("eval case `{case_id}` timed out after {timeout_secs}s")]
    Timeout {
        /// The case that timed out.
        case_id: String,
        /// The configured timeout in seconds.
        timeout_secs: u64,
    },

    /// A test case failed with a specific reason.
    #[error("eval case `{case_id}` failed: {reason}")]
    CaseFailed {
        /// The case that failed.
        case_id: String,
        /// Human-readable failure reason.
        reason: String,
    },

    /// The LLM judge could not produce a score.
    #[error("judge failed for case `{case_id}`: {reason}")]
    JudgeFailed {
        /// The case the judge was evaluating.
        case_id: String,
        /// What went wrong.
        reason: String,
    },

    /// A requested suite could not be found.
    #[error("suite not found: {suite_id}")]
    SuiteNotFound {
        /// The missing suite identifier.
        suite_id: String,
    },

    /// JSON serialization / deserialization error.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// I/O error (e.g. reading a suite file from disk).
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
