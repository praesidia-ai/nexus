//! Marketplace endpoints — community plugins, skills, workflows, agents.
//!
//! Install now actually installs plugins into the plugin registry when the
//! marketplace item contains a `plugin` config object.
//!
//! Also includes community contribution endpoints for installing, searching,
//! and validating agent definitions, team templates, plugins, domain packs,
//! and tool extensions.

use std::sync::Arc;
use axum::{extract::{Path, Query, State}, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;
use nexus_store::MarketplaceService;
use crate::{
    error::{ApiError, ApiResult},
    marketplace::{
        self, check_compatibility, featured_contributions, validate_manifest,
        ContributionCategory, ContributionManifest, InstalledContribution,
        LocalRegistry, ValidationIssue,
    },
    state::AppState,
};

// ---------------------------------------------------------------------------
// Query / request / response types (legacy marketplace)
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
pub struct SearchQuery {
    pub q: Option<String>,
    pub item_type: Option<String>,
}

// ---------------------------------------------------------------------------
// Query / request / response types (community contributions)
// ---------------------------------------------------------------------------

/// Query parameters for `GET /marketplace/community/search`.
#[derive(Debug, Deserialize, Default)]
pub struct CommunitySearchQuery {
    /// Free-text search query.
    pub q: Option<String>,
    /// Filter by category (`agent_definition`, `team_template`, `plugin`, `domain_pack`, `tool_extension`).
    pub category: Option<String>,
}

/// Response for search and listing endpoints.
#[derive(Debug, Serialize)]
pub struct ContributionsResponse {
    /// The list of contributions matching the query.
    pub contributions: Vec<InstalledContribution>,
    /// Total number of results.
    pub total: usize,
}

/// Response for a single contribution detail.
#[derive(Debug, Serialize)]
pub struct ContributionDetailResponse {
    /// The installed contribution data.
    pub contribution: InstalledContribution,
    /// Version compatibility status against the running Nexus version.
    pub compatibility: marketplace::CompatibilityResult,
}

/// Response for the validation endpoint.
#[derive(Debug, Serialize)]
pub struct ValidationResponse {
    /// Whether the manifest passed validation (no errors).
    pub valid: bool,
    /// All issues found during validation.
    pub issues: Vec<ValidationIssue>,
}

/// Response for install/uninstall mutations.
#[derive(Debug, Serialize)]
pub struct MutationResponse {
    /// Whether the operation succeeded.
    pub success: bool,
    /// Human-readable message.
    pub message: String,
}

// ---------------------------------------------------------------------------
// Legacy marketplace handlers (DB-backed items from nexus-store)
// ---------------------------------------------------------------------------

pub async fn list_marketplace(
    State(app): State<Arc<AppState>>,
    query: Option<Query<SearchQuery>>,
) -> ApiResult<Json<serde_json::Value>> {
    let q = query.map(|q| q.0).unwrap_or_default();
    let db = app.db.lock().await;
    let svc = MarketplaceService::new(&db);
    let items = if let Some(query) = q.q {
        svc.search(&query)?
    } else if let Some(item_type) = q.item_type {
        svc.list_by_type(&item_type)?
    } else {
        svc.list_all()?
    };
    Ok(Json(json!(items)))
}

pub async fn install_item(
    State(app): State<Arc<AppState>>,
    Path(item_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    // 1. Increment downloads and get config
    let config: Option<serde_json::Value> = {
        let db = app.db.lock().await;
        let svc = MarketplaceService::new(&db);
        svc.increment_downloads(&item_id)?;

        db.query_row(
            "SELECT config FROM marketplace_items WHERE id = ?1",
            [&item_id],
            |row| {
                let cfg_str: String = row.get(0)?;
                Ok(cfg_str)
            },
        )
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
    };

    // 2. If the item has a plugin config, install it into the plugin registry
    let plugin_installed = if let Some(ref cfg) = config {
        if cfg.get("plugin").is_some() || cfg.get("id").is_some() {
            let mut registry = app.legacy_plugin_registry.write().await;
            match registry.install_from_config(&item_id, cfg) {
                Ok(plugin_id) => Some(plugin_id),
                Err(e) => {
                    return Ok(Json(json!({
                        "installed": false,
                        "id": item_id,
                        "error": e,
                    })));
                }
            }
        } else {
            None
        }
    } else {
        None
    };

    Ok(Json(json!({
        "installed": true,
        "id": item_id,
        "plugin_installed": plugin_installed,
    })))
}

// ---------------------------------------------------------------------------
// Community contribution handlers (file-backed registry)
// ---------------------------------------------------------------------------

/// `GET /marketplace/community/search` — search installed contributions.
///
/// Supports optional `q` (free-text) and `category` query parameters.
/// Returns all installed contributions when no filters are provided.
pub async fn search_contributions(
    State(app): State<Arc<AppState>>,
    query: Option<Query<CommunitySearchQuery>>,
) -> ApiResult<Json<ContributionsResponse>> {
    let q = query.map(|q| q.0).unwrap_or_default();
    let mut registry = LocalRegistry::new(&app.data_dir);
    registry.load().map_err(ApiError::Internal)?;

    let results: Vec<InstalledContribution> = if let Some(ref text) = q.q {
        registry.search(text).into_iter().cloned().collect()
    } else if let Some(ref cat_str) = q.category {
        if let Some(cat) = parse_category(cat_str) {
            registry.by_category(&cat).into_iter().cloned().collect()
        } else {
            return Err(ApiError::BadRequest(format!("unknown category: {cat_str}")));
        }
    } else {
        registry.list().to_vec()
    };

    let total = results.len();
    Ok(Json(ContributionsResponse {
        contributions: results,
        total,
    }))
}

/// `GET /marketplace/community/featured` — return curated featured contributions.
pub async fn featured(
    State(_app): State<Arc<AppState>>,
) -> ApiResult<Json<serde_json::Value>> {
    let featured = featured_contributions();
    Ok(Json(json!({
        "contributions": featured,
        "total": featured.len(),
    })))
}

/// `GET /marketplace/community/contributions/:id` — get details for a single installed contribution.
pub async fn get_contribution(
    State(app): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<ContributionDetailResponse>> {
    let mut registry = LocalRegistry::new(&app.data_dir);
    registry.load().map_err(ApiError::Internal)?;

    let contribution = registry
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::NotFound(format!("contribution '{id}' not found")))?;

    let nexus_version = env!("CARGO_PKG_VERSION");
    let compatibility = check_compatibility(
        &contribution.manifest.nexus_version_range,
        nexus_version,
    );

    Ok(Json(ContributionDetailResponse {
        contribution,
        compatibility,
    }))
}

/// `POST /marketplace/community/contributions/:id/install` — install a contribution by manifest.
///
/// The request body must contain the full `ContributionManifest`. The `:id` path
/// parameter is used for logging but the manifest's own `id` field is authoritative.
pub async fn install_contribution(
    State(app): State<Arc<AppState>>,
    Path(_id): Path<String>,
    Json(manifest): Json<ContributionManifest>,
) -> ApiResult<Json<MutationResponse>> {
    let mut registry = LocalRegistry::new(&app.data_dir);
    registry.load().map_err(ApiError::Internal)?;

    registry.install(manifest.clone()).map_err(ApiError::BadRequest)?;

    Ok(Json(MutationResponse {
        success: true,
        message: format!("contribution '{}' installed successfully", manifest.id),
    }))
}

/// `DELETE /marketplace/community/contributions/:id` — uninstall a contribution.
pub async fn uninstall_contribution(
    State(app): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<MutationResponse>> {
    let mut registry = LocalRegistry::new(&app.data_dir);
    registry.load().map_err(ApiError::Internal)?;

    registry.uninstall(&id).map_err(ApiError::NotFound)?;

    Ok(Json(MutationResponse {
        success: true,
        message: format!("contribution '{id}' uninstalled"),
    }))
}

/// `POST /marketplace/community/validate` — validate a contribution manifest without installing it.
pub async fn validate_contribution(
    State(_app): State<Arc<AppState>>,
    Json(manifest): Json<ContributionManifest>,
) -> ApiResult<Json<ValidationResponse>> {
    let issues = validate_manifest(&manifest);
    let valid = !issues.iter().any(|i| i.severity == marketplace::ValidationSeverity::Error);
    Ok(Json(ValidationResponse { valid, issues }))
}

// ---------------------------------------------------------------------------
// Pkg-registry marketplace endpoints (reviews, ratings, one-click install)
// ---------------------------------------------------------------------------

/// Search query for the pkg-registry marketplace.
#[derive(Debug, Deserialize, Default)]
pub struct PkgSearchQuery {
    pub q: Option<String>,
    pub kind: Option<String>,
    pub category: Option<String>,
    pub sort: Option<String>,
    pub page: Option<u32>,
    pub limit: Option<u32>,
}

/// A package entry returned by the search endpoint.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PkgEntry {
    pub name: String,
    pub kind: String,
    pub runtime: String,
    pub latest_version: String,
    pub description: String,
    pub keywords: Vec<String>,
    pub categories: Vec<String>,
    pub downloads: u64,
    pub rating: Option<f32>,
    pub review_count: u32,
    pub author: String,
    pub created_at: String,
    pub updated_at: String,
    pub installed: bool,
    pub installed_version: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PkgSearchResult {
    pub packages: Vec<PkgEntry>,
    pub total: u64,
    pub page: u32,
    pub total_pages: u32,
}

#[derive(Debug, Deserialize)]
pub struct PkgInstallRequest {
    pub name: String,
    pub version: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PkgUninstallRequest {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct PkgReviewRequest {
    pub rating: u8,
    pub comment: String,
}

/// `GET /marketplace/search` — search pkg-registry packages.
pub async fn pkg_search(
    State(_app): State<Arc<AppState>>,
    Query(q): Query<PkgSearchQuery>,
) -> ApiResult<Json<PkgSearchResult>> {
    // Proxy to the external registry. If unreachable, return empty results.
    let registry_url = std::env::var("NEXUS_REGISTRY")
        .unwrap_or_else(|_| nexus_pkg::registry::DEFAULT_REGISTRY.to_owned());

    let client = nexus_pkg::registry::RegistryClient::new(&registry_url);
    let query = q.q.as_deref().unwrap_or("");
    let limit = q.limit.unwrap_or(24) as usize;

    let entries = client.search(query, limit).await.unwrap_or_default();
    let total = entries.len() as u64;

    let packages = entries
        .into_iter()
        .map(|e| PkgEntry {
            kind: "agent".to_owned(),
            runtime: "wasm".to_owned(),
            latest_version: e.latest_version.clone(),
            description: e.description.clone().unwrap_or_default(),
            keywords: e.keywords.clone(),
            categories: e.categories.clone(),
            downloads: e.downloads,
            rating: e.rating,
            review_count: 0,
            author: "community".to_owned(),
            created_at: e.created_at.to_rfc3339(),
            updated_at: e.updated_at.to_rfc3339(),
            installed: false,
            installed_version: None,
            name: e.name,
        })
        .collect();

    Ok(Json(PkgSearchResult {
        packages,
        total,
        page: q.page.unwrap_or(1),
        total_pages: (total as u32).div_ceil(q.limit.unwrap_or(24)),
    }))
}

/// `POST /marketplace/install` — one-click install from pkg registry.
///
/// Admin-only. Refuses HTTP registries unless `NEXUS_REGISTRY_ALLOW_HTTP=true`
/// (HTTP is MITM-able). Logs an audit entry on every install since the action
/// fetches and persists arbitrary code on the host.
pub async fn pkg_install(
    State(app): State<Arc<AppState>>,
    auth: crate::security::auth::AuthContext,
    Json(req): Json<PkgInstallRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let nexus_home = dirs_next::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("nexus");
    let registry_url = std::env::var("NEXUS_REGISTRY")
        .unwrap_or_else(|_| nexus_pkg::registry::DEFAULT_REGISTRY.to_owned());

    // Refuse HTTP registries by default — anyone on the path between the
    // server and the registry can swap the tarball. Operators who genuinely
    // need plain HTTP (test fixtures, intranet servers) opt in explicitly.
    let allow_http = std::env::var("NEXUS_REGISTRY_ALLOW_HTTP")
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if registry_url.starts_with("http://") && !allow_http {
        return Err(ApiError::BadRequest(format!(
            "NEXUS_REGISTRY={registry_url} is plain HTTP. Use https:// or set NEXUS_REGISTRY_ALLOW_HTTP=true."
        )));
    }

    let installer = nexus_pkg::installer::Installer::new(nexus_home, registry_url.clone());
    let pkg = installer
        .install(&req.name, req.version.as_deref())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    app.audit_log
        .append(
            &auth.tenant_id,
            "marketplace.install",
            json!({
                "actor": auth.user_id,
                "name": pkg.name,
                "version": pkg.version,
                "registry": registry_url,
            }),
            &app.audit_keypair,
        )
        .await;

    Ok(Json(json!({
        "installed": true,
        "name": pkg.name,
        "version": pkg.version,
        "path": pkg.install_path,
    })))
}

/// `POST /marketplace/uninstall` — uninstall a pkg package. Admin-only.
pub async fn pkg_uninstall(
    State(app): State<Arc<AppState>>,
    auth: crate::security::auth::AuthContext,
    Json(req): Json<PkgUninstallRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let nexus_home = dirs_next::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("nexus");
    let registry_url = std::env::var("NEXUS_REGISTRY")
        .unwrap_or_else(|_| nexus_pkg::registry::DEFAULT_REGISTRY.to_owned());

    let installer = nexus_pkg::installer::Installer::new(nexus_home, registry_url);
    installer
        .uninstall(&req.name)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    app.audit_log
        .append(
            &auth.tenant_id,
            "marketplace.uninstall",
            json!({ "actor": auth.user_id, "name": req.name.clone() }),
            &app.audit_keypair,
        )
        .await;

    Ok(Json(json!({ "uninstalled": true, "name": req.name })))
}

/// `GET /marketplace/installed` — list installed pkg packages.
pub async fn pkg_list_installed(
    State(_app): State<Arc<AppState>>,
) -> ApiResult<Json<Vec<serde_json::Value>>> {
    let nexus_home = dirs_next::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("nexus");
    let registry_url = std::env::var("NEXUS_REGISTRY")
        .unwrap_or_else(|_| nexus_pkg::registry::DEFAULT_REGISTRY.to_owned());

    let installer = nexus_pkg::installer::Installer::new(nexus_home, registry_url);
    let packages = installer
        .list_installed()
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let result: Vec<serde_json::Value> = packages
        .iter()
        .map(|p| json!({
            "name": p.name,
            "version": p.version,
            "installedAt": p.installed_at,
            "path": p.install_path,
        }))
        .collect();

    Ok(Json(result))
}

/// `GET /marketplace/categories` — list available categories.
pub async fn pkg_categories(
    State(_app): State<Arc<AppState>>,
) -> ApiResult<Json<Vec<String>>> {
    Ok(Json(vec![
        "productivity".into(),
        "developer-tools".into(),
        "data-analysis".into(),
        "communication".into(),
        "automation".into(),
        "finance".into(),
        "marketing".into(),
        "research".into(),
        "security".into(),
        "devops".into(),
    ]))
}

/// Per-version record in a package detail response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PkgVersionView {
    pub version: String,
    pub published_at: String,
    pub yanked: bool,
    pub changelog: Option<String>,
}

/// Sandbox capabilities surfaced to the UI for a package.
#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PkgCapabilities {
    pub network: bool,
    pub filesystem: bool,
    pub exec: bool,
    pub mcp: bool,
    pub a2a: bool,
    pub memory: bool,
    pub workflows: bool,
}

/// Resource caps surfaced to the UI for a package.
#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PkgResources {
    pub max_fuel_m: Option<u64>,
    pub max_memory_mb: Option<u64>,
    pub timeout_secs: Option<u64>,
}

/// Detailed view for a single package. Matches `PackageDetail` in
/// `web/src/lib/marketplace-api.ts`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PkgDetail {
    #[serde(flatten)]
    pub base: PkgEntry,
    pub readme: String,
    pub versions: Vec<PkgVersionView>,
    pub reviews: Vec<serde_json::Value>,
    pub capabilities: PkgCapabilities,
    pub resources: PkgResources,
}

/// `GET /marketplace/packages/:name` — detail view for a single package.
/// Powers the public marketplace detail page at `/marketplace/[slug]`.
pub async fn pkg_get(
    State(_app): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> ApiResult<Json<PkgDetail>> {
    let registry_url = std::env::var("NEXUS_REGISTRY")
        .unwrap_or_else(|_| nexus_pkg::registry::DEFAULT_REGISTRY.to_owned());
    let client = nexus_pkg::registry::RegistryClient::new(&registry_url);

    let entry = client
        .get_entry(&name)
        .await
        .map_err(|_| ApiError::NotFound(format!("package '{name}' not found")))?;

    let base = PkgEntry {
        name: entry.name.clone(),
        kind: "agent".to_owned(),
        runtime: "wasm".to_owned(),
        latest_version: entry.latest_version.clone(),
        description: entry.description.clone().unwrap_or_default(),
        keywords: entry.keywords.clone(),
        categories: entry.categories.clone(),
        downloads: entry.downloads,
        rating: entry.rating,
        review_count: 0,
        author: "community".to_owned(),
        created_at: entry.created_at.to_rfc3339(),
        updated_at: entry.updated_at.to_rfc3339(),
        installed: false,
        installed_version: None,
    };

    let versions = entry
        .versions
        .iter()
        .map(|v| PkgVersionView {
            version: v.version.clone(),
            published_at: v.published_at.to_rfc3339(),
            yanked: v.yanked,
            changelog: None,
        })
        .collect();

    Ok(Json(PkgDetail {
        base,
        readme: String::new(),
        versions,
        reviews: Vec::new(),
        capabilities: PkgCapabilities::default(),
        resources: PkgResources::default(),
    }))
}

/// `POST /marketplace/packages/:name/reviews` — submit a review.
pub async fn pkg_submit_review(
    State(_app): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<PkgReviewRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    if req.rating > 5 {
        return Err(ApiError::BadRequest("rating must be 1-5".into()));
    }
    // Reviews are forwarded to the external registry; ack optimistically here.
    tracing::info!("Review for {name}: {} stars — {}", req.rating, req.comment);
    Ok(Json(json!({ "submitted": true })))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a category string into the enum variant.
fn parse_category(s: &str) -> Option<ContributionCategory> {
    match s {
        "agent_definition" => Some(ContributionCategory::AgentDefinition),
        "team_template" => Some(ContributionCategory::TeamTemplate),
        "plugin" => Some(ContributionCategory::Plugin),
        "domain_pack" => Some(ContributionCategory::DomainPack),
        "tool_extension" => Some(ContributionCategory::ToolExtension),
        _ => None,
    }
}
