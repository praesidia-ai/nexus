//! Taste Engine HTTP handlers — stage-based UX quality pipeline.

use std::sync::Arc;

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    error::{ApiError, ApiResult},
    security::auth::Scope,
    security::project_access::ProjectAccess,
    state::AppState,
    taste_engine::{self, ProjectFile, TasteContext, TastePipeline, TasteProfile, TasteReport},
    taste_redesign,
};

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

/// Request body for scoring a project with the taste pipeline.
#[derive(Debug, Deserialize)]
pub struct ScoreRequest {
    /// Optional threshold — if score is below this, a redesign plan is generated.
    #[serde(default = "default_threshold")]
    pub threshold: f32,
    /// Optional profile name ("default", "minimal", "corporate", "bold").
    #[serde(default)]
    pub profile: Option<String>,
    /// Optional app type hint.
    #[serde(default)]
    pub app_type: Option<String>,
    /// Optional domain hint.
    #[serde(default)]
    pub domain: Option<String>,
}

fn default_threshold() -> f32 {
    70.0
}

/// Request body for setting a taste profile.
#[derive(Debug, Deserialize)]
pub struct SetProfileRequest {
    /// Profile name to set ("default", "minimal", "corporate", "bold", or custom name).
    pub name: String,
    /// Optional custom axis weights.
    #[serde(default)]
    pub axis_weights: Option<std::collections::HashMap<String, f32>>,
    /// Optional custom thresholds.
    #[serde(default)]
    pub thresholds: Option<std::collections::HashMap<String, f32>>,
    /// Optional style preference.
    #[serde(default)]
    pub style_preference: Option<String>,
}

/// Stored taste report entry.
#[derive(Debug, Serialize)]
pub struct StoredReport {
    pub report: TasteReport,
    pub profile: String,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /projects/:id/taste/score — run full taste pipeline.
///
/// Collects all UI files from the project's generated directory, builds a
/// `TasteContext`, runs the pipeline, optionally generates a redesign plan,
/// and stores the report for later retrieval.
pub async fn score_taste_full(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(body): Json<ScoreRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    access
        .require_scope(&Scope::ProjectWrite)
        .map_err(|_| ApiError::Forbidden("project:write scope required".into()))?;
    let project_id = access.project_id.clone();
    let project_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");
    if !project_dir.exists() {
        return Err(ApiError::BadRequest(
            "No generated code for this project".into(),
        ));
    }

    // Collect UI files
    let file_paths =
        crate::file_utils::collect_files_by_ext(&project_dir, &["tsx", "jsx", "css", "html", "ts"]);
    let mut files = Vec::new();
    for path in &file_paths {
        if let Ok(content) = std::fs::read_to_string(path) {
            let rel = path
                .strip_prefix(&project_dir)
                .unwrap_or(path)
                .display()
                .to_string();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_string();
            files.push(ProjectFile {
                path: rel,
                content,
                extension: ext,
            });
        }
    }

    if files.is_empty() {
        return Err(ApiError::BadRequest(
            "No UI files found in generated project".into(),
        ));
    }

    // Per-tenant fair-share + global LLM concurrency. The taste pipeline can
    // call out to an LLM for redesign planning when scores fall below the
    // threshold; we gate at the entry point rather than guessing whether
    // each individual run will hit the network.
    if let Err(reason) = app
        .tenant_rate_limiter
        .check(&access.tenant_id, "generation")
    {
        return Err(ApiError::TooManyRequests(reason));
    }
    let _llm_slot = app
        .rate_limiter
        .acquire_llm_slot()
        .await
        .map_err(|e| ApiError::TooManyRequests(format!("taste queue is full: {e}")))?;

    // Load or use default profile
    let profile_name = body.profile.as_deref().unwrap_or("default");
    let _profile = match profile_name {
        "minimal" => TasteProfile::minimal(),
        "corporate" => TasteProfile::corporate(),
        "bold" => TasteProfile::bold(),
        _ => TasteProfile::default_profile(),
    };

    let context = TasteContext {
        files,
        screenshots: None,
        app_type: body.app_type.unwrap_or_else(|| "saas".to_string()),
        domain: body.domain.unwrap_or_else(|| "general".to_string()),
        style_preference: _profile.style_preference.clone(),
    };

    // Run the pipeline
    let pipeline = TastePipeline::default_pipeline();
    let report = pipeline.score_with_plan(&context, body.threshold);

    // Store the report in the database. Schema lives in migration 002.
    let report_json = serde_json::to_string(&report).unwrap_or_default();
    {
        let db = app.db.lock().await;
        db.execute(
            "INSERT INTO taste_reports (project_id, profile, overall_score, report_json) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![project_id, profile_name, report.overall_score, report_json],
        ).map_err(|e| ApiError::Internal(e.to_string()))?;
    }

    Ok(Json(serde_json::to_value(&report).unwrap_or(json!({}))))
}

/// POST /projects/:id/taste/profile — set taste profile for a project.
///
/// Stores the profile configuration so future scoring runs use it.
pub async fn set_taste_profile(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(body): Json<SetProfileRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    access
        .require_scope(&Scope::ProjectWrite)
        .map_err(|_| ApiError::Forbidden("project:write scope required".into()))?;
    let project_id = access.project_id.clone();
    // Build the profile
    let profile = if let (Some(weights), Some(thresholds)) =
        (body.axis_weights.as_ref(), body.thresholds.as_ref())
    {
        TasteProfile {
            name: body.name.clone(),
            axis_weights: weights.clone(),
            thresholds: thresholds.clone(),
            style_preference: body.style_preference.clone(),
        }
    } else {
        match body.name.as_str() {
            "minimal" => TasteProfile::minimal(),
            "corporate" => TasteProfile::corporate(),
            "bold" => TasteProfile::bold(),
            _ => TasteProfile::default_profile(),
        }
    };

    let profile_json = serde_json::to_string(&profile).unwrap_or_default();

    // Schema lives in migration 002.
    let db = app.db.lock().await;
    db.execute(
        "INSERT INTO taste_profiles (project_id, profile_json) VALUES (?1, ?2)
         ON CONFLICT(project_id) DO UPDATE SET profile_json = ?2, updated_at = datetime('now')",
        rusqlite::params![project_id, profile_json],
    )
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(json!({
        "project_id": project_id,
        "profile": serde_json::to_value(&profile).unwrap_or_default(),
        "saved": true,
    })))
}

/// GET /projects/:id/taste/report — get the latest taste report.
///
/// Returns the most recent report stored for this project, or a message
/// indicating no report exists yet.
pub async fn get_taste_report(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    let project_id = access.project_id.clone();
    let db = app.db.lock().await;

    // Schema lives in migration 002.
    let result: Result<(String, String, f64, String), _> = db.query_row(
        "SELECT profile, report_json, overall_score, created_at
         FROM taste_reports
         WHERE project_id = ?1
         ORDER BY created_at DESC
         LIMIT 1",
        rusqlite::params![project_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    );

    match result {
        Ok((profile, report_json, overall_score, created_at)) => {
            let report: serde_json::Value = serde_json::from_str(&report_json).unwrap_or(json!({}));
            Ok(Json(json!({
                "project_id": project_id,
                "profile": profile,
                "overall_score": overall_score,
                "report": report,
                "created_at": created_at,
            })))
        }
        Err(_) => Ok(Json(json!({
            "project_id": project_id,
            "exists": false,
            "message": "No taste report found. Run POST /projects/:id/taste/score to generate one.",
        }))),
    }
}

// ---------------------------------------------------------------------------
// Legacy simple-scoring handlers
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RedesignRequest {
    #[serde(default)]
    pub threshold: Option<u32>,
    #[serde(default)]
    pub max_mutations: Option<usize>,
    #[serde(default)]
    pub target_axes: Vec<String>,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Deserialize)]
pub struct CopyRewriteRequest {
    pub file: String,
    pub section: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CopyRewriteResult {
    pub file: String,
    pub rewrites: Vec<CopyRewrite>,
    pub applied: bool,
}

#[derive(Debug, Serialize)]
pub struct CopyRewrite {
    pub original: String,
    pub improved: String,
    pub reason: String,
}

/// GET /projects/:id/taste — score project UX quality
pub async fn score_taste(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    let project_id = access.project_id.clone();
    let project_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");
    if !project_dir.exists() {
        return Err(ApiError::BadRequest("No generated code".into()));
    }

    let score = taste_engine::score_project(&project_dir);
    Ok(Json(serde_json::to_value(score).unwrap_or(json!({}))))
}

/// GET /projects/:id/decisions — get architecture decisions for this project
pub async fn get_decisions(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    let project_id = access.project_id.clone();
    let decisions_path = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated")
        .join(".nexus")
        .join("decisions.json");

    if decisions_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&decisions_path) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                return Ok(Json(val));
            }
        }
    }

    Ok(Json(
        json!({ "exists": false, "message": "No decisions recorded yet. Run intent-to-app or plan generation to create them." }),
    ))
}

/// POST /projects/:id/taste/redesign — surgical UI improvement from taste scores
pub async fn redesign_taste(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(body): Json<RedesignRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    access
        .require_scope(&Scope::ProjectWrite)
        .map_err(|_| ApiError::Forbidden("project:write scope required".into()))?;
    let project_id = access.project_id.clone();
    let project_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");
    if !project_dir.exists() {
        return Err(ApiError::BadRequest("No generated code".into()));
    }

    let config = taste_redesign::RedesignConfig {
        threshold: body.threshold.unwrap_or(70),
        max_mutations: body.max_mutations.unwrap_or(5),
        target_axes: body.target_axes,
        dry_run: body.dry_run,
    };

    let result = taste_redesign::redesign(&app, &project_id, &project_dir, &config)
        .await
        .map_err(ApiError::Internal)?;

    Ok(Json(serde_json::to_value(result).unwrap_or(json!({}))))
}

/// POST /projects/:id/taste/rewrite-copy — improve UI copy in a specific file
pub async fn rewrite_copy(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
    Json(body): Json<CopyRewriteRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    access
        .require_scope(&Scope::ProjectWrite)
        .map_err(|_| ApiError::Forbidden("project:write scope required".into()))?;
    let project_id = access.project_id.clone();
    let project_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");
    if !project_dir.exists() {
        return Err(ApiError::BadRequest("No generated code".into()));
    }

    let target_path = project_dir.join(&body.file);
    if !target_path.exists() {
        return Err(ApiError::BadRequest(format!(
            "File not found: {}",
            body.file
        )));
    }
    // Reject path traversal: canonicalize and require the target stay inside project_dir.
    let canonical_target = target_path
        .canonicalize()
        .map_err(|e| ApiError::BadRequest(format!("Invalid file path: {e}")))?;
    let canonical_root = project_dir
        .canonicalize()
        .map_err(|e| ApiError::Internal(format!("Project dir not readable: {e}")))?;
    if !canonical_target.starts_with(&canonical_root) {
        return Err(ApiError::BadRequest(
            "File path escapes project directory".into(),
        ));
    }

    let content = std::fs::read_to_string(&canonical_target)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let rewrites = generate_copy_improvements(&content, body.section.as_deref());

    if rewrites.is_empty() {
        return Ok(Json(json!({
            "file": body.file,
            "rewrites": [],
            "applied": false,
            "message": "No copy improvements identified",
        })));
    }

    // Apply each rewrite using `replacen(.., 1)` so a short/common original
    // string doesn't nuke unrelated sections of the file (global replace
    // would corrupt any document containing repeated snippets — e.g. a copy
    // rewrite of "Learn more" wiping every CTA in the page).
    let mut improved_content = content.clone();
    for rw in &rewrites {
        improved_content = improved_content.replacen(&rw.original, &rw.improved, 1);
    }

    // Atomic write: tmp file → fsync → rename. A crash mid-write previously
    // could leave the user's source truncated in `generated/`.
    {
        use std::io::Write;
        let tmp = canonical_target.with_extension(
            canonical_target
                .extension()
                .map(|e| format!("{}.nexus-tmp", e.to_string_lossy()))
                .unwrap_or_else(|| "nexus-tmp".to_string()),
        );
        let mut f = std::fs::File::create(&tmp)
            .map_err(|e| ApiError::Internal(format!("create tmp: {e}")))?;
        f.write_all(improved_content.as_bytes())
            .map_err(|e| ApiError::Internal(format!("write tmp: {e}")))?;
        f.sync_all()
            .map_err(|e| ApiError::Internal(format!("fsync tmp: {e}")))?;
        drop(f);
        std::fs::rename(&tmp, &canonical_target)
            .map_err(|e| ApiError::Internal(format!("rename tmp: {e}")))?;
    }

    let result = CopyRewriteResult {
        file: body.file,
        rewrites,
        applied: true,
    };

    Ok(Json(serde_json::to_value(result).unwrap_or(json!({}))))
}

/// GET /projects/:id/taste/history — get taste score history
pub async fn taste_history(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    let project_id = access.project_id.clone();
    let db = app.db.lock().await;
    let mut stmt = db.prepare(
        "SELECT overall, visual, ux, conversion, modern_ui, accessibility, responsiveness, improvements_applied, created_at
         FROM taste_history WHERE project_id = ?1 ORDER BY created_at ASC LIMIT 50"
    ).map_err(|e| ApiError::Internal(e.to_string()))?;

    let rows: Vec<serde_json::Value> = stmt
        .query_map(rusqlite::params![project_id], |row| {
            Ok(json!({
                "overall": row.get::<_, i64>(0)?,
                "visual": row.get::<_, i64>(1)?,
                "ux": row.get::<_, i64>(2)?,
                "conversion": row.get::<_, i64>(3)?,
                "modern_ui": row.get::<_, i64>(4)?,
                "accessibility": row.get::<_, i64>(5)?,
                "responsiveness": row.get::<_, i64>(6)?,
                "improvements_applied": row.get::<_, i64>(7)?,
                "created_at": row.get::<_, String>(8)?,
            }))
        })
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(Json(json!({ "project_id": project_id, "history": rows })))
}

// ---------------------------------------------------------------------------
// Copy improvement logic (deterministic, no LLM)
// ---------------------------------------------------------------------------

fn generate_copy_improvements(content: &str, _section: Option<&str>) -> Vec<CopyRewrite> {
    let mut rewrites = Vec::new();

    let weak_ctas = [
        ("Submit", "Get Started"),
        ("Click here", "Explore now"),
        ("Learn more", "See how it works"),
        ("Go", "Launch"),
        ("OK", "Got it"),
        ("Cancel", "Not now"),
        ("Send", "Send message"),
        ("Save", "Save changes"),
        ("Update", "Apply changes"),
        ("Delete", "Remove"),
    ];

    for (weak, strong) in &weak_ctas {
        let patterns = [
            format!(">{}</", weak),
            format!(">{} <", weak),
            format!("\"{}\"", weak),
        ];
        for pattern in &patterns {
            if content.contains(pattern.as_str()) {
                rewrites.push(CopyRewrite {
                    original: pattern.clone(),
                    improved: pattern.replace(weak, strong),
                    reason: format!("'{}' is passive. '{}' drives action.", weak, strong),
                });
                break;
            }
        }
    }

    let generic_headings = [
        ("Features", "What you get"),
        ("About", "Our story"),
        ("Contact Us", "Let's talk"),
        ("Welcome", "Good to see you"),
        ("Dashboard", "Your overview"),
        ("Loading...", "Getting things ready\u{2026}"),
        ("Error", "Something went wrong"),
        ("Not Found", "This page doesn't exist"),
    ];

    for (generic, specific) in &generic_headings {
        let tag_pattern = format!("><h[1-6]>{}</", generic);
        let simple_pattern = format!(">{}</h", generic);
        if content.contains(&format!(">{}<", generic)) {
            rewrites.push(CopyRewrite {
                original: format!(">{}<", generic),
                improved: format!(">{}<", specific),
                reason: format!("'{}' is generic. '{}' is more human.", generic, specific),
            });
        } else if content.contains(&simple_pattern) {
            let _ = (tag_pattern, specific);
        }
    }

    rewrites.truncate(8);
    rewrites
}

/// POST /intent/analyze-full — analyze intent AND produce architecture decisions
pub async fn analyze_with_decisions(
    State(app): State<Arc<AppState>>,
    Json(body): Json<crate::handlers::intent_to_app::AnalyzeReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let intent = crate::intent_engine::analyze_flat(&body.description);
    let decisions = crate::decision_engine::decide(&intent);
    let decision_context = crate::decision_engine::to_prompt_context(&decisions);

    let registry = app.legacy_plugin_registry.read().await;
    let plugin_rules = registry.all_decision_rules();
    let overrides: Vec<_> = plugin_rules
        .iter()
        .filter(|(_pid, rule)| {
            rule.when.iter().any(|keyword| {
                body.description
                    .to_lowercase()
                    .contains(&keyword.to_lowercase())
            })
        })
        .map(|(pid, rule)| {
            json!({
                "plugin": pid,
                "area": rule.area,
                "override": rule.then,
                "reason": rule.description,
                "priority": rule.priority,
            })
        })
        .collect();
    drop(registry);

    Ok(Json(json!({
        "intent": serde_json::to_value(&intent).unwrap_or_default(),
        "decisions": serde_json::to_value(&decisions).unwrap_or_default(),
        "decision_context": decision_context,
        "plugin_overrides": overrides,
    })))
}
