use criterion::{criterion_group, criterion_main, Criterion};
use nexus_mcp::types::{McpRequest, McpResponse, McpToolDefinition};
use std::collections::HashMap;

fn build_tool_registry(n: usize) -> HashMap<String, McpToolDefinition> {
    let mut registry = HashMap::new();
    for i in 0..n {
        let tool = McpToolDefinition {
            name: format!("tool_{i}"),
            description: format!("Benchmark tool number {i}"),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "input": { "type": "string" }
                },
                "required": ["input"]
            }),
        };
        registry.insert(tool.name.clone(), tool);
    }
    registry
}

fn bench_tool_registry_lookup(c: &mut Criterion) {
    let registry = build_tool_registry(500);

    c.bench_function("tool/registry_lookup", |b| {
        b.iter(|| {
            let tool = registry.get("tool_250");
            assert!(tool.is_some());
        });
    });
}

fn bench_json_rpc_parse(c: &mut Criterion) {
    let raw = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 42,
        "method": "tasks/send",
        "params": {
            "id": "task-abc-123",
            "message": {
                "role": "user",
                "parts": [{"type": "text", "text": "Build me a login page"}]
            }
        }
    })
    .to_string();

    c.bench_function("tool/json_rpc_parse", |b| {
        b.iter(|| {
            let req: McpRequest = serde_json::from_str(&raw).unwrap();
            assert_eq!(req.method, "tasks/send");
        });
    });
}

fn bench_json_rpc_serialize(c: &mut Criterion) {
    let resp = McpResponse::success(
        serde_json::json!(42),
        serde_json::json!({
            "id": "task-abc-123",
            "status": { "state": "completed" },
            "artifacts": [{
                "parts": [{"type": "text", "text": "Login page generated successfully"}],
                "index": 0
            }]
        }),
    );

    c.bench_function("tool/json_rpc_serialize", |b| {
        b.iter(|| {
            let json = serde_json::to_string(&resp).unwrap();
            assert!(!json.is_empty());
        });
    });
}

fn bench_tool_schema_validation(c: &mut Criterion) {
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "path": { "type": "string" },
            "content": { "type": "string" },
            "overwrite": { "type": "boolean" }
        },
        "required": ["path", "content"]
    });

    let valid_args = serde_json::json!({
        "path": "/tmp/test.txt",
        "content": "Hello, world!",
        "overwrite": true
    });

    c.bench_function("tool/schema_validation", |b| {
        b.iter(|| {
            let props = schema["properties"].as_object().unwrap();
            let required = schema["required"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>();

            let args = valid_args.as_object().unwrap();

            for req_field in &required {
                assert!(
                    args.contains_key(*req_field),
                    "Missing required field: {req_field}"
                );
            }

            for (key, value) in args {
                if let Some(prop_schema) = props.get(key) {
                    let expected_type = prop_schema["type"].as_str().unwrap_or("any");
                    let type_ok = match expected_type {
                        "string" => value.is_string(),
                        "number" => value.is_number(),
                        "boolean" => value.is_boolean(),
                        "object" => value.is_object(),
                        "array" => value.is_array(),
                        _ => true,
                    };
                    assert!(type_ok, "Type mismatch for field {key}");
                }
            }
        });
    });
}

criterion_group!(
    benches,
    bench_tool_registry_lookup,
    bench_json_rpc_parse,
    bench_json_rpc_serialize,
    bench_tool_schema_validation,
);
criterion_main!(benches);
