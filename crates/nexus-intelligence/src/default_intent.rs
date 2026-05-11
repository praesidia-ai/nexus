//! Default keyword-heuristic implementation of [`IntentAnalyzer`].
//!
//! Uses simple keyword matching and domain pack lookup to produce
//! a deterministic [`Intent`] from a raw description. No LLM calls.

use std::time::Instant;

use async_trait::async_trait;

use super::domain_packs::DomainPack;
use super::intent::*;

/// Built-in keyword-heuristic intent analyzer.
///
/// Scans the user description for known keywords to infer app type,
/// complexity, auth needs, database needs, etc. When a domain pack
/// matches, its defaults are merged into the result.
pub struct KeywordIntentAnalyzer {
    domain_packs: Vec<DomainPack>,
}

impl KeywordIntentAnalyzer {
    /// Create a new analyzer with the given domain packs.
    pub fn new(domain_packs: Vec<DomainPack>) -> Self {
        Self { domain_packs }
    }

    /// Create an analyzer with no domain packs (pure keyword heuristic).
    pub fn default_empty() -> Self {
        Self {
            domain_packs: Vec::new(),
        }
    }

    fn detect_app_type(&self, lower: &str) -> Inference<AppType> {
        let (app_type, confidence, reasoning) = if lower.contains("landing page")
            || lower.contains("launch page")
        {
            (AppType::LandingPage, 0.9, "Matched 'landing page' keyword")
        } else if lower.contains("dashboard") || lower.contains("analytics") {
            (AppType::Dashboard, 0.85, "Matched 'dashboard/analytics' keyword")
        } else if lower.contains("ecommerce")
            || lower.contains("e-commerce")
            || lower.contains("shop")
            || lower.contains("store")
            || lower.contains("cart")
        {
            (AppType::Ecommerce, 0.85, "Matched ecommerce keyword")
        } else if lower.contains("blog") || lower.contains("publication") {
            (AppType::Blog, 0.85, "Matched 'blog' keyword")
        } else if lower.contains("portfolio") {
            (AppType::Portfolio, 0.9, "Matched 'portfolio' keyword")
        } else if lower.contains("saas") || lower.contains("subscription") {
            (AppType::SaaS, 0.8, "Matched 'saas/subscription' keyword")
        } else if lower.contains("marketplace") {
            (AppType::Marketplace, 0.9, "Matched 'marketplace' keyword")
        } else if lower.contains("social") || lower.contains("community") {
            (AppType::Social, 0.75, "Matched 'social/community' keyword")
        } else if lower.contains("crm") || lower.contains("customer relationship") {
            (AppType::CRM, 0.85, "Matched 'crm' keyword")
        } else {
            (AppType::WebApp, 0.5, "Defaulted to WebApp — no specific keywords matched")
        };

        Inference {
            value: app_type,
            confidence,
            source: InferenceSource::KeywordHeuristic,
            reasoning: reasoning.to_string(),
        }
    }

    fn detect_complexity(&self, lower: &str) -> Inference<Complexity> {
        let word_count = lower.split_whitespace().count();
        let has_realtime = lower.contains("real-time") || lower.contains("realtime") || lower.contains("websocket") || lower.contains("live");
        let has_multi_role = lower.contains("admin") && lower.contains("user");
        let has_api = lower.contains("api") || lower.contains("integration");

        let complexity_score = word_count as f32 / 50.0
            + if has_realtime { 0.4 } else { 0.0 }
            + if has_multi_role { 0.3 } else { 0.0 }
            + if has_api { 0.2 } else { 0.0 };

        let (complexity, confidence) = if complexity_score > 1.0 {
            (Complexity::Complex, 0.7)
        } else if complexity_score > 0.5 {
            (Complexity::Medium, 0.65)
        } else {
            (Complexity::Simple, 0.6)
        };

        Inference {
            value: complexity,
            confidence,
            source: InferenceSource::KeywordHeuristic,
            reasoning: format!("Complexity score {complexity_score:.2} from word count + feature indicators"),
        }
    }

    fn detect_domain(&self, lower: &str) -> (Inference<String>, Option<&DomainPack>) {
        for pack in &self.domain_packs {
            for trigger in &pack.triggers {
                if lower.contains(trigger.as_str()) {
                    return (
                        Inference {
                            value: pack.domain.clone(),
                            confidence: 0.85,
                            source: InferenceSource::DomainPack,
                            reasoning: format!("Matched domain pack trigger '{trigger}'"),
                        },
                        Some(pack),
                    );
                }
            }
        }

        // Fallback: extract a broad domain from keywords
        let domain = if lower.contains("fitness") || lower.contains("gym") || lower.contains("workout") {
            "fitness"
        } else if lower.contains("finance") || lower.contains("fintech") || lower.contains("banking") {
            "fintech"
        } else if lower.contains("health") || lower.contains("medical") {
            "healthcare"
        } else if lower.contains("food") || lower.contains("restaurant") || lower.contains("recipe") {
            "food"
        } else if lower.contains("education") || lower.contains("course") || lower.contains("learning") {
            "education"
        } else if lower.contains("travel") || lower.contains("booking") || lower.contains("hotel") {
            "travel"
        } else {
            "general"
        };

        (
            Inference {
                value: domain.to_string(),
                confidence: if domain == "general" { 0.3 } else { 0.7 },
                source: InferenceSource::KeywordHeuristic,
                reasoning: format!("Inferred domain '{domain}' from keywords"),
            },
            None,
        )
    }

    fn detect_auth(&self, lower: &str) -> Inference<bool> {
        let needs_auth = lower.contains("login")
            || lower.contains("sign up")
            || lower.contains("signup")
            || lower.contains("auth")
            || lower.contains("user account")
            || lower.contains("register")
            || lower.contains("password")
            || lower.contains("admin");

        Inference {
            value: needs_auth,
            confidence: if needs_auth { 0.85 } else { 0.5 },
            source: InferenceSource::KeywordHeuristic,
            reasoning: if needs_auth {
                "Auth-related keywords found".to_string()
            } else {
                "No auth keywords — uncertain".to_string()
            },
        }
    }

    fn detect_db(&self, lower: &str) -> Inference<bool> {
        let needs_db = lower.contains("database")
            || lower.contains("data")
            || lower.contains("store")
            || lower.contains("save")
            || lower.contains("crud")
            || lower.contains("list")
            || lower.contains("manage");

        Inference {
            value: needs_db,
            confidence: if needs_db { 0.8 } else { 0.5 },
            source: InferenceSource::KeywordHeuristic,
            reasoning: if needs_db {
                "Data-persistence keywords found".to_string()
            } else {
                "No data keywords — uncertain".to_string()
            },
        }
    }

    fn detect_realtime(&self, lower: &str) -> Inference<bool> {
        let needs_rt = lower.contains("real-time")
            || lower.contains("realtime")
            || lower.contains("live")
            || lower.contains("websocket")
            || lower.contains("chat")
            || lower.contains("notification");

        Inference {
            value: needs_rt,
            confidence: if needs_rt { 0.8 } else { 0.6 },
            source: InferenceSource::KeywordHeuristic,
            reasoning: if needs_rt {
                "Real-time keywords found".to_string()
            } else {
                "No real-time keywords".to_string()
            },
        }
    }

    fn detect_ui_style(&self, lower: &str) -> Inference<UiStyle> {
        let style = if lower.contains("minimal") || lower.contains("clean") || lower.contains("simple design") {
            UiStyle::Minimal
        } else if lower.contains("bold") || lower.contains("vibrant") || lower.contains("colorful") {
            UiStyle::Bold
        } else if lower.contains("corporate") || lower.contains("enterprise") || lower.contains("professional") {
            UiStyle::Corporate
        } else if lower.contains("playful") || lower.contains("fun") || lower.contains("casual") {
            UiStyle::Playful
        } else if lower.contains("dark mode") || lower.contains("dark theme") {
            UiStyle::Dark
        } else {
            UiStyle::Modern
        };

        Inference {
            value: style,
            confidence: 0.6,
            source: InferenceSource::KeywordHeuristic,
            reasoning: "UI style inferred from description keywords".to_string(),
        }
    }

    fn extract_features(&self, lower: &str) -> Vec<Inference<String>> {
        let keywords = [
            ("search", "Search functionality"),
            ("filter", "Filtering and sorting"),
            ("notification", "Notification system"),
            ("chat", "Chat / messaging"),
            ("payment", "Payment processing"),
            ("upload", "File upload"),
            ("export", "Data export"),
            ("api", "API integration"),
            ("email", "Email notifications"),
            ("analytics", "Analytics dashboard"),
        ];

        keywords
            .iter()
            .filter(|(kw, _)| lower.contains(kw))
            .map(|(_, desc)| Inference {
                value: desc.to_string(),
                confidence: 0.75,
                source: InferenceSource::KeywordHeuristic,
                reasoning: "Keyword matched in description".to_string(),
            })
            .collect()
    }
}

#[async_trait]
impl IntentAnalyzer for KeywordIntentAnalyzer {
    async fn analyze(
        &self,
        description: &str,
    ) -> Result<Intent, Box<dyn std::error::Error + Send + Sync>> {
        let start = Instant::now();
        let lower = description.to_lowercase();

        let app_type = self.detect_app_type(&lower);
        let complexity = self.detect_complexity(&lower);
        let (domain, domain_pack) = self.detect_domain(&lower);
        let auth_needed = self.detect_auth(&lower);
        let db_needed = self.detect_db(&lower);
        let realtime_needed = self.detect_realtime(&lower);
        let ui_style = domain_pack
            .and_then(|p| p.ui_style_hint.as_ref())
            .map(|s| Inference {
                value: s.clone(),
                confidence: 0.7,
                source: InferenceSource::DomainPack,
                reasoning: "UI style from domain pack".to_string(),
            })
            .unwrap_or_else(|| self.detect_ui_style(&lower));

        let extracted_features = self.extract_features(&lower);

        // Merge domain pack suggestions if available
        let suggested_pages = domain_pack
            .map(|p| {
                p.default_pages
                    .iter()
                    .map(|page| Inference {
                        value: page.clone(),
                        confidence: 0.7,
                        source: InferenceSource::DomainPack,
                        reasoning: format!("Default page from '{}' domain pack", p.domain),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let suggested_entities = domain_pack
            .map(|p| {
                p.default_entities
                    .iter()
                    .map(|e| Inference {
                        value: e.clone(),
                        confidence: 0.7,
                        source: InferenceSource::DomainPack,
                        reasoning: format!("Default entity from '{}' domain pack", p.domain),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let hidden_requirements = domain_pack
            .map(|p| HiddenRequirements {
                requirements: p
                    .hidden_requirements
                    .iter()
                    .map(|r| Inference {
                        value: r.clone(),
                        confidence: 0.7,
                        source: InferenceSource::DomainPack,
                        reasoning: format!("Hidden requirement from '{}' domain pack", p.domain),
                    })
                    .collect(),
                domain_pack: Some(p.domain.clone()),
            })
            .unwrap_or_else(|| HiddenRequirements {
                requirements: Vec::new(),
                domain_pack: None,
            });

        let confidences: Vec<f32> = vec![
            app_type.confidence,
            complexity.confidence,
            domain.confidence,
            auth_needed.confidence,
            db_needed.confidence,
        ];
        let overall_confidence =
            confidences.iter().sum::<f32>() / confidences.len() as f32;

        let mut ambiguity_flags = Vec::new();
        if app_type.confidence < 0.6 {
            ambiguity_flags.push("app_type".to_string());
        }
        if domain.confidence < 0.5 {
            ambiguity_flags.push("domain".to_string());
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(Intent {
            explicit: ExplicitIntent {
                raw_description: description.to_string(),
                extracted_features,
                extracted_constraints: Vec::new(),
            },
            inferred: InferredIntent {
                app_type,
                complexity,
                domain,
                suggested_pages,
                suggested_entities,
                auth_needed,
                db_needed,
                realtime_needed,
                ui_style,
            },
            hidden: hidden_requirements,
            meta: IntentMeta {
                overall_confidence,
                ambiguity_flags,
                clarification_suggestions: Vec::new(),
                analysis_duration_ms: duration_ms,
            },
        })
    }

    async fn clarify(
        &self,
        intent: &Intent,
    ) -> Result<Vec<ClarificationQuestion>, Box<dyn std::error::Error + Send + Sync>> {
        let mut questions = Vec::new();

        if intent.inferred.app_type.confidence < 0.7 {
            questions.push(ClarificationQuestion {
                field: "app_type".to_string(),
                question: "What type of application are you building?".to_string(),
                current_confidence: intent.inferred.app_type.confidence,
                options: vec![
                    "Web App".to_string(),
                    "Landing Page".to_string(),
                    "Dashboard".to_string(),
                    "E-commerce Store".to_string(),
                    "Blog".to_string(),
                    "SaaS Product".to_string(),
                ],
            });
        }

        if intent.inferred.auth_needed.confidence < 0.7 {
            questions.push(ClarificationQuestion {
                field: "auth_needed".to_string(),
                question: "Does this app need user authentication (login/signup)?".to_string(),
                current_confidence: intent.inferred.auth_needed.confidence,
                options: vec!["Yes".to_string(), "No".to_string()],
            });
        }

        if intent.inferred.domain.confidence < 0.5 {
            questions.push(ClarificationQuestion {
                field: "domain".to_string(),
                question: "What industry or domain is this for?".to_string(),
                current_confidence: intent.inferred.domain.confidence,
                options: vec![
                    "Fitness".to_string(),
                    "Fintech".to_string(),
                    "Healthcare".to_string(),
                    "Education".to_string(),
                    "Food & Restaurant".to_string(),
                    "Travel".to_string(),
                    "Other".to_string(),
                ],
            });
        }

        Ok(questions)
    }
}
