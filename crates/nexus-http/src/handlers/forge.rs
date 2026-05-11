//! Nexus Forge — community-reputation marketplace handlers.
//!
//! Ships five endpoints:
//!
//!   POST /projects/:id/forge/:listingId/install
//!   POST /projects/:id/forge/:listingId/review   { rating, body? }
//!   GET  /forge/trending                         — installs_7d × rating
//!   GET  /forge/top-rated                        — Bayesian rating
//!   GET  /forge/newest                           — most recently created
//!
//! There is NO payment surface here — see `forge_reputation.rs` for
//! the no-money rationale. Installs and reviews are the only two
//! write paths; every other surface is a read over `forge_stats`.
//!
//! Concurrency note: each write is a short synchronous critical
//! section under the shared `app.db` mutex. Rows in `forge_stats`
//! are UPSERTed so the first-ever install on a listing is cheap and
//! doesn't need a "pre-create stats row" bootstrap migration.

use std::sync::Arc;

use axum::{extract::{Path, Query, State}, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    error::{ApiError, ApiResult},
    forge_reputation::{validate_rating, ListingStats},
    security::project_access::ProjectAccess,
    state::AppState,
};

// ---------------------------------------------------------------------------
// POST /projects/:id/forge/:listingId/install
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Default)]
pub struct InstallRequest {
    /// Client-reported install source. Defaults to "cli".
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub nexus_version: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InstallResponse {
    pub listing_id: String,
    pub first_time: bool,
    pub install_count: i64,
}

#[tracing::instrument(skip(app, body))]
pub async fn install(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Path((_project_id, listing_id)): Path<(String, String)>,
    Json(body): Json<InstallRequest>,
) -> ApiResult<Json<InstallResponse>> {
    let source = body
        .source
        .as_deref()
        .filter(|s| matches!(*s, "cli" | "web" | "api" | "mcp"))
        .unwrap_or("cli")
        .to_string();
    let account_id = access.tenant_id.clone();
    let install_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let db = app.db.lock().await;

    // Listing must exist. A foreign-key violation would surface the
    // same outcome but with a 500 — we catch it here for a clean 404.
    let exists: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM marketplace_items WHERE id = ?1",
            [&listing_id],
            |r| r.get(0),
        )
        .map_err(|e| ApiError::Internal(format!("lookup listing: {e}")))?;
    if exists == 0 {
        return Err(ApiError::NotFound(format!(
            "listing {listing_id} not found"
        )));
    }

    // INSERT OR IGNORE against the (listing_id, account_id) unique
    // key gives us idempotent "this user installed this plugin"
    // semantics — repeat calls return `first_time=false`.
    let first_time = db
        .execute(
            "INSERT OR IGNORE INTO forge_installs
                (id, listing_id, account_id, nexus_version, source, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                install_id,
                listing_id,
                account_id,
                body.nexus_version,
                source,
                now
            ],
        )
        .map_err(|e| ApiError::Internal(format!("insert install: {e}")))?
        > 0;

    // Bump forge_stats. We UPSERT so the first install on a
    // never-before-seen listing works without a bootstrap row.
    if first_time {
        db.execute(
            "INSERT INTO forge_stats (listing_id, install_count, install_count_7d, last_install_at)
             VALUES (?1, 1, 1, ?2)
             ON CONFLICT(listing_id) DO UPDATE SET
                install_count = install_count + 1,
                install_count_7d = install_count_7d + 1,
                last_install_at = ?2",
            rusqlite::params![listing_id, now],
        )
        .map_err(|e| ApiError::Internal(format!("bump install stats: {e}")))?;

        // Keep the legacy `marketplace_items.downloads` column in
        // sync for any older SDK/UI still reading it — cheap alias.
        let _ = db.execute(
            "UPDATE marketplace_items SET downloads = downloads + 1 WHERE id = ?1",
            [&listing_id],
        );
    }

    let install_count: i64 = db
        .query_row(
            "SELECT install_count FROM forge_stats WHERE listing_id = ?1",
            [&listing_id],
            |r| r.get(0),
        )
        .unwrap_or(0);

    Ok(Json(InstallResponse {
        listing_id,
        first_time,
        install_count,
    }))
}

// ---------------------------------------------------------------------------
// POST /projects/:id/forge/:listingId/review
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ReviewRequest {
    pub rating: i64,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ReviewResponse {
    pub listing_id: String,
    pub stats: ListingStats,
}

#[tracing::instrument(skip(app, body))]
pub async fn review(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Path((_project_id, listing_id)): Path<(String, String)>,
    Json(body): Json<ReviewRequest>,
) -> ApiResult<Json<ReviewResponse>> {
    let rating = validate_rating(body.rating).map_err(|e| ApiError::BadRequest(e.into()))?;
    let reviewer_id = access.tenant_id.clone();
    let now = chrono::Utc::now().to_rfc3339();

    let db = app.db.lock().await;

    // Listing must exist.
    let exists: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM marketplace_items WHERE id = ?1",
            [&listing_id],
            |r| r.get(0),
        )
        .map_err(|e| ApiError::Internal(format!("lookup listing: {e}")))?;
    if exists == 0 {
        return Err(ApiError::NotFound(format!(
            "listing {listing_id} not found"
        )));
    }

    // Capture the previous rating (if any) before the UPSERT so
    // `apply_review` knows whether we're inserting or updating.
    let previous: Option<i64> = db
        .query_row(
            "SELECT rating FROM forge_reviews
             WHERE listing_id = ?1 AND reviewer_id = ?2",
            rusqlite::params![listing_id, reviewer_id],
            |r| r.get(0),
        )
        .ok();

    let review_id = uuid::Uuid::new_v4().to_string();
    db.execute(
        "INSERT INTO forge_reviews
            (id, listing_id, reviewer_id, rating, body, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
         ON CONFLICT(listing_id, reviewer_id) DO UPDATE SET
            rating = excluded.rating,
            body = excluded.body,
            updated_at = excluded.updated_at",
        rusqlite::params![review_id, listing_id, reviewer_id, rating, body.body, now],
    )
    .map_err(|e| ApiError::Internal(format!("upsert review: {e}")))?;

    // Recompute and write aggregate stats. We fetch the current row
    // (if present), ask the pure helper to apply the delta, then
    // UPSERT the new counters back. Doing the math in Rust keeps
    // `apply_review`'s semantics unit-testable.
    let mut stats = read_stats(&db, &listing_id).unwrap_or_default();
    stats.apply_review(previous, rating);
    db.execute(
        "INSERT INTO forge_stats
            (listing_id, review_count, rating_sum, avg_rating)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(listing_id) DO UPDATE SET
            review_count = ?2,
            rating_sum   = ?3,
            avg_rating   = ?4",
        rusqlite::params![
            listing_id,
            stats.review_count,
            stats.rating_sum,
            stats.avg_rating
        ],
    )
    .map_err(|e| ApiError::Internal(format!("write review stats: {e}")))?;

    // Keep legacy `marketplace_items.rating` in sync for old readers.
    let _ = db.execute(
        "UPDATE marketplace_items SET rating = ?1 WHERE id = ?2",
        rusqlite::params![stats.avg_rating, listing_id],
    );

    Ok(Json(ReviewResponse {
        listing_id,
        stats,
    }))
}

// ---------------------------------------------------------------------------
// Gallery endpoints
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct GalleryParams {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub item_type: Option<String>, // filter e.g. "agent" | "workflow"
}

#[tracing::instrument(skip(app))]
pub async fn trending(
    State(app): State<Arc<AppState>>,
    Query(params): Query<GalleryParams>,
) -> ApiResult<Json<Value>> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let db = app.db.lock().await;
    // Trending score computed inline in SQL so sort is indexable.
    // `COALESCE(avg_rating, 3.0)` mirrors `ListingStats::trending_score`.
    let mut stmt = db
        .prepare(
            "SELECT mi.id, mi.item_type, mi.name, mi.description, mi.author,
                    mi.version, mi.icon, mi.tags, mi.is_official,
                    COALESCE(s.install_count, 0)     AS install_count,
                    COALESCE(s.install_count_7d, 0)  AS install_count_7d,
                    COALESCE(s.review_count, 0)      AS review_count,
                    COALESCE(s.avg_rating, 0.0)      AS avg_rating,
                    (COALESCE(s.install_count_7d, 0)
                     * CASE WHEN COALESCE(s.review_count,0)=0
                            THEN 3.0
                            ELSE COALESCE(s.avg_rating, 0.0)
                       END) AS score
             FROM marketplace_items mi
             LEFT JOIN forge_stats s ON s.listing_id = mi.id
             WHERE (?1 IS NULL OR mi.item_type = ?1)
             ORDER BY score DESC, install_count DESC
             LIMIT ?2",
        )
        .map_err(|e| ApiError::Internal(format!("prepare trending: {e}")))?;
    let items = collect_gallery(&mut stmt, params.item_type.as_deref(), limit)?;
    Ok(Json(json!({ "surface": "trending", "items": items })))
}

#[tracing::instrument(skip(app))]
pub async fn top_rated(
    State(app): State<Arc<AppState>>,
    Query(params): Query<GalleryParams>,
) -> ApiResult<Json<Value>> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let db = app.db.lock().await;
    // Bayesian rating with a prior of 5 neutral reviews (3.0) —
    // matches `ListingStats::bayesian_rating`.
    let mut stmt = db
        .prepare(
            "SELECT mi.id, mi.item_type, mi.name, mi.description, mi.author,
                    mi.version, mi.icon, mi.tags, mi.is_official,
                    COALESCE(s.install_count, 0)     AS install_count,
                    COALESCE(s.install_count_7d, 0)  AS install_count_7d,
                    COALESCE(s.review_count, 0)      AS review_count,
                    COALESCE(s.avg_rating, 0.0)      AS avg_rating,
                    ( (5.0 * 3.0 + COALESCE(s.rating_sum, 0))
                      / (5.0 + COALESCE(s.review_count, 0)) ) AS score
             FROM marketplace_items mi
             LEFT JOIN forge_stats s ON s.listing_id = mi.id
             WHERE (?1 IS NULL OR mi.item_type = ?1)
             ORDER BY score DESC, review_count DESC
             LIMIT ?2",
        )
        .map_err(|e| ApiError::Internal(format!("prepare top-rated: {e}")))?;
    let items = collect_gallery(&mut stmt, params.item_type.as_deref(), limit)?;
    Ok(Json(json!({ "surface": "top_rated", "items": items })))
}

#[tracing::instrument(skip(app))]
pub async fn newest(
    State(app): State<Arc<AppState>>,
    Query(params): Query<GalleryParams>,
) -> ApiResult<Json<Value>> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let db = app.db.lock().await;
    let mut stmt = db
        .prepare(
            "SELECT mi.id, mi.item_type, mi.name, mi.description, mi.author,
                    mi.version, mi.icon, mi.tags, mi.is_official,
                    COALESCE(s.install_count, 0),
                    COALESCE(s.install_count_7d, 0),
                    COALESCE(s.review_count, 0),
                    COALESCE(s.avg_rating, 0.0),
                    mi.created_at AS score
             FROM marketplace_items mi
             LEFT JOIN forge_stats s ON s.listing_id = mi.id
             WHERE (?1 IS NULL OR mi.item_type = ?1)
             ORDER BY mi.created_at DESC
             LIMIT ?2",
        )
        .map_err(|e| ApiError::Internal(format!("prepare newest: {e}")))?;
    let items = collect_gallery(&mut stmt, params.item_type.as_deref(), limit)?;
    Ok(Json(json!({ "surface": "newest", "items": items })))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn read_stats(db: &rusqlite::Connection, listing_id: &str) -> Option<ListingStats> {
    db.query_row(
        "SELECT install_count, install_count_7d, review_count, rating_sum, avg_rating
         FROM forge_stats WHERE listing_id = ?1",
        [listing_id],
        |r| {
            Ok(ListingStats {
                install_count: r.get(0)?,
                install_count_7d: r.get(1)?,
                review_count: r.get(2)?,
                rating_sum: r.get(3)?,
                avg_rating: r.get(4)?,
            })
        },
    )
    .ok()
}

fn collect_gallery(
    stmt: &mut rusqlite::Statement<'_>,
    item_type: Option<&str>,
    limit: i64,
) -> ApiResult<Vec<Value>> {
    let rows = stmt
        .query_map(rusqlite::params![item_type, limit], |r| {
            // `score` is read as a JSON number regardless of whether
            // the underlying column is REAL (trending/top_rated) or
            // TEXT (newest, the created_at timestamp).
            let score: Value = match r.get_ref(13)? {
                rusqlite::types::ValueRef::Real(f) => json!(f),
                rusqlite::types::ValueRef::Integer(i) => json!(i),
                rusqlite::types::ValueRef::Text(t) => {
                    json!(String::from_utf8_lossy(t).to_string())
                }
                _ => json!(null),
            };
            Ok(json!({
                "id": r.get::<_, String>(0)?,
                "item_type": r.get::<_, String>(1)?,
                "name": r.get::<_, String>(2)?,
                "description": r.get::<_, String>(3)?,
                "author": r.get::<_, String>(4)?,
                "version": r.get::<_, String>(5)?,
                "icon": r.get::<_, Option<String>>(6)?,
                "tags": r.get::<_, Option<String>>(7)?,
                "is_official": r.get::<_, i64>(8)? != 0,
                "install_count": r.get::<_, i64>(9)?,
                "install_count_7d": r.get::<_, i64>(10)?,
                "review_count": r.get::<_, i64>(11)?,
                "avg_rating": r.get::<_, f64>(12)?,
                "score": score,
            }))
        })
        .map_err(|e| ApiError::Internal(format!("query gallery: {e}")))?;
    let out: Vec<Value> = rows.filter_map(|r| r.ok()).collect();
    Ok(out)
}
