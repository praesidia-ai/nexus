//! "Explain My App" — one-click architecture analysis.

use std::sync::Arc;

use axum::{
    extract::State,
    Json,
};
use serde_json::json;

use crate::{
    error::{ApiError, ApiResult},
    project_brain::ProjectBrain,
    security::project_access::ProjectAccess,
    state::AppState,
};

/// GET /projects/:id/explain — generate a comprehensive explanation of the app
pub async fn explain_app(
    State(app): State<Arc<AppState>>,
    access: ProjectAccess,
) -> ApiResult<Json<serde_json::Value>> {
    let project_id = access.project_id.clone();
    let project_dir = app
        .data_dir
        .join("projects")
        .join(&project_id)
        .join("generated");
    if !project_dir.exists() {
        return Err(ApiError::BadRequest("No generated code found".into()));
    }

    let brain = ProjectBrain::load_or_scan(&project_dir);

    // Build a prompt for the LLM to explain the app
    let prompt = format!(
        r#"Analyze this project and provide a comprehensive explanation.

{}

Respond with ONLY a JSON object:
{{
  "summary": "2-3 sentence overview of what this app does",
  "architecture": {{
    "type": "monolith|microservices|serverless|spa|fullstack",
    "description": "How the app is structured",
    "layers": ["presentation", "business", "data"]
  }},
  "data_flow": [
    "User interacts with UI component",
    "Component calls API endpoint",
    "API handler queries database",
    "Response returned to UI"
  ],
  "tech_decisions": [
    {{"decision": "Using Next.js App Router", "reason": "Server-side rendering with React"}},
    {{"decision": "SQLite for storage", "reason": "Simple, file-based, no server needed"}}
  ],
  "risks": [
    {{"severity": "high|medium|low", "description": "What could go wrong", "mitigation": "How to fix it"}}
  ],
  "improvements": [
    {{"priority": "high|medium|low", "description": "What could be improved", "effort": "small|medium|large"}}
  ],
  "components": [
    {{"name": "ComponentName", "type": "page|component|api|service|config", "file": "path/to/file", "description": "What it does"}}
  ]
}}"#,
        brain.to_context()
    );

    // Also read a few key files for deeper analysis
    let mut file_context = String::new();
    for kf in brain.key_files.iter().take(5) {
        let path = project_dir.join(&kf.path);
        if let Ok(content) = std::fs::read_to_string(&path) {
            let preview: String = content.chars().take(2000).collect();
            file_context.push_str(&format!("\n--- {} ---\n{}\n", kf.path, preview));
        }
    }

    if !file_context.is_empty() {
        let full_prompt = format!("{}\n\nKey file contents:\n{}", prompt, file_context);
        // Call LLM
        let result =
            super::chat::call_llm_simple_for_project(&app, &full_prompt, Some(&project_id)).await;
        match result {
            Ok(response) => {
                // Try to parse JSON from response
                let cleaned = response
                    .trim()
                    .trim_start_matches("```json")
                    .trim_end_matches("```")
                    .trim();
                let explanation: serde_json::Value = serde_json::from_str(cleaned)
                    .unwrap_or_else(|_| json!({"summary": response, "raw": true}));

                Ok(Json(json!({
                    "brain": brain,
                    "explanation": explanation,
                })))
            }
            Err(e) => {
                // Fallback: return brain data without LLM analysis
                Ok(Json(json!({
                    "brain": brain,
                    "explanation": {
                        "summary": format!("A {} application built with {}", brain.stack.framework, brain.stack.language),
                        "error": format!("LLM analysis unavailable: {}", e),
                    }
                })))
            }
        }
    } else {
        Ok(Json(json!({
            "brain": brain,
            "explanation": {
                "summary": format!("A {} application built with {}", brain.stack.framework, brain.stack.language),
            }
        })))
    }
}
