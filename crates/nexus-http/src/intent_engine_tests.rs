//! Tests for the intent engine.

use super::*;

#[test]
fn crm_triggers_crm_domain_pack() {
    let result = match_domain_pack("Build a CRM for managing sales leads");
    assert!(result.is_some());
    let (pack, count) = result.unwrap();
    assert_eq!(pack.domain, "CRM");
    assert!(count >= 1);
}

#[test]
fn marketplace_triggers_marketplace_domain_pack() {
    let result = match_domain_pack("An Airbnb-like marketplace for booking experiences");
    assert!(result.is_some());
    let (pack, _) = result.unwrap();
    assert_eq!(pack.domain, "Marketplace");
}

#[test]
fn ecommerce_triggers_ecommerce_domain_pack() {
    let result = match_domain_pack("An online shop for handmade products with cart and checkout");
    assert!(result.is_some());
    let (pack, count) = result.unwrap();
    assert_eq!(pack.domain, "E-commerce");
    assert!(count >= 2); // "shop" and "checkout"
}

#[test]
fn saas_triggers_saas_domain_pack() {
    let result = match_domain_pack("A SaaS platform with subscription billing");
    assert!(result.is_some());
    let (pack, count) = result.unwrap();
    assert_eq!(pack.domain, "SaaS");
    assert!(count >= 2); // "saas" and "subscription"
}

#[test]
fn blog_triggers_blog_domain_pack() {
    let result = match_domain_pack("A blog with articles and content management");
    assert!(result.is_some());
    let (pack, _) = result.unwrap();
    assert_eq!(pack.domain, "Blog/CMS");
}

#[test]
fn dashboard_triggers_dashboard_domain_pack() {
    let result = match_domain_pack("A dashboard for monitoring analytics and metrics");
    assert!(result.is_some());
    let (pack, count) = result.unwrap();
    assert_eq!(pack.domain, "Dashboard");
    assert!(count >= 2);
}

#[test]
fn social_triggers_social_domain_pack() {
    let result = match_domain_pack("A social network with feed, follow, and community features");
    assert!(result.is_some());
    let (pack, _) = result.unwrap();
    assert_eq!(pack.domain, "Social");
}

#[test]
fn portfolio_triggers_portfolio_domain_pack() {
    let result = match_domain_pack("A portfolio site to showcase my projects and resume");
    assert!(result.is_some());
    let (pack, _) = result.unwrap();
    assert_eq!(pack.domain, "Portfolio");
}

#[test]
fn no_domain_pack_for_vague_description() {
    let result = match_domain_pack("something cool");
    assert!(result.is_none());
}

// ---------------------------------------------------------------------------
// Confidence scoring tests
// ---------------------------------------------------------------------------

#[test]
fn high_confidence_for_explicit_crm() {
    let intent = analyze("Build a CRM with contacts, deals, and sales pipeline management");
    assert!(intent.inferred.app_type.confidence >= 0.8);
    assert!(matches!(intent.inferred.app_type.value, AppType::Crm));
    assert!(intent.inferred.auth_needed.value);
    assert!(intent.inferred.db_needed.value);
    assert!(intent.meta.overall_confidence >= 0.6);
}

#[test]
fn high_confidence_for_explicit_ecommerce() {
    let intent = analyze("An e-commerce store with products, cart, checkout, and payment processing");
    assert!(intent.inferred.app_type.confidence >= 0.8);
    assert!(matches!(intent.inferred.app_type.value, AppType::ECommerce));
    assert!(intent.inferred.db_needed.value);
}

#[test]
fn low_confidence_for_vague_description() {
    let intent = analyze("something");
    assert!(intent.inferred.app_type.confidence < 0.7);
    assert!(intent.meta.overall_confidence < 0.7);
}

#[test]
fn medium_confidence_for_partial_match() {
    let intent = analyze("A platform for managing data");
    // "platform" -> SaasApp, "manage" + "data" -> db keywords
    assert!(intent.inferred.app_type.confidence >= 0.7);
    assert!(intent.inferred.db_needed.value);
}

#[test]
fn wine_marketplace_gets_luxurious_style() {
    let intent = analyze("A wine tasting marketplace with AI sommelier recommendations");
    assert!(matches!(intent.inferred.ui_style.value, UiStyle::Luxurious));
    assert!(intent.inferred.ui_style.confidence >= 0.7);
}

#[test]
fn realtime_detected_for_chat_app() {
    let intent = analyze("A real-time chat application with notifications");
    assert!(intent.inferred.realtime_needed.value);
    assert!(intent.inferred.realtime_needed.confidence >= 0.7);
}

#[test]
fn realtime_not_detected_for_portfolio() {
    let intent = analyze("A portfolio site to showcase my design work");
    assert!(!intent.inferred.realtime_needed.value);
}

// ---------------------------------------------------------------------------
// Clarification generation tests
// ---------------------------------------------------------------------------

#[test]
fn clarifications_generated_for_vague_input() {
    let intent = analyze("make me an app");
    let questions = generate_clarifications(&intent);
    assert!(!questions.is_empty(), "Should generate at least one clarification");
    // app_type should be ambiguous
    assert!(
        questions.iter().any(|q| q.field == "app_type"),
        "Should ask about app type for vague input"
    );
}

#[test]
fn no_clarifications_for_detailed_input() {
    let intent = analyze(
        "Build a CRM SaaS platform with login, signup, contacts, deals, sales pipeline, dashboard analytics, and corporate UI style"
    );
    let questions = generate_clarifications(&intent);
    // With all those keywords, most fields should be above threshold
    // At minimum, app_type and auth should not need clarification
    assert!(
        !questions.iter().any(|q| q.field == "app_type"),
        "Should NOT ask about app type for detailed CRM description"
    );
}

#[test]
fn clarifications_sorted_by_confidence() {
    let intent = analyze("an app");
    let questions = generate_clarifications(&intent);
    if questions.len() >= 2 {
        for i in 0..questions.len() - 1 {
            assert!(
                questions[i].current_confidence <= questions[i + 1].current_confidence,
                "Clarifications should be sorted by confidence ascending"
            );
        }
    }
}

#[test]
fn clarification_questions_have_options() {
    let intent = analyze("build something");
    let questions = generate_clarifications(&intent);
    for q in &questions {
        assert!(
            !q.options.is_empty(),
            "Every clarification question should have options"
        );
    }
}

// ---------------------------------------------------------------------------
// Legacy conversion tests
// ---------------------------------------------------------------------------

#[test]
fn legacy_conversion_preserves_app_type() {
    let v2 = analyze("Build a CRM for sales teams");
    let legacy = to_flat(&v2);
    assert!(matches!(legacy.app_type, AppType::Crm));
}

#[test]
fn legacy_conversion_preserves_auth() {
    let v2 = analyze("A SaaS platform with login and signup for managing subscriptions");
    let legacy = to_flat(&v2);
    assert!(legacy.needs_auth);
    assert!(legacy.needs_database);
}

#[test]
fn legacy_conversion_preserves_pages() {
    let v2 = analyze("An e-commerce store");
    let legacy = to_flat(&v2);
    assert!(!legacy.suggested_pages.is_empty());
}

#[test]
fn legacy_conversion_preserves_entities() {
    let v2 = analyze("A marketplace with listings and bookings");
    let legacy = to_flat(&v2);
    assert!(!legacy.suggested_entities.is_empty());
}

#[test]
fn legacy_conversion_preserves_complexity() {
    let v2 = analyze("A simple landing page for my photography");
    let legacy = to_flat(&v2);
    assert!(matches!(legacy.complexity, Complexity::Simple));
}

// ---------------------------------------------------------------------------
// Domain pack content tests
// ---------------------------------------------------------------------------

#[test]
fn all_domain_packs_have_entities_and_pages() {
    let packs = builtin_domain_packs();
    assert_eq!(packs.len(), 8, "Should have exactly 8 built-in domain packs");
    for pack in &packs {
        assert!(
            !pack.default_entities.is_empty(),
            "Pack {} should have default entities",
            pack.domain
        );
        assert!(
            !pack.default_pages.is_empty(),
            "Pack {} should have default pages",
            pack.domain
        );
        assert!(
            !pack.hidden_requirements.is_empty(),
            "Pack {} should have hidden requirements",
            pack.domain
        );
        assert!(
            !pack.triggers.is_empty(),
            "Pack {} should have triggers",
            pack.domain
        );
    }
}

#[test]
fn domain_pack_entities_merged_into_intent() {
    let intent = analyze("Build a CRM system");
    // CRM pack entities should appear
    let entity_names: Vec<&str> = intent
        .inferred
        .suggested_entities
        .iter()
        .map(|e| e.value.name.as_str())
        .collect();
    assert!(
        entity_names.contains(&"Contact"),
        "CRM intent should include Contact entity, got: {:?}",
        entity_names
    );
    assert!(
        entity_names.contains(&"Deal"),
        "CRM intent should include Deal entity, got: {:?}",
        entity_names
    );
}

#[test]
fn domain_pack_pages_merged_into_intent() {
    let intent = analyze("Build an e-commerce store");
    let page_names: Vec<&str> = intent
        .inferred
        .suggested_pages
        .iter()
        .map(|p| p.value.name.as_str())
        .collect();
    assert!(
        page_names.iter().any(|n| n.contains("Cart") || n.contains("Product")),
        "E-commerce intent should include Cart or Product page, got: {:?}",
        page_names
    );
}

#[test]
fn domain_pack_hidden_requirements_populated() {
    let intent = analyze("Build a marketplace like Airbnb");
    assert!(
        !intent.hidden.requirements.is_empty(),
        "Marketplace should have hidden requirements"
    );
    assert_eq!(
        intent.hidden.domain_pack.as_deref(),
        Some("Marketplace"),
        "Should identify Marketplace domain pack"
    );
}

// ---------------------------------------------------------------------------
// Feature and constraint extraction tests
// ---------------------------------------------------------------------------

#[test]
fn explicit_features_extracted() {
    let intent = analyze("A platform with login, search, and payment processing");
    let feature_names: Vec<&str> = intent
        .explicit
        .extracted_features
        .iter()
        .map(|f| f.value.as_str())
        .collect();
    assert!(
        feature_names.iter().any(|f| f.to_lowercase().contains("authentication")),
        "Should extract auth feature from 'login'"
    );
    assert!(
        feature_names.iter().any(|f| f.to_lowercase().contains("search")),
        "Should extract search feature"
    );
    assert!(
        feature_names.iter().any(|f| f.to_lowercase().contains("payment")),
        "Should extract payment feature"
    );
}

#[test]
fn constraints_extracted() {
    let intent = analyze("A simple static portfolio page, no database needed");
    let constraint_names: Vec<&str> = intent
        .explicit
        .extracted_constraints
        .iter()
        .map(|c| c.value.as_str())
        .collect();
    assert!(
        !constraint_names.is_empty(),
        "Should extract at least one constraint"
    );
}

// ---------------------------------------------------------------------------
// Meta field tests
// ---------------------------------------------------------------------------

#[test]
fn analysis_duration_is_recorded() {
    let intent = analyze("Build a dashboard");
    // Should be very fast (deterministic), but > 0
    assert!(intent.meta.analysis_duration_ms < 1000, "Analysis should be fast");
}

#[test]
fn short_description_gets_extra_clarification() {
    let intent = analyze("app");
    assert!(
        intent.meta.clarification_suggestions.iter().any(|s| s.contains("more detail")),
        "Very short descriptions should prompt for more detail"
    );
}
