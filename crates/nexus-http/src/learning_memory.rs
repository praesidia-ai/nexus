//! Unified Adaptive Learning Memory System.
//!
//! Consolidates all learning signals — stack decisions, taste scores, guarantee
//! results, build outcomes, user overrides, feedback, and generation speed — into
//! a single `learning_signals` table.  Provides query methods for best-choice
//! recommendations, style preferences, quality baselines, and dashboard insights.
//!
//! This module is self-contained: it manages its own SQLite database file
//! (`learning_memory.db`) under the provided data directory so it can be used
//! independently of the main `nexus.db` connection.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tracing::info;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// A single learning signal captured from any subsystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningSignal {
    /// Unique identifier for this signal.
    pub id: String,
    /// Tenant that produced the signal.
    pub tenant_id: String,
    /// Optional project scope.
    pub project_id: Option<String>,
    /// What kind of signal this is.
    pub signal_type: SignalType,
    /// Arbitrary context payload (JSON).
    pub context: serde_json::Value,
    /// Arbitrary outcome payload (JSON).
    pub outcome: serde_json::Value,
    /// Numeric quality score (0.0–1.0 typical, but unbounded).
    pub quality: f32,
    /// ISO-8601 timestamp.
    pub timestamp: String,
}

/// Discriminated signal type — each variant carries the minimum data needed
/// for downstream aggregation queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SignalType {
    /// An architecture / stack decision was made.
    StackDecision {
        /// Decision area (e.g. "framework", "database").
        area: String,
        /// The value that was chosen.
        choice: String,
    },
    /// A taste-engine score was computed.
    TasteScore {
        /// Overall taste score (0–100).
        score: f32,
        /// Per-axis breakdown.
        axes: HashMap<String, f32>,
    },
    /// An outcome-guarantee cycle completed.
    GuaranteeResult {
        /// Whether the guarantee passed.
        passed: bool,
        /// Number of repair cycles consumed.
        cycles: u32,
    },
    /// A build finished.
    BuildSuccess {
        /// True if the build succeeded on the first attempt.
        first_try: bool,
    },
    /// The user manually overrode a decision.
    UserOverride {
        /// What was overridden (e.g. "framework").
        what: String,
        /// Original value.
        from: String,
        /// New value chosen by the user.
        to: String,
    },
    /// Explicit user feedback.
    UserFeedback {
        /// Sentiment score (-1.0 to 1.0).
        sentiment: f32,
        /// Free-text context.
        context: String,
    },
    /// A taste-engine redesign suggestion was accepted.
    UserRedesignAccepted {
        /// Which taste axis was redesigned.
        axis: String,
    },
    /// A taste-engine redesign suggestion was rejected.
    UserRedesignRejected {
        /// Which taste axis was rejected.
        axis: String,
    },
    /// Code generation speed measurement.
    GenerationSpeed {
        /// Wall-clock duration in milliseconds.
        duration_ms: u64,
    },
    /// Generated code was accepted (possibly with modifications).
    CodeAccepted {
        /// Number of manual edits the user made after accepting.
        modifications_after: u32,
    },
}

// ---------------------------------------------------------------------------
// Query result types
// ---------------------------------------------------------------------------

/// A learned best-choice recommendation for a decision area.
#[derive(Debug, Clone, Serialize)]
pub struct LearnedPreference {
    /// The recommended choice value.
    pub choice: String,
    /// Confidence in the recommendation (0.0–1.0).
    pub confidence: f32,
    /// How many signals contributed.
    pub sample_size: usize,
    /// Average quality across those signals.
    pub avg_quality: f32,
    /// Human-readable explanation.
    pub explanation: String,
}

/// Aggregated style preferences for a tenant.
#[derive(Debug, Clone, Serialize)]
pub struct StylePreferences {
    /// Most frequently chosen UI style, if any.
    pub preferred_ui_style: Option<String>,
    /// Most frequently chosen theme, if any.
    pub preferred_theme: Option<String>,
    /// Fraction of redesign suggestions that were accepted.
    pub redesign_acceptance_rate: f32,
    /// Most common user overrides as `(what, to)` pairs.
    pub common_overrides: Vec<(String, String)>,
}

/// Quality baseline statistics for an app type.
#[derive(Debug, Clone, Serialize)]
pub struct QualityBaseline {
    /// Average taste score across projects.
    pub avg_taste_score: f32,
    /// Fraction of builds that succeeded on first try.
    pub avg_build_success_rate: f32,
    /// Fraction of guarantee cycles that passed.
    pub avg_guarantee_pass_rate: f32,
    /// Total signals that contributed.
    pub sample_size: usize,
}

/// Dashboard-level learning insights.
#[derive(Debug, Clone, Serialize)]
pub struct LearningInsights {
    /// Top-performing stack choices.
    pub top_stacks: Vec<StackInsight>,
    /// Recurring failure patterns.
    pub common_failures: Vec<FailurePattern>,
    /// Quality trend over time.
    pub quality_trend: Vec<QualityPoint>,
    /// Total number of recorded signals.
    pub total_signals: usize,
}

/// Insight for a single stack choice.
#[derive(Debug, Clone, Serialize)]
pub struct StackInsight {
    /// Decision area.
    pub area: String,
    /// Choice value.
    pub choice: String,
    /// Fraction of associated builds that succeeded.
    pub success_rate: f32,
    /// Average taste score when this choice was used.
    pub avg_taste: f32,
    /// Number of times this choice was observed.
    pub usage_count: usize,
}

/// A recurring failure pattern.
#[derive(Debug, Clone, Serialize)]
pub struct FailurePattern {
    /// Description of the pattern.
    pub pattern: String,
    /// How many times it was seen.
    pub frequency: usize,
    /// When it was last observed (ISO-8601).
    pub last_seen: String,
}

/// A single data point on the quality trend line.
#[derive(Debug, Clone, Serialize)]
pub struct QualityPoint {
    /// Date (YYYY-MM-DD).
    pub date: String,
    /// Average taste score on that date.
    pub avg_taste: f32,
    /// Number of projects that contributed.
    pub projects_count: usize,
}

// ---------------------------------------------------------------------------
// LearningMemory
// ---------------------------------------------------------------------------

/// Persistent adaptive memory backed by a dedicated SQLite database.
///
/// All public methods open a short-lived connection, execute, and close —
/// no long-held locks, no async mutex needed.
pub struct LearningMemory {
    /// Path to the SQLite database file.
    db_path: PathBuf,
}

impl LearningMemory {
    /// Create a new `LearningMemory` that stores its database under `data_dir`.
    pub fn new(data_dir: &Path) -> Self {
        Self {
            db_path: data_dir.join("learning_memory.db"),
        }
    }

    /// Create the `learning_signals` table and indices if they do not exist.
    pub fn init_tables(&self) -> Result<(), String> {
        let conn = self.connect()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS learning_signals (
                id TEXT PRIMARY KEY,
                tenant_id TEXT NOT NULL,
                project_id TEXT,
                signal_type TEXT NOT NULL,
                context TEXT NOT NULL DEFAULT '{}',
                outcome TEXT NOT NULL DEFAULT '{}',
                quality REAL NOT NULL DEFAULT 0.0,
                timestamp TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_signals_tenant ON learning_signals(tenant_id);
            CREATE INDEX IF NOT EXISTS idx_signals_type ON learning_signals(signal_type);",
        )
        .map_err(|e| format!("Failed to create learning_signals table: {e}"))?;
        Ok(())
    }

    /// Record a learning signal.
    pub fn record(&self, signal: &LearningSignal) -> Result<(), String> {
        let conn = self.connect()?;
        let signal_type_tag = signal_type_tag(&signal.signal_type);
        let context_str =
            serde_json::to_string(&signal.context).unwrap_or_else(|_| "{}".into());
        let outcome_str =
            serde_json::to_string(&signal.outcome).unwrap_or_else(|_| "{}".into());

        conn.execute(
            "INSERT INTO learning_signals (id, tenant_id, project_id, signal_type, context, outcome, quality, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                signal.id,
                signal.tenant_id,
                signal.project_id,
                signal_type_tag,
                context_str,
                outcome_str,
                signal.quality,
                signal.timestamp,
            ],
        )
        .map_err(|e| format!("Failed to insert learning signal: {e}"))?;

        info!(
            id = %signal.id,
            tenant = %signal.tenant_id,
            signal_type = signal_type_tag,
            quality = signal.quality,
            "Learning signal recorded"
        );
        Ok(())
    }

    /// Query the best choice for a decision `area`, scoped to `app_type` (stored
    /// in context JSON) and `tenant_id`.
    ///
    /// Returns `None` when there is no relevant data.
    pub fn best_choice(
        &self,
        area: &str,
        app_type: &str,
        tenant_id: &str,
    ) -> Option<LearnedPreference> {
        let conn = self.connect().ok()?;

        // Fetch all stack_decision signals for this area and tenant.
        let mut stmt = conn
            .prepare(
                "SELECT outcome, quality FROM learning_signals
                 WHERE signal_type = 'stack_decision'
                   AND tenant_id = ?1
                   AND json_extract(outcome, '$.area') = ?2",
            )
            .ok()?;

        // Group by choice
        let mut choice_stats: HashMap<String, (f32, usize)> = HashMap::new();

        let rows = stmt
            .query_map(params![tenant_id, area], |row| {
                let outcome_str: String = row.get(0)?;
                let quality: f32 = row.get(1)?;
                Ok((outcome_str, quality))
            })
            .ok()?;

        for row in rows.flatten() {
            let (outcome_str, quality) = row;
            if let Ok(outcome_val) = serde_json::from_str::<serde_json::Value>(&outcome_str) {
                if let Some(choice) = outcome_val.get("choice").and_then(|v| v.as_str()) {
                    // Optionally filter by app_type in context
                    let entry = choice_stats
                        .entry(choice.to_string())
                        .or_insert((0.0, 0));
                    entry.0 += quality;
                    entry.1 += 1;
                }
            }
        }

        // Also check context for app_type filtering if non-empty
        if !app_type.is_empty() {
            // Re-query with app_type filter
            let mut stmt2 = conn
                .prepare(
                    "SELECT outcome, quality FROM learning_signals
                     WHERE signal_type = 'stack_decision'
                       AND tenant_id = ?1
                       AND json_extract(outcome, '$.area') = ?2
                       AND json_extract(context, '$.app_type') = ?3",
                )
                .ok()?;

            let mut filtered_stats: HashMap<String, (f32, usize)> = HashMap::new();
            let rows2 = stmt2
                .query_map(params![tenant_id, area, app_type], |row| {
                    let outcome_str: String = row.get(0)?;
                    let quality: f32 = row.get(1)?;
                    Ok((outcome_str, quality))
                })
                .ok()?;

            for row in rows2.flatten() {
                let (outcome_str, quality) = row;
                if let Ok(outcome_val) = serde_json::from_str::<serde_json::Value>(&outcome_str)
                {
                    if let Some(choice) = outcome_val.get("choice").and_then(|v| v.as_str()) {
                        let entry = filtered_stats
                            .entry(choice.to_string())
                            .or_insert((0.0, 0));
                        entry.0 += quality;
                        entry.1 += 1;
                    }
                }
            }

            // Prefer filtered results when available
            if !filtered_stats.is_empty() {
                choice_stats = filtered_stats;
            }
        }

        if choice_stats.is_empty() {
            return None;
        }

        // Pick the choice with the highest average quality
        let best = choice_stats
            .iter()
            .map(|(choice, (total_q, count))| {
                let avg = total_q / *count as f32;
                (choice.clone(), avg, *count)
            })
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))?;

        let (choice, avg_quality, sample_size) = best;
        let total_samples: usize = choice_stats.values().map(|(_, c)| c).sum();
        let confidence = compute_confidence(sample_size, total_samples);

        Some(LearnedPreference {
            choice: choice.clone(),
            confidence,
            sample_size,
            avg_quality,
            explanation: format!(
                "'{}' had avg quality {:.2} across {} signal(s) for area '{}' (confidence {:.0}%)",
                choice,
                avg_quality,
                sample_size,
                area,
                confidence * 100.0
            ),
        })
    }

    /// Get aggregated style preferences for a tenant.
    pub fn style_preferences(&self, tenant_id: &str) -> StylePreferences {
        let conn = match self.connect() {
            Ok(c) => c,
            Err(_) => return default_style(),
        };

        // Preferred UI style: most common from user_feedback with context containing ui_style
        let preferred_ui_style = conn
            .query_row(
                "SELECT json_extract(outcome, '$.style') FROM learning_signals
                 WHERE signal_type = 'user_feedback' AND tenant_id = ?1
                   AND json_extract(outcome, '$.style') IS NOT NULL
                 GROUP BY json_extract(outcome, '$.style')
                 ORDER BY COUNT(*) DESC LIMIT 1",
                params![tenant_id],
                |row| row.get::<_, String>(0),
            )
            .ok();

        // Preferred theme
        let preferred_theme = conn
            .query_row(
                "SELECT json_extract(outcome, '$.theme') FROM learning_signals
                 WHERE signal_type = 'user_feedback' AND tenant_id = ?1
                   AND json_extract(outcome, '$.theme') IS NOT NULL
                 GROUP BY json_extract(outcome, '$.theme')
                 ORDER BY COUNT(*) DESC LIMIT 1",
                params![tenant_id],
                |row| row.get::<_, String>(0),
            )
            .ok();

        // Redesign acceptance rate
        let accepted: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM learning_signals
                 WHERE signal_type = 'user_redesign_accepted' AND tenant_id = ?1",
                params![tenant_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let rejected: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM learning_signals
                 WHERE signal_type = 'user_redesign_rejected' AND tenant_id = ?1",
                params![tenant_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let redesign_total = accepted + rejected;
        let redesign_acceptance_rate = if redesign_total > 0 {
            accepted as f32 / redesign_total as f32
        } else {
            0.0
        };

        // Common overrides
        let common_overrides = Self::query_common_overrides(&conn, tenant_id);

        StylePreferences {
            preferred_ui_style,
            preferred_theme,
            redesign_acceptance_rate,
            common_overrides,
        }
    }

    /// Get quality baseline statistics, optionally scoped to an app type.
    pub fn quality_baseline(&self, app_type: &str) -> QualityBaseline {
        let conn = match self.connect() {
            Ok(c) => c,
            Err(_) => return default_quality_baseline(),
        };

        let type_filter = if app_type.is_empty() {
            String::new()
        } else {
            format!(
                " AND json_extract(context, '$.app_type') = '{}'",
                app_type.replace('\'', "''")
            )
        };

        // Average taste score
        let avg_taste: f32 = conn
            .query_row(
                &format!(
                    "SELECT COALESCE(AVG(quality), 0.0) FROM learning_signals
                     WHERE signal_type = 'taste_score'{type_filter}"
                ),
                [],
                |row| row.get(0),
            )
            .unwrap_or(0.0);

        // Build success rate
        let build_total: i64 = conn
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM learning_signals
                     WHERE signal_type = 'build_success'{type_filter}"
                ),
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let build_first_try: i64 = conn
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM learning_signals
                     WHERE signal_type = 'build_success'
                       AND json_extract(outcome, '$.first_try') = 1{type_filter}"
                ),
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let avg_build_success_rate = if build_total > 0 {
            build_first_try as f32 / build_total as f32
        } else {
            0.0
        };

        // Guarantee pass rate
        let guarantee_total: i64 = conn
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM learning_signals
                     WHERE signal_type = 'guarantee_result'{type_filter}"
                ),
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let guarantee_passed: i64 = conn
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM learning_signals
                     WHERE signal_type = 'guarantee_result'
                       AND json_extract(outcome, '$.passed') = 1{type_filter}"
                ),
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let avg_guarantee_pass_rate = if guarantee_total > 0 {
            guarantee_passed as f32 / guarantee_total as f32
        } else {
            0.0
        };

        let sample_size = (build_total + guarantee_total) as usize;

        QualityBaseline {
            avg_taste_score: avg_taste,
            avg_build_success_rate,
            avg_guarantee_pass_rate,
            sample_size,
        }
    }

    /// Get learning insights for a tenant dashboard.
    pub fn insights(&self, tenant_id: &str) -> LearningInsights {
        let conn = match self.connect() {
            Ok(c) => c,
            Err(_) => return default_insights(),
        };

        let total_signals: usize = conn
            .query_row(
                "SELECT COUNT(*) FROM learning_signals WHERE tenant_id = ?1",
                params![tenant_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0) as usize;

        let top_stacks = Self::query_top_stacks(&conn, tenant_id);
        let common_failures = Self::query_common_failures(&conn, tenant_id);
        let quality_trend = Self::query_quality_trend(&conn, tenant_id);

        LearningInsights {
            top_stacks,
            common_failures,
            quality_trend,
            total_signals,
        }
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Open a short-lived connection to the learning memory database.
    fn connect(&self) -> Result<Connection, String> {
        Connection::open(&self.db_path)
            .map_err(|e| format!("Failed to open learning_memory.db: {e}"))
    }

    /// Query top stack choices by success rate and taste.
    fn query_top_stacks(conn: &Connection, tenant_id: &str) -> Vec<StackInsight> {
        let mut stmt = match conn.prepare(
            "SELECT
                json_extract(outcome, '$.area') AS area,
                json_extract(outcome, '$.choice') AS choice,
                COUNT(*) AS cnt,
                AVG(quality) AS avg_q
             FROM learning_signals
             WHERE signal_type = 'stack_decision' AND tenant_id = ?1
               AND area IS NOT NULL AND choice IS NOT NULL
             GROUP BY area, choice
             ORDER BY avg_q DESC
             LIMIT 20",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        stmt.query_map(params![tenant_id], |row| {
            Ok(StackInsight {
                area: row.get::<_, String>(0).unwrap_or_default(),
                choice: row.get::<_, String>(1).unwrap_or_default(),
                success_rate: row.get::<_, f32>(3).unwrap_or(0.0), // using avg quality as proxy
                avg_taste: row.get::<_, f32>(3).unwrap_or(0.0),
                usage_count: row.get::<_, i64>(2).unwrap_or(0) as usize,
            })
        })
        .ok()
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    /// Query recurring failure patterns.
    fn query_common_failures(conn: &Connection, tenant_id: &str) -> Vec<FailurePattern> {
        let mut stmt = match conn.prepare(
            "SELECT
                json_extract(outcome, '$.pattern') AS pattern,
                COUNT(*) AS cnt,
                MAX(timestamp) AS last_ts
             FROM learning_signals
             WHERE tenant_id = ?1
               AND (signal_type = 'guarantee_result' AND json_extract(outcome, '$.passed') = 0)
             GROUP BY pattern
             ORDER BY cnt DESC
             LIMIT 10",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        stmt.query_map(params![tenant_id], |row| {
            Ok(FailurePattern {
                pattern: row
                    .get::<_, String>(0)
                    .unwrap_or_else(|_| "unknown".into()),
                frequency: row.get::<_, i64>(1).unwrap_or(0) as usize,
                last_seen: row
                    .get::<_, String>(2)
                    .unwrap_or_else(|_| "unknown".into()),
            })
        })
        .ok()
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    /// Query quality trend over time (daily averages).
    fn query_quality_trend(conn: &Connection, tenant_id: &str) -> Vec<QualityPoint> {
        let mut stmt = match conn.prepare(
            "SELECT
                DATE(timestamp) AS day,
                AVG(quality) AS avg_q,
                COUNT(DISTINCT project_id) AS proj_cnt
             FROM learning_signals
             WHERE tenant_id = ?1 AND signal_type = 'taste_score'
             GROUP BY day
             ORDER BY day DESC
             LIMIT 30",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        stmt.query_map(params![tenant_id], |row| {
            Ok(QualityPoint {
                date: row.get::<_, String>(0).unwrap_or_default(),
                avg_taste: row.get::<_, f32>(1).unwrap_or(0.0),
                projects_count: row.get::<_, i64>(2).unwrap_or(0) as usize,
            })
        })
        .ok()
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    /// Query the most common user overrides for a tenant.
    fn query_common_overrides(conn: &Connection, tenant_id: &str) -> Vec<(String, String)> {
        let mut stmt = match conn.prepare(
            "SELECT
                json_extract(outcome, '$.what') AS what,
                json_extract(outcome, '$.to') AS to_val,
                COUNT(*) AS cnt
             FROM learning_signals
             WHERE signal_type = 'user_override' AND tenant_id = ?1
               AND what IS NOT NULL
             GROUP BY what, to_val
             ORDER BY cnt DESC
             LIMIT 10",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        stmt.query_map(params![tenant_id], |row| {
            Ok((
                row.get::<_, String>(0).unwrap_or_default(),
                row.get::<_, String>(1).unwrap_or_default(),
            ))
        })
        .ok()
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Compute a confidence score that accounts for sample size.
fn compute_confidence(choice_count: usize, total_count: usize) -> f32 {
    if total_count == 0 {
        return 0.0;
    }
    let proportion = choice_count as f32 / total_count as f32;
    // Dampen confidence when sample size is small (ramps to 1.0 at 10 samples)
    let sample_weight = (choice_count as f32 / 10.0).min(1.0);
    proportion * sample_weight + 0.5 * (1.0 - sample_weight)
}

/// Extract the serde tag string for a `SignalType` variant.
fn signal_type_tag(st: &SignalType) -> &'static str {
    match st {
        SignalType::StackDecision { .. } => "stack_decision",
        SignalType::TasteScore { .. } => "taste_score",
        SignalType::GuaranteeResult { .. } => "guarantee_result",
        SignalType::BuildSuccess { .. } => "build_success",
        SignalType::UserOverride { .. } => "user_override",
        SignalType::UserFeedback { .. } => "user_feedback",
        SignalType::UserRedesignAccepted { .. } => "user_redesign_accepted",
        SignalType::UserRedesignRejected { .. } => "user_redesign_rejected",
        SignalType::GenerationSpeed { .. } => "generation_speed",
        SignalType::CodeAccepted { .. } => "code_accepted",
    }
}

/// Default style preferences when no data is available.
fn default_style() -> StylePreferences {
    StylePreferences {
        preferred_ui_style: None,
        preferred_theme: None,
        redesign_acceptance_rate: 0.0,
        common_overrides: Vec::new(),
    }
}

/// Default quality baseline when no data is available.
fn default_quality_baseline() -> QualityBaseline {
    QualityBaseline {
        avg_taste_score: 0.0,
        avg_build_success_rate: 0.0,
        avg_guarantee_pass_rate: 0.0,
        sample_size: 0,
    }
}

/// Default insights when no data is available.
fn default_insights() -> LearningInsights {
    LearningInsights {
        top_stacks: Vec::new(),
        common_failures: Vec::new(),
        quality_trend: Vec::new(),
        total_signals: 0,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    /// Create a temporary `LearningMemory` with tables initialized.
    fn temp_memory() -> (tempfile::TempDir, LearningMemory) {
        let dir = tempfile::tempdir().expect("tempdir");
        let mem = LearningMemory::new(dir.path());
        mem.init_tables().expect("init_tables");
        (dir, mem)
    }

    /// Helper to build a stack-decision signal.
    fn stack_signal(
        tenant: &str,
        area: &str,
        choice: &str,
        quality: f32,
        app_type: &str,
    ) -> LearningSignal {
        LearningSignal {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id: tenant.into(),
            project_id: Some("proj-1".into()),
            signal_type: SignalType::StackDecision {
                area: area.into(),
                choice: choice.into(),
            },
            context: json!({ "app_type": app_type }),
            outcome: json!({ "area": area, "choice": choice }),
            quality,
            timestamp: "2026-04-04T12:00:00Z".into(),
        }
    }

    #[test]
    fn test_record_and_total_signals() {
        let (_dir, mem) = temp_memory();

        let signal = stack_signal("t1", "framework", "nextjs", 0.9, "saas");
        mem.record(&signal).expect("record");

        let insights = mem.insights("t1");
        assert_eq!(insights.total_signals, 1);
    }

    #[test]
    fn test_best_choice_with_multiple_signals() {
        let (_dir, mem) = temp_memory();

        // Record 3 nextjs signals (avg quality 0.8) and 2 remix signals (avg quality 0.6)
        for _ in 0..3 {
            mem.record(&stack_signal("t1", "framework", "nextjs", 0.8, "saas"))
                .unwrap();
        }
        for _ in 0..2 {
            mem.record(&stack_signal("t1", "framework", "remix", 0.6, "saas"))
                .unwrap();
        }

        let pref = mem.best_choice("framework", "saas", "t1");
        assert!(pref.is_some());
        let pref = pref.unwrap();
        assert_eq!(pref.choice, "nextjs");
        assert!((pref.avg_quality - 0.8).abs() < 0.01);
        assert_eq!(pref.sample_size, 3);
        assert!(pref.confidence > 0.0);
    }

    #[test]
    fn test_best_choice_returns_none_for_empty() {
        let (_dir, mem) = temp_memory();
        let pref = mem.best_choice("framework", "saas", "t1");
        assert!(pref.is_none());
    }

    #[test]
    fn test_style_preferences_empty_tenant() {
        let (_dir, mem) = temp_memory();
        let style = mem.style_preferences("unknown-tenant");
        assert_eq!(style.preferred_ui_style, None);
        assert_eq!(style.preferred_theme, None);
        assert_eq!(style.redesign_acceptance_rate, 0.0);
        assert!(style.common_overrides.is_empty());
    }

    #[test]
    fn test_style_preferences_redesign_rate() {
        let (_dir, mem) = temp_memory();

        // 3 accepted, 1 rejected => 75% acceptance
        for _ in 0..3 {
            mem.record(&LearningSignal {
                id: uuid::Uuid::new_v4().to_string(),
                tenant_id: "t1".into(),
                project_id: None,
                signal_type: SignalType::UserRedesignAccepted {
                    axis: "layout".into(),
                },
                context: json!({}),
                outcome: json!({}),
                quality: 0.0,
                timestamp: "2026-04-04T12:00:00Z".into(),
            })
            .unwrap();
        }
        mem.record(&LearningSignal {
            id: uuid::Uuid::new_v4().to_string(),
            tenant_id: "t1".into(),
            project_id: None,
            signal_type: SignalType::UserRedesignRejected {
                axis: "color".into(),
            },
            context: json!({}),
            outcome: json!({}),
            quality: 0.0,
            timestamp: "2026-04-04T12:00:00Z".into(),
        })
        .unwrap();

        let style = mem.style_preferences("t1");
        assert!((style.redesign_acceptance_rate - 0.75).abs() < 0.01);
    }

    #[test]
    fn test_quality_baseline_calculation() {
        let (_dir, mem) = temp_memory();

        // Taste scores
        for q in [0.7, 0.8, 0.9] {
            mem.record(&LearningSignal {
                id: uuid::Uuid::new_v4().to_string(),
                tenant_id: "t1".into(),
                project_id: Some("proj-1".into()),
                signal_type: SignalType::TasteScore {
                    score: q * 100.0,
                    axes: HashMap::new(),
                },
                context: json!({ "app_type": "saas" }),
                outcome: json!({}),
                quality: q,
                timestamp: "2026-04-04T12:00:00Z".into(),
            })
            .unwrap();
        }

        // Build results: 2 first-try, 1 not
        for first_try in [true, true, false] {
            mem.record(&LearningSignal {
                id: uuid::Uuid::new_v4().to_string(),
                tenant_id: "t1".into(),
                project_id: Some("proj-1".into()),
                signal_type: SignalType::BuildSuccess { first_try },
                context: json!({ "app_type": "saas" }),
                outcome: json!({ "first_try": first_try }),
                quality: if first_try { 1.0 } else { 0.0 },
                timestamp: "2026-04-04T12:00:00Z".into(),
            })
            .unwrap();
        }

        // Guarantee results: 1 passed, 1 failed
        for passed in [true, false] {
            mem.record(&LearningSignal {
                id: uuid::Uuid::new_v4().to_string(),
                tenant_id: "t1".into(),
                project_id: Some("proj-1".into()),
                signal_type: SignalType::GuaranteeResult {
                    passed,
                    cycles: 2,
                },
                context: json!({ "app_type": "saas" }),
                outcome: json!({ "passed": passed }),
                quality: if passed { 1.0 } else { 0.0 },
                timestamp: "2026-04-04T12:00:00Z".into(),
            })
            .unwrap();
        }

        let baseline = mem.quality_baseline("saas");
        assert!((baseline.avg_taste_score - 0.8).abs() < 0.01);
        assert!((baseline.avg_build_success_rate - 2.0 / 3.0).abs() < 0.01);
        assert!((baseline.avg_guarantee_pass_rate - 0.5).abs() < 0.01);
        assert_eq!(baseline.sample_size, 5); // 3 builds + 2 guarantees
    }

    #[test]
    fn test_quality_baseline_empty() {
        let (_dir, mem) = temp_memory();
        let baseline = mem.quality_baseline("");
        assert_eq!(baseline.avg_taste_score, 0.0);
        assert_eq!(baseline.avg_build_success_rate, 0.0);
        assert_eq!(baseline.avg_guarantee_pass_rate, 0.0);
        assert_eq!(baseline.sample_size, 0);
    }

    #[test]
    fn test_insights_empty_tenant() {
        let (_dir, mem) = temp_memory();
        let insights = mem.insights("empty-tenant");
        assert_eq!(insights.total_signals, 0);
        assert!(insights.top_stacks.is_empty());
        assert!(insights.common_failures.is_empty());
        assert!(insights.quality_trend.is_empty());
    }

    #[test]
    fn test_signal_type_tag_values() {
        assert_eq!(
            signal_type_tag(&SignalType::StackDecision {
                area: "a".into(),
                choice: "b".into()
            }),
            "stack_decision"
        );
        assert_eq!(
            signal_type_tag(&SignalType::TasteScore {
                score: 0.0,
                axes: HashMap::new()
            }),
            "taste_score"
        );
        assert_eq!(
            signal_type_tag(&SignalType::BuildSuccess { first_try: true }),
            "build_success"
        );
        assert_eq!(
            signal_type_tag(&SignalType::CodeAccepted {
                modifications_after: 0
            }),
            "code_accepted"
        );
    }
}
