//! Tests for nexus-intelligence types and default implementations.

#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use crate::intent::*;
    use crate::decisions::*;
    use crate::product::*;
    use crate::learning::*;
    use crate::domain_packs::*;
    use crate::default_intent::KeywordIntentAnalyzer;

    // -----------------------------------------------------------------------
    // Intent type serialization roundtrips
    // -----------------------------------------------------------------------

    #[test]
    fn app_type_serde_roundtrip() {
        for app_type in [
            AppType::WebApp,
            AppType::LandingPage,
            AppType::Dashboard,
            AppType::Ecommerce,
            AppType::Blog,
            AppType::Portfolio,
            AppType::SaaS,
            AppType::Marketplace,
            AppType::Social,
            AppType::CRM,
            AppType::Custom("myapp".into()),
        ] {
            let json = serde_json::to_string(&app_type).unwrap();
            let back: AppType = serde_json::from_str(&json).unwrap();
            assert_eq!(back, app_type);
        }
    }

    #[test]
    fn complexity_serde_roundtrip() {
        for c in [Complexity::Simple, Complexity::Medium, Complexity::Complex] {
            let json = serde_json::to_string(&c).unwrap();
            let back: Complexity = serde_json::from_str(&json).unwrap();
            assert_eq!(back, c);
        }
    }

    #[test]
    fn ui_style_serde_roundtrip() {
        for s in [
            UiStyle::Modern, UiStyle::Minimal, UiStyle::Bold,
            UiStyle::Corporate, UiStyle::Playful, UiStyle::Dark, UiStyle::Light,
        ] {
            let json = serde_json::to_string(&s).unwrap();
            let back: UiStyle = serde_json::from_str(&json).unwrap();
            assert_eq!(back, s);
        }
    }

    #[test]
    fn inference_serde_roundtrip() {
        let inf = Inference {
            value: "test".to_string(),
            confidence: 0.85,
            source: InferenceSource::KeywordHeuristic,
            reasoning: "matched keyword".to_string(),
        };
        let json = serde_json::to_string(&inf).unwrap();
        let back: Inference<String> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.value, "test");
        assert!((back.confidence - 0.85).abs() < f32::EPSILON);
    }

    #[test]
    fn page_suggestion_serde_roundtrip() {
        let page = PageSuggestion {
            name: "Dashboard".into(),
            route: "/dashboard".into(),
            description: "Main dashboard view".into(),
        };
        let json = serde_json::to_string(&page).unwrap();
        let back: PageSuggestion = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "Dashboard");
    }

    #[test]
    fn entity_suggestion_serde_roundtrip() {
        let entity = EntitySuggestion {
            name: "User".into(),
            fields: vec!["id".into(), "email".into(), "name".into()],
        };
        let json = serde_json::to_string(&entity).unwrap();
        let back: EntitySuggestion = serde_json::from_str(&json).unwrap();
        assert_eq!(back.fields.len(), 3);
    }

    // -----------------------------------------------------------------------
    // Decision type serialization
    // -----------------------------------------------------------------------

    #[test]
    fn explained_decision_serde_roundtrip() {
        let decision = ExplainedDecision {
            area: "frontend_framework".into(),
            choice: "Next.js".into(),
            confidence: 0.9,
            explanation: DecisionExplanation {
                primary_reason: "Best for SSR apps".into(),
                factors: vec![DecisionFactor {
                    factor: "complexity".into(),
                    weight: 0.5,
                    source: "intent".into(),
                }],
                alternatives: vec![Alternative {
                    choice: "Vite + React".into(),
                    score: 0.7,
                    reason_rejected: "No built-in SSR".into(),
                }],
                learning_influence: 0.2,
            },
        };
        let json = serde_json::to_string(&decision).unwrap();
        let back: ExplainedDecision = serde_json::from_str(&json).unwrap();
        assert_eq!(back.choice, "Next.js");
    }

    #[test]
    fn coherence_report_serde_roundtrip() {
        let report = CoherenceReport {
            overall_score: 0.95,
            warnings: vec![],
            incompatibilities: vec![],
            recommendations: vec!["All good".into()],
        };
        let json = serde_json::to_string(&report).unwrap();
        let back: CoherenceReport = serde_json::from_str(&json).unwrap();
        assert!((back.overall_score - 0.95).abs() < f32::EPSILON);
    }

    // -----------------------------------------------------------------------
    // Product type serialization
    // -----------------------------------------------------------------------

    #[test]
    fn product_brief_serde_roundtrip() {
        let brief = ProductBrief {
            domain: "fitness".into(),
            hero_headline: "Get Fit Today".into(),
            hero_subheadline: "Your personal trainer".into(),
            cta_text: "Start Free Trial".into(),
            features: vec![Feature {
                name: "Workout Tracking".into(),
                description: "Track your workouts".into(),
                priority: 1,
            }],
            personas: vec![Persona {
                name: "Alex the Runner".into(),
                role: "Amateur athlete".into(),
                goals: vec!["Run a marathon".into()],
                pain_points: vec!["No structured plan".into()],
            }],
            monetization: Some(MonetizationStrategy {
                model: "freemium".into(),
                tiers: vec![PricingTier {
                    name: "Free".into(),
                    price: "$0/mo".into(),
                    features: vec!["Basic tracking".into()],
                }],
            }),
        };
        let json = serde_json::to_string(&brief).unwrap();
        let back: ProductBrief = serde_json::from_str(&json).unwrap();
        assert_eq!(back.domain, "fitness");
        assert_eq!(back.features.len(), 1);
    }

    // -----------------------------------------------------------------------
    // Learning type serialization
    // -----------------------------------------------------------------------

    #[test]
    fn learning_signal_serde_roundtrip() {
        let signal = LearningSignal {
            id: "sig-1".into(),
            tenant_id: "tenant-1".into(),
            project_id: Some("proj-1".into()),
            signal_type: SignalType::StackDecision {
                area: "database".into(),
                choice: "PostgreSQL".into(),
            },
            context: serde_json::json!({"app_type": "SaaS"}),
            outcome: serde_json::json!({"build_success": true}),
            quality: 0.85,
            timestamp: "2026-01-01T00:00:00Z".into(),
        };
        let json = serde_json::to_string(&signal).unwrap();
        let back: LearningSignal = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "sig-1");
        assert!((back.quality - 0.85).abs() < f32::EPSILON);
    }

    #[test]
    fn signal_type_variants_serialize() {
        let variants: Vec<SignalType> = vec![
            SignalType::StackDecision { area: "db".into(), choice: "pg".into() },
            SignalType::TasteScore { score: 0.8 },
            SignalType::GuaranteeResult { passed: true, cycles: 1 },
            SignalType::BuildSuccess { first_try: true },
            SignalType::UserOverride { what: "db".into(), from: "mysql".into(), to: "pg".into() },
            SignalType::UserFeedback { sentiment: 0.5, context: "good".into() },
        ];
        for v in variants {
            let json = serde_json::to_string(&v).unwrap();
            let back: SignalType = serde_json::from_str(&json).unwrap();
            let _ = back; // Just verify deserialization succeeds
        }
    }

    // -----------------------------------------------------------------------
    // Domain pack serialization
    // -----------------------------------------------------------------------

    #[test]
    fn domain_pack_serde_roundtrip() {
        let pack = DomainPack {
            domain: "ecommerce".into(),
            triggers: vec!["shop".into(), "store".into(), "cart".into()],
            default_entities: vec![EntitySuggestion {
                name: "Product".into(),
                fields: vec!["id".into(), "name".into(), "price".into()],
            }],
            default_pages: vec![PageSuggestion {
                name: "Product Catalog".into(),
                route: "/products".into(),
                description: "Browse products".into(),
            }],
            hidden_requirements: vec![Requirement {
                category: "security".into(),
                description: "PCI DSS compliance for payments".into(),
                priority: RequirementPriority::Critical,
            }],
            ui_style_hint: Some(UiStyle::Modern),
        };
        let json = serde_json::to_string(&pack).unwrap();
        let back: DomainPack = serde_json::from_str(&json).unwrap();
        assert_eq!(back.domain, "ecommerce");
        assert_eq!(back.triggers.len(), 3);
    }

    // -----------------------------------------------------------------------
    // KeywordIntentAnalyzer tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn analyzer_detects_ecommerce() {
        let analyzer = KeywordIntentAnalyzer::default_empty();
        let intent = analyzer.analyze("Build an ecommerce store with a shopping cart").await.unwrap();
        assert_eq!(intent.inferred.app_type.value, AppType::Ecommerce);
    }

    #[tokio::test]
    async fn analyzer_detects_dashboard() {
        let analyzer = KeywordIntentAnalyzer::default_empty();
        let intent = analyzer.analyze("Create an analytics dashboard").await.unwrap();
        assert_eq!(intent.inferred.app_type.value, AppType::Dashboard);
    }

    #[tokio::test]
    async fn analyzer_detects_landing_page() {
        let analyzer = KeywordIntentAnalyzer::default_empty();
        let intent = analyzer.analyze("Build a landing page for my product").await.unwrap();
        assert_eq!(intent.inferred.app_type.value, AppType::LandingPage);
    }

    #[tokio::test]
    async fn analyzer_detects_saas() {
        let analyzer = KeywordIntentAnalyzer::default_empty();
        let intent = analyzer.analyze("Build a SaaS platform with subscription billing").await.unwrap();
        assert_eq!(intent.inferred.app_type.value, AppType::SaaS);
    }

    #[tokio::test]
    async fn analyzer_defaults_to_webapp() {
        let analyzer = KeywordIntentAnalyzer::default_empty();
        let intent = analyzer.analyze("Build something").await.unwrap();
        assert_eq!(intent.inferred.app_type.value, AppType::WebApp);
    }

    #[tokio::test]
    async fn analyzer_detects_auth_needed() {
        let analyzer = KeywordIntentAnalyzer::default_empty();
        let intent = analyzer.analyze("Build an app with login and signup").await.unwrap();
        assert!(intent.inferred.auth_needed.value);
        assert!(intent.inferred.auth_needed.confidence > 0.7);
    }

    #[tokio::test]
    async fn analyzer_detects_realtime() {
        let analyzer = KeywordIntentAnalyzer::default_empty();
        let intent = analyzer.analyze("Build a real-time chat application").await.unwrap();
        assert!(intent.inferred.realtime_needed.value);
    }

    #[tokio::test]
    async fn analyzer_extracts_features() {
        let analyzer = KeywordIntentAnalyzer::default_empty();
        let intent = analyzer.analyze("Build an app with search, payment processing, and file upload").await.unwrap();
        assert!(intent.explicit.extracted_features.len() >= 2);
    }

    #[tokio::test]
    async fn analyzer_detects_domain_fitness() {
        let analyzer = KeywordIntentAnalyzer::default_empty();
        let intent = analyzer.analyze("Build a fitness tracking app for workouts").await.unwrap();
        assert_eq!(intent.inferred.domain.value, "fitness");
    }

    #[tokio::test]
    async fn analyzer_preserves_raw_description() {
        let desc = "Build a fancy app";
        let analyzer = KeywordIntentAnalyzer::default_empty();
        let intent = analyzer.analyze(desc).await.unwrap();
        assert_eq!(intent.explicit.raw_description, desc);
    }

    #[tokio::test]
    async fn analyzer_clarify_low_confidence() {
        let analyzer = KeywordIntentAnalyzer::default_empty();
        let intent = analyzer.analyze("Build something").await.unwrap();
        let questions = analyzer.clarify(&intent).await.unwrap();
        // With low confidence on app_type (0.5) and domain (0.3), should get questions
        assert!(!questions.is_empty());
    }

    #[tokio::test]
    async fn analyzer_with_domain_pack() {
        let pack = DomainPack {
            domain: "restaurant".into(),
            triggers: vec!["restaurant".into(), "menu".into()],
            default_entities: vec![EntitySuggestion {
                name: "MenuItem".into(),
                fields: vec!["name".into(), "price".into()],
            }],
            default_pages: vec![PageSuggestion {
                name: "Menu".into(),
                route: "/menu".into(),
                description: "Restaurant menu".into(),
            }],
            hidden_requirements: vec![],
            ui_style_hint: Some(UiStyle::Modern),
        };
        let analyzer = KeywordIntentAnalyzer::new(vec![pack]);
        let intent = analyzer.analyze("Build a restaurant ordering system with a menu").await.unwrap();
        assert_eq!(intent.inferred.domain.value, "restaurant");
        assert!(!intent.inferred.suggested_pages.is_empty());
        assert!(!intent.inferred.suggested_entities.is_empty());
    }

    #[tokio::test]
    async fn analyzer_meta_has_duration() {
        let analyzer = KeywordIntentAnalyzer::default_empty();
        let intent = analyzer.analyze("Build a blog").await.unwrap();
        // Duration should be recorded (even if 0ms for fast analysis)
        assert!(intent.meta.analysis_duration_ms < 10000); // sanity check
    }

    #[tokio::test]
    async fn analyzer_detects_ui_style_minimal() {
        let analyzer = KeywordIntentAnalyzer::default_empty();
        let intent = analyzer.analyze("Build a minimal clean portfolio").await.unwrap();
        assert_eq!(intent.inferred.ui_style.value, UiStyle::Minimal);
    }

    #[tokio::test]
    async fn analyzer_detects_complex_app() {
        let analyzer = KeywordIntentAnalyzer::default_empty();
        let intent = analyzer.analyze(
            "Build a real-time collaborative platform with admin panel, user dashboard, \
             API integrations, websocket notifications, and multi-role access control"
        ).await.unwrap();
        assert_eq!(intent.inferred.complexity.value, Complexity::Complex);
    }
}
