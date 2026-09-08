//! Platform telemetry — what an **operator** of Oxy sees in ClickStack /
//! HyperDX: every `tracing` span and event this process emits, shipped over
//! OTLP to the cluster's OpenTelemetry collector, plus the structured log
//! format the container runtime captures from stderr.
//!
//! Not to be confused with `oxy-observability`, the **product** feature: that
//! crate's `SpanCollectorLayer` writes agent / automation spans into the
//! tenant-facing ClickHouse (`OXY_CLICKHOUSE_*`) that the in-app Traces console
//! reads. The two are different stores answering different questions, and they
//! are wired independently — a deployment can run either, both, or neither.
//!
//! One `tracing` subscriber, several sinks. `oxy-server`'s `logging` module
//! composes them; this crate owns the pieces:
//!
//! | Module          | What it provides                                                      |
//! |-----------------|-----------------------------------------------------------------------|
//! | [`otel`]        | The OTLP trace + log exporters as subscriber layers, and their shutdown |
//! | [`resource`]    | Who is emitting: `service.name` per fleet role, version, environment  |
//! | [`json_format`] | The stderr JSON line: flat fields, `trace_id` / `span_id`, current span |
//! | [`http_trace`]  | One `SERVER` span per HTTP request, named by route, W3C parent honoured |
//! | [`propagation`] | `traceparent` injection for the internal serve → ide hop               |
//!
//! ## Configuration is the OpenTelemetry environment contract
//!
//! The context layer (ids on every span and log line, `traceparent` on the
//! internal hop) is on for every server-shaped process; `OTEL_SDK_DISABLED`
//! turns it off. Nothing *leaves* until `OTEL_EXPORTER_OTLP_ENDPOINT` (or a
//! per-signal `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` / `_LOGS_ENDPOINT`) is set —
//! the SDK's own `localhost:4318` default is deliberately *not* honoured, or a
//! laptop without a collector would log an export failure every five seconds.
//! From there the standard variables apply: `OTEL_SERVICE_NAME`,
//! `OTEL_RESOURCE_ATTRIBUTES`, `OTEL_EXPORTER_OTLP_HEADERS` (HyperDX Cloud's
//! `authorization`), `OTEL_TRACES_SAMPLER` / `_ARG`, `OTEL_BSP_*`,
//! `OTEL_SDK_DISABLED`, `OTEL_TRACES_EXPORTER` (`none` switches traces off) and
//! `OTEL_LOGS_EXPORTER` (`otlp` switches logs *on* — opt-in, see `otel`). The
//! one Oxy-specific knob is
//! `OXY_OTEL_FILTER`, a `RUST_LOG`-style directive string that decides what is
//! *exported*, independent of what is *printed* (`OXY_LOG_LEVEL`).
//!
//! The full operator guide is `internal-docs/platform-telemetry.md`.

pub mod http_trace;
pub mod json_format;
pub mod otel;
pub mod propagation;
pub mod resource;
pub mod with_dispatch;

/// Framework crates whose `info`/`debug` output is wire-level chatter, not
/// something an operator acts on: HTTP frames, raw SQL, TLS handshakes. Both
/// the stderr layers and the OTLP layers append this to whatever level the
/// operator asked for, so raising `OXY_LOG_LEVEL=debug` stays readable.
/// Measured on oxy-dev at debug (2026-09-08): the OpenTelemetry SDK narrating
/// its own exports (`BatchSpanProcessor.ExportingDueToTimer`,
/// `HttpClient.ExportStarted/Succeeded`) was ~9k lines an hour and the AWS SDK
/// config/credential chain ~2.5k — none of it about oxy, all of it in the way.
/// `RUST_LOG` / `OXY_OTEL_FILTER` bypass it entirely — the expert escape hatch.
///
/// `custom_app_function` is not framework chatter but a privacy boundary: it
/// is the target of a tenant's `ctx.log()` lines from an Oxy Function, whose
/// content routinely carries application data. The platform store (stderr,
/// OTLP) gets an app's `warn`/`error` lines with their trace id and nothing
/// below; the product store keeps every line behind the app-admin gate
/// through its own filter, which this list does not touch.
pub const NOISY_CRATE_DIRECTIVES: &str = "tower_http=warn,h2=warn,hyper=warn,hyper_util=warn,\
     reqwest=warn,sqlx=warn,sea_orm=warn,tonic=warn,rustls=warn,tokio_postgres=warn,\
     tungstenite=warn,tokio_tungstenite=warn,deser_incomplete=off,\
     custom_app_function=warn,\
     opentelemetry=warn,opentelemetry_sdk=warn,opentelemetry-otlp=warn,opentelemetry-http=warn,\
     aws_config=warn,aws_runtime=warn,aws_smithy_runtime=warn,aws_smithy_runtime_api=warn,\
     aws_smithy_http=warn,aws_sdk_s3=warn,aws_sdk_sesv2=warn,tower=warn,hyper_rustls=warn";

/// `"{level},{NOISY_CRATE_DIRECTIVES}"` — the directive string both the stderr
/// and the export filters are built from when the operator gives only a level.
pub fn directives_for_level(level: &str) -> String {
    format!("{level},{NOISY_CRATE_DIRECTIVES}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::EnvFilter;

    #[test]
    fn the_noisy_list_parses_as_env_filter_directives() {
        // A typo here would make EnvFilter::new panic at boot, in every mode.
        for level in ["warn", "info", "debug", "trace"] {
            let directives = directives_for_level(level);
            EnvFilter::try_new(&directives)
                .unwrap_or_else(|e| panic!("{directives:?} did not parse: {e}"));
        }
    }
}
