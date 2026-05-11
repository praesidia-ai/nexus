//! User-input length caps — prevents prompt-size bomb / LLM-cost DoS.
//!
//! Every LLM-reaching string we accept from a user must pass through one
//! of the `require_*` helpers so a hostile client can't submit 10 MB of
//! description text and multiply it across every subsequent LLM call.
//!
//! Limits are intentionally generous: any realistic product description or
//! chat message fits well under 32 KB.

use crate::error::ApiError;

/// Max bytes for a single chat message.
pub const MAX_CHAT_MESSAGE_BYTES: usize = 32 * 1024;

/// Max bytes for a generation / oneshot description.
pub const MAX_DESCRIPTION_BYTES: usize = 32 * 1024;

/// Max bytes for an agent task prompt.
pub const MAX_AGENT_TASK_BYTES: usize = 64 * 1024;

/// Max bytes for a mutation instruction.
pub const MAX_MUTATION_INSTRUCTION_BYTES: usize = 16 * 1024;

/// Max bytes for a project name.
pub const MAX_PROJECT_NAME_BYTES: usize = 512;

/// Max bytes for a single generated-app source file write.
/// 2 MiB accommodates any hand-written component file while preventing a
/// malicious client from submitting hundreds of MB of JSON at once.
pub const MAX_FILE_WRITE_BYTES: usize = 2 * 1024 * 1024;

/// Max bytes for a vault key. Vault keys are env-var-style identifiers; 256
/// bytes is plenty and rejects pathological inputs.
pub const MAX_VAULT_KEY_BYTES: usize = 256;

/// Max bytes for a vault value. The body limit is 10 MiB, but a single
/// tenant should not be able to fill the database (or audit log) with one
/// 10-MiB secret; 256 KiB is more than enough for any realistic credential
/// (TLS cert, JSON service-account, etc.).
pub const MAX_VAULT_VALUE_BYTES: usize = 256 * 1024;

/// Require a non-empty, bounded user input. Rejects empty strings with a
/// clear error and over-long inputs with the limit embedded in the message
/// so the UI can surface actionable feedback.
pub fn require_bounded(field: &str, value: &str, max_bytes: usize) -> Result<(), ApiError> {
    if value.trim().is_empty() {
        return Err(ApiError::BadRequest(format!("{field} is required")));
    }
    if value.len() > max_bytes {
        return Err(ApiError::BadRequest(format!(
            "{field} exceeds {max_bytes} bytes (got {})",
            value.len()
        )));
    }
    Ok(())
}

/// Like `require_bounded` but allows empty (e.g. optional description).
pub fn require_bounded_opt(
    field: &str,
    value: Option<&str>,
    max_bytes: usize,
) -> Result<(), ApiError> {
    if let Some(v) = value {
        if v.len() > max_bytes {
            return Err(ApiError::BadRequest(format!(
                "{field} exceeds {max_bytes} bytes (got {})",
                v.len()
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty() {
        let err = require_bounded("msg", "", 100).unwrap_err();
        assert!(matches!(err, ApiError::BadRequest(_)));
    }

    #[test]
    fn rejects_whitespace_only() {
        let err = require_bounded("msg", "   \n\t ", 100).unwrap_err();
        assert!(matches!(err, ApiError::BadRequest(_)));
    }

    #[test]
    fn rejects_over_limit() {
        let long = "a".repeat(101);
        let err = require_bounded("msg", &long, 100).unwrap_err();
        if let ApiError::BadRequest(s) = err {
            assert!(s.contains("exceeds 100 bytes"));
        } else {
            panic!("expected BadRequest");
        }
    }

    #[test]
    fn accepts_exactly_at_limit() {
        let at_limit = "a".repeat(100);
        assert!(require_bounded("msg", &at_limit, 100).is_ok());
    }

    #[test]
    fn opt_accepts_none() {
        assert!(require_bounded_opt("msg", None, 100).is_ok());
    }
}
