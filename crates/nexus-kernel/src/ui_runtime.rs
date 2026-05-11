use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{broadcast, RwLock};

#[derive(Debug, Error)]
pub enum WidgetError {
    #[error("widget not found: {0}")]
    NotFound(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageFormat {
    Text,
    Markdown,
    Html,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetAction {
    pub label: String,
    pub action: String,
    pub variant: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormField {
    pub name: String,
    pub label: String,
    pub field_type: String,
    pub required: bool,
    pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChartType {
    Line,
    Bar,
    Pie,
    Scatter,
    Area,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableColumn {
    pub key: String,
    pub label: String,
    pub sortable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressStep {
    pub label: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WidgetPosition {
    Main,
    Sidebar,
    Modal,
    Notification,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentWidget {
    Message {
        content: String,
        format: MessageFormat,
        actions: Vec<WidgetAction>,
    },
    Form {
        title: String,
        fields: Vec<FormField>,
        submit_label: String,
        callback_channel: String,
    },
    Chart {
        chart_type: ChartType,
        data: serde_json::Value,
        title: String,
    },
    Table {
        columns: Vec<TableColumn>,
        rows: Vec<Vec<serde_json::Value>>,
        title: String,
    },
    Progress {
        title: String,
        steps: Vec<ProgressStep>,
        current: usize,
    },
    FilePreview {
        path: String,
        content: String,
        language: Option<String>,
    },
    Diff {
        title: String,
        before: String,
        after: String,
        language: Option<String>,
    },
    Approval {
        title: String,
        description: String,
        approve_label: String,
        reject_label: String,
        callback_channel: String,
    },
    Terminal {
        title: String,
        content: String,
        streaming: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetUpdate {
    pub widget_id: String,
    pub update_type: WidgetUpdateType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WidgetUpdateType {
    Created(AgentWidget),
    Updated(AgentWidget),
    Removed,
}

pub struct WidgetManager {
    widgets: RwLock<HashMap<String, (String, AgentWidget, WidgetPosition)>>,
    event_tx: broadcast::Sender<WidgetUpdate>,
}

impl WidgetManager {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(256);
        Self {
            widgets: RwLock::new(HashMap::new()),
            event_tx: tx,
        }
    }

    pub async fn push(
        &self,
        pid: &str,
        widget: AgentWidget,
        position: WidgetPosition,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        self.widgets
            .write()
            .await
            .insert(id.clone(), (pid.to_string(), widget.clone(), position));
        let _ = self.event_tx.send(WidgetUpdate {
            widget_id: id.clone(),
            update_type: WidgetUpdateType::Created(widget),
        });
        id
    }

    pub async fn update(
        &self,
        widget_id: &str,
        widget: AgentWidget,
    ) -> Result<(), WidgetError> {
        let mut widgets = self.widgets.write().await;
        let entry = widgets
            .get_mut(widget_id)
            .ok_or_else(|| WidgetError::NotFound(widget_id.into()))?;
        entry.1 = widget.clone();
        let _ = self.event_tx.send(WidgetUpdate {
            widget_id: widget_id.to_string(),
            update_type: WidgetUpdateType::Updated(widget),
        });
        Ok(())
    }

    pub async fn remove(&self, widget_id: &str) -> Result<(), WidgetError> {
        self.widgets
            .write()
            .await
            .remove(widget_id)
            .ok_or_else(|| WidgetError::NotFound(widget_id.into()))?;
        let _ = self.event_tx.send(WidgetUpdate {
            widget_id: widget_id.to_string(),
            update_type: WidgetUpdateType::Removed,
        });
        Ok(())
    }

    pub async fn list_for_agent(
        &self,
        pid: &str,
    ) -> Vec<(String, AgentWidget, WidgetPosition)> {
        self.widgets
            .read()
            .await
            .iter()
            .filter(|(_, (agent_pid, _, _))| agent_pid == pid)
            .map(|(id, (_, w, p))| (id.clone(), w.clone(), p.clone()))
            .collect()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<WidgetUpdate> {
        self.event_tx.subscribe()
    }
}

impl Default for WidgetManager {
    fn default() -> Self {
        Self::new()
    }
}
