//! Explicit, ordered LLM provider registry — ADR-003 §5.
//!
//! `nexus-http`'s `LlmClient` consumes this registry; adding a new provider
//! is a 50-line PR + one line in `register_builtins`. The registry holds
//! `Arc<dyn LlmProvider>` so it can be cloned cheaply into any handler.

use std::collections::HashMap;
use std::sync::Arc;

use super::provider::LlmProvider;

/// Stable identifier for a provider. Lower-case, ASCII, no spaces.
pub type ProviderId = String;

/// In-memory registry of available LLM providers.
#[derive(Default, Clone)]
pub struct ProviderRegistry {
    inner: HashMap<ProviderId, Arc<dyn LlmProvider>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new provider. Last write wins on duplicate id.
    pub fn register(&mut self, id: impl Into<ProviderId>, provider: Arc<dyn LlmProvider>) {
        self.inner.insert(id.into(), provider);
    }

    /// Look up a provider by id.
    pub fn get(&self, id: &str) -> Option<Arc<dyn LlmProvider>> {
        self.inner.get(id).cloned()
    }

    /// Iterate over all registered providers.
    pub fn iter(&self) -> impl Iterator<Item = (&ProviderId, &Arc<dyn LlmProvider>)> {
        self.inner.iter()
    }

    /// True if no providers are registered. The dispatcher should treat this
    /// as a misconfiguration and refuse calls (rather than silently picking
    /// the stub).
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }
}

/// Keys + base URLs that callers (typically `AppState::init`) thread through
/// to populate the provider registry. Per the project memory rule, API keys
/// come from the user's saved Settings — NOT from process env at boot — so
/// the registry is only populated once the operator has configured them.
#[derive(Default, Debug, Clone)]
pub struct BuiltinProviderConfig {
    pub openai_api_key: Option<String>,
    pub openai_base_url: Option<String>,
    pub anthropic_api_key: Option<String>,
    pub ollama_base_url: Option<String>,
}

/// Register the built-in providers defined inside `nexus-providers`.
///
/// Per ADR-003 §6: concrete `LlmProvider` impls for OpenAI, Anthropic, and
/// Ollama live in this crate. Adding another provider is a 50-line file +
/// one line in this function.
///
/// The function is idempotent: calling it twice rebinds the same provider
/// ids, so a settings reload simply replaces the providers in place without
/// dropping registered hooks.
pub fn register_builtins(reg: &mut ProviderRegistry, cfg: &BuiltinProviderConfig) {
    if let Some(key) = cfg
        .openai_api_key
        .as_ref()
        .filter(|s| !s.is_empty())
    {
        let mut config = crate::OpenAiConfig::new("openai", key);
        if let Some(base) = &cfg.openai_base_url {
            config = config.with_base_url(base);
        }
        reg.register(
            "openai",
            std::sync::Arc::new(crate::OpenAiProvider::new(config)),
        );
    }

    if let Some(key) = cfg
        .anthropic_api_key
        .as_ref()
        .filter(|s| !s.is_empty())
    {
        reg.register(
            "anthropic",
            std::sync::Arc::new(crate::AnthropicProvider::new(crate::AnthropicConfig::new(
                key,
            ))),
        );
    }

    // Ollama is local-first — no API key needed. Always registered, but only
    // becomes useful when an Ollama daemon is reachable at the configured URL.
    let mut ollama_cfg = crate::OllamaConfig::default();
    if let Some(base) = &cfg.ollama_base_url {
        ollama_cfg.base_url = base.clone();
    }
    reg.register("ollama", std::sync::Arc::new(crate::OllamaProvider::new(ollama_cfg)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stub_provider::StubProvider;

    #[test]
    fn register_and_lookup() {
        let mut reg = ProviderRegistry::new();
        reg.register("stub", Arc::new(StubProvider::default()));
        assert_eq!(reg.len(), 1);
        assert!(reg.get("stub").is_some());
        assert!(reg.get("missing").is_none());
    }

    #[test]
    fn empty_registry_is_empty() {
        let reg = ProviderRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn builtins_register_with_keys() {
        let mut reg = ProviderRegistry::new();
        let cfg = BuiltinProviderConfig {
            openai_api_key: Some("sk-x".into()),
            anthropic_api_key: Some("ak-y".into()),
            ..Default::default()
        };
        register_builtins(&mut reg, &cfg);
        // openai + anthropic + ollama (always registered) = 3.
        assert_eq!(reg.len(), 3);
        assert!(reg.get("openai").is_some());
        assert!(reg.get("anthropic").is_some());
        assert!(reg.get("ollama").is_some());
    }

    #[test]
    fn builtins_skip_providers_without_keys() {
        let mut reg = ProviderRegistry::new();
        let cfg = BuiltinProviderConfig::default();
        register_builtins(&mut reg, &cfg);
        // Only ollama (no key needed).
        assert_eq!(reg.len(), 1);
        assert!(reg.get("ollama").is_some());
        assert!(reg.get("openai").is_none());
        assert!(reg.get("anthropic").is_none());
    }
}
