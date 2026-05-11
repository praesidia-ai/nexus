//! LLM-assisted semantic fallback for the intent engine.
//!
//! Lives in a separate module from [`crate::intent_engine`] to preserve
//! the invariant that `intent_engine.rs` itself is fully deterministic
//! (no LLM calls). Callers that want richer inference for ambiguous
//! descriptions invoke [`semantic_fallback`] explicitly.

use std::sync::Arc;

use crate::intent_engine::{
    AppType, EntitySuggestion, InferenceSource, Intent, IntentInference, PageSuggestion, UiStyle,
};
use crate::state::AppState;

/// Run LLM-assisted semantic analysis for ambiguous descriptions.
///
/// Only meaningful when deterministic confidence is below threshold.
/// Merges the LLM analysis back into the existing `Intent`, with
/// deterministic results winning on conflicts.
pub async fn semantic_fallback(
    app: &Arc<AppState>,
    description: &str,
    deterministic: &Intent,
) -> Intent {
    let prompt = format!(
        r#"Analyze this app description and return a JSON object with your analysis.

Description: "{description}"

The deterministic engine already inferred:
- App type: {:?} (confidence: {:.2})
- Domain: {} (confidence: {:.2})
- Auth needed: {} (confidence: {:.2})
- DB needed: {} (confidence: {:.2})

For any field where confidence is below 0.7, provide your best guess.

Return ONLY a JSON object:
{{
  "app_type": "SaasApp|Dashboard|Marketplace|Crm|ECommerce|Blog|Portfolio|LandingPage|InternalTool|ApiOnly|Custom",
  "domain": "string",
  "auth_needed": true/false,
  "db_needed": true/false,
  "realtime_needed": true/false,
  "ui_style": "Minimal|Corporate|Playful|Luxurious|Technical|Bold",
  "additional_features": ["feature1", "feature2"],
  "additional_entities": [{{"name": "Entity", "fields": ["field1", "field2"]}}],
  "additional_pages": [{{"name": "Page", "route": "/page", "description": "What it does"}}]
}}"#,
        deterministic.inferred.app_type.value,
        deterministic.inferred.app_type.confidence,
        deterministic.inferred.domain.value,
        deterministic.inferred.domain.confidence,
        deterministic.inferred.auth_needed.value,
        deterministic.inferred.auth_needed.confidence,
        deterministic.inferred.db_needed.value,
        deterministic.inferred.db_needed.confidence,
    );

    let llm_result = crate::handlers::chat::call_llm_simple(app, &prompt).await;

    let mut result = deterministic.clone();

    let Ok(response_text) = llm_result else {
        tracing::warn!("Semantic fallback LLM call failed, returning deterministic result");
        return result;
    };

    let cleaned = response_text
        .trim()
        .trim_start_matches("```json")
        .trim_end_matches("```")
        .trim();

    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(cleaned) else {
        tracing::warn!("Semantic fallback returned unparseable JSON");
        return result;
    };

    // Merge: only upgrade fields where deterministic confidence < 0.7
    if result.inferred.app_type.confidence < 0.7 {
        if let Some(at) = parsed.get("app_type").and_then(|v| v.as_str()) {
            if let Some(parsed_type) = parse_app_type(at) {
                result.inferred.app_type = IntentInference {
                    value: parsed_type,
                    confidence: 0.75,
                    source: InferenceSource::SemanticAnalysis,
                    reasoning: "Upgraded by LLM semantic analysis".into(),
                };
            }
        }
    }

    if result.inferred.domain.confidence < 0.7 {
        if let Some(d) = parsed.get("domain").and_then(|v| v.as_str()) {
            result.inferred.domain = IntentInference {
                value: d.to_string(),
                confidence: 0.75,
                source: InferenceSource::SemanticAnalysis,
                reasoning: "Domain identified by LLM semantic analysis".into(),
            };
        }
    }

    if result.inferred.auth_needed.confidence < 0.7 {
        if let Some(auth) = parsed.get("auth_needed").and_then(|v| v.as_bool()) {
            result.inferred.auth_needed = IntentInference {
                value: auth,
                confidence: 0.75,
                source: InferenceSource::SemanticAnalysis,
                reasoning: "Auth requirement clarified by LLM".into(),
            };
        }
    }

    if result.inferred.db_needed.confidence < 0.7 {
        if let Some(db) = parsed.get("db_needed").and_then(|v| v.as_bool()) {
            result.inferred.db_needed = IntentInference {
                value: db,
                confidence: 0.75,
                source: InferenceSource::SemanticAnalysis,
                reasoning: "Database requirement clarified by LLM".into(),
            };
        }
    }

    if result.inferred.realtime_needed.confidence < 0.7 {
        if let Some(rt) = parsed.get("realtime_needed").and_then(|v| v.as_bool()) {
            result.inferred.realtime_needed = IntentInference {
                value: rt,
                confidence: 0.7,
                source: InferenceSource::SemanticAnalysis,
                reasoning: "Realtime requirement clarified by LLM".into(),
            };
        }
    }

    if result.inferred.ui_style.confidence < 0.7 {
        if let Some(style_str) = parsed.get("ui_style").and_then(|v| v.as_str()) {
            if let Some(style) = parse_ui_style(style_str) {
                result.inferred.ui_style = IntentInference {
                    value: style,
                    confidence: 0.7,
                    source: InferenceSource::SemanticAnalysis,
                    reasoning: "UI style suggested by LLM".into(),
                };
            }
        }
    }

    // Add LLM-suggested features
    if let Some(features) = parsed.get("additional_features").and_then(|v| v.as_array()) {
        for f in features {
            if let Some(feat) = f.as_str() {
                if !result
                    .explicit
                    .extracted_features
                    .iter()
                    .any(|ef| ef.value == feat)
                {
                    result.explicit.extracted_features.push(IntentInference {
                        value: feat.to_string(),
                        confidence: 0.65,
                        source: InferenceSource::SemanticAnalysis,
                        reasoning: "Additional feature suggested by LLM".into(),
                    });
                }
            }
        }
    }

    // Add LLM-suggested entities
    if let Some(entities) = parsed.get("additional_entities").and_then(|v| v.as_array()) {
        for e in entities {
            let name = e.get("name").and_then(|v| v.as_str()).unwrap_or_default();
            if !name.is_empty()
                && !result
                    .inferred
                    .suggested_entities
                    .iter()
                    .any(|se| se.value.name == name)
            {
                let fields: Vec<String> = e
                    .get("fields")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|f| f.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                result.inferred.suggested_entities.push(IntentInference {
                    value: EntitySuggestion {
                        name: name.to_string(),
                        fields,
                    },
                    confidence: 0.6,
                    source: InferenceSource::SemanticAnalysis,
                    reasoning: format!("Entity '{}' suggested by LLM", name),
                });
            }
        }
    }

    // Add LLM-suggested pages
    if let Some(pages) = parsed.get("additional_pages").and_then(|v| v.as_array()) {
        for p in pages {
            let name = p.get("name").and_then(|v| v.as_str()).unwrap_or_default();
            if !name.is_empty()
                && !result
                    .inferred
                    .suggested_pages
                    .iter()
                    .any(|sp| sp.value.name == name)
            {
                result.inferred.suggested_pages.push(IntentInference {
                    value: PageSuggestion {
                        name: name.to_string(),
                        route: p
                            .get("route")
                            .and_then(|v| v.as_str())
                            .unwrap_or("/")
                            .to_string(),
                        description: p
                            .get("description")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                    },
                    confidence: 0.6,
                    source: InferenceSource::SemanticAnalysis,
                    reasoning: format!("Page '{}' suggested by LLM", name),
                });
            }
        }
    }

    // Recalculate overall confidence after merge
    let key_confs = [
        result.inferred.app_type.confidence,
        result.inferred.auth_needed.confidence,
        result.inferred.db_needed.confidence,
        result.inferred.complexity.confidence,
        result.inferred.ui_style.confidence,
    ];
    result.meta.overall_confidence = key_confs.iter().sum::<f32>() / key_confs.len() as f32;

    // Remove ambiguity flags for fields that were resolved
    result.meta.ambiguity_flags.retain(|flag| match flag.as_str() {
        "app_type" => result.inferred.app_type.confidence < 0.7,
        "auth_needed" => result.inferred.auth_needed.confidence < 0.7,
        "db_needed" => result.inferred.db_needed.confidence < 0.7,
        "domain" => result.inferred.domain.confidence < 0.7,
        "ui_style" => result.inferred.ui_style.confidence < 0.7,
        _ => true,
    });

    result
}

fn parse_app_type(s: &str) -> Option<AppType> {
    match s {
        "LandingPage" => Some(AppType::LandingPage),
        "SaasApp" => Some(AppType::SaasApp),
        "Dashboard" => Some(AppType::Dashboard),
        "Marketplace" => Some(AppType::Marketplace),
        "Crm" => Some(AppType::Crm),
        "ECommerce" => Some(AppType::ECommerce),
        "Portfolio" => Some(AppType::Portfolio),
        "Blog" => Some(AppType::Blog),
        "InternalTool" => Some(AppType::InternalTool),
        "MobileApp" => Some(AppType::MobileApp),
        "ApiOnly" => Some(AppType::ApiOnly),
        "Custom" => Some(AppType::Custom),
        _ => None,
    }
}

fn parse_ui_style(s: &str) -> Option<UiStyle> {
    match s {
        "Minimal" => Some(UiStyle::Minimal),
        "Corporate" => Some(UiStyle::Corporate),
        "Playful" => Some(UiStyle::Playful),
        "Luxurious" => Some(UiStyle::Luxurious),
        "Technical" => Some(UiStyle::Technical),
        "Bold" => Some(UiStyle::Bold),
        _ => None,
    }
}
