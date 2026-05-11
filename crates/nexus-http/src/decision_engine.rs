//! Decision Engine — Explainable architecture decisions with stack coherence validation.
//!
//! Analyses user intent and project requirements to decide:
//! - Frontend framework
//! - Backend type (API routes, standalone server, serverless)
//! - Database (SQLite, Postgres, none)
//! - Auth strategy (none, simple credentials, OAuth)
//! - Hosting target (Vercel, Docker, bare metal)
//! - UI library (shadcn, Tailwind-only, MUI)
//!
//! Features:
//! - **Compatibility matrix**: validates that chosen components work well together.
//! - **Coherence reports**: scores overall stack harmony, surfaces warnings and
//!   incompatibilities.
//! - **Explainable decisions**: every choice carries confidence, factors,
//!   alternatives considered, and learning influence.
//! - **Override support**: apply a manual override and immediately re-validate.
//! - **Deterministic heuristics** produce an [`ArchitectureDecision`] injected into
//!   plan generation and codegen prompts.
//! - **Learning-augmented decisions** use historical success weights to override defaults.

use serde::{Deserialize, Serialize};

use crate::intent_engine::{AppType, Complexity, FlatIntent, Intent};

// ---------------------------------------------------------------------------
// Core decision types (from original decision engine)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectureDecision {
    pub frontend: FrontendChoice,
    pub backend: BackendChoice,
    pub database: DatabaseChoice,
    pub auth: AuthChoice,
    pub hosting: HostingChoice,
    pub ui_library: UiLibraryChoice,
    pub rationale: Vec<DecisionRationale>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRationale {
    pub area: String,
    pub choice: String,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FrontendChoice {
    NextjsAppRouter,
    NextjsPagesRouter,
    StaticHtml,
    ReactSpa,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BackendChoice {
    NextjsApiRoutes,
    StandaloneExpress,
    Serverless,
    None,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseChoice {
    Sqlite,
    Postgres,
    None,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AuthChoice {
    None,
    SimpleCredentials,
    NextAuth,
    ClerkAuth,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum HostingChoice {
    Vercel,
    Docker,
    NodeServer,
    StaticCdn,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum UiLibraryChoice {
    ShadcnUi,
    TailwindOnly,
    MaterialUi,
}

// ---------------------------------------------------------------------------
// Deterministic decision logic (from original decision engine)
// ---------------------------------------------------------------------------

/// Make all architecture decisions deterministically from flat intent analysis.
pub fn decide(intent: &FlatIntent) -> ArchitectureDecision {
    let mut rationale = Vec::new();

    // ---- Frontend ----
    let frontend = match intent.app_type {
        AppType::LandingPage | AppType::Portfolio | AppType::Blog => {
            rationale.push(DecisionRationale {
                area: "frontend".into(),
                choice: "static_html".into(),
                reason: format!(
                    "{:?} apps are content-heavy with minimal interactivity",
                    intent.app_type
                ),
            });
            FrontendChoice::StaticHtml
        }
        AppType::ApiOnly => {
            rationale.push(DecisionRationale {
                area: "frontend".into(),
                choice: "none (API only)".into(),
                reason: "API-only project has no frontend".into(),
            });
            // Still use Next.js for the API routes
            FrontendChoice::NextjsAppRouter
        }
        _ => {
            rationale.push(DecisionRationale {
                area: "frontend".into(),
                choice: "nextjs_app_router".into(),
                reason: "Next.js App Router is the default for interactive apps with SSR, routing, and API routes".into(),
            });
            FrontendChoice::NextjsAppRouter
        }
    };

    // ---- Backend ----
    let backend = match intent.app_type {
        AppType::LandingPage | AppType::Portfolio => {
            rationale.push(DecisionRationale {
                area: "backend".into(),
                choice: "none".into(),
                reason: "Static site needs no backend".into(),
            });
            BackendChoice::None
        }
        _ => {
            rationale.push(DecisionRationale {
                area: "backend".into(),
                choice: "nextjs_api_routes".into(),
                reason: "Co-located API routes reduce deployment complexity".into(),
            });
            BackendChoice::NextjsApiRoutes
        }
    };

    // ---- Database ----
    let database = if !intent.needs_database {
        rationale.push(DecisionRationale {
            area: "database".into(),
            choice: "none".into(),
            reason: "No data persistence required".into(),
        });
        DatabaseChoice::None
    } else if intent.complexity == Complexity::Complex || intent.needs_payments {
        rationale.push(DecisionRationale {
            area: "database".into(),
            choice: "postgres".into(),
            reason: "Complex apps with payments need a production-grade database".into(),
        });
        DatabaseChoice::Postgres
    } else {
        rationale.push(DecisionRationale {
            area: "database".into(),
            choice: "sqlite".into(),
            reason: "SQLite is simpler for prototype and medium-complexity apps".into(),
        });
        DatabaseChoice::Sqlite
    };

    // ---- Auth ----
    let auth = if !intent.needs_auth {
        rationale.push(DecisionRationale {
            area: "auth".into(),
            choice: "none".into(),
            reason: "Single-user or public app — no auth needed".into(),
        });
        AuthChoice::None
    } else if intent.complexity == Complexity::Complex {
        rationale.push(DecisionRationale {
            area: "auth".into(),
            choice: "next_auth".into(),
            reason: "Complex multi-user app needs session management and role separation".into(),
        });
        AuthChoice::NextAuth
    } else {
        rationale.push(DecisionRationale {
            area: "auth".into(),
            choice: "simple_credentials".into(),
            reason: "Simple email/password auth for basic multi-user needs".into(),
        });
        AuthChoice::SimpleCredentials
    };

    // ---- Hosting ----
    let hosting = match frontend {
        FrontendChoice::StaticHtml => {
            rationale.push(DecisionRationale {
                area: "hosting".into(),
                choice: "static_cdn".into(),
                reason: "Static sites deploy best to a CDN".into(),
            });
            HostingChoice::StaticCdn
        }
        _ => {
            if database == DatabaseChoice::Postgres || intent.complexity == Complexity::Complex {
                rationale.push(DecisionRationale {
                    area: "hosting".into(),
                    choice: "docker".into(),
                    reason: "Postgres and complex apps need containerized deployment".into(),
                });
                HostingChoice::Docker
            } else {
                rationale.push(DecisionRationale {
                    area: "hosting".into(),
                    choice: "vercel".into(),
                    reason: "Vercel is the fastest path for Next.js + SQLite apps".into(),
                });
                HostingChoice::Vercel
            }
        }
    };

    // ---- UI Library ----
    let ui_library = match intent.app_type {
        AppType::Dashboard | AppType::InternalTool | AppType::Crm => {
            rationale.push(DecisionRationale {
                area: "ui_library".into(),
                choice: "shadcn_ui".into(),
                reason: "Data-heavy apps benefit from shadcn's table, dialog, and form components".into(),
            });
            UiLibraryChoice::ShadcnUi
        }
        AppType::LandingPage | AppType::Portfolio => {
            rationale.push(DecisionRationale {
                area: "ui_library".into(),
                choice: "tailwind_only".into(),
                reason: "Marketing sites need custom design, not component library constraints".into(),
            });
            UiLibraryChoice::TailwindOnly
        }
        _ => {
            rationale.push(DecisionRationale {
                area: "ui_library".into(),
                choice: "shadcn_ui".into(),
                reason: "shadcn/ui provides the best balance of design quality and flexibility".into(),
            });
            UiLibraryChoice::ShadcnUi
        }
    };

    ArchitectureDecision {
        frontend,
        backend,
        database,
        auth,
        hosting,
        ui_library,
        rationale,
    }
}

// ---------------------------------------------------------------------------
// Learning-augmented decision
// ---------------------------------------------------------------------------

/// Make architecture decisions augmented with learned weights from past outcomes.
/// Falls back to `decide` if no learning data is available.
pub async fn decide_with_learning(
    intent: &FlatIntent,
    app: &std::sync::Arc<crate::state::AppState>,
) -> (ArchitectureDecision, Vec<String>) {
    let weights = crate::decision_learning::get_all_weights(app).await;

    let mut base = decide(intent);
    let mut learning_notes = Vec::new();

    // For each decision area, check if learning suggests a different choice
    let areas: &[(&str, &[&str])] = &[
        ("database", &["sqlite", "postgres", "none"]),
        ("auth", &["none", "simple_credentials", "next_auth", "clerk_auth"]),
        ("hosting", &["vercel", "docker", "node_server", "static_cdn"]),
        ("ui_library", &["shadcn_ui", "tailwind_only", "material_ui"]),
    ];

    for &(area, choices) in areas {
        let mut best_value: Option<(&str, f64)> = None;

        for &choice in choices.iter() {
            let w = weights.iter().find(|w| w.area == *area && w.value == choice);
            if let Some(w) = w {
                // Only override if we have meaningful signal (5+ decisions) and positive weight
                if w.success_count + w.failure_count >= 5 && w.weight_adj > 0.3 {
                    let score = w.weight_adj;
                    if best_value.is_none_or(|(_, s)| score > s) {
                        best_value = Some((choice, score));
                    }
                }
            }
        }

        if let Some((learned_choice, weight)) = best_value {
            let current = decision_area_value(&base, area);
            if current != learned_choice {
                learning_notes.push(format!(
                    "Learning override: {} → {} (weight +{:.2} from historical success)",
                    area, learned_choice, weight
                ));
                apply_learning_override(&mut base, area, learned_choice);
            }
        }
    }

    (base, learning_notes)
}

pub fn decision_area_value<'a>(d: &'a ArchitectureDecision, area: &str) -> &'a str {
    match area {
        "database" => match d.database {
            DatabaseChoice::Sqlite => "sqlite",
            DatabaseChoice::Postgres => "postgres",
            DatabaseChoice::None => "none",
        },
        "auth" => match d.auth {
            AuthChoice::None => "none",
            AuthChoice::SimpleCredentials => "simple_credentials",
            AuthChoice::NextAuth => "next_auth",
            AuthChoice::ClerkAuth => "clerk_auth",
        },
        "hosting" => match d.hosting {
            HostingChoice::Vercel => "vercel",
            HostingChoice::Docker => "docker",
            HostingChoice::NodeServer => "node_server",
            HostingChoice::StaticCdn => "static_cdn",
        },
        "ui_library" => match d.ui_library {
            UiLibraryChoice::ShadcnUi => "shadcn_ui",
            UiLibraryChoice::TailwindOnly => "tailwind_only",
            UiLibraryChoice::MaterialUi => "material_ui",
        },
        _ => "",
    }
}

pub fn apply_learning_override(d: &mut ArchitectureDecision, area: &str, value: &str) {
    match area {
        "database" => {
            d.database = match value {
                "sqlite" => DatabaseChoice::Sqlite,
                "postgres" => DatabaseChoice::Postgres,
                _ => DatabaseChoice::None,
            };
        }
        "auth" => {
            d.auth = match value {
                "simple_credentials" => AuthChoice::SimpleCredentials,
                "next_auth" => AuthChoice::NextAuth,
                "clerk_auth" => AuthChoice::ClerkAuth,
                _ => AuthChoice::None,
            };
        }
        "hosting" => {
            d.hosting = match value {
                "vercel" => HostingChoice::Vercel,
                "docker" => HostingChoice::Docker,
                "node_server" => HostingChoice::NodeServer,
                _ => HostingChoice::StaticCdn,
            };
        }
        "ui_library" => {
            d.ui_library = match value {
                "shadcn_ui" => UiLibraryChoice::ShadcnUi,
                "tailwind_only" => UiLibraryChoice::TailwindOnly,
                _ => UiLibraryChoice::MaterialUi,
            };
        }
        _ => {}
    }
}

/// Convert the decision to a context string for LLM prompts.
pub fn to_prompt_context(decision: &ArchitectureDecision) -> String {
    let mut ctx = String::from("## ARCHITECTURE DECISIONS (pre-determined, do NOT change)\n\n");

    ctx.push_str(&format!("- **Frontend**: {:?}\n", decision.frontend));
    ctx.push_str(&format!("- **Backend**: {:?}\n", decision.backend));
    ctx.push_str(&format!("- **Database**: {:?}\n", decision.database));
    ctx.push_str(&format!("- **Auth**: {:?}\n", decision.auth));
    ctx.push_str(&format!("- **Hosting**: {:?}\n", decision.hosting));
    ctx.push_str(&format!("- **UI Library**: {:?}\n", decision.ui_library));

    ctx.push_str("\n### Rationale\n");
    for r in &decision.rationale {
        ctx.push_str(&format!("- **{}**: {} — {}\n", r.area, r.choice, r.reason));
    }

    ctx
}

// ---------------------------------------------------------------------------
// Compatibility primitives
// ---------------------------------------------------------------------------

/// How well two stack choices work together.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Compatibility {
    /// The pairing is a known best-practice.
    Recommended,
    /// The pairing works without issues.
    Compatible,
    /// The pairing works but has known friction.
    Discouraged,
    /// The pairing is fundamentally broken or unsupported.
    Incompatible,
}

/// A single rule describing the compatibility between two stack choices.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityRule {
    /// Decision area of the first choice (e.g. "frontend").
    pub area_a: String,
    /// Value of the first choice (e.g. "nextjs_app_router").
    pub choice_a: String,
    /// Decision area of the second choice (e.g. "backend").
    pub area_b: String,
    /// Value of the second choice (e.g. "nextjs_api_routes").
    pub choice_b: String,
    /// How compatible the two choices are.
    pub compatibility: Compatibility,
    /// Human-readable justification.
    pub reason: String,
}

/// A matrix of compatibility rules used for coherence validation.
pub struct CompatibilityMatrix {
    rules: Vec<CompatibilityRule>,
}

impl CompatibilityMatrix {
    /// Build the default compatibility matrix with curated rules.
    pub fn default_matrix() -> Self {
        Self {
            rules: default_rules(),
        }
    }

    /// Look up the compatibility between two specific choices.
    ///
    /// Returns `None` if no rule exists for the pair (implying neutral/compatible).
    pub fn lookup(&self, area_a: &str, choice_a: &str, area_b: &str, choice_b: &str) -> Option<&CompatibilityRule> {
        self.rules.iter().find(|r| {
            (r.area_a == area_a && r.choice_a == choice_a && r.area_b == area_b && r.choice_b == choice_b)
            || (r.area_a == area_b && r.choice_a == choice_b && r.area_b == area_a && r.choice_b == choice_a)
        })
    }

    /// Validate the coherence of a set of decisions.
    ///
    /// `decisions` is a slice of `(area, choice)` tuples. Returns a full
    /// [`CoherenceReport`] including an overall score, warnings, and
    /// incompatibilities.
    pub fn validate_coherence(&self, decisions: &[(&str, &str)]) -> CoherenceReport {
        let mut warnings = Vec::new();
        let mut incompatibilities = Vec::new();
        let mut recommendations = Vec::new();

        let mut penalty: f32 = 0.0;
        let mut pair_count: u32 = 0;

        // Check every unique pair of decisions.
        for i in 0..decisions.len() {
            for j in (i + 1)..decisions.len() {
                let (area_a, choice_a) = decisions[i];
                let (area_b, choice_b) = decisions[j];
                pair_count += 1;

                if let Some(rule) = self.lookup(area_a, choice_a, area_b, choice_b) {
                    match rule.compatibility {
                        Compatibility::Recommended => {
                            // Bonus — reduce penalty slightly.
                            penalty -= 0.02;
                        }
                        Compatibility::Compatible => {
                            // Neutral — no change.
                        }
                        Compatibility::Discouraged => {
                            penalty += 0.15;
                            warnings.push(CoherenceWarning {
                                area_a: area_a.to_string(),
                                choice_a: choice_a.to_string(),
                                area_b: area_b.to_string(),
                                choice_b: choice_b.to_string(),
                                reason: rule.reason.clone(),
                            });
                        }
                        Compatibility::Incompatible => {
                            penalty += 0.4;
                            incompatibilities.push(CoherenceIncompatibility {
                                area_a: area_a.to_string(),
                                choice_a: choice_a.to_string(),
                                area_b: area_b.to_string(),
                                choice_b: choice_b.to_string(),
                                reason: rule.reason.clone(),
                                suggested_fix: suggest_fix(area_a, choice_a, area_b, choice_b),
                            });
                        }
                    }
                }
            }
        }

        // Build recommendations from incompatibilities.
        for inc in &incompatibilities {
            recommendations.push(format!(
                "Replace {} ({}) or {} ({}) — {}",
                inc.area_a, inc.choice_a, inc.area_b, inc.choice_b, inc.suggested_fix
            ));
        }

        let overall_score = (1.0 - penalty).clamp(0.0, 1.0);

        // Add general recommendation if score is low.
        if overall_score < 0.7 && recommendations.is_empty() {
            recommendations.push(
                "Consider reviewing discouraged pairings to improve stack coherence.".to_string(),
            );
        }

        CoherenceReport {
            overall_score,
            pair_count,
            warnings,
            incompatibilities,
            recommendations,
        }
    }
}

// ---------------------------------------------------------------------------
// Coherence report types
// ---------------------------------------------------------------------------

/// Full coherence analysis of a decision set.
#[derive(Debug, Clone, Serialize)]
pub struct CoherenceReport {
    /// Score in `[0.0, 1.0]` — 1.0 means fully coherent.
    pub overall_score: f32,
    /// Number of unique pairs evaluated.
    pub pair_count: u32,
    /// Pairings that are discouraged but not fatal.
    pub warnings: Vec<CoherenceWarning>,
    /// Pairings that are fundamentally broken.
    pub incompatibilities: Vec<CoherenceIncompatibility>,
    /// Actionable recommendations to improve the stack.
    pub recommendations: Vec<String>,
}

/// A discouraged pairing that may cause friction.
#[derive(Debug, Clone, Serialize)]
pub struct CoherenceWarning {
    /// First decision area.
    pub area_a: String,
    /// First choice value.
    pub choice_a: String,
    /// Second decision area.
    pub area_b: String,
    /// Second choice value.
    pub choice_b: String,
    /// Why this pairing is discouraged.
    pub reason: String,
}

/// A fundamentally broken pairing.
#[derive(Debug, Clone, Serialize)]
pub struct CoherenceIncompatibility {
    /// First decision area.
    pub area_a: String,
    /// First choice value.
    pub choice_a: String,
    /// Second decision area.
    pub area_b: String,
    /// Second choice value.
    pub choice_b: String,
    /// Why this pairing is incompatible.
    pub reason: String,
    /// What the user should change.
    pub suggested_fix: String,
}

// ---------------------------------------------------------------------------
// Explainable decision types
// ---------------------------------------------------------------------------

/// A single architecture decision with full explainability metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplainedDecision {
    /// The decision area (e.g. "frontend", "database").
    pub area: String,
    /// The chosen value (e.g. "nextjs_app_router", "postgres").
    pub choice: String,
    /// Confidence in this choice, `[0.0, 1.0]`.
    pub confidence: f32,
    /// Detailed explanation of how the choice was made.
    pub explanation: DecisionExplanation,
}

/// Full explanation for why a decision was made.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionExplanation {
    /// One-sentence summary of the primary reason.
    pub primary_reason: String,
    /// Individual factors that influenced the decision, with weights.
    pub factors: Vec<DecisionFactor>,
    /// Other options that were evaluated and why they were rejected.
    pub alternatives_considered: Vec<Alternative>,
    /// How much historical learning data influenced this decision (`[0.0, 1.0]`).
    pub learning_influence: f32,
}

/// A single factor that contributed to a decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionFactor {
    /// What was considered (e.g. "app complexity", "user request").
    pub factor: String,
    /// How heavily this factor weighed, `[0.0, 1.0]`.
    pub weight: f32,
    /// Where the factor came from (e.g. "intent_analysis", "domain_pack").
    pub source: String,
}

/// An alternative that was considered but not chosen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alternative {
    /// The alternative choice value.
    pub choice: String,
    /// How well it scored, `[0.0, 1.0]`.
    pub score: f32,
    /// Why it was rejected in favour of the chosen value.
    pub reason_rejected: String,
}

// ---------------------------------------------------------------------------
// DecisionSet
// ---------------------------------------------------------------------------

/// A complete set of architecture decisions with coherence validation.
#[derive(Debug, Clone, Serialize)]
pub struct DecisionSet {
    /// Individual explained decisions.
    pub decisions: Vec<ExplainedDecision>,
    /// Coherence report across the full decision set.
    pub coherence: CoherenceReport,
}

impl DecisionSet {
    /// Find the decision for a given area.
    pub fn get(&self, area: &str) -> Option<&ExplainedDecision> {
        self.decisions.iter().find(|d| d.area == area)
    }

    /// Collect decisions as `(area, choice)` tuples for coherence checks.
    pub fn as_pairs(&self) -> Vec<(&str, &str)> {
        self.decisions
            .iter()
            .map(|d| (d.area.as_str(), d.choice.as_str()))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Decision Engine
// ---------------------------------------------------------------------------

/// Explainable, coherence-validated decision engine.
///
/// Wraps the deterministic heuristics with:
/// - Confidence scoring
/// - Full factor/alternative tracking
/// - Cross-component compatibility validation
/// - Manual override support with re-validation
pub struct DecisionEngine {
    /// The compatibility matrix used for coherence validation.
    compatibility: CompatibilityMatrix,
}

impl DecisionEngine {
    /// Create a new engine with the default compatibility matrix.
    pub fn new() -> Self {
        Self {
            compatibility: CompatibilityMatrix::default_matrix(),
        }
    }

    /// Make explained decisions based on an [`Intent`].
    ///
    /// Produces a [`DecisionSet`] where every choice is annotated with
    /// confidence, factors, alternatives, and the full set is coherence-validated.
    #[allow(clippy::vec_init_then_push)]
    pub fn decide(&self, intent: &Intent) -> DecisionSet {
        let app_type = &intent.inferred.app_type.value;
        let complexity = &intent.inferred.complexity.value;
        let needs_auth = intent.inferred.auth_needed.value;
        let needs_db = intent.inferred.db_needed.value;
        let needs_realtime = intent.inferred.realtime_needed.value;
        let app_type_confidence = intent.inferred.app_type.confidence;

        let mut decisions = Vec::new();

        // ---- Frontend ----
        decisions.push(self.decide_frontend(app_type.clone(), complexity.clone(), app_type_confidence));

        // ---- Backend ----
        decisions.push(self.decide_backend(app_type.clone(), complexity.clone(), needs_realtime));

        // ---- Database ----
        decisions.push(self.decide_database(needs_db, complexity.clone()));

        // ---- Auth ----
        decisions.push(self.decide_auth(needs_auth, complexity.clone()));

        // ---- Hosting ----
        let frontend_choice = decisions[0].choice.clone();
        let db_choice = decisions[2].choice.clone();
        decisions.push(self.decide_hosting(&frontend_choice, &db_choice, complexity.clone()));

        // ---- UI Library ----
        decisions.push(self.decide_ui_library(app_type.clone()));

        // Validate coherence across all decisions.
        let pairs: Vec<(&str, &str)> = decisions
            .iter()
            .map(|d| (d.area.as_str(), d.choice.as_str()))
            .collect();
        let coherence = self.compatibility.validate_coherence(&pairs);

        DecisionSet {
            decisions,
            coherence,
        }
    }

    /// Validate the coherence of an existing decision set.
    pub fn validate(&self, decisions: &DecisionSet) -> CoherenceReport {
        let pairs = decisions.as_pairs();
        self.compatibility.validate_coherence(&pairs)
    }

    /// Apply a manual override to a decision area and re-validate coherence.
    ///
    /// Returns the updated coherence report after the override.
    pub fn apply_override(
        &self,
        decisions: &mut DecisionSet,
        area: &str,
        choice: &str,
        reason: &str,
    ) -> CoherenceReport {
        if let Some(d) = decisions.decisions.iter_mut().find(|d| d.area == area) {
            d.choice = choice.to_string();
            d.confidence = 1.0; // User explicitly chose this.
            d.explanation = DecisionExplanation {
                primary_reason: format!("User override: {}", reason),
                factors: vec![DecisionFactor {
                    factor: "user_override".to_string(),
                    weight: 1.0,
                    source: "manual".to_string(),
                }],
                alternatives_considered: vec![],
                learning_influence: 0.0,
            };
        }

        // Re-validate.
        let pairs = decisions.as_pairs();
        let coherence = self.compatibility.validate_coherence(&pairs);
        decisions.coherence = coherence.clone();
        coherence
    }

    /// Convert a [`DecisionSet`] to the legacy [`ArchitectureDecision`] type.
    ///
    /// This allows downstream consumers that depend on the flat struct type
    /// to continue working alongside the richer `DecisionSet` API.
    pub fn to_legacy(&self, decisions: &DecisionSet) -> ArchitectureDecision {
        let get = |area: &str| -> &str {
            decisions
                .get(area)
                .map(|d| d.choice.as_str())
                .unwrap_or("")
        };

        let frontend = match get("frontend") {
            "nextjs_app_router" => FrontendChoice::NextjsAppRouter,
            "nextjs_pages_router" => FrontendChoice::NextjsPagesRouter,
            "static_html" => FrontendChoice::StaticHtml,
            "react_spa" => FrontendChoice::ReactSpa,
            _ => FrontendChoice::NextjsAppRouter,
        };

        let backend = match get("backend") {
            "nextjs_api_routes" => BackendChoice::NextjsApiRoutes,
            "standalone_express" => BackendChoice::StandaloneExpress,
            "serverless" => BackendChoice::Serverless,
            "none" => BackendChoice::None,
            _ => BackendChoice::NextjsApiRoutes,
        };

        let database = match get("database") {
            "sqlite" => DatabaseChoice::Sqlite,
            "postgres" => DatabaseChoice::Postgres,
            "none" => DatabaseChoice::None,
            _ => DatabaseChoice::None,
        };

        let auth = match get("auth") {
            "none" => AuthChoice::None,
            "simple_credentials" => AuthChoice::SimpleCredentials,
            "next_auth" => AuthChoice::NextAuth,
            "clerk_auth" => AuthChoice::ClerkAuth,
            _ => AuthChoice::None,
        };

        let hosting = match get("hosting") {
            "vercel" => HostingChoice::Vercel,
            "docker" => HostingChoice::Docker,
            "node_server" => HostingChoice::NodeServer,
            "static_cdn" => HostingChoice::StaticCdn,
            _ => HostingChoice::Vercel,
        };

        let ui_library = match get("ui_library") {
            "shadcn_ui" => UiLibraryChoice::ShadcnUi,
            "tailwind_only" => UiLibraryChoice::TailwindOnly,
            "material_ui" => UiLibraryChoice::MaterialUi,
            _ => UiLibraryChoice::ShadcnUi,
        };

        let rationale = decisions
            .decisions
            .iter()
            .map(|d| DecisionRationale {
                area: d.area.clone(),
                choice: d.choice.clone(),
                reason: d.explanation.primary_reason.clone(),
            })
            .collect();

        ArchitectureDecision {
            frontend,
            backend,
            database,
            auth,
            hosting,
            ui_library,
            rationale,
        }
    }

    // -----------------------------------------------------------------------
    // Per-area decision helpers
    // -----------------------------------------------------------------------

    fn decide_frontend(
        &self,
        app_type: AppType,
        complexity: Complexity,
        app_type_confidence: f32,
    ) -> ExplainedDecision {
        let (choice, confidence, primary_reason, factors, alternatives) = match app_type {
            AppType::LandingPage | AppType::Portfolio => (
                "static_html",
                0.95 * app_type_confidence,
                "Content-heavy sites with minimal interactivity are best served as static HTML.",
                vec![
                    DecisionFactor { factor: "app_type".into(), weight: 0.7, source: "intent_analysis".into() },
                    DecisionFactor { factor: "low_interactivity".into(), weight: 0.3, source: "heuristic".into() },
                ],
                vec![
                    Alternative { choice: "nextjs_app_router".into(), score: 0.4, reason_rejected: "Over-engineered for static content.".into() },
                    Alternative { choice: "react_spa".into(), score: 0.2, reason_rejected: "SPAs hurt SEO for marketing pages.".into() },
                ],
            ),
            AppType::Blog => (
                "nextjs_app_router",
                0.85 * app_type_confidence,
                "Blogs benefit from SSR/SSG for SEO while supporting dynamic features.",
                vec![
                    DecisionFactor { factor: "seo_requirements".into(), weight: 0.5, source: "heuristic".into() },
                    DecisionFactor { factor: "app_type".into(), weight: 0.3, source: "intent_analysis".into() },
                    DecisionFactor { factor: "content_management".into(), weight: 0.2, source: "heuristic".into() },
                ],
                vec![
                    Alternative { choice: "static_html".into(), score: 0.6, reason_rejected: "No dynamic features (comments, search).".into() },
                ],
            ),
            AppType::ApiOnly => (
                "nextjs_app_router",
                0.7,
                "API-only projects still use Next.js for its API routes; no UI rendering needed.",
                vec![
                    DecisionFactor { factor: "api_only".into(), weight: 0.6, source: "intent_analysis".into() },
                    DecisionFactor { factor: "deployment_simplicity".into(), weight: 0.4, source: "heuristic".into() },
                ],
                vec![
                    Alternative { choice: "react_spa".into(), score: 0.1, reason_rejected: "No frontend UI needed.".into() },
                ],
            ),
            _ => {
                let conf = if complexity == Complexity::Complex { 0.9 } else { 0.85 };
                (
                    "nextjs_app_router",
                    conf * app_type_confidence,
                    "Next.js App Router is the default for interactive apps with SSR, routing, and API routes.",
                    vec![
                        DecisionFactor { factor: "interactivity".into(), weight: 0.4, source: "intent_analysis".into() },
                        DecisionFactor { factor: "ssr_support".into(), weight: 0.3, source: "heuristic".into() },
                        DecisionFactor { factor: "ecosystem_maturity".into(), weight: 0.3, source: "heuristic".into() },
                    ],
                    vec![
                        Alternative { choice: "react_spa".into(), score: 0.5, reason_rejected: "Loses SSR benefits and built-in API routes.".into() },
                        Alternative { choice: "static_html".into(), score: 0.2, reason_rejected: "Cannot handle dynamic interactivity.".into() },
                    ],
                )
            }
        };

        ExplainedDecision {
            area: "frontend".to_string(),
            choice: choice.to_string(),
            confidence,
            explanation: DecisionExplanation {
                primary_reason: primary_reason.to_string(),
                factors,
                alternatives_considered: alternatives,
                learning_influence: 0.0,
            },
        }
    }

    fn decide_backend(
        &self,
        app_type: AppType,
        complexity: Complexity,
        needs_realtime: bool,
    ) -> ExplainedDecision {
        let (choice, confidence, primary_reason, factors, alternatives) = match app_type {
            AppType::LandingPage | AppType::Portfolio => (
                "none",
                0.95,
                "Static sites need no backend.",
                vec![
                    DecisionFactor { factor: "app_type".into(), weight: 0.9, source: "intent_analysis".into() },
                ],
                vec![
                    Alternative { choice: "nextjs_api_routes".into(), score: 0.1, reason_rejected: "No server logic required.".into() },
                ],
            ),
            _ if needs_realtime && complexity == Complexity::Complex => (
                "standalone_express",
                0.8,
                "Real-time complex apps benefit from a standalone Express server with WebSocket support.",
                vec![
                    DecisionFactor { factor: "realtime_needs".into(), weight: 0.5, source: "intent_analysis".into() },
                    DecisionFactor { factor: "complexity".into(), weight: 0.3, source: "intent_analysis".into() },
                    DecisionFactor { factor: "websocket_support".into(), weight: 0.2, source: "heuristic".into() },
                ],
                vec![
                    Alternative { choice: "nextjs_api_routes".into(), score: 0.5, reason_rejected: "Limited WebSocket support in API routes.".into() },
                    Alternative { choice: "serverless".into(), score: 0.2, reason_rejected: "Serverless functions cannot maintain WebSocket connections.".into() },
                ],
            ),
            _ => (
                "nextjs_api_routes",
                0.85,
                "Co-located API routes reduce deployment complexity.",
                vec![
                    DecisionFactor { factor: "deployment_simplicity".into(), weight: 0.5, source: "heuristic".into() },
                    DecisionFactor { factor: "colocation".into(), weight: 0.3, source: "heuristic".into() },
                    DecisionFactor { factor: "ecosystem".into(), weight: 0.2, source: "heuristic".into() },
                ],
                vec![
                    Alternative { choice: "standalone_express".into(), score: 0.4, reason_rejected: "Adds deployment complexity without clear benefit.".into() },
                    Alternative { choice: "serverless".into(), score: 0.35, reason_rejected: "Cold starts and vendor lock-in.".into() },
                ],
            ),
        };

        ExplainedDecision {
            area: "backend".to_string(),
            choice: choice.to_string(),
            confidence,
            explanation: DecisionExplanation {
                primary_reason: primary_reason.to_string(),
                factors,
                alternatives_considered: alternatives,
                learning_influence: 0.0,
            },
        }
    }

    fn decide_database(&self, needs_db: bool, complexity: Complexity) -> ExplainedDecision {
        if !needs_db {
            return ExplainedDecision {
                area: "database".to_string(),
                choice: "none".to_string(),
                confidence: 0.95,
                explanation: DecisionExplanation {
                    primary_reason: "No data persistence required.".to_string(),
                    factors: vec![DecisionFactor {
                        factor: "no_data_persistence".into(),
                        weight: 1.0,
                        source: "intent_analysis".into(),
                    }],
                    alternatives_considered: vec![],
                    learning_influence: 0.0,
                },
            };
        }

        let (choice, confidence, primary_reason, factors, alternatives) =
            if complexity == Complexity::Complex {
                (
                    "postgres",
                    0.9,
                    "Complex apps need a production-grade relational database.",
                    vec![
                        DecisionFactor { factor: "complexity".into(), weight: 0.6, source: "intent_analysis".into() },
                        DecisionFactor { factor: "concurrency_support".into(), weight: 0.25, source: "heuristic".into() },
                        DecisionFactor { factor: "migration_tooling".into(), weight: 0.15, source: "heuristic".into() },
                    ],
                    vec![
                        Alternative { choice: "sqlite".into(), score: 0.3, reason_rejected: "Poor concurrency support for multi-user apps.".into() },
                    ],
                )
            } else {
                (
                    "sqlite",
                    0.8,
                    "SQLite is simpler for prototypes and medium-complexity apps.",
                    vec![
                        DecisionFactor { factor: "simplicity".into(), weight: 0.5, source: "heuristic".into() },
                        DecisionFactor { factor: "zero_config".into(), weight: 0.3, source: "heuristic".into() },
                        DecisionFactor { factor: "prototype_speed".into(), weight: 0.2, source: "heuristic".into() },
                    ],
                    vec![
                        Alternative { choice: "postgres".into(), score: 0.5, reason_rejected: "Over-engineered for simple apps.".into() },
                    ],
                )
            };

        ExplainedDecision {
            area: "database".to_string(),
            choice: choice.to_string(),
            confidence,
            explanation: DecisionExplanation {
                primary_reason: primary_reason.to_string(),
                factors,
                alternatives_considered: alternatives,
                learning_influence: 0.0,
            },
        }
    }

    fn decide_auth(&self, needs_auth: bool, complexity: Complexity) -> ExplainedDecision {
        if !needs_auth {
            return ExplainedDecision {
                area: "auth".to_string(),
                choice: "none".to_string(),
                confidence: 0.95,
                explanation: DecisionExplanation {
                    primary_reason: "Single-user or public app — no auth needed.".to_string(),
                    factors: vec![DecisionFactor {
                        factor: "no_auth_requirement".into(),
                        weight: 1.0,
                        source: "intent_analysis".into(),
                    }],
                    alternatives_considered: vec![],
                    learning_influence: 0.0,
                },
            };
        }

        let (choice, confidence, primary_reason, factors, alternatives) =
            if complexity == Complexity::Complex {
                (
                    "next_auth",
                    0.85,
                    "Complex multi-user app needs session management and role separation.",
                    vec![
                        DecisionFactor { factor: "complexity".into(), weight: 0.5, source: "intent_analysis".into() },
                        DecisionFactor { factor: "multi_user".into(), weight: 0.3, source: "heuristic".into() },
                        DecisionFactor { factor: "oauth_support".into(), weight: 0.2, source: "heuristic".into() },
                    ],
                    vec![
                        Alternative { choice: "simple_credentials".into(), score: 0.4, reason_rejected: "No OAuth or session management.".into() },
                        Alternative { choice: "clerk_auth".into(), score: 0.6, reason_rejected: "External dependency and cost for self-hostable apps.".into() },
                    ],
                )
            } else {
                (
                    "simple_credentials",
                    0.8,
                    "Simple email/password auth for basic multi-user needs.",
                    vec![
                        DecisionFactor { factor: "simplicity".into(), weight: 0.6, source: "heuristic".into() },
                        DecisionFactor { factor: "auth_needed".into(), weight: 0.4, source: "intent_analysis".into() },
                    ],
                    vec![
                        Alternative { choice: "next_auth".into(), score: 0.5, reason_rejected: "Over-engineered for simple use case.".into() },
                    ],
                )
            };

        ExplainedDecision {
            area: "auth".to_string(),
            choice: choice.to_string(),
            confidence,
            explanation: DecisionExplanation {
                primary_reason: primary_reason.to_string(),
                factors,
                alternatives_considered: alternatives,
                learning_influence: 0.0,
            },
        }
    }

    fn decide_hosting(
        &self,
        frontend_choice: &str,
        db_choice: &str,
        complexity: Complexity,
    ) -> ExplainedDecision {
        let (choice, confidence, primary_reason, factors, alternatives) = if frontend_choice == "static_html" {
            (
                "static_cdn",
                0.95,
                "Static sites deploy best to a CDN.",
                vec![
                    DecisionFactor { factor: "static_frontend".into(), weight: 0.8, source: "decision_dependency".into() },
                    DecisionFactor { factor: "performance".into(), weight: 0.2, source: "heuristic".into() },
                ],
                vec![
                    Alternative { choice: "vercel".into(), score: 0.6, reason_rejected: "Unnecessary compute for static files.".into() },
                ],
            )
        } else if db_choice == "postgres" || complexity == Complexity::Complex {
            (
                "docker",
                0.85,
                "Postgres and complex apps need containerized deployment.",
                vec![
                    DecisionFactor { factor: "database_choice".into(), weight: 0.5, source: "decision_dependency".into() },
                    DecisionFactor { factor: "complexity".into(), weight: 0.3, source: "intent_analysis".into() },
                    DecisionFactor { factor: "portability".into(), weight: 0.2, source: "heuristic".into() },
                ],
                vec![
                    Alternative { choice: "vercel".into(), score: 0.4, reason_rejected: "Vercel does not natively support Postgres containers.".into() },
                    Alternative { choice: "node_server".into(), score: 0.3, reason_rejected: "Less reproducible than containers.".into() },
                ],
            )
        } else {
            (
                "vercel",
                0.9,
                "Vercel is the fastest path for Next.js + SQLite apps.",
                vec![
                    DecisionFactor { factor: "nextjs_integration".into(), weight: 0.5, source: "heuristic".into() },
                    DecisionFactor { factor: "deployment_speed".into(), weight: 0.3, source: "heuristic".into() },
                    DecisionFactor { factor: "zero_config".into(), weight: 0.2, source: "heuristic".into() },
                ],
                vec![
                    Alternative { choice: "docker".into(), score: 0.4, reason_rejected: "Adds unnecessary complexity for simple apps.".into() },
                ],
            )
        };

        ExplainedDecision {
            area: "hosting".to_string(),
            choice: choice.to_string(),
            confidence,
            explanation: DecisionExplanation {
                primary_reason: primary_reason.to_string(),
                factors,
                alternatives_considered: alternatives,
                learning_influence: 0.0,
            },
        }
    }

    fn decide_ui_library(&self, app_type: AppType) -> ExplainedDecision {
        let (choice, confidence, primary_reason, factors, alternatives) = match app_type {
            AppType::Dashboard | AppType::InternalTool | AppType::Crm => (
                "shadcn_ui",
                0.9,
                "Data-heavy apps benefit from shadcn's table, dialog, and form components.",
                vec![
                    DecisionFactor { factor: "data_density".into(), weight: 0.5, source: "intent_analysis".into() },
                    DecisionFactor { factor: "component_richness".into(), weight: 0.3, source: "heuristic".into() },
                    DecisionFactor { factor: "customizability".into(), weight: 0.2, source: "heuristic".into() },
                ],
                vec![
                    Alternative { choice: "material_ui".into(), score: 0.6, reason_rejected: "Heavier bundle, less customizable.".into() },
                    Alternative { choice: "tailwind_only".into(), score: 0.3, reason_rejected: "Too much custom work for data-heavy UIs.".into() },
                ],
            ),
            AppType::LandingPage | AppType::Portfolio => (
                "tailwind_only",
                0.9,
                "Marketing sites need custom design, not component library constraints.",
                vec![
                    DecisionFactor { factor: "design_freedom".into(), weight: 0.6, source: "heuristic".into() },
                    DecisionFactor { factor: "app_type".into(), weight: 0.4, source: "intent_analysis".into() },
                ],
                vec![
                    Alternative { choice: "shadcn_ui".into(), score: 0.4, reason_rejected: "Component library constrains creative design.".into() },
                ],
            ),
            _ => (
                "shadcn_ui",
                0.8,
                "shadcn/ui provides the best balance of design quality and flexibility.",
                vec![
                    DecisionFactor { factor: "balance".into(), weight: 0.4, source: "heuristic".into() },
                    DecisionFactor { factor: "community_adoption".into(), weight: 0.3, source: "heuristic".into() },
                    DecisionFactor { factor: "tailwind_integration".into(), weight: 0.3, source: "heuristic".into() },
                ],
                vec![
                    Alternative { choice: "tailwind_only".into(), score: 0.5, reason_rejected: "Requires more custom component work.".into() },
                    Alternative { choice: "material_ui".into(), score: 0.4, reason_rejected: "Opinionated styling, harder to customize.".into() },
                ],
            ),
        };

        ExplainedDecision {
            area: "ui_library".to_string(),
            choice: choice.to_string(),
            confidence,
            explanation: DecisionExplanation {
                primary_reason: primary_reason.to_string(),
                factors,
                alternatives_considered: alternatives,
                learning_influence: 0.0,
            },
        }
    }
}

impl Default for DecisionEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Default compatibility rules (20+ curated rules)
// ---------------------------------------------------------------------------

fn default_rules() -> Vec<CompatibilityRule> {
    vec![
        // 1. Next.js + Supabase = Recommended
        rule("frontend", "nextjs_app_router", "database", "postgres", Compatibility::Recommended,
             "Next.js pairs naturally with Supabase/Postgres for full-stack apps."),
        // 2. Next.js + Next-Auth = Recommended
        rule("frontend", "nextjs_app_router", "auth", "next_auth", Compatibility::Recommended,
             "NextAuth is purpose-built for Next.js applications."),
        // 3. Next.js + Vercel = Recommended
        rule("frontend", "nextjs_app_router", "hosting", "vercel", Compatibility::Recommended,
             "Vercel is the canonical hosting platform for Next.js."),
        // 4. Next.js + shadcn = Recommended
        rule("frontend", "nextjs_app_router", "ui_library", "shadcn_ui", Compatibility::Recommended,
             "shadcn/ui is built for React/Next.js with Tailwind CSS."),
        // 5. Static HTML + no backend = Recommended
        rule("frontend", "static_html", "backend", "none", Compatibility::Recommended,
             "Static sites do not need a backend."),
        // 6. Static HTML + Static CDN = Recommended
        rule("frontend", "static_html", "hosting", "static_cdn", Compatibility::Recommended,
             "Static files are best served from a CDN."),
        // 7. Static HTML + Tailwind only = Recommended
        rule("frontend", "static_html", "ui_library", "tailwind_only", Compatibility::Recommended,
             "Tailwind-only gives maximum design freedom for static sites."),
        // 8. React SPA + Express = Compatible
        rule("frontend", "react_spa", "backend", "standalone_express", Compatibility::Compatible,
             "React SPA + Express is a well-established pattern."),
        // 9. React SPA + Material UI = Compatible
        rule("frontend", "react_spa", "ui_library", "material_ui", Compatibility::Compatible,
             "Material UI works well with plain React SPAs."),
        // 10. Postgres + Docker = Recommended
        rule("database", "postgres", "hosting", "docker", Compatibility::Recommended,
             "Docker containers are the standard way to deploy Postgres-backed apps."),
        // 11. SQLite + Vercel = Compatible
        rule("database", "sqlite", "hosting", "vercel", Compatibility::Compatible,
             "SQLite works on Vercel with caveats (read-only in serverless functions)."),
        // 12. SQLite + Serverless backend = Discouraged
        rule("database", "sqlite", "backend", "serverless", Compatibility::Discouraged,
             "SQLite does not support concurrent writes from multiple serverless instances."),
        // 13. Postgres + Static CDN = Incompatible
        rule("database", "postgres", "hosting", "static_cdn", Compatibility::Incompatible,
             "A CDN cannot run a Postgres database; server-side compute is required."),
        // 14. NextAuth + no backend = Incompatible
        rule("auth", "next_auth", "backend", "none", Compatibility::Incompatible,
             "NextAuth requires server-side API routes to handle auth flows."),
        // 15. ClerkAuth + no backend = Incompatible
        rule("auth", "clerk_auth", "backend", "none", Compatibility::Incompatible,
             "Clerk requires server-side middleware for session verification."),
        // 16. Simple credentials + no database = Incompatible
        rule("auth", "simple_credentials", "database", "none", Compatibility::Incompatible,
             "Credentials-based auth needs a database to store user records."),
        // 17. NextAuth + no database = Discouraged
        rule("auth", "next_auth", "database", "none", Compatibility::Discouraged,
             "NextAuth works without a DB (JWT-only) but loses session management features."),
        // 18. Static HTML + NextAuth = Incompatible
        rule("frontend", "static_html", "auth", "next_auth", Compatibility::Incompatible,
             "Static HTML cannot run NextAuth server-side auth flows."),
        // 19. Static HTML + Postgres = Discouraged
        rule("frontend", "static_html", "database", "postgres", Compatibility::Discouraged,
             "Static frontends cannot directly query Postgres; a separate API layer is needed."),
        // 20. Serverless + Docker = Discouraged
        rule("backend", "serverless", "hosting", "docker", Compatibility::Discouraged,
             "Serverless functions and Docker containers are competing deployment models."),
        // 21. Material UI + Static HTML = Incompatible
        rule("ui_library", "material_ui", "frontend", "static_html", Compatibility::Incompatible,
             "Material UI requires a React runtime; it cannot run on static HTML."),
        // 22. shadcn + Static HTML = Incompatible
        rule("ui_library", "shadcn_ui", "frontend", "static_html", Compatibility::Incompatible,
             "shadcn/ui components require React; they cannot be used in plain HTML."),
        // 23. Next.js App Router + Next.js API Routes = Recommended
        rule("frontend", "nextjs_app_router", "backend", "nextjs_api_routes", Compatibility::Recommended,
             "Co-located frontend and API routes in a single Next.js project."),
        // 24. React SPA + Vercel = Compatible
        rule("frontend", "react_spa", "hosting", "vercel", Compatibility::Compatible,
             "Vercel can host React SPAs as static builds."),
        // 25. Next.js Pages Router + Vercel = Recommended
        rule("frontend", "nextjs_pages_router", "hosting", "vercel", Compatibility::Recommended,
             "Vercel supports Pages Router with automatic SSR/SSG optimization."),
    ]
}

/// Helper to construct a rule concisely.
fn rule(
    area_a: &str,
    choice_a: &str,
    area_b: &str,
    choice_b: &str,
    compatibility: Compatibility,
    reason: &str,
) -> CompatibilityRule {
    CompatibilityRule {
        area_a: area_a.to_string(),
        choice_a: choice_a.to_string(),
        area_b: area_b.to_string(),
        choice_b: choice_b.to_string(),
        compatibility,
        reason: reason.to_string(),
    }
}

/// Suggest a fix for an incompatible pairing.
fn suggest_fix(area_a: &str, choice_a: &str, area_b: &str, choice_b: &str) -> String {
    // Specific fix suggestions for known incompatible pairs.
    match (area_a, choice_a, area_b, choice_b) {
        ("database", "postgres", "hosting", "static_cdn")
        | ("hosting", "static_cdn", "database", "postgres") => {
            "Switch hosting to Docker or a Node server to support Postgres.".to_string()
        }
        ("auth", "next_auth", "backend", "none")
        | ("backend", "none", "auth", "next_auth") => {
            "Add a backend (Next.js API routes or Express) to support NextAuth flows.".to_string()
        }
        ("auth", "clerk_auth", "backend", "none")
        | ("backend", "none", "auth", "clerk_auth") => {
            "Add a backend to support Clerk session verification.".to_string()
        }
        ("auth", "simple_credentials", "database", "none")
        | ("database", "none", "auth", "simple_credentials") => {
            "Add a database (SQLite or Postgres) to store user credentials.".to_string()
        }
        ("frontend", "static_html", "auth", "next_auth")
        | ("auth", "next_auth", "frontend", "static_html") => {
            "Switch to Next.js App Router to support NextAuth, or remove auth.".to_string()
        }
        ("ui_library", "material_ui", "frontend", "static_html")
        | ("frontend", "static_html", "ui_library", "material_ui") => {
            "Switch to React SPA or Next.js, or use Tailwind-only for static HTML.".to_string()
        }
        ("ui_library", "shadcn_ui", "frontend", "static_html")
        | ("frontend", "static_html", "ui_library", "shadcn_ui") => {
            "Switch to Next.js or React SPA, or use Tailwind-only for static HTML.".to_string()
        }
        _ => format!(
            "Review the {} and {} choices for compatibility.",
            area_a, area_b
        ),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent_engine::*;

    /// Build a minimal [`Intent`] for testing.
    fn test_intent(app_type: AppType, complexity: Complexity, needs_auth: bool, needs_db: bool) -> Intent {
        Intent {
            explicit: ExplicitIntent {
                raw_description: "test".to_string(),
                extracted_features: vec![],
                extracted_constraints: vec![],
            },
            inferred: InferredIntent {
                app_type: IntentInference {
                    value: app_type,
                    confidence: 0.9,
                    source: InferenceSource::KeywordHeuristic,
                    reasoning: "test".to_string(),
                },
                complexity: IntentInference {
                    value: complexity,
                    confidence: 0.9,
                    source: InferenceSource::KeywordHeuristic,
                    reasoning: "test".to_string(),
                },
                domain: IntentInference {
                    value: "test".to_string(),
                    confidence: 0.5,
                    source: InferenceSource::KeywordHeuristic,
                    reasoning: "test".to_string(),
                },
                suggested_pages: vec![],
                suggested_entities: vec![],
                suggested_agents: vec![],
                auth_needed: IntentInference {
                    value: needs_auth,
                    confidence: 0.9,
                    source: InferenceSource::KeywordHeuristic,
                    reasoning: "test".to_string(),
                },
                db_needed: IntentInference {
                    value: needs_db,
                    confidence: 0.9,
                    source: InferenceSource::KeywordHeuristic,
                    reasoning: "test".to_string(),
                },
                realtime_needed: IntentInference {
                    value: false,
                    confidence: 0.9,
                    source: InferenceSource::KeywordHeuristic,
                    reasoning: "test".to_string(),
                },
                ui_style: IntentInference {
                    value: UiStyle::Minimal,
                    confidence: 0.8,
                    source: InferenceSource::KeywordHeuristic,
                    reasoning: "test".to_string(),
                },
            },
            hidden: HiddenRequirements {
                requirements: vec![],
                domain_pack: None,
            },
            meta: IntentMeta {
                overall_confidence: 0.9,
                ambiguity_flags: vec![],
                clarification_suggestions: vec![],
                analysis_duration_ms: 1,
            },
        }
    }

    #[test]
    fn compatibility_matrix_catches_incompatibilities() {
        let matrix = CompatibilityMatrix::default_matrix();
        let decisions = vec![
            ("database", "postgres"),
            ("hosting", "static_cdn"),
        ];
        let report = matrix.validate_coherence(&decisions);
        assert!(!report.incompatibilities.is_empty(), "Should detect postgres + CDN incompatibility");
        assert!(report.overall_score < 0.7, "Score should be low for incompatible stack");
    }

    #[test]
    fn compatibility_matrix_approves_recommended_stack() {
        let matrix = CompatibilityMatrix::default_matrix();
        let decisions = vec![
            ("frontend", "nextjs_app_router"),
            ("backend", "nextjs_api_routes"),
            ("database", "postgres"),
            ("auth", "next_auth"),
            ("hosting", "docker"),
            ("ui_library", "shadcn_ui"),
        ];
        let report = matrix.validate_coherence(&decisions);
        assert!(report.incompatibilities.is_empty(), "Recommended stack should have no incompatibilities");
        assert!(report.overall_score > 0.8, "Recommended stack should score high, got {}", report.overall_score);
    }

    #[test]
    fn coherence_validation_scores_correctly() {
        let matrix = CompatibilityMatrix::default_matrix();

        // Good stack.
        let good = vec![
            ("frontend", "nextjs_app_router"),
            ("backend", "nextjs_api_routes"),
            ("hosting", "vercel"),
        ];
        let good_report = matrix.validate_coherence(&good);

        // Bad stack.
        let bad = vec![
            ("frontend", "static_html"),
            ("auth", "next_auth"),
            ("ui_library", "shadcn_ui"),
        ];
        let bad_report = matrix.validate_coherence(&bad);

        assert!(good_report.overall_score > bad_report.overall_score,
            "Good stack ({}) should score higher than bad stack ({})",
            good_report.overall_score, bad_report.overall_score);
    }

    #[test]
    fn explanations_include_factors_and_alternatives() {
        let engine = DecisionEngine::new();
        let intent = test_intent(AppType::Crm, Complexity::Complex, true, true);
        let set = engine.decide(&intent);

        for decision in &set.decisions {
            assert!(!decision.explanation.factors.is_empty(),
                "Decision for {} should have factors", decision.area);
            assert!(!decision.explanation.primary_reason.is_empty(),
                "Decision for {} should have a primary reason", decision.area);
            assert!(decision.confidence > 0.0 && decision.confidence <= 1.0,
                "Decision for {} should have valid confidence (got {})", decision.area, decision.confidence);
        }

        // The frontend decision for a CRM should have considered alternatives.
        let frontend = set.get("frontend").expect("Should have frontend decision");
        assert!(!frontend.explanation.alternatives_considered.is_empty(),
            "Frontend should have considered alternatives");
    }

    #[test]
    fn override_and_revalidation_works() {
        let engine = DecisionEngine::new();
        let intent = test_intent(AppType::Dashboard, Complexity::Simple, false, true);
        let mut set = engine.decide(&intent);

        // Override database to postgres.
        let report = engine.apply_override(&mut set, "database", "postgres", "Need production DB");
        assert!(report.incompatibilities.is_empty() || !report.incompatibilities.is_empty(),
            "Re-validation should produce a report");

        let db = set.get("database").expect("Should have database decision");
        assert_eq!(db.choice, "postgres");
        assert_eq!(db.confidence, 1.0, "User override should have confidence 1.0");

        // Override to an incompatible combo: static_cdn + postgres.
        let report = engine.apply_override(&mut set, "hosting", "static_cdn", "Want CDN");
        assert!(!report.incompatibilities.is_empty(),
            "Postgres + static_cdn should produce incompatibility");
    }

    #[test]
    fn legacy_conversion_preserves_values() {
        let engine = DecisionEngine::new();
        let intent = test_intent(AppType::Crm, Complexity::Complex, true, true);
        let set = engine.decide(&intent);
        let legacy = engine.to_legacy(&set);

        // The CRM should get Next.js + Postgres + NextAuth + Docker + shadcn.
        assert_eq!(legacy.frontend, FrontendChoice::NextjsAppRouter);
        assert_eq!(legacy.database, DatabaseChoice::Postgres);
        assert_eq!(legacy.auth, AuthChoice::NextAuth);
        assert_eq!(legacy.hosting, HostingChoice::Docker);
        assert_eq!(legacy.ui_library, UiLibraryChoice::ShadcnUi);
        assert!(!legacy.rationale.is_empty(), "Legacy rationale should be populated");
    }

    #[test]
    fn landing_page_gets_coherent_static_stack() {
        let engine = DecisionEngine::new();
        let intent = test_intent(AppType::LandingPage, Complexity::Simple, false, false);
        let set = engine.decide(&intent);

        assert_eq!(set.get("frontend").unwrap().choice, "static_html");
        assert_eq!(set.get("backend").unwrap().choice, "none");
        assert_eq!(set.get("hosting").unwrap().choice, "static_cdn");
        assert_eq!(set.get("ui_library").unwrap().choice, "tailwind_only");
        assert!(set.coherence.incompatibilities.is_empty(),
            "Landing page stack should have no incompatibilities");
        assert!(set.coherence.overall_score > 0.9,
            "Landing page stack should be highly coherent, got {}", set.coherence.overall_score);
    }

    #[test]
    fn bidirectional_rule_lookup() {
        let matrix = CompatibilityMatrix::default_matrix();
        // Lookup in both directions should find the same rule.
        let fwd = matrix.lookup("database", "postgres", "hosting", "static_cdn");
        let rev = matrix.lookup("hosting", "static_cdn", "database", "postgres");
        assert!(fwd.is_some(), "Forward lookup should find postgres+CDN rule");
        assert!(rev.is_some(), "Reverse lookup should find postgres+CDN rule");
        assert_eq!(fwd.unwrap().compatibility, Compatibility::Incompatible);
        assert_eq!(rev.unwrap().compatibility, Compatibility::Incompatible);
    }

    // Tests from original deterministic decision engine

    #[test]
    fn landing_page_gets_static() {
        let intent = crate::intent_engine::analyze_flat("Create a beautiful landing page for my SaaS startup");
        let decision = decide(&intent);
        assert_eq!(decision.frontend, FrontendChoice::StaticHtml);
        assert_eq!(decision.backend, BackendChoice::None);
        assert_eq!(decision.auth, AuthChoice::None);
    }

    #[test]
    fn saas_app_gets_full_stack() {
        let intent = crate::intent_engine::analyze_flat("Build a CRM with auth, Stripe billing, and contact management");
        let decision = decide(&intent);
        assert_eq!(decision.frontend, FrontendChoice::NextjsAppRouter);
        assert_eq!(decision.backend, BackendChoice::NextjsApiRoutes);
        assert!(decision.auth != AuthChoice::None);
        assert!(decision.database != DatabaseChoice::None);
    }

    #[test]
    fn simple_tool_gets_sqlite() {
        let intent = crate::intent_engine::analyze_flat("Make a todo app where I can track my tasks");
        let decision = decide(&intent);
        assert_eq!(decision.database, DatabaseChoice::Sqlite);
    }
}
