//! Log/error redaction for provider secrets.
//!
//! LLM providers frequently echo parts of the request in their error bodies
//! (OpenAI 401s include the offending `Authorization: Bearer sk-…` header).
//! Any string we forward into `tracing` or into a client-visible error is
//! first run through [`redact_secrets`] so we do not leak keys into the log
//! aggregator.

use std::borrow::Cow;
use std::sync::OnceLock;

use regex::Regex;

static SECRET_RE: OnceLock<Regex> = OnceLock::new();

fn secret_re() -> &'static Regex {
    SECRET_RE.get_or_init(|| {
        // Covers OpenAI (sk-..., sk-proj-...), Anthropic (sk-ant-...),
        // Nexus-issued API keys (nxk_...), AWS access keys (AKIA/ASIA),
        // Stripe live + restricted + webhook (sk_live_, rk_live_, whsec_),
        // GitHub PATs (ghp_, gho_, ghu_, ghs_, ghr_), Slack bot/user tokens
        // (xoxb-, xoxp-, xoxa-, xoxr-), and generic Bearer tokens of
        // reasonable length.
        Regex::new(
            r"(?i)\b(sk-ant-[a-z0-9_\-]{10,}|sk-proj-[a-z0-9_\-]{10,}|sk-[a-z0-9_\-]{10,}|nxk_[a-z0-9_\-]{10,}|sk_live_[a-z0-9]{10,}|sk_test_[a-z0-9]{10,}|rk_live_[a-z0-9]{10,}|whsec_[a-z0-9]{10,}|gh[pousr]_[a-z0-9]{20,}|github_pat_[a-z0-9_]{20,}|xox[baprs]-[a-z0-9\-]{10,}|AKIA[0-9A-Z]{16}|ASIA[0-9A-Z]{16}|Bearer\s+[A-Za-z0-9._\-]{20,})",
        )
        .expect("secret redaction regex must compile")
    })
}

/// Replace any credential-looking substring with `***redacted***`.
pub fn redact_secrets(input: &str) -> Cow<'_, str> {
    secret_re().replace_all(input, "***redacted***")
}

/// Convenience wrapper returning an owned String. Useful inside `format!`.
pub fn redacted(input: &str) -> String {
    redact_secrets(input).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_openai_key() {
        let s = redacted("API returned 401: { \"key\": \"sk-abcdef1234567890abcdef\" }");
        assert!(!s.contains("sk-abcdef"), "should have redacted: {s}");
        assert!(s.contains("***redacted***"));
    }

    #[test]
    fn redacts_anthropic_key() {
        let s = redacted("Authorization: Bearer sk-ant-api03-abcdef1234567890");
        assert!(!s.contains("sk-ant-api03"));
    }

    #[test]
    fn redacts_generic_bearer_token() {
        let s = redacted("Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.payloadstuffhere.signaturehere");
        assert!(s.contains("***redacted***"));
    }

    #[test]
    fn passes_through_benign_text() {
        let s = redacted("hello world, no secrets here");
        assert_eq!(s, "hello world, no secrets here");
    }

    #[test]
    fn redacts_stripe_live_key() {
        let s = redacted("STRIPE_SECRET_KEY=sk_live_abcdef1234567890ZZZ");
        assert!(!s.contains("sk_live_abcdef"), "got: {s}");
    }

    #[test]
    fn redacts_stripe_webhook_secret() {
        let s = redacted("whsec_abcdef1234567890");
        assert!(s.contains("***redacted***"));
    }

    #[test]
    fn redacts_github_pat() {
        let s = redacted("token=ghp_abcdefghijklmnopqrstuvwxyz0123456789");
        assert!(!s.contains("ghp_abcdef"), "got: {s}");
    }

    #[test]
    fn redacts_slack_bot_token() {
        let s = redacted("Authorization: xoxb-1234567890-abcdef-ghijklm");
        assert!(!s.contains("xoxb-1234567890"), "got: {s}");
    }
}
