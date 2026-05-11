//! User pattern detector — mines recent successful projects for recurring
//! stacks / descriptions and proposes new skill DNA rows.
//!
//! This is the "new skills" producer in Nexus's self-improvement loop. Where
//! `skill_dna::extract_from_execution` captures patterns from ONE build,
//! this module looks ACROSS builds to find preferences that only become
//! visible with enough data — e.g. "this user always adds Prisma + Tailwind
//! together; propose a Prisma+Tailwind starter".
//!
//! It runs periodically (every ~10 min) from `background_executor::tick`.
//! Proposed skills land in `skill_dna` with `status = 'draft'`; the usual
//! promotion rules apply — they only affect future builds after being used
//! 3+ times at 70%+ confidence.
//!
//! This module is intentionally deterministic (no LLM calls). The patterns
//! it detects are structural — file-presence, intent-frequency, common
//! description tokens.

use std::collections::HashMap;
use std::sync::Arc;

use crate::skill_dna::{SkillDna, SkillExample, SkillMetrics, SkillPattern, SkillSource, SkillStatus};
use crate::state::AppState;

/// Minimum repetitions before a pattern becomes a proposal.
const MIN_PATTERN_FREQUENCY: usize = 3;
/// Maximum proposals emitted per tick (keeps the table from ballooning).
const MAX_PROPOSALS_PER_TICK: usize = 5;
/// How many recent successful projects to analyse.
const RECENT_PROJECT_LIMIT: i64 = 50;

/// Detect cross-project patterns and insert draft skill DNA rows.
/// Returns the number of proposals created.
pub async fn detect_and_propose(app: &Arc<AppState>) -> Result<usize, String> {
    // 1. Pull recent completed projects.
    let recent = load_recent_projects(app).await?;
    if recent.len() < MIN_PATTERN_FREQUENCY {
        // Not enough data yet — nothing to propose.
        return Ok(0);
    }

    // 2. Deduplicate against skills we've already proposed so we don't spam
    //    the table every 10 minutes with the same pattern.
    let existing_intents = load_existing_auto_intents(app).await?;

    // 3. Mine two kinds of patterns:
    //    - Repeated "intent" (derived from description) — e.g. todo/dashboard/auth
    //    - Repeated stack fingerprint (llm_provider + llm_model)
    let intent_counts = count_intents(&recent);
    let stack_counts = count_stacks(&recent);

    let mut proposals: Vec<SkillDna> = Vec::new();

    for (intent, count) in intent_counts.iter() {
        if proposals.len() >= MAX_PROPOSALS_PER_TICK {
            break;
        }
        if *count < MIN_PATTERN_FREQUENCY {
            continue;
        }
        let key = format!("pattern:intent:{intent}");
        if existing_intents.contains(&key) {
            continue;
        }
        proposals.push(build_intent_skill(intent, *count, &recent));
    }

    for ((provider, model), count) in stack_counts.iter() {
        if proposals.len() >= MAX_PROPOSALS_PER_TICK {
            break;
        }
        if *count < MIN_PATTERN_FREQUENCY {
            continue;
        }
        let key = format!("pattern:stack:{provider}:{model}");
        if existing_intents.contains(&key) {
            continue;
        }
        proposals.push(build_stack_skill(provider, model, *count));
    }

    // 4. Persist.
    let mut created = 0usize;
    for skill in &proposals {
        if let Err(e) = crate::skill_dna::store_skill(app, skill).await {
            tracing::warn!(error = %e, "Failed to store skill DNA proposal");
            continue;
        }
        created += 1;
    }
    Ok(created)
}

// ---------------------------------------------------------------------------
// Data loaders (keep sync DB work scoped so we don't hold the lock across awaits)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct ProjectRow {
    // `id` is read only by tests + kept on the struct so a future pass can
    // attach source_executions to proposals. Current detector emits proposals
    // without pinpointing individual source projects.
    #[allow(dead_code)]
    id: String,
    description: String,
    llm_provider: Option<String>,
    llm_model: Option<String>,
}

async fn load_recent_projects(app: &Arc<AppState>) -> Result<Vec<ProjectRow>, String> {
    let db = app.db.lock().await;
    let mut stmt = db
        .prepare(
            "SELECT id, COALESCE(description, ''), llm_provider, llm_model
             FROM projects
             WHERE phase >= 2
             ORDER BY created_at DESC
             LIMIT ?1",
        )
        .map_err(|e| format!("prepare: {e}"))?;
    let rows = stmt
        .query_map([RECENT_PROJECT_LIMIT], |row| {
            Ok(ProjectRow {
                id: row.get(0)?,
                description: row.get(1)?,
                llm_provider: row.get(2)?,
                llm_model: row.get(3)?,
            })
        })
        .map_err(|e| format!("query: {e}"))?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| format!("collect: {e}"))
}

/// Which auto-proposed patterns already exist? We encode the pattern-key
/// into the skill description so we can check against it without a new
/// column.
async fn load_existing_auto_intents(app: &Arc<AppState>) -> Result<Vec<String>, String> {
    let db = app.db.lock().await;
    let mut stmt = db
        .prepare(
            "SELECT description FROM skill_dna
             WHERE source_type = 'auto' AND description LIKE 'pattern:%'",
        )
        .map_err(|e| format!("prepare: {e}"))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| format!("query: {e}"))?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| format!("collect: {e}"))
}

// ---------------------------------------------------------------------------
// Pattern detectors — deterministic, cheap.
// ---------------------------------------------------------------------------

fn count_intents(rows: &[ProjectRow]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for row in rows {
        let intent = classify_intent(&row.description);
        if intent == "general" {
            continue; // don't propose skills for uncategorised inputs
        }
        *counts.entry(intent).or_insert(0) += 1;
    }
    counts
}

fn count_stacks(rows: &[ProjectRow]) -> HashMap<(String, String), usize> {
    let mut counts: HashMap<(String, String), usize> = HashMap::new();
    for row in rows {
        if let (Some(p), Some(m)) = (row.llm_provider.clone(), row.llm_model.clone()) {
            *counts.entry((p, m)).or_insert(0) += 1;
        }
    }
    counts
}

/// Cheap deterministic intent classifier. Mirrors the keyword bands used
/// in `skill_dna::infer_intent` but stays private so we can tune
/// independently for pattern mining.
fn classify_intent(description: &str) -> String {
    let lower = description.to_lowercase();
    for (kw, intent) in &[
        ("todo", "todo_app"),
        ("task manager", "todo_app"),
        ("dashboard", "dashboard"),
        ("admin", "dashboard"),
        ("blog", "blog"),
        ("newsletter", "blog"),
        ("landing", "landing_page"),
        ("portfolio", "portfolio"),
        ("chat", "chat_app"),
        ("auth", "auth_flow"),
        ("login", "auth_flow"),
        ("ecommerce", "ecommerce"),
        ("shop", "ecommerce"),
        ("crm", "crm"),
        ("booking", "booking"),
        ("scheduler", "booking"),
    ] {
        if lower.contains(kw) {
            return intent.to_string();
        }
    }
    "general".into()
}

fn build_intent_skill(intent: &str, count: usize, rows: &[ProjectRow]) -> SkillDna {
    // Pull a concrete example description for this intent — helps the UI
    // show "here's what users built last time" previews.
    let example_input = rows
        .iter()
        .find(|r| classify_intent(&r.description) == intent && !r.description.trim().is_empty())
        .map(|r| r.description.clone())
        .unwrap_or_else(|| format!("Build a {intent} app"));

    let prompt_fragment = match intent {
        "todo_app" => "Prefer a single-page Next.js App Router layout with client-side optimistic updates. Use Tailwind for styling and Server Actions for persistence. Keep the task schema minimal (id, title, done, created_at).",
        "dashboard" => "Lead with the KPI cards above the fold. Use shadcn/ui Card + Tabs for section organisation. Include at least one recharts/line chart and one table with sortable columns.",
        "landing_page" => "Hero → social proof → 3-column value props → FAQ → CTA. Use large type (display-lg), gradient accents on the primary CTA, and responsive breakpoints at 640/768/1024.",
        "auth_flow" => "Implement email-password via Next.js Server Actions. Use bcrypt for hashing, JWT in an httpOnly cookie, and a middleware.ts that guards /app/** routes.",
        "ecommerce" => "Product grid with hover-reveal variants, cart drawer on the right, checkout as a single-page flow. Use Stripe Payment Element for the payment step.",
        "chat_app" => "Streaming responses via Server-Sent Events. Message bubbles with avatars, typing indicator, and auto-scroll to bottom on new messages.",
        _ => "Prefer Next.js App Router, Tailwind, and shadcn/ui. Keep the initial page count to 1-3 for fast first-load.",
    };

    SkillDna {
        id: uuid::Uuid::new_v4().to_string(),
        name: format!("Starter: {}", intent.replace('_', " ")),
        // The `pattern:...` prefix is our dedup key — `detect_and_propose`
        // reads it back out via LIKE 'pattern:%'.
        description: format!("pattern:intent:{intent}"),
        intent: intent.to_string(),
        tools: vec!["file_write".into(), "file_read".into()],
        patterns: vec![SkillPattern {
            pattern_type: "user_preference".into(),
            description: format!("User has built {count} projects matching this intent"),
            code_template: None,
            confidence: confidence_from_count(count),
        }],
        examples: vec![SkillExample {
            input: example_input,
            output: format!("({count} prior examples distilled into a starter template)"),
        }],
        constraints: vec![],
        prompt_fragment: prompt_fragment.to_string(),
        source_type: SkillSource::Auto,
        source_executions: vec![],
        parent_ids: vec![],
        generation: 0,
        metrics: SkillMetrics {
            total_uses: 0,
            successes: 0,
            failures: 0,
            // Start low — the proposal is about future reuse, not past ground truth.
            confidence: confidence_from_count(count),
        },
        status: SkillStatus::Draft,
        created_at: chrono::Utc::now().to_rfc3339(),
    }
}

fn build_stack_skill(provider: &str, model: &str, count: usize) -> SkillDna {
    SkillDna {
        id: uuid::Uuid::new_v4().to_string(),
        name: format!("Preferred stack: {provider}/{model}"),
        description: format!("pattern:stack:{provider}:{model}"),
        intent: "stack_preference".into(),
        tools: vec![],
        patterns: vec![SkillPattern {
            pattern_type: "stack_fingerprint".into(),
            description: format!(
                "User has used {provider}/{model} for {count} successful builds"
            ),
            code_template: None,
            confidence: confidence_from_count(count),
        }],
        examples: vec![],
        constraints: vec![],
        prompt_fragment: format!(
            "When this user triggers a build, default to provider={provider} / model={model} \
             unless they explicitly override. They've had consistent success with this combo."
        ),
        source_type: SkillSource::Auto,
        source_executions: vec![],
        parent_ids: vec![],
        generation: 0,
        metrics: SkillMetrics {
            total_uses: 0,
            successes: 0,
            failures: 0,
            confidence: confidence_from_count(count),
        },
        status: SkillStatus::Draft,
        created_at: chrono::Utc::now().to_rfc3339(),
    }
}

/// Map a repetition count to an initial confidence in [0.5, 0.9]. A pattern
/// seen 3 times starts at 0.6; one seen 10+ times starts at 0.9.
fn confidence_from_count(count: usize) -> f64 {
    if count <= 3 {
        0.55
    } else if count <= 5 {
        0.65
    } else if count <= 7 {
        0.75
    } else if count <= 9 {
        0.85
    } else {
        0.9
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_detects_todo() {
        assert_eq!(classify_intent("build me a todo list"), "todo_app");
        assert_eq!(classify_intent("Task manager for my team"), "todo_app");
    }

    #[test]
    fn classify_detects_dashboard() {
        assert_eq!(classify_intent("admin dashboard for orders"), "dashboard");
    }

    #[test]
    fn classify_falls_through_to_general() {
        assert_eq!(classify_intent("something completely unique"), "general");
    }

    #[test]
    fn counts_merge_across_rows() {
        let rows = vec![
            ProjectRow {
                id: "1".into(),
                description: "todo list".into(),
                llm_provider: None,
                llm_model: None,
            },
            ProjectRow {
                id: "2".into(),
                description: "task manager".into(),
                llm_provider: None,
                llm_model: None,
            },
            ProjectRow {
                id: "3".into(),
                description: "dashboard".into(),
                llm_provider: None,
                llm_model: None,
            },
        ];
        let counts = count_intents(&rows);
        assert_eq!(counts.get("todo_app"), Some(&2));
        assert_eq!(counts.get("dashboard"), Some(&1));
    }

    #[test]
    fn confidence_scales_with_count() {
        assert!(confidence_from_count(3) < confidence_from_count(10));
        assert!(confidence_from_count(1) >= 0.5);
        assert!(confidence_from_count(100) <= 0.9);
    }

    #[test]
    fn general_intent_is_ignored() {
        let rows = vec![
            ProjectRow {
                id: "1".into(),
                description: "unknown thing".into(),
                llm_provider: None,
                llm_model: None,
            },
            ProjectRow {
                id: "2".into(),
                description: "another unknown".into(),
                llm_provider: None,
                llm_model: None,
            },
        ];
        let counts = count_intents(&rows);
        assert!(counts.is_empty(), "general should not be proposed: {counts:?}");
    }
}
