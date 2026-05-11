//! Tests for nexus-quality-core types and default implementations.

#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use crate::scoring::*;
    use crate::guarantee::*;
    use crate::fix_plan::*;
    use crate::default_scoring::HeuristicQualityScorer;

    // -----------------------------------------------------------------------
    // Scoring types serialization
    // -----------------------------------------------------------------------

    #[test]
    fn taste_score_serde_roundtrip() {
        let score = TasteScore {
            overall: 82.5,
            dimensions: vec![DimensionScore {
                dimension: "layout".into(),
                score: 85.0,
                weight: 0.3,
                details: vec!["Good semantic HTML".into()],
            }],
            issues: vec![QualityIssue {
                severity: IssueSeverity::Warning,
                dimension: "layout".into(),
                description: "Missing viewport meta".into(),
                location: Some(IssueLocation {
                    file: "index.html".into(),
                    line: Some(1),
                    selector: None,
                }),
                fix_suggestion: Some("Add viewport meta tag".into()),
            }],
            suggestions: vec!["Add viewport meta".into()],
            grade: "B+".into(),
            passes_threshold: true,
        };
        let json = serde_json::to_string(&score).unwrap();
        let back: TasteScore = serde_json::from_str(&json).unwrap();
        assert!((back.overall - 82.5).abs() < f32::EPSILON);
        assert_eq!(back.grade, "B+");
        assert!(back.passes_threshold);
    }

    #[test]
    fn issue_severity_serde_roundtrip() {
        for sev in [IssueSeverity::Critical, IssueSeverity::Warning, IssueSeverity::Info] {
            let json = serde_json::to_string(&sev).unwrap();
            let back: IssueSeverity = serde_json::from_str(&json).unwrap();
            assert_eq!(back, sev);
        }
    }

    #[test]
    fn dimension_score_serde_roundtrip() {
        let dim = DimensionScore {
            dimension: "typography".into(),
            score: 72.0,
            weight: 0.25,
            details: vec!["Font sizes within range".into()],
        };
        let json = serde_json::to_string(&dim).unwrap();
        let back: DimensionScore = serde_json::from_str(&json).unwrap();
        assert_eq!(back.dimension, "typography");
    }

    // -----------------------------------------------------------------------
    // Guarantee types serialization
    // -----------------------------------------------------------------------

    #[test]
    fn guarantee_config_default() {
        let config = GuaranteeConfig::default();
        assert_eq!(config.max_cycles, 3);
        assert!(config.auto_fix);
        assert_eq!(config.checks.len(), 3);
        assert_eq!(config.min_quality_score, Some(70.0));
    }

    #[test]
    fn guarantee_config_serde_roundtrip() {
        let config = GuaranteeConfig {
            max_cycles: 5,
            checks: vec!["build".into(), "quality".into()],
            min_quality_score: Some(80.0),
            auto_fix: false,
        };
        let json = serde_json::to_string(&config).unwrap();
        let back: GuaranteeConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.max_cycles, 5);
        assert!(!back.auto_fix);
    }

    #[test]
    fn check_result_serde_roundtrip() {
        let result = CheckResult {
            check: "build".into(),
            passed: true,
            details: "Build succeeded".into(),
            duration_ms: 1234,
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: CheckResult = serde_json::from_str(&json).unwrap();
        assert!(back.passed);
        assert_eq!(back.duration_ms, 1234);
    }

    #[test]
    fn guarantee_certificate_serde_roundtrip() {
        let cert = GuaranteeCertificate {
            id: "cert-1".into(),
            project_id: "proj-1".into(),
            issued_at: "2026-01-01T00:00:00Z".into(),
            checks: vec![CheckResult {
                check: "build".into(),
                passed: true,
                details: "ok".into(),
                duration_ms: 100,
            }],
            cycles_used: 1,
            max_cycles: 3,
            final_score: None,
            all_passed: true,
        };
        let json = serde_json::to_string(&cert).unwrap();
        let back: GuaranteeCertificate = serde_json::from_str(&json).unwrap();
        assert!(back.all_passed);
        assert_eq!(back.cycles_used, 1);
    }

    // -----------------------------------------------------------------------
    // Fix plan types serialization
    // -----------------------------------------------------------------------

    #[test]
    fn fix_plan_serde_roundtrip() {
        let plan = FixPlan {
            id: "fix-1".into(),
            project_id: "proj-1".into(),
            steps: vec![FixStep {
                order: 1,
                description: "Add viewport meta tag".into(),
                file: "index.html".into(),
                change_type: ChangeType::Insert,
                change: serde_json::json!({"content": "<meta name=\"viewport\" ...>"}),
                confidence: 0.95,
            }],
            estimated_effort: FixEffort::Trivial,
            addresses_checks: vec!["quality".into()],
        };
        let json = serde_json::to_string(&plan).unwrap();
        let back: FixPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(back.steps.len(), 1);
    }

    #[test]
    fn change_type_all_variants_serialize() {
        for ct in [
            ChangeType::Replace, ChangeType::Insert, ChangeType::Delete,
            ChangeType::CreateFile, ChangeType::DeleteFile, ChangeType::RunCommand,
        ] {
            let json = serde_json::to_string(&ct).unwrap();
            let back: ChangeType = serde_json::from_str(&json).unwrap();
            let _ = back;
        }
    }

    #[test]
    fn fix_effort_all_variants_serialize() {
        for effort in [FixEffort::Trivial, FixEffort::Minor, FixEffort::Moderate, FixEffort::Major] {
            let json = serde_json::to_string(&effort).unwrap();
            let back: FixEffort = serde_json::from_str(&json).unwrap();
            let _ = back;
        }
    }

    // -----------------------------------------------------------------------
    // HeuristicQualityScorer tests
    // -----------------------------------------------------------------------

    #[test]
    fn heuristic_scorer_default_threshold() {
        let scorer = HeuristicQualityScorer::default();
        assert!((scorer.threshold() - 70.0).abs() < f32::EPSILON);
    }

    #[test]
    fn heuristic_scorer_custom_threshold() {
        let scorer = HeuristicQualityScorer::new(85.0);
        assert!((scorer.threshold() - 85.0).abs() < f32::EPSILON);
    }

    #[tokio::test]
    async fn heuristic_scorer_no_files_returns_default() {
        let scorer = HeuristicQualityScorer::default();
        let score = scorer.score("/nonexistent/path", &[]).await.unwrap();
        assert!((score.overall - 50.0).abs() < f32::EPSILON);
        assert!(!score.passes_threshold);
    }

    #[tokio::test]
    async fn heuristic_scorer_with_html_file() {
        use std::io::Write;
        let dir = std::env::temp_dir().join("nexus-test-quality");
        let _ = std::fs::create_dir_all(&dir);
        let html_path = dir.join("index.html");
        let mut f = std::fs::File::create(&html_path).unwrap();
        writeln!(f, "<!DOCTYPE html><html><head><meta name=\"viewport\" content=\"width=device-width\"></head><body><header>Hi</header><main>Content</main><footer>End</footer></body></html>").unwrap();

        let scorer = HeuristicQualityScorer::default();
        let score = scorer.score(dir.to_str().unwrap(), &["index.html".into()]).await.unwrap();
        assert!(score.overall > 60.0, "Expected decent score for valid HTML, got {}", score.overall);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn heuristic_scorer_penalizes_missing_doctype() {
        use std::io::Write;
        let dir = std::env::temp_dir().join("nexus-test-quality-bad");
        let _ = std::fs::create_dir_all(&dir);
        let html_path = dir.join("bad.html");
        let mut f = std::fs::File::create(&html_path).unwrap();
        writeln!(f, "<html><body><div>No doctype</div></body></html>").unwrap();

        let scorer = HeuristicQualityScorer::default();
        let score = scorer.score(dir.to_str().unwrap(), &["bad.html".into()]).await.unwrap();
        // Should have issues about missing doctype and viewport
        assert!(!score.issues.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
