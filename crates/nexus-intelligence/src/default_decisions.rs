//! Default rule-based implementation of [`DecisionMaker`].
//!
//! Uses the analyzed intent to make architecture decisions via weighted
//! scoring rules. Each decision area has a set of candidate options
//! scored against intent signals.

use async_trait::async_trait;

use super::decisions::*;
use super::intent::{AppType, Complexity, Intent};

/// Built-in rule-based decision maker.
///
/// Scores technology candidates per decision area using intent signals
/// (app type, complexity, domain, auth/db/realtime needs).
pub struct RuleBasedDecisionMaker;

impl RuleBasedDecisionMaker {
    pub fn new() -> Self {
        Self
    }

    fn decide_frontend(&self, intent: &Intent) -> ExplainedDecision {
        let is_simple = intent.inferred.complexity.value == Complexity::Simple;
        let is_landing = intent.inferred.app_type.value == AppType::LandingPage;

        let (choice, reason, alt_reason) = if is_landing || is_simple {
            (
                "Next.js (static)",
                "Simple/landing app benefits from static generation",
                "React SPA would add unnecessary client-side complexity",
            )
        } else {
            (
                "Next.js (App Router)",
                "Complex app benefits from server components and API routes",
                "Plain React lacks built-in routing and SSR",
            )
        };

        ExplainedDecision {
            area: "frontend_framework".to_string(),
            choice: choice.to_string(),
            confidence: 0.8,
            explanation: DecisionExplanation {
                primary_reason: reason.to_string(),
                factors: vec![
                    DecisionFactor {
                        factor: "app_complexity".to_string(),
                        weight: 0.4,
                        source: "intent".to_string(),
                    },
                    DecisionFactor {
                        factor: "app_type".to_string(),
                        weight: 0.3,
                        source: "intent".to_string(),
                    },
                ],
                alternatives: vec![Alternative {
                    choice: "React SPA".to_string(),
                    score: 0.5,
                    reason_rejected: alt_reason.to_string(),
                }],
                learning_influence: 0.0,
            },
        }
    }

    fn decide_database(&self, intent: &Intent) -> ExplainedDecision {
        let needs_db = intent.inferred.db_needed.value;
        let is_complex = intent.inferred.complexity.value == Complexity::Complex;

        let (choice, reason) = if !needs_db {
            ("None", "No database needed based on intent")
        } else if is_complex {
            ("PostgreSQL", "Complex app benefits from relational integrity and JSON support")
        } else {
            ("SQLite", "Simple/medium app works well with embedded SQLite")
        };

        ExplainedDecision {
            area: "database".to_string(),
            choice: choice.to_string(),
            confidence: if needs_db { 0.75 } else { 0.6 },
            explanation: DecisionExplanation {
                primary_reason: reason.to_string(),
                factors: vec![
                    DecisionFactor {
                        factor: "db_needed".to_string(),
                        weight: 0.5,
                        source: "intent".to_string(),
                    },
                    DecisionFactor {
                        factor: "complexity".to_string(),
                        weight: 0.3,
                        source: "intent".to_string(),
                    },
                ],
                alternatives: vec![Alternative {
                    choice: "MongoDB".to_string(),
                    score: 0.4,
                    reason_rejected: "Relational data model better for most apps".to_string(),
                }],
                learning_influence: 0.0,
            },
        }
    }

    fn decide_auth(&self, intent: &Intent) -> ExplainedDecision {
        let needs_auth = intent.inferred.auth_needed.value;

        let (choice, reason) = if !needs_auth {
            ("None", "No authentication needed")
        } else {
            ("NextAuth.js + JWT", "Standard auth for Next.js apps with session management")
        };

        ExplainedDecision {
            area: "auth".to_string(),
            choice: choice.to_string(),
            confidence: if needs_auth { 0.8 } else { 0.7 },
            explanation: DecisionExplanation {
                primary_reason: reason.to_string(),
                factors: vec![DecisionFactor {
                    factor: "auth_needed".to_string(),
                    weight: 0.8,
                    source: "intent".to_string(),
                }],
                alternatives: if needs_auth {
                    vec![Alternative {
                        choice: "Clerk".to_string(),
                        score: 0.6,
                        reason_rejected: "Third-party dependency; self-hosted preferred".to_string(),
                    }]
                } else {
                    vec![]
                },
                learning_influence: 0.0,
            },
        }
    }

    fn decide_styling(&self, _intent: &Intent) -> ExplainedDecision {
        ExplainedDecision {
            area: "styling".to_string(),
            choice: "Tailwind CSS + shadcn/ui".to_string(),
            confidence: 0.85,
            explanation: DecisionExplanation {
                primary_reason: "Modern utility-first CSS with accessible component primitives".to_string(),
                factors: vec![
                    DecisionFactor {
                        factor: "ui_style".to_string(),
                        weight: 0.3,
                        source: "intent".to_string(),
                    },
                    DecisionFactor {
                        factor: "best_practice".to_string(),
                        weight: 0.5,
                        source: "default_rules".to_string(),
                    },
                ],
                alternatives: vec![Alternative {
                    choice: "CSS Modules".to_string(),
                    score: 0.4,
                    reason_rejected: "Less productive than utility-first for rapid generation".to_string(),
                }],
                learning_influence: 0.0,
            },
        }
    }

    fn decide_hosting(&self, intent: &Intent) -> ExplainedDecision {
        let needs_realtime = intent.inferred.realtime_needed.value;

        let (choice, reason) = if needs_realtime {
            ("Node.js server (long-running)", "Real-time features need persistent connections")
        } else {
            ("Vercel / serverless", "Serverless ideal for request-response apps")
        };

        ExplainedDecision {
            area: "hosting".to_string(),
            choice: choice.to_string(),
            confidence: 0.7,
            explanation: DecisionExplanation {
                primary_reason: reason.to_string(),
                factors: vec![DecisionFactor {
                    factor: "realtime_needed".to_string(),
                    weight: 0.6,
                    source: "intent".to_string(),
                }],
                alternatives: vec![],
                learning_influence: 0.0,
            },
        }
    }
}

impl Default for RuleBasedDecisionMaker {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DecisionMaker for RuleBasedDecisionMaker {
    async fn decide(
        &self,
        intent: &Intent,
    ) -> Result<DecisionSet, Box<dyn std::error::Error + Send + Sync>> {
        let decisions = vec![
            self.decide_frontend(intent),
            self.decide_database(intent),
            self.decide_auth(intent),
            self.decide_styling(intent),
            self.decide_hosting(intent),
        ];

        let coherence = self.validate_coherence(&decisions).await?;

        Ok(DecisionSet {
            decisions,
            coherence,
        })
    }

    async fn validate_coherence(
        &self,
        decisions: &[ExplainedDecision],
    ) -> Result<CoherenceReport, Box<dyn std::error::Error + Send + Sync>> {
        let mut warnings = Vec::new();
        let mut incompatibilities = Vec::new();

        // Check: serverless + realtime WebSocket
        let hosting = decisions.iter().find(|d| d.area == "hosting");
        let has_serverless = hosting
            .map(|h| h.choice.contains("serverless") || h.choice.contains("Vercel"))
            .unwrap_or(false);

        // If the description implies real-time but hosting is serverless, flag it
        if has_serverless {
            // Check if any decision implies real-time
            let has_realtime_signal = decisions
                .iter()
                .any(|d| d.choice.contains("WebSocket") || d.choice.contains("real-time"));

            if has_realtime_signal {
                incompatibilities.push(CoherenceIncompatibility {
                    area_a: "hosting".to_string(),
                    choice_a: hosting.map(|h| h.choice.clone()).unwrap_or_default(),
                    area_b: "realtime".to_string(),
                    choice_b: "WebSocket".to_string(),
                    reason: "Serverless platforms have limited WebSocket support".to_string(),
                    suggested_fix: "Switch to a long-running Node.js server for real-time features".to_string(),
                });
            }
        }

        // Check: no DB + auth (auth usually needs persistence)
        let db = decisions.iter().find(|d| d.area == "database");
        let auth = decisions.iter().find(|d| d.area == "auth");
        let no_db = db.map(|d| d.choice == "None").unwrap_or(true);
        let has_auth = auth.map(|a| a.choice != "None").unwrap_or(false);

        if no_db && has_auth {
            warnings.push(CoherenceWarning {
                area_a: "database".to_string(),
                choice_a: "None".to_string(),
                area_b: "auth".to_string(),
                choice_b: auth.map(|a| a.choice.clone()).unwrap_or_default(),
                reason: "Authentication typically requires a database to store user sessions".to_string(),
            });
        }

        let score = if incompatibilities.is_empty() && warnings.is_empty() {
            1.0
        } else if incompatibilities.is_empty() {
            0.8
        } else {
            0.4
        };

        Ok(CoherenceReport {
            overall_score: score,
            warnings,
            incompatibilities,
            recommendations: Vec::new(),
        })
    }
}
