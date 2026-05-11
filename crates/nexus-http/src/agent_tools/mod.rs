//! Agent tool ecosystem — comprehensive toolkit for AI agents.
//!
//! This module provides concrete [`Tool`] implementations organized by category:
//!
//! - **filesystem** — read, write, edit, list, search, and inspect files
//! - **code_execution** — shell commands, npm scripts, test runners, build tools
//! - **web** — HTTP requests and URL content fetching
//! - **data** — JSON/CSV parsing and math evaluation
//! - **nexus_native** — Nexus-specific tools (taste scoring, runtime management)
//! - **llm** — LLM completion delegation
//! - **safety** — pre-execution validation layer
//!
//! The [`legacy`] sub-module preserves the original monolithic `ToolRegistry`
//! (project-dir-scoped, string-based dispatch) for backward compatibility.

pub mod ai;
pub mod app_specific;
pub mod browser;
pub mod code_analysis;
pub mod code_execution;
pub mod communication;
pub mod data;
pub mod database;
pub mod dependency_checker;
pub mod devops;
pub mod documents;
pub mod filesystem;
pub mod legacy;
pub mod llm;
pub mod media;
pub mod memory_tools;
pub mod meta;
pub mod nexus_native;
pub mod safety;
pub mod web;

use crate::agents::tools::ToolRegistry as TraitToolRegistry;
use std::sync::Arc;

// Re-export legacy types so existing code (`crate::agent_tools::ToolRegistry`, etc.) still compiles.
pub use legacy::{ToolCall, ToolDef, ToolRegistry as LegacyToolRegistry, ToolResult};
// Backwards-compatible alias used by agent_loop, coding_agents, llm_client.
#[doc(hidden)]
pub use legacy::ToolRegistry;

/// Register all default tools into the trait-based [`ToolRegistry`].
///
/// Call this once at startup to populate the registry with every built-in tool.
pub fn register_default_tools(registry: &mut TraitToolRegistry) {
    // File system
    registry.register(Arc::new(filesystem::ReadFileTool));
    registry.register(Arc::new(filesystem::WriteFileTool));
    registry.register(Arc::new(filesystem::EditFileTool));
    registry.register(Arc::new(filesystem::ListDirectoryTool));
    registry.register(Arc::new(filesystem::SearchFilesTool));
    registry.register(Arc::new(filesystem::FileInfoTool));

    // Code execution
    registry.register(Arc::new(code_execution::ShellCommandTool));
    registry.register(Arc::new(code_execution::NpmInstallTool));
    registry.register(Arc::new(code_execution::NpmRunTool));
    registry.register(Arc::new(code_execution::RunTestsTool));
    registry.register(Arc::new(code_execution::BuildProjectTool));

    // Web
    registry.register(Arc::new(web::HttpRequestTool));
    registry.register(Arc::new(web::WebFetchTool));

    // Data
    registry.register(Arc::new(data::JsonParseTool));
    registry.register(Arc::new(data::CsvParseTool));
    registry.register(Arc::new(data::CalculateTool));

    // Nexus native
    registry.register(Arc::new(nexus_native::ScoreTasteTool));
    registry.register(Arc::new(nexus_native::ManageRuntimeTool));

    // LLM
    registry.register(Arc::new(llm::LlmCompletionTool));

    // Memory
    registry.register(Arc::new(memory_tools::RememberTool));
    registry.register(Arc::new(memory_tools::RecallTool));
    registry.register(Arc::new(memory_tools::ForgetTool));

    // Meta
    registry.register(Arc::new(meta::TaskCompleteTool));

    // Browser
    registry.register(Arc::new(browser::WebSearchTool));
    registry.register(Arc::new(browser::BrowserFetchTool));
    registry.register(Arc::new(browser::ScreenshotTool));
    registry.register(Arc::new(browser::BrowserExtractTool));
    registry.register(Arc::new(browser::BrowserJavaScriptTool));

    // Database
    registry.register(Arc::new(database::SqlQueryTool));
    registry.register(Arc::new(database::DbSchemaTool));

    // Code analysis
    registry.register(Arc::new(code_analysis::SecurityScanTool));
    registry.register(Arc::new(code_analysis::ComplexityAnalysisTool));
    registry.register(Arc::new(code_analysis::LintFixTool));
    registry.register(Arc::new(code_analysis::DependencyAuditTool));

    // Media
    registry.register(Arc::new(media::ImageGenerateTool));
    registry.register(Arc::new(media::PlaceholderImageTool));
    registry.register(Arc::new(media::SvgIconTool));

    // Communication
    registry.register(Arc::new(communication::WebhookTool));
    registry.register(Arc::new(communication::GitHubIssueTool));
    registry.register(Arc::new(communication::GitHubPrTool));

    // Documents
    registry.register(Arc::new(documents::DataTransformTool));
    registry.register(Arc::new(documents::SpreadsheetTool));

    // DevOps
    registry.register(Arc::new(devops::DockerBuildTool));
    registry.register(Arc::new(devops::DockerRunTool));
    registry.register(Arc::new(devops::SystemInfoTool));
    registry.register(Arc::new(devops::EnvManageTool));
    registry.register(Arc::new(devops::DockerfileGenerateTool));

    // AI
    registry.register(Arc::new(ai::LlmSubtaskTool));
    registry.register(Arc::new(ai::EmbeddingTool));
    registry.register(Arc::new(ai::ClassifyTextTool));
    registry.register(Arc::new(ai::SummarizeTool));
    registry.register(Arc::new(ai::TranslateTool));

    // App-specific
    registry.register(Arc::new(app_specific::AppDatabaseQueryTool));
    registry.register(Arc::new(app_specific::AppApiCallTool));
    registry.register(Arc::new(app_specific::AppContentEditTool));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_default_tools_populates_registry() {
        let mut registry = TraitToolRegistry::new();
        register_default_tools(&mut registry);

        let names = registry.list_names();
        assert!(
            names.len() >= 50,
            "Expected at least 50 tools, got {}",
            names.len()
        );

        // Spot-check a few names
        assert!(registry.get("read_file").is_some());
        assert!(registry.get("shell_command").is_some());
        assert!(registry.get("http_request").is_some());
        assert!(registry.get("json_parse").is_some());
        assert!(registry.get("llm_completion").is_some());
        assert!(registry.get("score_taste").is_some());
    }

    #[test]
    fn tools_grouped_by_category() {
        let mut registry = TraitToolRegistry::new();
        register_default_tools(&mut registry);

        let categories = registry.by_category();
        assert!(categories.contains_key("file_ops"));
        assert!(categories.contains_key("build_ops"));
        assert!(categories.contains_key("external_ops"));
        assert!(categories.contains_key("data_ops"));
        assert!(categories.contains_key("project_ops"));
        assert!(categories.contains_key("llm_ops"));
    }
}
