//! HTTP handlers for the Plugin System.
//!
//! These endpoints expose the unified, composable, observable plugin system
//! through the REST API. They cover installation, listing, validation,
//! hook introspection, and execution-log observability.
//!
//! SECURITY: `POST /plugins/install` accepts either a raw manifest or a
//! `SignedManifest`. When the env var `NEXUS_REQUIRE_SIGNED_PLUGINS=1` is set
//! the unsigned path is rejected. Operators that ship a `SignedManifest` get
//! Ed25519 verification of the manifest bytes before any registry mutation,
//! closing the supply-chain attack surface flagged in the audit (SEC-027).

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use nexus_kernel::SignedManifest;
use serde::Deserialize;
use serde_json::json;
use tracing::{info, warn};

use crate::{
    error::ApiResult,
    plugin_system::{
        check_conflicts, list_available_hook_points, validate_manifest, PluginManifest,
    },
    state::AppState,
};

/// Returns true if the operator has opted in to strict signed-plugin mode.
fn require_signed_plugins() -> bool {
    matches!(
        std::env::var("NEXUS_REQUIRE_SIGNED_PLUGINS")
            .as_deref()
            .map(str::trim),
        Ok("1") | Ok("true") | Ok("TRUE")
    )
}

/// Request body for `POST /plugins/install`.
///
/// Either `signed_manifest` OR `manifest` must be present. If both are
/// supplied the signed payload wins and its `manifest_json` is the
/// authoritative source — the bare `manifest` field is ignored.
#[derive(Debug, Deserialize)]
pub struct InstallPluginRequest {
    /// Ed25519-signed manifest. Required when `NEXUS_REQUIRE_SIGNED_PLUGINS=1`.
    #[serde(default)]
    pub signed_manifest: Option<SignedManifest>,
    /// Unsigned plugin manifest (legacy path). Subject to operator opt-in.
    #[serde(default)]
    pub manifest: Option<PluginManifest>,
}

/// Request body for `POST /plugins/validate`.
///
/// Accepts the same shape as install: either a `signed_manifest` or a raw
/// `manifest`. Validation does NOT mutate the registry.
#[derive(Debug, Deserialize)]
pub struct ValidatePluginRequest {
    /// Ed25519-signed manifest (preferred).
    #[serde(default)]
    pub signed_manifest: Option<SignedManifest>,
    /// Unsigned plugin manifest.
    #[serde(default)]
    pub manifest: Option<PluginManifest>,
}

/// `POST /plugins/install` -- install a plugin from its manifest.
///
/// Validates the manifest, checks for conflicts with installed plugins,
/// and installs the plugin if everything is clean.
///
/// Two paths:
///   * `signed_manifest` — Ed25519 signature is verified before install.
///     The signer's pubkey is recorded in the install response so an
///     operator can compare it against an out-of-band trust list.
///   * `manifest` — unsigned. Rejected when `NEXUS_REQUIRE_SIGNED_PLUGINS=1`.
///
/// At least one of the two MUST be supplied; supplying both prefers the
/// signed payload (`manifest_json` from the signed envelope is authoritative).
pub async fn install_plugin(
    State(app): State<Arc<AppState>>,
    Json(body): Json<InstallPluginRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    // Resolve the authoritative manifest, applying the signing policy.
    let (manifest, signer_pubkey): (PluginManifest, Option<String>) = match (
        body.signed_manifest,
        body.manifest,
    ) {
        (Some(signed), _) => {
            // Verify the Ed25519 signature BEFORE parsing or installing.
            // A failure here means the manifest bytes were tampered with or
            // the embedded pubkey doesn't actually own the signature.
            if !signed.verify() {
                warn!(
                    signer_pubkey = %signed.signer_pubkey,
                    "Plugin install rejected: signature verification failed",
                );
                return Ok(Json(json!({
                    "status": "rejected",
                    "errors": ["signed manifest signature verification failed"],
                })));
            }
            let parsed: PluginManifest = match serde_json::from_str(&signed.manifest_json) {
                Ok(m) => m,
                Err(e) => {
                    return Ok(Json(json!({
                        "status": "rejected",
                        "errors": [format!("signed manifest parse failed: {e}")],
                    })));
                }
            };
            info!(
                signer_pubkey = %signed.signer_pubkey,
                plugin_id = %parsed.id,
                "Verified signed plugin manifest",
            );
            (parsed, Some(signed.signer_pubkey))
        }
        (None, Some(unsigned)) => {
            if require_signed_plugins() {
                warn!(
                    plugin_id = %unsigned.id,
                    "Plugin install rejected: NEXUS_REQUIRE_SIGNED_PLUGINS=1 and no signature supplied",
                );
                return Ok(Json(json!({
                    "status": "rejected",
                    "errors": ["signed manifest required (NEXUS_REQUIRE_SIGNED_PLUGINS=1)"],
                })));
            }
            warn!(
                plugin_id = %unsigned.id,
                "Installing unsigned plugin manifest — recommend supplying a SignedManifest",
            );
            (unsigned, None)
        }
        (None, None) => {
            return Ok(Json(json!({
                "status": "rejected",
                "errors": ["request must contain `signed_manifest` or `manifest`"],
            })));
        }
    };

    // Validate the manifest first.
    let warnings = validate_manifest(&manifest);
    let critical = warnings
        .iter()
        .any(|w| w.contains("empty") || w.contains("not valid semver"));
    if critical {
        return Ok(Json(json!({
            "status": "rejected",
            "errors": warnings,
            "signer_pubkey": signer_pubkey,
        })));
    }

    let mut registry = app.plugin_registry.write().await;

    // Check for conflicts with already-installed plugins.
    let conflicts = check_conflicts(&registry, &manifest);
    if !conflicts.is_empty() {
        return Ok(Json(json!({
            "status": "conflict",
            "conflicts": conflicts,
            "warnings": warnings,
            "signer_pubkey": signer_pubkey,
        })));
    }

    // Install.
    let id = manifest.id.clone();
    let install_result = registry.install(manifest);
    drop(registry);

    // Forensic audit: every plugin-install attempt — signed or not — lands
    // in the signed Merkle chain. This is what an operator would replay to
    // answer "who installed what plugin and was its signature verified?"
    let outcome = match &install_result {
        Ok(()) => "installed",
        Err(_) => "install_error",
    };
    app.audit_log
        .append(
            "system",
            "plugin_install",
            json!({
                "plugin_id": id,
                "signed": signer_pubkey.is_some(),
                "signer_pubkey": signer_pubkey.clone(),
                "outcome": outcome,
            }),
            &app.audit_keypair,
        )
        .await;

    match install_result {
        Ok(()) => Ok(Json(json!({
            "status": "installed",
            "plugin_id": id,
            "warnings": warnings,
            "signer_pubkey": signer_pubkey,
            "signed": signer_pubkey.is_some(),
        }))),
        Err(e) => Ok(Json(json!({
            "status": "error",
            "error": e,
            "signer_pubkey": signer_pubkey,
        }))),
    }
}

/// `GET /plugins` -- list all installed plugins.
pub async fn list_plugins(State(app): State<Arc<AppState>>) -> ApiResult<Json<serde_json::Value>> {
    let registry = app.plugin_registry.read().await;
    let plugins: Vec<serde_json::Value> = registry
        .list()
        .iter()
        .map(|p| {
            json!({
                "id": p.manifest.id,
                "name": p.manifest.name,
                "version": p.manifest.version,
                "author": p.manifest.author,
                "description": p.manifest.description,
                "enabled": p.enabled,
                "installed_at": p.installed_at,
                "capabilities": p.manifest.capabilities.len(),
                "hooks": p.manifest.hooks.len(),
            })
        })
        .collect();

    Ok(Json(json!({
        "total": plugins.len(),
        "plugins": plugins,
    })))
}

/// `GET /plugins/:id/hooks` -- list all hook declarations for a specific plugin.
pub async fn get_plugin_hooks(
    State(app): State<Arc<AppState>>,
    Path(plugin_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let registry = app.plugin_registry.read().await;

    match registry.get(&plugin_id) {
        Some(plugin) => {
            let hooks: Vec<serde_json::Value> = plugin
                .manifest
                .hooks
                .iter()
                .map(|h| {
                    json!({
                        "hook_point": h.hook_point.as_key(),
                        "priority": h.priority,
                        "condition": h.condition,
                        "timeout_ms": h.timeout_ms,
                        "blocking": h.blocking,
                        "requires": h.requires,
                    })
                })
                .collect();

            Ok(Json(json!({
                "plugin_id": plugin_id,
                "plugin_name": plugin.manifest.name,
                "hooks": hooks,
            })))
        }
        None => Ok(Json(json!({
            "error": format!("Plugin '{}' not found", plugin_id),
        }))),
    }
}

/// `POST /plugins/validate` -- validate a manifest without installing it.
///
/// Returns validation warnings and conflict checks against the current registry.
/// When a `signed_manifest` is supplied, also reports the signature status.
pub async fn validate_plugin(
    State(app): State<Arc<AppState>>,
    Json(body): Json<ValidatePluginRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let (manifest, signature_valid, signer_pubkey) = match (body.signed_manifest, body.manifest) {
        (Some(signed), _) => {
            let sig_ok = signed.verify();
            let parsed: PluginManifest = match serde_json::from_str(&signed.manifest_json) {
                Ok(m) => m,
                Err(e) => {
                    return Ok(Json(json!({
                        "valid": false,
                        "warnings": [format!("signed manifest parse failed: {e}")],
                        "conflicts": [],
                        "signature_valid": sig_ok,
                        "signer_pubkey": signed.signer_pubkey,
                    })));
                }
            };
            (parsed, Some(sig_ok), Some(signed.signer_pubkey))
        }
        (None, Some(unsigned)) => (unsigned, None, None),
        (None, None) => {
            return Ok(Json(json!({
                "valid": false,
                "warnings": ["request must contain `signed_manifest` or `manifest`"],
                "conflicts": [],
            })));
        }
    };

    let warnings = validate_manifest(&manifest);
    let registry = app.plugin_registry.read().await;
    let conflicts = check_conflicts(&registry, &manifest);

    let valid = warnings.is_empty() && conflicts.is_empty() && signature_valid != Some(false);

    Ok(Json(json!({
        "valid": valid,
        "warnings": warnings,
        "conflicts": conflicts,
        "signature_valid": signature_valid,
        "signer_pubkey": signer_pubkey,
    })))
}

/// `GET /projects/:id/plugins/execution-log` -- get the hook execution log.
///
/// Returns all recorded hook executions. The caller can filter by project ID
/// client-side (the log is global but each record includes context).
pub async fn get_execution_log(
    State(app): State<Arc<AppState>>,
    Path(_project_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let registry = app.plugin_registry.read().await;
    let log = registry.execution_log();

    let entries: Vec<serde_json::Value> = log
        .iter()
        .map(|e| {
            json!({
                "hook_point": e.hook_point,
                "plugin_id": e.plugin_id,
                "started_at": e.started_at,
                "duration_ms": e.duration_ms,
                "result": e.result,
                "mutations": e.mutations,
            })
        })
        .collect();

    Ok(Json(json!({
        "total": entries.len(),
        "executions": entries,
    })))
}

/// `GET /plugins/hooks/available` -- list all available hook points.
///
/// Returns every hook point in the system with a description, so plugin
/// authors know what they can intercept.
pub async fn list_available_hooks() -> ApiResult<Json<serde_json::Value>> {
    let hooks = list_available_hook_points();
    Ok(Json(json!({
        "hook_points": hooks,
    })))
}
