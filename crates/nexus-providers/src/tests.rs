//! Tests for nexus-providers types and stub implementation.

#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use crate::types::*;
    use crate::provider::LlmProvider;
    use crate::stub_provider::StubProvider;

    // -----------------------------------------------------------------------
    // Type serialization roundtrips
    // -----------------------------------------------------------------------

    #[test]
    fn role_serde_roundtrip() {
        for role in [Role::System, Role::User, Role::Assistant, Role::Tool] {
            let json = serde_json::to_string(&role).unwrap();
            let back: Role = serde_json::from_str(&json).unwrap();
            assert_eq!(back, role);
        }
    }

    #[test]
    fn message_serde_roundtrip() {
        let msg = Message {
            role: Role::User,
            content: "Hello, world!".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(back.content, "Hello, world!");
    }

    #[test]
    fn completion_request_serde_roundtrip() {
        let req = CompletionRequest {
            model: "gpt-4o".into(),
            messages: vec![
                Message { role: Role::System, content: "You are helpful.".into() },
                Message { role: Role::User, content: "Hello".into() },
            ],
            max_tokens: Some(1000),
            temperature: Some(0.7),
            ..Default::default()
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: CompletionRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.model, "gpt-4o");
        assert_eq!(back.messages.len(), 2);
    }

    #[test]
    fn completion_response_serde_roundtrip() {
        let resp = CompletionResponse {
            content: "Hello!".into(),
            tool_calls: vec![],
            usage: TokenUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            },
            model: "gpt-4o".into(),
            finish_reason: Some("stop".into()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: CompletionResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.usage.total_tokens, 15);
    }

    #[test]
    fn tool_definition_serde_roundtrip() {
        let tool = ToolDefinition {
            name: "file_read".into(),
            description: "Read a file".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"}
                }
            }),
        };
        let json = serde_json::to_string(&tool).unwrap();
        let back: ToolDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "file_read");
    }

    #[test]
    fn tool_call_serde_roundtrip() {
        let tc = ToolCall {
            id: "call-1".into(),
            name: "file_read".into(),
            arguments: serde_json::json!({"path": "/tmp/test"}),
        };
        let json = serde_json::to_string(&tc).unwrap();
        let back: ToolCall = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "call-1");
    }

    #[test]
    fn token_usage_default() {
        let usage = TokenUsage::default();
        assert_eq!(usage.prompt_tokens, 0);
        assert_eq!(usage.completion_tokens, 0);
        assert_eq!(usage.total_tokens, 0);
    }

    #[test]
    fn model_info_serde_roundtrip() {
        let info = ModelInfo {
            id: "gpt-4o".into(),
            provider: "openai".into(),
            display_name: "GPT-4o".into(),
            context_window: 128_000,
            max_output_tokens: Some(4_096),
            cost_per_m_input: 5.0,
            cost_per_m_output: 15.0,
            supports_tools: true,
            supports_streaming: true,
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: ModelInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.context_window, 128_000);
        assert!(back.supports_tools);
    }

    #[test]
    fn stream_chunk_serde_roundtrip() {
        let chunk = StreamChunk {
            content: Some("Hello".into()),
            tool_call_delta: None,
            done: false,
        };
        let json = serde_json::to_string(&chunk).unwrap();
        let back: StreamChunk = serde_json::from_str(&json).unwrap();
        assert_eq!(back.content.as_deref(), Some("Hello"));
        assert!(!back.done);
    }

    // -----------------------------------------------------------------------
    // StubProvider tests
    // -----------------------------------------------------------------------

    #[test]
    fn stub_provider_name() {
        let provider = StubProvider::default();
        assert_eq!(provider.name(), "stub");
    }

    #[test]
    fn stub_provider_custom_name() {
        let provider = StubProvider::new("test-provider");
        assert_eq!(provider.name(), "test-provider");
    }

    #[tokio::test]
    async fn stub_provider_complete() {
        let provider = StubProvider::default();
        let req = CompletionRequest {
            model: "stub-model".into(),
            messages: vec![Message {
                role: Role::User,
                content: "Hello".into(),
            }],
            ..Default::default()
        };
        let resp = provider.complete(&req).await.unwrap();
        assert!(resp.content.contains("stub-provider"));
        assert!(resp.usage.total_tokens > 0);
        assert_eq!(resp.model, "stub-model");
        assert_eq!(resp.finish_reason.as_deref(), Some("stop"));
    }

    #[tokio::test]
    async fn stub_provider_stream() {
        let provider = StubProvider::default();
        let req = CompletionRequest {
            model: "stub-model".into(),
            messages: vec![Message {
                role: Role::User,
                content: "Hello".into(),
            }],
            stream: true,
            ..Default::default()
        };
        let (tx, mut rx) = tokio::sync::mpsc::channel(100);
        provider.stream(&req, tx).await.unwrap();

        let mut chunks = Vec::new();
        while let Ok(chunk) = rx.try_recv() {
            chunks.push(chunk);
        }
        assert!(!chunks.is_empty(), "Should have received at least one chunk");
        // Last chunk should have done=true
        assert!(chunks.last().unwrap().done);
    }

    #[tokio::test]
    async fn stub_provider_list_models() {
        let provider = StubProvider::default();
        let models = provider.list_models().await.unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "stub-model");
    }

    #[tokio::test]
    async fn stub_provider_model_info() {
        let provider = StubProvider::default();
        let info = provider.model_info("my-model").await.unwrap();
        assert!(info.is_some());
        assert_eq!(info.unwrap().id, "my-model");
    }

    #[tokio::test]
    async fn stub_provider_health_check() {
        let provider = StubProvider::default();
        let healthy = provider.health_check().await.unwrap();
        assert!(healthy);
    }
}
