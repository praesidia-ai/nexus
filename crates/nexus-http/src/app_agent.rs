//! App-as-Agent platform — automatically attaches business agents to generated apps.
//!
//! When an app is generated or started, this module:
//!  1. Detects the app's domain (e.g. "e-commerce", "CRM") from its manifest.
//!  2. Creates a `BusinessAgent` bound to that specific app.
//!  3. Provides app-aware tools (query data, update records, trigger actions).
//!  4. Exposes the app-agent pair via A2A so external agents can invoke app actions.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::RwLock;
use tracing::info;

/// The binding between a generated app and its business agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppAgentBinding {
    pub id: String,
    pub project_id: String,
    pub app_name: String,
    pub domain: String,
    pub agent_id: String,
    pub agent_name: String,
    pub tools: Vec<AppTool>,
    pub a2a_skill_id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub status: BindingStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BindingStatus {
    Active,
    Paused,
    Detached,
}

/// An app-aware tool that lets an agent interact with the running app.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppTool {
    /// Tool identifier (e.g. "query_customers", "create_invoice").
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Description for the agent.
    pub description: String,
    /// JSON Schema for the input parameters.
    pub parameters: serde_json::Value,
    /// HTTP endpoint or internal route that implements the tool.
    pub endpoint: Option<String>,
}

/// Registry of active app-agent bindings.
pub struct AppAgentRegistry {
    bindings: RwLock<HashMap<String, AppAgentBinding>>,
}

impl Default for AppAgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl AppAgentRegistry {
    pub fn new() -> Self {
        Self {
            bindings: RwLock::new(HashMap::new()),
        }
    }

    pub async fn bind(&self, binding: AppAgentBinding) {
        info!(
            "App-agent binding created: project={} agent={}",
            binding.project_id, binding.agent_name
        );
        self.bindings.write().await.insert(binding.id.clone(), binding);
    }

    pub async fn unbind(&self, binding_id: &str) -> bool {
        self.bindings.write().await.remove(binding_id).is_some()
    }

    pub async fn get(&self, binding_id: &str) -> Option<AppAgentBinding> {
        self.bindings.read().await.get(binding_id).cloned()
    }

    pub async fn by_project(&self, project_id: &str) -> Vec<AppAgentBinding> {
        self.bindings
            .read()
            .await
            .values()
            .filter(|b| b.project_id == project_id)
            .cloned()
            .collect()
    }

    pub async fn list_all(&self) -> Vec<AppAgentBinding> {
        self.bindings.read().await.values().cloned().collect()
    }
}

/// Auto-generate a set of app-aware tools based on the app's domain.
pub fn generate_app_tools(domain: &str, project_id: &str, base_url: &str) -> Vec<AppTool> {
    let base = format!("{}/projects/{}", base_url.trim_end_matches('/'), project_id);

    match domain {
        "e-commerce" | "ecommerce" => vec![
            AppTool {
                id: "list_products".into(),
                name: "List Products".into(),
                description: "List all products in the store.".into(),
                parameters: serde_json::json!({ "type": "object", "properties": { "limit": { "type": "integer" } } }),
                endpoint: Some(format!("{base}/api/products")),
            },
            AppTool {
                id: "create_order".into(),
                name: "Create Order".into(),
                description: "Place a new order.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "required": ["customer_id", "items"],
                    "properties": {
                        "customer_id": { "type": "string" },
                        "items": { "type": "array" }
                    }
                }),
                endpoint: Some(format!("{base}/api/orders")),
            },
            AppTool {
                id: "refund_order".into(),
                name: "Refund Order".into(),
                description: "Issue a refund for an order.".into(),
                parameters: serde_json::json!({ "type": "object", "required": ["order_id"], "properties": { "order_id": { "type": "string" } } }),
                endpoint: Some(format!("{base}/api/orders/refund")),
            },
        ],
        "crm" => vec![
            AppTool {
                id: "list_contacts".into(),
                name: "List Contacts".into(),
                description: "Return CRM contacts matching a filter.".into(),
                parameters: serde_json::json!({ "type": "object", "properties": { "status": { "type": "string" }, "limit": { "type": "integer" } } }),
                endpoint: Some(format!("{base}/api/contacts")),
            },
            AppTool {
                id: "create_lead".into(),
                name: "Create Lead".into(),
                description: "Add a new lead to the CRM.".into(),
                parameters: serde_json::json!({ "type": "object", "required": ["name", "email"], "properties": { "name": { "type": "string" }, "email": { "type": "string" } } }),
                endpoint: Some(format!("{base}/api/leads")),
            },
            AppTool {
                id: "update_deal_stage".into(),
                name: "Update Deal Stage".into(),
                description: "Move a deal to a new pipeline stage.".into(),
                parameters: serde_json::json!({ "type": "object", "required": ["deal_id", "stage"], "properties": { "deal_id": { "type": "string" }, "stage": { "type": "string" } } }),
                endpoint: Some(format!("{base}/api/deals")),
            },
        ],
        "finance" | "accounting" => vec![
            AppTool {
                id: "create_invoice".into(),
                name: "Create Invoice".into(),
                description: "Create and send a new invoice.".into(),
                parameters: serde_json::json!({ "type": "object", "required": ["client_id", "amount"], "properties": { "client_id": { "type": "string" }, "amount": { "type": "number" } } }),
                endpoint: Some(format!("{base}/api/invoices")),
            },
            AppTool {
                id: "get_revenue_report".into(),
                name: "Revenue Report".into(),
                description: "Generate a revenue summary for a date range.".into(),
                parameters: serde_json::json!({ "type": "object", "properties": { "start": { "type": "string" }, "end": { "type": "string" } } }),
                endpoint: Some(format!("{base}/api/reports/revenue")),
            },
        ],
        _ => vec![
            AppTool {
                id: "query_data".into(),
                name: "Query App Data".into(),
                description: "Run a read query against the app's data store.".into(),
                parameters: serde_json::json!({ "type": "object", "required": ["collection"], "properties": { "collection": { "type": "string" }, "filter": { "type": "object" }, "limit": { "type": "integer" } } }),
                endpoint: Some(format!("{base}/api/data/query")),
            },
            AppTool {
                id: "trigger_action".into(),
                name: "Trigger App Action".into(),
                description: "Invoke a named app action with parameters.".into(),
                parameters: serde_json::json!({ "type": "object", "required": ["action"], "properties": { "action": { "type": "string" }, "params": { "type": "object" } } }),
                endpoint: Some(format!("{base}/api/actions")),
            },
        ],
    }
}

/// Infer the domain of an app from its description.
pub fn infer_domain(description: &str) -> String {
    let lower = description.to_lowercase();
    if lower.contains("shop") || lower.contains("ecommerce") || lower.contains("e-commerce") || lower.contains("store") {
        "e-commerce".into()
    } else if lower.contains("crm") || lower.contains("customer") || lower.contains("lead") || lower.contains("sales") {
        "crm".into()
    } else if lower.contains("invoice") || lower.contains("accounting") || lower.contains("finance") || lower.contains("payroll") {
        "finance".into()
    } else if lower.contains("task") || lower.contains("todo") || lower.contains("project management") {
        "productivity".into()
    } else if lower.contains("blog") || lower.contains("cms") || lower.contains("content") {
        "content".into()
    } else if lower.contains("analytics") || lower.contains("dashboard") || lower.contains("metrics") {
        "analytics".into()
    } else {
        "general".into()
    }
}
