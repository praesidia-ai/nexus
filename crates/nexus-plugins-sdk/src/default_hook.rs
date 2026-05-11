//! Default no-op and logging implementations of [`PluginHook`].
//!
//! Provides a passthrough hook (continues without modification) and
//! a logging hook (logs the hook point and continues).

use async_trait::async_trait;

use super::hooks::*;

/// A no-op hook that always continues without modification.
///
/// Useful as a default or placeholder in hook registries.
pub struct PassthroughHook;

impl PassthroughHook {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PassthroughHook {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PluginHook for PassthroughHook {
    async fn execute(
        &self,
        _context: &HookContext,
    ) -> Result<HookResult, Box<dyn std::error::Error + Send + Sync>> {
        Ok(HookResult::ok())
    }
}

/// A hook that logs the hook point to a shared log buffer and continues.
///
/// Useful for debugging and testing plugin hook execution order.
pub struct LoggingHook {
    prefix: String,
    log: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl LoggingHook {
    /// Create a new logging hook with the given prefix.
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            log: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// Get a clone of the log buffer arc for inspection.
    pub fn log_buffer(&self) -> std::sync::Arc<std::sync::Mutex<Vec<String>>> {
        self.log.clone()
    }
}

#[async_trait]
impl PluginHook for LoggingHook {
    async fn execute(
        &self,
        context: &HookContext,
    ) -> Result<HookResult, Box<dyn std::error::Error + Send + Sync>> {
        let message = format!(
            "[{}] hook_point={:?} tenant={} project={:?}",
            self.prefix, context.hook_point, context.tenant_id, context.project_id,
        );

        if let Ok(mut log) = self.log.lock() {
            log.push(message.clone());
        }

        Ok(HookResult {
            action: HookAction::Continue,
            data: None,
            messages: vec![message],
        })
    }
}
