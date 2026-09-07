//! W3C `traceparent` / `tracestate` on the wire.
//!
//! Inbound, the HTTP span in [`crate::http_trace`] adopts a valid
//! `traceparent` as its parent, so a browser SDK or an upstream service can
//! hand Oxy the trace it started. Outbound, the one internal hop Oxy makes on
//! its own behalf — a stateless `serve` replica forwarding an `IdeOnly` route
//! to the `ide` pod — stamps the current span's context on the forwarded
//! request, so both pods' spans land in one trace instead of two that share
//! only an `x-oxy-request-id`.
//!
//! Neither direction needs an OTLP endpoint: the `otel` layer runs on every
//! server-shaped process, so the hop is linked even on a cluster with no
//! receiver yet. Both are no-ops only without that layer (`OTEL_SDK_DISABLED`,
//! or a one-shot CLI command), where there is no span context to read and an
//! inbound header is simply left untouched for whoever is downstream.

use http::HeaderMap;
use opentelemetry::Context;
use opentelemetry::trace::TraceContextExt;
use opentelemetry_http::{HeaderExtractor, HeaderInjector};
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// The parent context carried by `traceparent` / `tracestate`, when the
/// headers hold a well-formed one. Anything malformed reads as "no parent".
pub fn extract(headers: &HeaderMap) -> Option<Context> {
    let cx = opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.extract(&HeaderExtractor(headers))
    });
    cx.span().span_context().is_valid().then_some(cx)
}

/// Write the current `tracing` span's context into `headers` as
/// `traceparent` (+ `tracestate`), replacing any inbound value. Does nothing
/// when no traced span is active — an event outside any span, or a process
/// without the `otel` layer.
pub fn inject_current(headers: &mut HeaderMap) {
    let cx = tracing::Span::current().context();
    if !cx.span().span_context().is_valid() {
        return;
    }
    opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.inject_context(&cx, &mut HeaderInjector(headers))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_sdk::propagation::TraceContextPropagator;
    use opentelemetry_sdk::trace::SdkTracerProvider;
    use tracing_subscriber::layer::SubscriberExt;

    fn install_propagator() {
        opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());
    }

    #[test]
    fn without_an_exported_span_nothing_is_injected() {
        install_propagator();
        let mut headers = HeaderMap::new();
        inject_current(&mut headers);
        assert!(headers.is_empty());
    }

    #[test]
    fn injects_the_current_span_and_extract_reads_it_back() {
        install_propagator();
        let provider = SdkTracerProvider::builder().build();
        let subscriber = tracing_subscriber::registry()
            .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")));

        let mut headers = HeaderMap::new();
        let expected = tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!("hop");
            let _g = span.enter();
            inject_current(&mut headers);
            let sc = span.context().span().span_context().clone();
            (
                format!("{:032x}", sc.trace_id()),
                format!("{:016x}", sc.span_id()),
            )
        });

        let traceparent = headers["traceparent"].to_str().unwrap().to_string();
        let parts: Vec<&str> = traceparent.split('-').collect();
        assert_eq!(parts.len(), 4, "{traceparent}");
        assert_eq!(parts[0], "00");
        assert_eq!(parts[1], expected.0);
        assert_eq!(parts[2], expected.1);

        let parent = extract(&headers).expect("a valid traceparent extracts");
        let sc = parent.span().span_context().clone();
        assert!(sc.is_remote());
        assert_eq!(format!("{:032x}", sc.trace_id()), expected.0);
    }

    #[test]
    fn a_malformed_traceparent_reads_as_no_parent() {
        install_propagator();
        let mut headers = HeaderMap::new();
        headers.insert("traceparent", "garbage".parse().unwrap());
        assert!(extract(&headers).is_none());
        assert!(extract(&HeaderMap::new()).is_none());
    }
}
