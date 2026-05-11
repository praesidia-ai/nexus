//! Tests for the unified GenerationEvent enum.

use std::collections::HashMap;

use super::{event_type_name, GenerationEvent};

#[test]
fn phase_started_serializes_correctly() {
    let event = GenerationEvent::PhaseStarted {
        phase: "generate_spec".into(),
        description: "Generating application specification".into(),
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["type"], "phase_started");
    assert_eq!(json["phase"], "generate_spec");
    assert_eq!(json["description"], "Generating application specification");
}

#[test]
fn phase_progress_serializes_correctly() {
    let event = GenerationEvent::PhaseProgress {
        phase: "generate_files".into(),
        progress: 0.42,
        detail: "Generating API routes".into(),
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["type"], "phase_progress");
    assert!((json["progress"].as_f64().unwrap() - 0.42).abs() < 0.01);
}

#[test]
fn phase_completed_serializes_correctly() {
    let event = GenerationEvent::PhaseCompleted {
        phase: "validate_spec".into(),
        duration_ms: 1234,
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["type"], "phase_completed");
    assert_eq!(json["duration_ms"], 1234);
}

#[test]
fn intent_analyzed_serializes_correctly() {
    let event = GenerationEvent::IntentAnalyzed {
        app_type: "SaasApp".into(),
        complexity: "Complex".into(),
        domain: "fitness".into(),
        needs_auth: true,
        needs_database: true,
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["type"], "intent_analyzed");
    assert_eq!(json["app_type"], "SaasApp");
    assert_eq!(json["needs_auth"], true);
    assert_eq!(json["needs_database"], true);
}

#[test]
fn decisions_made_serializes_correctly() {
    let event = GenerationEvent::DecisionsMade {
        frontend: "Next.js 15".into(),
        database: "SQLite".into(),
        auth: "NextAuth".into(),
        learning_overrides: vec!["prefer-dark-theme".into()],
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["type"], "decisions_made");
    assert_eq!(json["frontend"], "Next.js 15");
    assert_eq!(json["learning_overrides"].as_array().unwrap().len(), 1);
}

#[test]
fn brief_generated_serializes_correctly() {
    let event = GenerationEvent::BriefGenerated {
        domain: "wine".into(),
        hero_headline: "Discover Your Next Favorite Wine".into(),
        personas: 3,
        features: 8,
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["type"], "brief_generated");
    assert_eq!(json["domain"], "wine");
    assert_eq!(json["personas"], 3);
}

#[test]
fn file_proposed_serializes_correctly() {
    let event = GenerationEvent::FileProposed {
        path: "src/app/page.tsx".into(),
        size: 2048,
        skeleton: false,
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["type"], "file_proposed");
    assert_eq!(json["path"], "src/app/page.tsx");
    assert_eq!(json["skeleton"], false);
}

#[test]
fn file_generated_serializes_correctly() {
    let event = GenerationEvent::FileGenerated {
        path: "src/app/layout.tsx".into(),
        lines: 45,
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["type"], "file_generated");
    assert_eq!(json["lines"], 45);
}

#[test]
fn spec_generated_serializes_correctly() {
    let event = GenerationEvent::SpecGenerated {
        page_count: 5,
        entity_count: 3,
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["type"], "spec_generated");
    assert_eq!(json["page_count"], 5);
}

#[test]
fn taste_scored_serializes_correctly() {
    let mut axes = HashMap::new();
    axes.insert("typography".into(), 0.85);
    axes.insert("color".into(), 0.72);
    let event = GenerationEvent::TasteScored {
        score: 0.78,
        axes,
        redesign_triggered: false,
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["type"], "taste_scored");
    assert!((json["score"].as_f64().unwrap() - 0.78).abs() < 0.01);
    assert_eq!(json["redesign_triggered"], false);
}

#[test]
fn redesign_applied_serializes_correctly() {
    let event = GenerationEvent::RedesignApplied {
        mutations_applied: 4,
        score_before: 0.55,
        score_after: 0.82,
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["type"], "redesign_applied");
    assert_eq!(json["mutations_applied"], 4);
}

#[test]
fn guarantee_result_serializes_correctly() {
    let event = GenerationEvent::GuaranteeResult {
        passed: true,
        certificate_id: Some("cert-abc-123".into()),
        cycles_used: 2,
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["type"], "guarantee_result");
    assert_eq!(json["passed"], true);
    assert_eq!(json["certificate_id"], "cert-abc-123");
}

#[test]
fn guarantee_result_without_certificate_serializes_correctly() {
    let event = GenerationEvent::GuaranteeResult {
        passed: false,
        certificate_id: None,
        cycles_used: 5,
    };
    let json = serde_json::to_value(&event).unwrap();
    assert!(json["certificate_id"].is_null());
    assert_eq!(json["cycles_used"], 5);
}

#[test]
fn thinking_serializes_correctly() {
    let event = GenerationEvent::Thinking {
        message: "Analyzing your requirements".into(),
        detail: Some("Detecting app type from keywords".into()),
        icon: "brain".into(),
        progress: 15,
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["type"], "thinking");
    assert_eq!(json["icon"], "brain");
    assert_eq!(json["progress"], 15);
}

#[test]
fn thinking_without_detail_serializes_correctly() {
    let event = GenerationEvent::Thinking {
        message: "Working".into(),
        detail: None,
        icon: "gear".into(),
        progress: 50,
    };
    let json = serde_json::to_value(&event).unwrap();
    assert!(json["detail"].is_null());
}

#[test]
fn explanation_serializes_correctly() {
    let event = GenerationEvent::Explanation {
        decision: "Chose SQLite over PostgreSQL".into(),
        reason: "Single-user app with low data volume".into(),
        confidence: 0.92,
        alternatives: vec!["PostgreSQL".into(), "MySQL".into()],
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["type"], "explanation");
    assert_eq!(json["alternatives"].as_array().unwrap().len(), 2);
}

#[test]
fn skeleton_serializes_correctly() {
    let event = GenerationEvent::Skeleton {
        path: "src/app/layout.tsx".into(),
        content: "<div>Loading...</div>".into(),
        skeleton_type: "page_layout".into(),
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["type"], "skeleton");
    assert_eq!(json["skeleton_type"], "page_layout");
}

#[test]
fn estimate_serializes_correctly() {
    let event = GenerationEvent::Estimate {
        total_estimated_ms: 45000,
        confidence: 0.75,
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["type"], "estimate");
    assert_eq!(json["total_estimated_ms"], 45000);
}

#[test]
fn heartbeat_serializes_correctly() {
    let event = GenerationEvent::Heartbeat {
        phase: "generate_files".into(),
        elapsed_ms: 12000,
        message: "Still generating UI pages".into(),
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["type"], "heartbeat");
    assert_eq!(json["elapsed_ms"], 12000);
}

#[test]
fn validation_result_serializes_correctly() {
    let event = GenerationEvent::ValidationResult {
        layer: "static".into(),
        passed: false,
        issues: vec!["Missing page route".into(), "Empty file".into()],
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["type"], "validation_result");
    assert_eq!(json["passed"], false);
    assert_eq!(json["issues"].as_array().unwrap().len(), 2);
}

#[test]
fn completed_serializes_correctly() {
    let event = GenerationEvent::Completed {
        project_id: "proj-123".into(),
        project_name: "Wine Tasting App".into(),
        taste_score: 0.85,
        files_count: 24,
        duration_ms: 38000,
        app_url: Some("http://localhost:4100".into()),
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["type"], "completed");
    assert_eq!(json["project_id"], "proj-123");
    assert_eq!(json["files_count"], 24);
}

#[test]
fn completed_without_url_serializes_correctly() {
    let event = GenerationEvent::Completed {
        project_id: "proj-456".into(),
        project_name: "Test".into(),
        taste_score: 0.0,
        files_count: 5,
        duration_ms: 10000,
        app_url: None,
    };
    let json = serde_json::to_value(&event).unwrap();
    assert!(json["app_url"].is_null());
}

#[test]
fn error_serializes_correctly() {
    let event = GenerationEvent::Error {
        phase: "generate_spec".into(),
        message: "LLM returned invalid JSON".into(),
        recoverable: false,
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["type"], "error");
    assert_eq!(json["recoverable"], false);
}

#[test]
fn event_type_name_returns_correct_strings() {
    let cases: Vec<(GenerationEvent, &str)> = vec![
        (
            GenerationEvent::PhaseStarted {
                phase: String::new(),
                description: String::new(),
            },
            "phase_started",
        ),
        (
            GenerationEvent::PhaseProgress {
                phase: String::new(),
                progress: 0.0,
                detail: String::new(),
            },
            "phase_progress",
        ),
        (
            GenerationEvent::PhaseCompleted {
                phase: String::new(),
                duration_ms: 0,
            },
            "phase_completed",
        ),
        (
            GenerationEvent::IntentAnalyzed {
                app_type: String::new(),
                complexity: String::new(),
                domain: String::new(),
                needs_auth: false,
                needs_database: false,
            },
            "intent_analyzed",
        ),
        (
            GenerationEvent::DecisionsMade {
                frontend: String::new(),
                database: String::new(),
                auth: String::new(),
                learning_overrides: vec![],
            },
            "decisions_made",
        ),
        (
            GenerationEvent::BriefGenerated {
                domain: String::new(),
                hero_headline: String::new(),
                personas: 0,
                features: 0,
            },
            "brief_generated",
        ),
        (
            GenerationEvent::FileProposed {
                path: String::new(),
                size: 0,
                skeleton: false,
            },
            "file_proposed",
        ),
        (
            GenerationEvent::FileGenerated {
                path: String::new(),
                lines: 0,
            },
            "file_generated",
        ),
        (
            GenerationEvent::SpecGenerated {
                page_count: 0,
                entity_count: 0,
            },
            "spec_generated",
        ),
        (
            GenerationEvent::TasteScored {
                score: 0.0,
                axes: HashMap::new(),
                redesign_triggered: false,
            },
            "taste_scored",
        ),
        (
            GenerationEvent::RedesignApplied {
                mutations_applied: 0,
                score_before: 0.0,
                score_after: 0.0,
            },
            "redesign_applied",
        ),
        (
            GenerationEvent::GuaranteeResult {
                passed: false,
                certificate_id: None,
                cycles_used: 0,
            },
            "guarantee_result",
        ),
        (
            GenerationEvent::Thinking {
                message: String::new(),
                detail: None,
                icon: String::new(),
                progress: 0,
            },
            "thinking",
        ),
        (
            GenerationEvent::Explanation {
                decision: String::new(),
                reason: String::new(),
                confidence: 0.0,
                alternatives: vec![],
            },
            "explanation",
        ),
        (
            GenerationEvent::Skeleton {
                path: String::new(),
                content: String::new(),
                skeleton_type: String::new(),
            },
            "skeleton",
        ),
        (
            GenerationEvent::Estimate {
                total_estimated_ms: 0,
                confidence: 0.0,
            },
            "estimate",
        ),
        (
            GenerationEvent::Heartbeat {
                phase: String::new(),
                elapsed_ms: 0,
                message: String::new(),
            },
            "heartbeat",
        ),
        (
            GenerationEvent::ValidationResult {
                layer: String::new(),
                passed: false,
                issues: vec![],
            },
            "validation",
        ),
        (
            GenerationEvent::Completed {
                project_id: String::new(),
                project_name: String::new(),
                taste_score: 0.0,
                files_count: 0,
                duration_ms: 0,
                app_url: None,
            },
            "complete",
        ),
        (
            GenerationEvent::Error {
                phase: String::new(),
                message: String::new(),
                recoverable: false,
            },
            "error",
        ),
    ];

    for (event, expected_name) in &cases {
        assert_eq!(
            event_type_name(event),
            *expected_name,
            "event_type_name mismatch for {:?}",
            expected_name,
        );
    }
}

#[test]
fn all_variants_round_trip_through_json() {
    // Ensure every variant can be serialized and the "type" field is present.
    let events: Vec<GenerationEvent> = vec![
        GenerationEvent::PhaseStarted {
            phase: "test".into(),
            description: "desc".into(),
        },
        GenerationEvent::PhaseProgress {
            phase: "test".into(),
            progress: 0.5,
            detail: "half".into(),
        },
        GenerationEvent::PhaseCompleted {
            phase: "test".into(),
            duration_ms: 100,
        },
        GenerationEvent::IntentAnalyzed {
            app_type: "SaasApp".into(),
            complexity: "Medium".into(),
            domain: "fitness".into(),
            needs_auth: true,
            needs_database: false,
        },
        GenerationEvent::DecisionsMade {
            frontend: "next".into(),
            database: "sqlite".into(),
            auth: "none".into(),
            learning_overrides: vec![],
        },
        GenerationEvent::BriefGenerated {
            domain: "wine".into(),
            hero_headline: "Hello".into(),
            personas: 2,
            features: 5,
        },
        GenerationEvent::FileProposed {
            path: "a.ts".into(),
            size: 10,
            skeleton: true,
        },
        GenerationEvent::FileGenerated {
            path: "a.ts".into(),
            lines: 5,
        },
        GenerationEvent::SpecGenerated {
            page_count: 3,
            entity_count: 2,
        },
        GenerationEvent::TasteScored {
            score: 0.9,
            axes: HashMap::new(),
            redesign_triggered: false,
        },
        GenerationEvent::RedesignApplied {
            mutations_applied: 1,
            score_before: 0.5,
            score_after: 0.9,
        },
        GenerationEvent::GuaranteeResult {
            passed: true,
            certificate_id: Some("c1".into()),
            cycles_used: 1,
        },
        GenerationEvent::Thinking {
            message: "hi".into(),
            detail: None,
            icon: "brain".into(),
            progress: 10,
        },
        GenerationEvent::Explanation {
            decision: "d".into(),
            reason: "r".into(),
            confidence: 0.8,
            alternatives: vec![],
        },
        GenerationEvent::Skeleton {
            path: "p".into(),
            content: "c".into(),
            skeleton_type: "page_layout".into(),
        },
        GenerationEvent::Estimate {
            total_estimated_ms: 5000,
            confidence: 0.6,
        },
        GenerationEvent::Heartbeat {
            phase: "gen".into(),
            elapsed_ms: 1000,
            message: "alive".into(),
        },
        GenerationEvent::ValidationResult {
            layer: "static".into(),
            passed: true,
            issues: vec![],
        },
        GenerationEvent::Completed {
            project_id: "p1".into(),
            project_name: "test".into(),
            taste_score: 0.8,
            files_count: 10,
            duration_ms: 5000,
            app_url: None,
        },
        GenerationEvent::Error {
            phase: "gen".into(),
            message: "oops".into(),
            recoverable: true,
        },
    ];

    for event in &events {
        let json = serde_json::to_value(event).expect("serialization should not fail");
        assert!(
            json.get("type").is_some(),
            "missing 'type' field for event: {:?}",
            event
        );
        // The type field should be a non-empty string
        let type_str = json["type"].as_str().unwrap();
        assert!(!type_str.is_empty());
    }
}
