use opentelemetry::{KeyValue, global, trace::TracerProvider as _};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    Resource, runtime,
    trace::{RandomIdGenerator, Sampler, SpanLimits, TracerProvider},
};
use std::time::Duration;
use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt, util::SubscriberInitExt};

const DEFAULT_LOG_LEVEL: &str = "warn";
const DEFAULT_OTLP_LOG_LEVEL: &str = "debug";

/// Build an EnvFilter for console output, falling back to DEFAULT_LOG_LEVEL if OXY_LOG_LEVEL is invalid
fn build_env_filter() -> EnvFilter {
    // First try RUST_LOG (standard env var)
    if let Ok(filter) = EnvFilter::try_from_default_env() {
        return filter.add_directive("deser_incomplete=off".parse().unwrap());
    }

    // Then try OXY_LOG_LEVEL with validation
    let level = std::env::var("OXY_LOG_LEVEL").unwrap_or_else(|_| DEFAULT_LOG_LEVEL.to_string());

    match EnvFilter::try_new(&level) {
        Ok(filter) => filter.add_directive("deser_incomplete=off".parse().unwrap()),
        Err(_) => {
            eprintln!(
                "Warning: Invalid OXY_LOG_LEVEL='{}', falling back to '{}'",
                level, DEFAULT_LOG_LEVEL
            );
            EnvFilter::try_new(DEFAULT_LOG_LEVEL)
                .unwrap()
                .add_directive("deser_incomplete=off".parse().unwrap())
        }
    }
}

/// Build an EnvFilter for OTLP export (traces to ClickHouse, Jaeger, etc.)
/// Uses OXY_OTLP_LOG_LEVEL env var, defaults to "debug" to capture all traces
fn build_otlp_filter() -> EnvFilter {
    let level =
        std::env::var("OXY_OTLP_LOG_LEVEL").unwrap_or_else(|_| DEFAULT_OTLP_LOG_LEVEL.to_string());

    match EnvFilter::try_new(&level) {
        Ok(filter) => filter.add_directive("deser_incomplete=off".parse().unwrap()),
        Err(_) => {
            eprintln!(
                "Warning: Invalid OXY_OTLP_LOG_LEVEL='{}', falling back to '{}'",
                level, DEFAULT_OTLP_LOG_LEVEL
            );
            EnvFilter::try_new(DEFAULT_OTLP_LOG_LEVEL)
                .unwrap()
                .add_directive("deser_incomplete=off".parse().unwrap())
        }
    }
}

/// Initialize OTLP exporter (for Jaeger, ClickHouse, etc.)
///
/// # Example
/// ```rust,ignore
/// use oxy::observability::telemetry;
///
/// telemetry::init_otlp("http://localhost:4317")?;
/// // ... your app ...
/// telemetry::shutdown();
/// ```
pub fn init_otlp(endpoint: &str) -> Result<(), opentelemetry::trace::TraceError> {
    let service_name = std::env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| "oxy".to_string());

    let sampling_ratio: f64 = std::env::var("OTEL_SAMPLING_RATIO")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1.0);

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .with_timeout(Duration::from_secs(10))
        .build()?;

    let resource = Resource::new(vec![
        KeyValue::new("service.name", service_name.clone()),
        KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
    ]);

    // Increase span limits to handle many events (default is 128)
    let span_limits = SpanLimits {
        max_events_per_span: 1024,
        max_attributes_per_span: 256,
        ..Default::default()
    };

    let provider = TracerProvider::builder()
        .with_batch_exporter(exporter, runtime::Tokio)
        .with_resource(resource)
        .with_id_generator(RandomIdGenerator::default())
        .with_sampler(Sampler::TraceIdRatioBased(sampling_ratio))
        .with_span_limits(span_limits)
        .build();

    global::set_tracer_provider(provider.clone());

    // Leak the service name to get a 'static lifetime (required by tracer)
    let service_name_static: &'static str = Box::leak(service_name.clone().into_boxed_str());
    let telemetry_layer = tracing_opentelemetry::layer()
        .with_tracer(provider.tracer(service_name_static))
        // Automatically set span status to ERROR when error-level events occur
        .with_error_records_to_exceptions(true)
        // Propagate exception details to span attributes
        .with_error_fields_to_exceptions(true);

    // Apply separate filters per layer:
    // - Console output respects OXY_LOG_LEVEL (default: warn)
    // - OTLP export respects OXY_OTLP_LOG_LEVEL (default: debug) to capture all traces
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_filter(build_env_filter()))
        .with(telemetry_layer.with_filter(build_otlp_filter()))
        .init();

    tracing::debug!("OTLP tracing initialized: {}", endpoint);
    Ok(())
}

/// Initialize OpenTelemetry with default OTLP endpoint from environment.
///
/// Uses `OTEL_EXPORTER_OTLP_ENDPOINT` env var, defaults to `http://localhost:4317`.
pub fn init_telemetry() -> Result<(), opentelemetry::trace::TraceError> {
    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4317".to_string());
    init_otlp(&endpoint)
}

/// Initialize stdout logging only (no OTLP export)
pub fn init_stdout() {
    tracing_subscriber::registry()
        .with(build_env_filter())
        .with(tracing_subscriber::fmt::layer())
        .init();
}

/// Shutdown OpenTelemetry gracefully
pub fn shutdown() {
    global::shutdown_tracer_provider();
}
