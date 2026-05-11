//! Meta tools — agent lifecycle signals.
//!
//! These tools do not perform external work; they communicate control signals
//! back to the agent runtime (e.g. "I am done").

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::agents::tools::{Tool, ToolInput, ToolOutput};

/// A tool that agents call to signal task completion.
///
/// When the runtime detects this tool call it stops the agent loop, extracts
/// the summary from the tool's input, and emits a `Completed` event.
pub struct TaskCompleteTool;

#[async_trait]
impl Tool for TaskCompleteTool {
    fn name(&self) -> &str {
        "task_complete"
    }

    fn description(&self) -> &str {
        "Call this tool when you have fully completed the assigned task. \
         Provide a short summary of what was accomplished."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "summary": {
                    "type": "string",
                    "description": "Brief summary of what was accomplished."
                }
            },
            "required": ["summary"]
        })
    }

    async fn execute(&self, input: ToolInput) -> ToolOutput {
        let summary = input
            .parameters
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or("Task completed.")
            .to_string();

        ToolOutput {
            result: json!({ "status": "completed", "summary": summary }),
            success: true,
            error: None,
        }
    }

    fn category(&self) -> &str {
        "meta"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn task_complete_tool_returns_summary() {
        let tool = TaskCompleteTool;
        assert_eq!(tool.name(), "task_complete");

        let output = tool
            .execute(ToolInput {
                parameters: json!({"summary": "All files updated."}),
            })
            .await;

        assert!(output.success);
        assert_eq!(
            output.result.get("summary").and_then(|v| v.as_str()),
            Some("All files updated.")
        );
    }

    #[tokio::test]
    async fn task_complete_tool_default_summary() {
        let tool = TaskCompleteTool;
        let output = tool
            .execute(ToolInput {
                parameters: json!({}),
            })
            .await;

        assert!(output.success);
        assert_eq!(
            output.result.get("summary").and_then(|v| v.as_str()),
            Some("Task completed.")
        );
    }
}
