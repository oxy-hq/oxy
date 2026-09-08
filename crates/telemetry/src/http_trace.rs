//! One `SERVER` span per HTTP request, shaped the way HyperDX's request views
//! expect: named `"{method} {route}"`, attributes from the OpenTelemetry HTTP
//! semantic conventions, an inbound `traceparent` honoured as the parent,
//! and the server-minted `x-oxy-request-id` on the span so a support ticket's
//! id is one click from the trace.
//!
//! This replaces `tower_http::trace::DefaultMakeSpan`, whose span was named
//! `request` with `method` / `uri` fields — fine for a log line, invisible to
//! a tracing backend that groups by route.
//!
//! Two attributes are redacted before they leave the process, because this
//! store has a long retention and a wider reader set than the token vault:
//! `url.query` keeps its keys and replaces every value (`code=REDACTED`), and
//! `url.path` is replaced by the route pattern whenever that pattern has a
//! secret-shaped parameter (`/invitations/{token}/accept`). `http.route` and
//! the redacted forms carry all the debugging value the raw strings did.
//!
//! The route is `axum::extract::MatchedPath`, which is only present when the
//! layer is attached with `Router::layer` (axum matches first, then runs
//! per-route middleware). On the static fallback there is no route, so the
//! span is named by method alone — the semconv rule for unrouted requests —
//! and a low-cardinality dashboard is not polluted with one series per asset.

use std::time::Duration;

use axum::extract::MatchedPath;
use http::{HeaderMap, Request, Response};
use tower_http::classify::{ServerErrorsAsFailures, ServerErrorsFailureClass, SharedClassifier};
use tower_http::trace::{
    DefaultOnBodyChunk, DefaultOnEos, MakeSpan, OnFailure, OnRequest, OnResponse, TraceLayer,
};
use tracing::Span;
use tracing::field::Empty;
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// The concrete layer type, so a router can name it in a signature.
pub type OxyTraceLayer = TraceLayer<
    SharedClassifier<ServerErrorsAsFailures>,
    OxyMakeSpan,
    OxyOnRequest,
    OxyOnResponse,
    DefaultOnBodyChunk,
    DefaultOnEos,
    OxyOnFailure,
>;

/// Build the layer. `request_id_header` is the name of the header the
/// request-id middleware stamps on the request (Oxy's `x-oxy-request-id`);
/// it is passed in rather than imported so this crate stays free of
/// `oxy-shared`.
pub fn trace_layer(request_id_header: &'static str) -> OxyTraceLayer {
    TraceLayer::new_for_http()
        .make_span_with(OxyMakeSpan { request_id_header })
        .on_request(OxyOnRequest)
        .on_response(OxyOnResponse)
        .on_failure(OxyOnFailure)
}

/// `"GET /api/threads/{id}"`, or just `"GET"` when nothing was routed.
pub fn span_name(method: &str, route: Option<&str>) -> String {
    match route {
        Some(route) => format!("{method} {route}"),
        None => method.to_string(),
    }
}

/// `a=1&code=xyz&flag` → `a=REDACTED&code=REDACTED&flag`. Keys tell you what
/// the caller was doing; values are where the OAuth codes and magic-link
/// tokens live, and the semconv asks for exactly this treatment.
pub fn redacted_query(query: &str) -> String {
    query
        .split('&')
        .map(|pair| match pair.split_once('=') {
            Some((key, _)) => format!("{key}=REDACTED"),
            None => pair.to_string(),
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// Route parameters whose *value* must never be recorded.
const SECRET_PARAMS: &[&str] = &[
    "token",
    "code",
    "secret",
    "key",
    "password",
    "signature",
    "sig",
    "session",
];

/// The `url.path` to record: the raw path, unless the matched route carries a
/// secret-shaped parameter, in which case the route pattern itself — the only
/// variable part of such a path is the secret.
pub fn path_for_span(route: Option<&str>, path: &str) -> String {
    let secret_route = route.is_some_and(|r| {
        r.split('/').any(|seg| {
            seg.strip_prefix('{')
                .and_then(|s| s.strip_suffix('}'))
                .is_some_and(|name| SECRET_PARAMS.contains(&name.trim_start_matches('*')))
        })
    });
    match route {
        Some(r) if secret_route => r.to_string(),
        _ => path.to_string(),
    }
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

/// The originating client, per `X-Forwarded-For`'s first hop. Only the
/// header is consulted: behind the load balancer the socket peer is the LB.
fn client_address(headers: &HeaderMap) -> Option<String> {
    header_str(headers, "x-forwarded-for")
        .and_then(|v| v.split(',').next())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

/// Request paths that get **no span and no log line**: the kubelet's
/// readiness / liveness probes and the load balancer's health check. At the
/// chart's cadence (readiness every 5 s, liveness every 30 s, the ALB on top,
/// per pod) they are the majority of all requests a fleet serves, so tracing
/// them would fill the platform trace store — and the tenant-visible product
/// store, which sees the same span — with hundreds of thousands of identical
/// rows a day before a single user request. A probe that *fails* still logs:
/// `on_failure` runs whether or not the request had a span.
pub const PROBE_PATHS: &[&str] = &[
    "/api/health",
    "/api/ready",
    "/api/live",
    "/health",
    "/ready",
    "/live",
];

/// Whether a request path is one of [`PROBE_PATHS`] (exact match).
pub fn is_probe(path: &str) -> bool {
    PROBE_PATHS.contains(&path)
}

#[derive(Clone, Copy, Debug)]
pub struct OxyMakeSpan {
    request_id_header: &'static str,
}

impl<B> MakeSpan<B> for OxyMakeSpan {
    fn make_span(&mut self, req: &Request<B>) -> Span {
        if is_probe(req.uri().path()) {
            return Span::none();
        }
        let method = req.method().as_str();
        let route = req
            .extensions()
            .get::<MatchedPath>()
            .map(MatchedPath::as_str);
        let name = span_name(method, route);
        let headers = req.headers();
        let span = tracing::info_span!(
            "http.server.request",
            otel.name = name.as_str(),
            otel.kind = "server",
            otel.status_code = Empty,
            http.request.method = method,
            http.route = route,
            http.response.status_code = Empty,
            error.type = Empty,
            url.path = path_for_span(route, req.uri().path()).as_str(),
            url.query = req.uri().query().map(redacted_query).as_deref(),
            server.address =
                header_str(headers, "x-forwarded-host").or_else(|| header_str(headers, "host")),
            client.address = client_address(headers).as_deref(),
            user_agent.original = header_str(headers, "user-agent"),
            oxy.request_id = header_str(headers, self.request_id_header),
        );
        if let Some(parent) = crate::propagation::extract(headers) {
            // Err means no export layer is installed (or the span somehow
            // started already); in both cases there is nothing to link to.
            let _ = span.set_parent(parent);
        }
        span
    }
}

#[derive(Clone, Copy, Debug)]
pub struct OxyOnRequest;

impl<B> OnRequest<B> for OxyOnRequest {
    fn on_request(&mut self, _req: &Request<B>, span: &Span) {
        if span.is_none() {
            return; // a probe: no span, no line
        }
        tracing::debug!("request received");
    }
}

#[derive(Clone, Copy, Debug)]
pub struct OxyOnResponse;

impl<B> OnResponse<B> for OxyOnResponse {
    fn on_response(self, response: &Response<B>, latency: Duration, span: &Span) {
        if span.is_none() {
            return; // a probe: no span, no line (a failing one logs via `on_failure`)
        }
        let status = response.status();
        span.record("http.response.status_code", status.as_u16());
        let latency_ms = latency.as_millis() as u64;
        if status.is_server_error() {
            // Semconv for SERVER spans: only 5xx is an error; a 4xx is the
            // client's problem and leaves the status unset. `on_failure`
            // writes the error line for a 5xx, so nothing is logged here.
            span.record("otel.status_code", "ERROR");
        } else if status.is_client_error() {
            tracing::info!(status = status.as_u16(), latency_ms, "request");
        } else {
            // Successes are `debug`: the span is the per-request record, and an
            // `info` line per request (health probes included) is an access
            // log nobody asked for at OXY_LOG_LEVEL=info.
            tracing::debug!(status = status.as_u16(), latency_ms, "request");
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct OxyOnFailure;

impl OnFailure<ServerErrorsFailureClass> for OxyOnFailure {
    fn on_failure(&mut self, class: ServerErrorsFailureClass, latency: Duration, span: &Span) {
        span.record("otel.status_code", "ERROR");
        let latency_ms = latency.as_millis() as u64;
        match class {
            ServerErrorsFailureClass::StatusCode(code) => {
                span.record("error.type", code.as_str());
                tracing::error!(status = code.as_u16(), latency_ms, "request failed");
            }
            ServerErrorsFailureClass::Error(err) => {
                span.record("error.type", "transport");
                tracing::error!(error = %err, latency_ms, "request failed");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::http::StatusCode;
    use axum::routing::get;
    use opentelemetry::trace::{SpanId, SpanKind, Status, TraceId, TracerProvider as _};
    use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider, SpanData};
    use tower::ServiceExt;
    use tracing_subscriber::layer::SubscriberExt;

    #[test]
    fn span_names_follow_the_semconv_rule() {
        assert_eq!(
            span_name("GET", Some("/api/threads/{id}")),
            "GET /api/threads/{id}"
        );
        assert_eq!(span_name("POST", None), "POST");
    }

    #[test]
    fn query_values_are_redacted_and_keys_kept() {
        assert_eq!(
            redacted_query("code=4/0Ab&state=xyz"),
            "code=REDACTED&state=REDACTED"
        );
        assert_eq!(redacted_query("refresh"), "refresh");
        assert_eq!(redacted_query("a=1&flag&b="), "a=REDACTED&flag&b=REDACTED");
    }

    #[test]
    fn a_secret_route_parameter_hides_the_whole_path() {
        assert_eq!(
            path_for_span(
                Some("/invitations/{token}/accept"),
                "/invitations/abc123/accept"
            ),
            "/invitations/{token}/accept"
        );
        assert_eq!(path_for_span(Some("/items/{id}"), "/items/42"), "/items/42");
        assert_eq!(path_for_span(None, "/assets/app.js"), "/assets/app.js");
    }

    #[test]
    fn client_address_is_the_first_forwarded_hop() {
        let mut h = HeaderMap::new();
        assert_eq!(client_address(&h), None);
        h.insert("x-forwarded-for", " 203.0.113.9, 10.0.0.2".parse().unwrap());
        assert_eq!(client_address(&h).as_deref(), Some("203.0.113.9"));
    }

    fn app() -> Router {
        Router::new()
            .route("/items/{id}", get(|| async { "ok" }))
            .route("/boom", get(|| async { StatusCode::INTERNAL_SERVER_ERROR }))
            .route("/api/ready", get(|| async { "ready" }))
            .layer(trace_layer("x-oxy-request-id"))
    }

    #[test]
    fn probe_paths_are_exact_and_cover_both_prefix_forms() {
        for p in [
            "/api/health",
            "/api/ready",
            "/api/live",
            "/health",
            "/ready",
            "/live",
        ] {
            assert!(is_probe(p), "{p}");
        }
        assert!(!is_probe("/api/healthz"));
        assert!(!is_probe("/api/ready/"));
        assert!(!is_probe("/api/threads"));
    }

    #[tokio::test]
    async fn a_probe_request_exports_no_span_and_a_real_request_still_does() {
        let (status, spans) = spans_for(
            Request::builder()
                .uri("/api/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            spans.is_empty(),
            "a probe must not reach any store: {spans:?}"
        );

        let (status, spans) = spans_for(
            Request::builder()
                .uri("/items/7")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].name, "GET /items/{id}");
    }

    /// Run one request under an in-memory exporter and return the finished
    /// spans. The span closes when the response body is dropped, so the body
    /// is drained before reading.
    async fn spans_for(req: Request<Body>) -> (StatusCode, Vec<SpanData>) {
        opentelemetry::global::set_text_map_propagator(
            opentelemetry_sdk::propagation::TraceContextPropagator::new(),
        );
        let exporter = InMemorySpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let subscriber = tracing_subscriber::registry()
            .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")));
        let _guard = tracing::subscriber::set_default(subscriber);

        let res = app().oneshot(req).await.unwrap();
        let status = res.status();
        let _ = axum::body::to_bytes(res.into_body(), usize::MAX).await;
        provider.force_flush().unwrap();
        (status, exporter.get_finished_spans().unwrap())
    }

    fn attr(span: &SpanData, key: &str) -> Option<String> {
        span.attributes
            .iter()
            .find(|kv| kv.key.as_str() == key)
            .map(|kv| kv.value.to_string())
    }

    #[tokio::test]
    async fn names_the_span_by_route_and_adopts_the_inbound_traceparent() {
        let req = Request::builder()
            .uri("/items/42?x=1")
            .header(
                "traceparent",
                "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01",
            )
            .header("x-oxy-request-id", "11111111-2222-3333-4444-555555555555")
            .header("user-agent", "test-agent")
            .header("x-forwarded-for", "203.0.113.9")
            .header("host", "app.example.test")
            .body(Body::empty())
            .unwrap();
        let (status, spans) = spans_for(req).await;
        assert_eq!(status, StatusCode::OK);

        let span = spans
            .iter()
            .find(|s| s.name == "GET /items/{id}")
            .unwrap_or_else(|| {
                panic!(
                    "no routed span in {:?}",
                    spans.iter().map(|s| s.name.clone()).collect::<Vec<_>>()
                )
            });
        assert_eq!(span.span_kind, SpanKind::Server);
        assert_eq!(
            span.span_context.trace_id(),
            TraceId::from_hex("0af7651916cd43dd8448eb211c80319c").unwrap()
        );
        assert_eq!(
            span.parent_span_id,
            SpanId::from_hex("b7ad6b7169203331").unwrap()
        );
        assert_eq!(attr(span, "http.request.method").as_deref(), Some("GET"));
        assert_eq!(attr(span, "http.route").as_deref(), Some("/items/{id}"));
        assert_eq!(attr(span, "url.path").as_deref(), Some("/items/42"));
        assert_eq!(attr(span, "url.query").as_deref(), Some("x=REDACTED"));
        assert_eq!(
            attr(span, "http.response.status_code").as_deref(),
            Some("200")
        );
        assert_eq!(
            attr(span, "server.address").as_deref(),
            Some("app.example.test")
        );
        assert_eq!(attr(span, "client.address").as_deref(), Some("203.0.113.9"));
        assert_eq!(
            attr(span, "user_agent.original").as_deref(),
            Some("test-agent")
        );
        assert_eq!(
            attr(span, "oxy.request_id").as_deref(),
            Some("11111111-2222-3333-4444-555555555555")
        );
        assert_eq!(span.status, Status::Unset, "a 200 leaves the status unset");
    }

    #[tokio::test]
    async fn a_5xx_is_an_error_span_and_a_bare_request_is_a_root() {
        let req = Request::builder().uri("/boom").body(Body::empty()).unwrap();
        let (status, spans) = spans_for(req).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);

        let span = spans
            .iter()
            .find(|s| s.name == "GET /boom")
            .expect("routed span");
        assert!(
            matches!(span.status, Status::Error { .. }),
            "{:?}",
            span.status
        );
        assert_eq!(
            attr(span, "http.response.status_code").as_deref(),
            Some("500")
        );
        assert_eq!(attr(span, "error.type").as_deref(), Some("500"));
        assert_eq!(
            span.parent_span_id,
            SpanId::INVALID,
            "no traceparent → root span"
        );
        assert!(span.span_context.is_valid());
    }
}
