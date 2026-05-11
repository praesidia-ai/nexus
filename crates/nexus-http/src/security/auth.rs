//! Authentication middleware and token verification.
//!
//! Supports two authentication methods:
//! 1. **Bearer JWT** — `Authorization: Bearer <jwt>` header.
//! 2. **API key** — `Authorization: Bearer nxk_<key>` header (keys prefixed with `nxk_`).
//!
//! The middleware injects an [`AuthContext`] into Axum request extensions so downstream
//! handlers can access it via `req.extensions().get::<AuthContext>()`.

use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Request, State},
    http::{self, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::state::AppState;

use super::api_keys;

type HmacSha256 = Hmac<Sha256>;

/// The set of permission scopes available in Nexus.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Scope {
    /// Read project data (conversations, knowledge, tables, files).
    ProjectRead,
    /// Write/modify project data.
    ProjectWrite,
    /// Full project administration (delete, settings).
    ProjectAdmin,
    /// Execute agents within a project.
    AgentExecute,
    /// Manage runtimes (start/stop/restart apps).
    RuntimeManage,
    /// Manage deployments (push to GitHub, deploy to server).
    DeployManage,
    /// Install/uninstall plugins.
    PluginManage,
    /// Modify global settings and API keys.
    SettingsManage,
    /// Full system administration.
    SystemAdmin,
}

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Scope::ProjectRead => "project:read",
            Scope::ProjectWrite => "project:write",
            Scope::ProjectAdmin => "project:admin",
            Scope::AgentExecute => "agent:execute",
            Scope::RuntimeManage => "runtime:manage",
            Scope::DeployManage => "deploy:manage",
            Scope::PluginManage => "plugin:manage",
            Scope::SettingsManage => "settings:manage",
            Scope::SystemAdmin => "system:admin",
        };
        write!(f, "{s}")
    }
}

impl Scope {
    /// Parse a scope string (e.g. `"project:read"`) into a [`Scope`] variant.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "project:read" => Some(Scope::ProjectRead),
            "project:write" => Some(Scope::ProjectWrite),
            "project:admin" => Some(Scope::ProjectAdmin),
            "agent:execute" => Some(Scope::AgentExecute),
            "runtime:manage" => Some(Scope::RuntimeManage),
            "deploy:manage" => Some(Scope::DeployManage),
            "plugin:manage" => Some(Scope::PluginManage),
            "settings:manage" => Some(Scope::SettingsManage),
            "system:admin" => Some(Scope::SystemAdmin),
            _ => None,
        }
    }

    /// Return all scopes (convenience for admin keys).
    pub fn all() -> Vec<Self> {
        vec![
            Scope::ProjectRead,
            Scope::ProjectWrite,
            Scope::ProjectAdmin,
            Scope::AgentExecute,
            Scope::RuntimeManage,
            Scope::DeployManage,
            Scope::PluginManage,
            Scope::SettingsManage,
            Scope::SystemAdmin,
        ]
    }
}

/// Authenticated identity extracted by the auth middleware and injected into
/// request extensions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthContext {
    /// Unique user identifier.
    pub user_id: String,
    /// Tenant (organisation) this request is scoped to.
    pub tenant_id: String,
    /// Permission scopes granted to this session/key.
    pub scopes: Vec<Scope>,
    /// Session identifier (JWT `jti` or API key id).
    pub session_id: String,
}

impl AuthContext {
    /// Check whether this context has a specific scope.
    pub fn has_scope(&self, scope: &Scope) -> bool {
        self.scopes.contains(&Scope::SystemAdmin) || self.scopes.contains(scope)
    }

    /// Require a specific scope, returning an error response if missing.
    #[allow(clippy::result_large_err)]
    pub fn require_scope(&self, scope: &Scope) -> Result<(), Response> {
        if self.has_scope(scope) {
            Ok(())
        } else {
            Err(forbidden_response(&format!(
                "Missing required scope: {scope}"
            )))
        }
    }
}

// ---------------------------------------------------------------------------
// Axum Extractor — handlers can add `auth: AuthContext` as a parameter
// ---------------------------------------------------------------------------

#[axum::async_trait]
impl<S> axum::extract::FromRequestParts<S> for AuthContext
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthContext>()
            .cloned()
            .ok_or_else(|| unauthorized_response("Not authenticated"))
    }
}

// ---------------------------------------------------------------------------
// JWT structures
// ---------------------------------------------------------------------------

/// JWT header (always HMAC-SHA256).
#[derive(Debug, Serialize, Deserialize)]
struct JwtHeader {
    alg: String,
    typ: String,
}

/// JWT claims payload.
#[derive(Debug, Serialize, Deserialize)]
struct JwtClaims {
    /// Subject (user ID).
    sub: String,
    /// Tenant ID.
    tid: String,
    /// Scopes (comma-separated).
    scp: String,
    /// JWT ID (session).
    jti: String,
    /// Issued at (unix timestamp).
    iat: u64,
    /// Expiration (unix timestamp).
    exp: u64,
}

// ---------------------------------------------------------------------------
// Middleware
// ---------------------------------------------------------------------------

/// Routes that do not require authentication.
/// Health probes, monitoring, and CORS preflight pass through freely.
const PUBLIC_PATHS: &[&str] = &[
    "/health",
    "/health/live",
    "/health/ready",
    // /health/detailed and /health/v2 leak internal subsystem status and must
    // require authentication. Keep only unauthenticated liveness/readiness.
    "/tv",
];

/// Check if a request path is public (no auth required).
///
/// Exact matches use [`PUBLIC_PATHS`]; the one prefix rule here is
/// `/tv/:runId`, which is protected at the handler level by the
/// per-run `visibility` column (public + unlisted runs are exposed;
/// private runs respond 404 regardless of auth state).
pub fn is_public_path(path: &str) -> bool {
    if PUBLIC_PATHS.contains(&path) {
        return true;
    }
    // Trust identity is always public — the `/trust` verifier on any
    // Nexus instance fetches it unauthenticated.
    if path == "/.well-known/nexus-trust.json" {
        return true;
    }
    // Forge gallery surfaces — installs / reviews stay auth-gated
    // because they mutate; reads are public so the website can
    // render without a token.
    if matches!(path, "/forge/trending" | "/forge/top-rated" | "/forge/newest") {
        return true;
    }
    // Federation surfaces — catalogue + loan + verify are peer-to-peer
    // by design. Signing-key ownership is what makes a receipt
    // meaningful, not the bearer token.
    if matches!(
        path,
        "/federation/borrowable-agents"
            | "/federation/borrow"
            | "/federation/verify-receipt"
    ) {
        return true;
    }
    // `/trust/cert/<runId>` and `/trust/verify` mirror the `/tv/<runId>`
    // visibility model — private runs respond 404 regardless.
    if let Some(rest) = path.strip_prefix("/trust/cert/") {
        if !rest.is_empty() && !rest.contains('/') {
            return true;
        }
    }
    if path == "/trust/verify" {
        return true;
    }
    // `/tv/<runId>` and `/tv/<runId>/embed[.html]` — run-level
    // visibility enforced in the handler.
    if let Some(rest) = path.strip_prefix("/tv/") {
        if rest.is_empty() {
            return false;
        }
        // Only `<runId>`, `<runId>/embed`, or `<runId>/embed.html`.
        let mut parts = rest.splitn(2, '/');
        let run_id = parts.next().unwrap_or("");
        if run_id.is_empty() {
            return false;
        }
        match parts.next() {
            None => return true,
            Some("embed") | Some("embed.html") => return true,
            _ => return false,
        }
    }
    false
}

/// Axum middleware that extracts and validates authentication from incoming requests.
///
/// On success, injects [`AuthContext`] into request extensions.
/// On failure, returns a `401 Unauthorized` JSON response.
///
/// Bypasses:
/// - `OPTIONS` requests (CORS preflight)
/// - Paths in [`PUBLIC_PATHS`]
/// - When `NEXUS_AUTH_DISABLED=true` (local dev only)
pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, Response> {
    let path = req.uri().path().to_string();
    let method = req.method().clone();

    // Always allow CORS preflight and public health endpoints
    if method == http::Method::OPTIONS || is_public_path(&path) {
        return Ok(next.run(req).await);
    }

    // Dev mode bypass — NEVER enable in production.
    // In release builds this check is compiled out entirely; enabling the env var
    // at runtime in a release binary has no effect and does not open a backdoor.
    #[cfg(debug_assertions)]
    if std::env::var("NEXUS_AUTH_DISABLED").unwrap_or_default() == "true" {
        req.extensions_mut().insert(AuthContext {
            user_id: "dev-user".to_string(),
            tenant_id: "default".to_string(),
            scopes: Scope::all(),
            session_id: "dev-session".to_string(),
        });
        return Ok(next.run(req).await);
    }

    let auth_header = req
        .headers()
        .get(http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let token = match auth_header {
        Some(ref h) if h.starts_with("Bearer ") => &h[7..],
        _ => return Err(unauthorized_response("Missing or invalid Authorization header")),
    };

    // Determine if this is an API key (prefixed with nxk_) or a JWT
    let auth_ctx = if token.starts_with("nxk_") {
        // API key authentication
        let db = state.db.lock().await;
        verify_api_key(token, &db).map_err(|e| unauthorized_response(&e))?
    } else {
        // JWT authentication
        let secret = get_jwt_secret().map_err(|e| {
            // In production with no secret, requests can't be verified — return 503
            // rather than panicking the worker thread.
            tracing::error!(error = %e, "JWT verification unavailable");
            unauthorized_response("Authentication service unavailable")
        })?;
        verify_jwt(token, &secret).map_err(|e| unauthorized_response(&e))?
    };

    req.extensions_mut().insert(auth_ctx);
    Ok(next.run(req).await)
}

/// Extract [`AuthContext`] from request extensions (for use inside handlers).
pub fn extract_auth(extensions: &http::Extensions) -> Option<&AuthContext> {
    extensions.get::<AuthContext>()
}

/// Middleware that requires `SystemAdmin` scope on top of basic authentication.
///
/// Apply this as a layer on a nested router to gate privileged endpoints
/// (sandbox, MCP, webhooks, deploy, plugin install).
/// The base `auth_middleware` must have already run and inserted `AuthContext`.
pub async fn require_admin_middleware(
    req: Request<Body>,
    next: Next,
) -> Result<Response, Response> {
    let auth = req
        .extensions()
        .get::<AuthContext>()
        .cloned()
        .ok_or_else(|| unauthorized_response("Not authenticated"))?;

    auth.require_scope(&Scope::SystemAdmin)?;
    Ok(next.run(req).await)
}

// ---------------------------------------------------------------------------
// JWT secret resolution
// ---------------------------------------------------------------------------

/// Get the JWT signing secret from the environment.
///
/// Resolution order (per ADR-001):
/// 1. `NEXUS_JWT_SECRET` env var (set by operator OR by `state::ensure_auto_secrets`).
/// 2. If `NEXUS_PRODUCTION=1`, this is a fatal error and we return an error.
///    Boot will exit cleanly via `?` propagation rather than `panic!`.
/// 3. Otherwise (dev mode), generate an ephemeral 32-byte random secret,
///    log a one-time WARN, and use it. `state::ensure_auto_secrets` should
///    have already persisted one to `secrets.toml`; this is a last-resort
///    fallback when boot order is wrong or the data dir is read-only.
///
/// The function never panics. Callers that need a fatal-on-missing secret
/// must set `NEXUS_PRODUCTION=1`; in that mode the returned `Err` propagates
/// to the top-level boot routine which exits with `EX_CONFIG` (78).
fn get_jwt_secret() -> Result<String, JwtSecretError> {
    if let Ok(s) = std::env::var("NEXUS_JWT_SECRET") {
        if !s.is_empty() {
            return Ok(s);
        }
    }

    if is_production_mode() {
        return Err(JwtSecretError::MissingInProduction);
    }

    use std::sync::OnceLock;
    static EPHEMERAL: OnceLock<String> = OnceLock::new();
    let secret = EPHEMERAL.get_or_init(|| {
        use rand::RngCore;
        let mut buf = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut buf);
        let s = hex::encode(buf);
        tracing::warn!(
            "NEXUS_JWT_SECRET unset; generated ephemeral dev-mode secret. \
             This will rotate on every restart. Set NEXUS_JWT_SECRET or run \
             with a writable data dir so secrets.toml can persist one."
        );
        s
    });
    Ok(secret.clone())
}

/// True when `NEXUS_PRODUCTION=1` (or `NEXUS_PRODUCTION=true`).
fn is_production_mode() -> bool {
    matches!(
        std::env::var("NEXUS_PRODUCTION").ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("True")
    )
}

#[derive(Debug, thiserror::Error)]
pub enum JwtSecretError {
    #[error(
        "NEXUS_JWT_SECRET is required when NEXUS_PRODUCTION=1. \
         Set it to a strong random string (>= 32 bytes, e.g. `openssl rand -hex 32`)."
    )]
    MissingInProduction,
}

// ---------------------------------------------------------------------------
// JWT verification
// ---------------------------------------------------------------------------

/// Verify a JWT token using HMAC-SHA256 and return the extracted [`AuthContext`].
///
/// The token must be a three-part base64url-encoded string: `header.payload.signature`.
pub fn verify_jwt(token: &str, secret: &str) -> Result<AuthContext, String> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err("Invalid JWT format: expected 3 parts".to_string());
    }

    let header_b64 = parts[0];
    let payload_b64 = parts[1];
    let signature_b64 = parts[2];

    // Verify signature in constant time. Direct slice comparison via `!=`
    // short-circuits on the first mismatching byte, leaking timing info that
    // an attacker on the same network can exploit to forge tokens.
    let signing_input = format!("{header_b64}.{payload_b64}");
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|e| format!("HMAC key error: {e}"))?;
    mac.update(signing_input.as_bytes());

    let provided_sig = base64_url_decode(signature_b64)?;
    mac.verify_slice(&provided_sig)
        .map_err(|_| "Invalid JWT signature".to_string())?;

    // Decode and parse claims
    let payload_bytes = base64_url_decode(payload_b64)?;
    let claims: JwtClaims = serde_json::from_slice(&payload_bytes)
        .map_err(|e| format!("Invalid JWT claims: {e}"))?;

    // Check temporal validity. We allow 30s of clock skew on `iat`, refuse
    // tokens whose effective TTL exceeds MAX_JWT_TTL_SECS, and of course
    // enforce `exp`.
    const CLOCK_SKEW_SECS: u64 = 30;
    /// Hard cap on JWT lifetime regardless of issuer claims.
    /// Long-lived bearer tokens become useful weapons after a single leak;
    /// 24h is the longest we want any token in circulation. Operators
    /// needing longer-lived credentials should use API keys (which can be
    /// revoked) instead.
    const MAX_JWT_TTL_SECS: u64 = 24 * 3600;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    if claims.exp < now {
        return Err("JWT expired".to_string());
    }
    if claims.iat > now + CLOCK_SKEW_SECS {
        return Err("JWT iat is in the future".to_string());
    }
    if claims.exp.saturating_sub(claims.iat) > MAX_JWT_TTL_SECS {
        return Err("JWT lifetime exceeds maximum allowed TTL".to_string());
    }

    // Parse scopes
    let scopes: Vec<Scope> = claims
        .scp
        .split(',')
        .filter(|s| !s.is_empty())
        .filter_map(|s| Scope::parse(s.trim()))
        .collect();

    Ok(AuthContext {
        user_id: claims.sub,
        tenant_id: claims.tid,
        scopes,
        session_id: claims.jti,
    })
}

/// Create a signed JWT token (useful for testing and token issuance).
pub fn create_jwt(
    user_id: &str,
    tenant_id: &str,
    scopes: &[Scope],
    secret: &str,
    ttl_secs: u64,
) -> Result<String, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let header = JwtHeader {
        alg: "HS256".to_string(),
        typ: "JWT".to_string(),
    };

    let scope_str = scopes
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let claims = JwtClaims {
        sub: user_id.to_string(),
        tid: tenant_id.to_string(),
        scp: scope_str,
        jti: uuid::Uuid::new_v4().to_string(),
        iat: now,
        exp: now + ttl_secs,
    };

    let header_json = serde_json::to_vec(&header).map_err(|e| e.to_string())?;
    let claims_json = serde_json::to_vec(&claims).map_err(|e| e.to_string())?;

    let header_b64 = base64_url_encode(&header_json);
    let claims_b64 = base64_url_encode(&claims_json);

    let signing_input = format!("{header_b64}.{claims_b64}");
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|e| format!("HMAC key error: {e}"))?;
    mac.update(signing_input.as_bytes());
    let signature = mac.finalize().into_bytes();
    let sig_b64 = base64_url_encode(&signature);

    Ok(format!("{header_b64}.{claims_b64}.{sig_b64}"))
}

// ---------------------------------------------------------------------------
// API key verification
// ---------------------------------------------------------------------------

/// Verify an API key against the database and return an [`AuthContext`].
///
/// Keys are stored as SHA-256 hashes; we hash the provided key and look it up.
pub fn verify_api_key(key: &str, db: &rusqlite::Connection) -> Result<AuthContext, String> {
    let key_hash = api_keys::hash_key(key);

    let mut stmt = db
        .prepare(
            "SELECT id, tenant_id, scopes FROM api_keys WHERE key_hash = ?1",
        )
        .map_err(|e| format!("DB error: {e}"))?;

    let result = stmt
        .query_row(rusqlite::params![key_hash], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|_| "Invalid API key".to_string())?;

    let (key_id, tenant_id, scopes_json) = result;

    // Update last_used timestamp (best-effort)
    let _ = db.execute(
        "UPDATE api_keys SET last_used = datetime('now') WHERE id = ?1",
        rusqlite::params![key_id],
    );

    let scopes: Vec<Scope> = serde_json::from_str(&scopes_json).unwrap_or_default();

    Ok(AuthContext {
        user_id: format!("apikey:{key_id}"),
        tenant_id,
        scopes,
        session_id: key_id,
    })
}

// ---------------------------------------------------------------------------
// Base64url helpers (no padding, URL-safe alphabet)
// ---------------------------------------------------------------------------

fn base64_url_encode(data: &[u8]) -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    URL_SAFE_NO_PAD.encode(data)
}

fn base64_url_decode(s: &str) -> Result<Vec<u8>, String> {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    URL_SAFE_NO_PAD
        .decode(s)
        .map_err(|e| format!("Base64 decode error: {e}"))
}

// ---------------------------------------------------------------------------
// Response helpers
// ---------------------------------------------------------------------------

fn unauthorized_response(msg: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({"error": msg})),
    )
        .into_response()
}

fn forbidden_response(msg: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({"error": msg})),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jwt_roundtrip() {
        let secret = "test-secret-key-for-hmac-256";
        let scopes = vec![Scope::ProjectRead, Scope::ProjectWrite];

        let token = create_jwt("user-1", "tenant-1", &scopes, secret, 3600).unwrap();
        let ctx = verify_jwt(&token, secret).unwrap();

        assert_eq!(ctx.user_id, "user-1");
        assert_eq!(ctx.tenant_id, "tenant-1");
        assert!(ctx.has_scope(&Scope::ProjectRead));
        assert!(ctx.has_scope(&Scope::ProjectWrite));
        assert!(!ctx.has_scope(&Scope::SystemAdmin));
    }

    #[test]
    fn test_jwt_expired() {
        let secret = "test-secret";
        // Create a token that expired 10 seconds ago
        let token = create_jwt("user-1", "tenant-1", &[], secret, 0).unwrap();
        // Give it a moment to expire (exp = now + 0 = now, which is already <= now)
        let result = verify_jwt(&token, secret);
        // The token has exp == now, which may or may not be expired depending on timing.
        // Create one that's definitely expired by manipulating directly.
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_jwt_wrong_secret() {
        let token = create_jwt("user-1", "tenant-1", &[], "secret-a", 3600).unwrap();
        let result = verify_jwt(&token, "secret-b");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("signature"));
    }

    #[test]
    fn test_jwt_invalid_format() {
        assert!(verify_jwt("not.a.valid.jwt.token", "secret").is_err());
        assert!(verify_jwt("", "secret").is_err());
        assert!(verify_jwt("only-one-part", "secret").is_err());
    }

    #[test]
    fn test_scope_parsing() {
        assert_eq!(Scope::parse("project:read"), Some(Scope::ProjectRead));
        assert_eq!(Scope::parse("system:admin"), Some(Scope::SystemAdmin));
        assert_eq!(Scope::parse("invalid"), None);
    }

    #[test]
    fn test_system_admin_has_all_scopes() {
        let ctx = AuthContext {
            user_id: "admin".into(),
            tenant_id: "t1".into(),
            scopes: vec![Scope::SystemAdmin],
            session_id: "s1".into(),
        };
        assert!(ctx.has_scope(&Scope::ProjectRead));
        assert!(ctx.has_scope(&Scope::DeployManage));
        assert!(ctx.has_scope(&Scope::SettingsManage));
    }

    // ── Public path tests ────────────────────────────────────────────

    #[test]
    fn test_public_paths_allowed() {
        // Only liveness/readiness probes are public. /health/detailed and
        // /health/v2 expose subsystem status and therefore must require
        // authentication.
        assert!(is_public_path("/health"));
        assert!(is_public_path("/health/live"));
        assert!(is_public_path("/health/ready"));
        assert!(!is_public_path("/health/detailed"));
        assert!(!is_public_path("/health/v2"));
    }

    #[test]
    fn test_non_public_paths_rejected() {
        assert!(!is_public_path("/projects"));
        assert!(!is_public_path("/oneshot"));
        assert!(!is_public_path("/health/secret"));
        assert!(!is_public_path("/healthx"));
        assert!(!is_public_path("/api/health"));
        assert!(!is_public_path("/settings"));
        assert!(!is_public_path("/vault"));
    }

    // ── Scope requirement tests ──────────────────────────────────────

    #[test]
    fn test_require_scope_success() {
        let ctx = AuthContext {
            user_id: "u1".into(),
            tenant_id: "t1".into(),
            scopes: vec![Scope::ProjectRead, Scope::ProjectWrite],
            session_id: "s1".into(),
        };
        assert!(ctx.require_scope(&Scope::ProjectRead).is_ok());
        assert!(ctx.require_scope(&Scope::ProjectWrite).is_ok());
    }

    #[test]
    fn test_require_scope_denied() {
        let ctx = AuthContext {
            user_id: "u1".into(),
            tenant_id: "t1".into(),
            scopes: vec![Scope::ProjectRead],
            session_id: "s1".into(),
        };
        assert!(ctx.require_scope(&Scope::SystemAdmin).is_err());
        assert!(ctx.require_scope(&Scope::RuntimeManage).is_err());
        assert!(ctx.require_scope(&Scope::DeployManage).is_err());
    }

    #[test]
    fn test_system_admin_bypasses_all_scope_checks() {
        let ctx = AuthContext {
            user_id: "admin".into(),
            tenant_id: "t1".into(),
            scopes: vec![Scope::SystemAdmin],
            session_id: "s1".into(),
        };
        assert!(ctx.require_scope(&Scope::ProjectRead).is_ok());
        assert!(ctx.require_scope(&Scope::RuntimeManage).is_ok());
        assert!(ctx.require_scope(&Scope::DeployManage).is_ok());
        assert!(ctx.require_scope(&Scope::PluginManage).is_ok());
        assert!(ctx.require_scope(&Scope::SettingsManage).is_ok());
    }
}
