use crate::action::{ActionResult, BrowserAction, PageState};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserSession {
    pub id: String,
    pub status: SessionStatus,
    pub current_url: Option<String>,
    pub action_history: Vec<ActionResult>,
    pub screenshots: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Active,
    Idle,
    Closed,
    Error,
}

impl BrowserSession {
    pub fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            status: SessionStatus::Active,
            current_url: None,
            action_history: Vec::new(),
            screenshots: Vec::new(),
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    pub fn record_action(&mut self, result: ActionResult) {
        if let Some(ref screenshot) = result.screenshot {
            self.screenshots.push(screenshot.clone());
        }
        self.action_history.push(result);
    }

    pub fn action_count(&self) -> usize {
        self.action_history.len()
    }

    pub fn success_rate(&self) -> f64 {
        if self.action_history.is_empty() {
            return 0.0;
        }
        let successes = self.action_history.iter().filter(|a| a.success).count();
        successes as f64 / self.action_history.len() as f64
    }
}

impl Default for BrowserSession {
    fn default() -> Self {
        Self::new()
    }
}

/// Browser automation driver trait — implementations can use Playwright, CDP, etc.
#[async_trait::async_trait]
pub trait BrowserDriver: Send + Sync {
    async fn execute(&self, action: BrowserAction) -> ActionResult;
    async fn get_page_state(&self) -> Result<PageState, crate::error::BrowserError>;
    async fn close(&self) -> Result<(), crate::error::BrowserError>;
}
