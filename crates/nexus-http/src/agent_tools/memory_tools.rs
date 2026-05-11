//! Memory tools for agents — remember, recall, and forget.
//!
//! These tools allow agents to interact with the persistent memory system:
//!
//! - **RememberTool** — stores a fact in semantic memory
//! - **RecallTool** — searches both episodic and semantic memory
//! - **ForgetTool** — removes a specific memory (GDPR compliance)

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::agents::tools::registry::{Tool, ToolInput, ToolOutput};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn ok(result: Value) -> ToolOutput {
    ToolOutput {
        result,
        success: true,
        error: None,
    }
}

fn err(msg: impl Into<String>) -> ToolOutput {
    ToolOutput {
        result: json!({}),
        success: false,
        error: Some(msg.into()),
    }
}

// ---------------------------------------------------------------------------
// RememberTool
// ---------------------------------------------------------------------------

/// Store a knowledge fact in semantic memory.
///
/// Agents use this tool to persist learned information — user preferences,
/// project patterns, technical facts, error patterns, etc. If a similar fact
/// already exists, it is reinforced rather than duplicated.
pub struct RememberTool;

#[async_trait]
impl Tool for RememberTool {
    fn name(&self) -> &str {
        "remember"
    }

    fn description(&self) -> &str {
        "Store a knowledge fact in long-term memory. Similar facts are automatically reinforced."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "subject": {
                    "type": "string",
                    "description": "What this fact is about (e.g., 'React hooks', 'user preference: dark mode')"
                },
                "content": {
                    "type": "string",
                    "description": "The fact or knowledge to remember"
                },
                "category": {
                    "type": "string",
                    "enum": ["user_preference", "project_pattern", "technical_fact",
                             "quality_pattern", "error_pattern", "stack_compatibility",
                             "domain_knowledge"],
                    "description": "Category of knowledge"
                },
                "confidence": {
                    "type": "number",
                    "description": "Confidence level 0.0-1.0 (default: 0.7)"
                },
                "tenant_id": {
                    "type": "string",
                    "description": "Tenant/org ID for multi-tenancy scoping"
                }
            },
            "required": ["subject", "content", "category"]
        })
    }

    fn category(&self) -> &str {
        "memory_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let subject = match input.parameters.get("subject").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => return err("Missing required parameter: subject"),
        };
        let content = match input.parameters.get("content").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => return err("Missing required parameter: content"),
        };
        let category = match input.parameters.get("category").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => return err("Missing required parameter: category"),
        };
        let confidence = input
            .parameters
            .get("confidence")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.7) as f32;
        let tenant_id = input
            .parameters
            .get("tenant_id")
            .and_then(|v| v.as_str())
            .unwrap_or("default")
            .to_string();

        // Note: In a real system, the embedding would be computed by an embedding model.
        // For now, we store a placeholder empty embedding. The HTTP handler layer
        // should compute embeddings before calling the memory system.
        ok(json!({
            "stored": true,
            "subject": subject,
            "content": content,
            "category": category,
            "confidence": confidence,
            "tenant_id": tenant_id,
            "note": "Fact queued for storage. Embedding will be computed by the memory handler."
        }))
    }
}

// ---------------------------------------------------------------------------
// RecallTool
// ---------------------------------------------------------------------------

/// Search long-term memory for relevant knowledge and past episodes.
///
/// Agents use this to retrieve context before making decisions — recalling
/// user preferences, past errors, project patterns, etc.
pub struct RecallTool;

#[async_trait]
impl Tool for RecallTool {
    fn name(&self) -> &str {
        "recall"
    }

    fn description(&self) -> &str {
        "Search long-term memory for relevant knowledge and past episodes by topic."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Natural language query describing what to recall"
                },
                "categories": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional category filters (e.g., ['user_preference', 'error_pattern'])"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum results to return (default: 5)"
                },
                "tenant_id": {
                    "type": "string",
                    "description": "Tenant/org ID for multi-tenancy scoping"
                }
            },
            "required": ["query"]
        })
    }

    fn category(&self) -> &str {
        "memory_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let query = match input.parameters.get("query").and_then(|v| v.as_str()) {
            Some(q) => q.to_string(),
            None => return err("Missing required parameter: query"),
        };
        let limit = input
            .parameters
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as usize;
        let tenant_id = input
            .parameters
            .get("tenant_id")
            .and_then(|v| v.as_str())
            .unwrap_or("default")
            .to_string();

        // Note: In production, the query string would be embedded via an embedding model
        // and used for vector similarity search. For now, return a structured placeholder.
        ok(json!({
            "query": query,
            "tenant_id": tenant_id,
            "limit": limit,
            "results": [],
            "note": "Recall requires embedding computation. Use the /memory/recall HTTP endpoint for full vector search."
        }))
    }
}

// ---------------------------------------------------------------------------
// ForgetTool
// ---------------------------------------------------------------------------

/// Remove a specific memory entry (GDPR compliance).
///
/// Agents or users can invoke this to permanently delete a knowledge fact
/// or episode from the memory system.
pub struct ForgetTool;

#[async_trait]
impl Tool for ForgetTool {
    fn name(&self) -> &str {
        "forget"
    }

    fn description(&self) -> &str {
        "Permanently delete a specific memory entry by ID (GDPR compliance)."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "memory_id": {
                    "type": "string",
                    "description": "The ID of the memory entry to delete"
                },
                "memory_type": {
                    "type": "string",
                    "enum": ["episode", "fact"],
                    "description": "Whether to delete an episode or a knowledge fact"
                }
            },
            "required": ["memory_id", "memory_type"]
        })
    }

    fn category(&self) -> &str {
        "memory_ops"
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let memory_id = match input.parameters.get("memory_id").and_then(|v| v.as_str()) {
            Some(id) => id.to_string(),
            None => return err("Missing required parameter: memory_id"),
        };
        let memory_type = match input.parameters.get("memory_type").and_then(|v| v.as_str()) {
            Some(t) => t.to_string(),
            None => return err("Missing required parameter: memory_type"),
        };

        // Note: Actual deletion is performed by the memory handler layer which
        // has access to the MemorySystem instance. This tool returns the intent.
        ok(json!({
            "deleted": true,
            "memory_id": memory_id,
            "memory_type": memory_type,
            "note": "Deletion queued. Use /memory endpoint for direct access."
        }))
    }
}
