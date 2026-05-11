//! OpenTelemetry integration for Nexus.
//!
//! Provides distributed tracing and metrics via OTLP export.
//!
//! # Environment variables
//!
//!   NEXUS_OTEL_ENABLED           — set to "true" to enable (default: false)
//!   OTEL_EXPORTER_OTLP_ENDPOINT  — OTLP gRPC endpoint (default: http://localhost:4317)
//!   OTEL_SERVICE_NAME            — service name (default: nexus-agent-os)

use opentelemetry::{global, KeyValue};
use tracing::info;

/// Guard that shuts down OTel providers on drop.
pub struct OtelGuard {
    tracer_provider: Option<opentelemetry_sdk::trace::SdkTracerProvider>,
    meter_provider: Option<opentelemetry_sdk::metrics::SdkMeterProvider>,
}

impl Drop for OtelGuard {
    fn drop(&mut self) {
        if let Some(tp) = self.tracer_provider.take() {
            if let Err(e) = tp.shutdown() {
                tracing::warn!("OTel tracer shutdown error: {e}");
            }
        }
        if let Some(mp) = self.meter_provider.take() {
            if let Err(e) = mp.shutdown() {
                tracing::warn!("OTel meter shutdown error: {e}");
            }
        }
    }
}

/// Initialize OpenTelemetry. Returns None when NEXUS_OTEL_ENABLED != "true".
pub fn init_otel() -> Option<OtelGuard> {
    let enabled = std::env::var("NEXUS_OTEL_ENABLED")
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or(false);

    if !enabled {
        return None;
    }

    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4317".to_string());

    let service_name = std::env::var("OTEL_SERVICE_NAME")
        .unwrap_or_else(|_| "nexus-agent-os".to_string());

    info!(endpoint = %endpoint, service = %service_name, "Initialising OpenTelemetry");

    let resource = opentelemetry_sdk::Resource::builder_empty()
        .with_attribute(KeyValue::new(
            opentelemetry_semantic_conventions::resource::SERVICE_NAME,
            service_name.clone(),
        ))
        .with_attribute(KeyValue::new(
            opentelemetry_semantic_conventions::resource::SERVICE_VERSION,
            env!("CARGO_PKG_VERSION"),
        ))
        .build();

    let tracer_provider = build_tracer_provider(&endpoint, resource.clone());
    let meter_provider = build_meter_provider(&endpoint, resource);

    if let Some(ref tp) = tracer_provider {
        global::set_tracer_provider(tp.clone());
    }
    if let Some(ref mp) = meter_provider {
        global::set_meter_provider(mp.clone());
    }

    Some(OtelGuard { tracer_provider, meter_provider })
}

fn build_tracer_provider(
    endpoint: &str,
    resource: opentelemetry_sdk::Resource,
) -> Option<opentelemetry_sdk::trace::SdkTracerProvider> {
    use opentelemetry_otlp::WithExportConfig;

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()
        .ok()?;

    Some(
        opentelemetry_sdk::trace::SdkTracerProvider::builder()
            .with_resource(resource)
            .with_batch_exporter(exporter)
            .build(),
    )
}

fn build_meter_provider(
    endpoint: &str,
    resource: opentelemetry_sdk::Resource,
) -> Option<opentelemetry_sdk::metrics::SdkMeterProvider> {
    use opentelemetry_otlp::WithExportConfig;

    let exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()
        .ok()?;

    Some(
        opentelemetry_sdk::metrics::SdkMeterProvider::builder()
            .with_resource(resource)
            .with_periodic_exporter(exporter)
            .build(),
    )
}

// ---------------------------------------------------------------------------
// GenAI semantic convention attribute keys
// ---------------------------------------------------------------------------

pub mod attrs {
    use opentelemetry::Key;

    pub const AI_AGENT_ID: Key = Key::from_static_str("ai.agent.id");
    pub const AI_AGENT_ROLE: Key = Key::from_static_str("ai.agent.role");
    pub const AI_MODEL_NAME: Key = Key::from_static_str("ai.model.name");
    pub const AI_MODEL_PROVIDER: Key = Key::from_static_str("ai.model.provider");
    pub const AI_TOOL_CALL_NAME: Key = Key::from_static_str("ai.tool.call.name");
    pub const AI_TOKEN_INPUT: Key = Key::from_static_str("ai.token.input");
    pub const AI_TOKEN_OUTPUT: Key = Key::from_static_str("ai.token.output");
    pub const AI_TASK_ID: Key = Key::from_static_str("ai.task.id");
    pub const AI_TEAM_ID: Key = Key::from_static_str("ai.team.id");
    pub const AI_PROJECT_ID: Key = Key::from_static_str("ai.project.id");
    pub const AI_COST_USD: Key = Key::from_static_str("ai.cost.usd");
}

// ---------------------------------------------------------------------------
// Metric helpers
// ---------------------------------------------------------------------------

/// Record an LLM call in OTel metrics.
pub fn record_llm_call(
    model: &str,
    provider: &str,
    input_tokens: u64,
    output_tokens: u64,
    cost_usd: f64,
    duration_ms: u64,
) {
    let meter = global::meter("nexus.llm");
    let attrs = [
        KeyValue::new("model", model.to_string()),
        KeyValue::new("provider", provider.to_string()),
    ];

    meter
        .u64_counter("nexus.llm.tokens.total")
        .with_description("Total LLM tokens (input + output)")
        .build()
        .add(input_tokens + output_tokens, &attrs);

    meter
        .f64_counter("nexus.llm.cost.usd")
        .with_description("LLM cost in USD")
        .build()
        .add(cost_usd, &attrs);

    meter
        .u64_histogram("nexus.llm.latency.ms")
        .with_description("LLM call latency in milliseconds")
        .build()
        .record(duration_ms, &attrs);
}

/// Record an agent task in OTel metrics.
pub fn record_agent_task(agent_id: &str, role: &str, success: bool, duration_ms: u64) {
    let meter = global::meter("nexus.agent");
    let attrs = [
        KeyValue::new("agent.id", agent_id.to_string()),
        KeyValue::new("agent.role", role.to_string()),
        KeyValue::new("success", success.to_string()),
    ];

    meter
        .u64_counter("nexus.agent.tasks.total")
        .with_description("Total agent task executions")
        .build()
        .add(1, &attrs);

    meter
        .u64_histogram("nexus.agent.task.latency.ms")
        .with_description("Agent task latency in milliseconds")
        .build()
        .record(duration_ms, &attrs);
}

/// Record a team run in OTel metrics.
pub fn record_team_run(team_id: &str, protocol: &str, member_count: u64, duration_ms: u64) {
    let meter = global::meter("nexus.team");
    let attrs = [
        KeyValue::new("team.id", team_id.to_string()),
        KeyValue::new("protocol", protocol.to_string()),
    ];

    meter
        .u64_counter("nexus.team.runs.total")
        .build()
        .add(1, &attrs);

    meter
        .u64_histogram("nexus.team.run.latency.ms")
        .build()
        .record(duration_ms, &attrs);

    let _ = member_count; // available for future use
}
