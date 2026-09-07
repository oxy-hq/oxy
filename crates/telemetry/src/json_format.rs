//! The stderr line a container runtime captures, shaped for a log store that
//! parses JSON: one flat object per event.
//!
//! `tracing-subscriber`'s stock JSON formatter nests the event's fields under
//! `"fields": {…}` and the enclosing span under `"span": {…}` with no trace
//! id anywhere. HyperDX indexes what it can see at the top level, so a search
//! for `user_id:42` or a click from a trace to "its logs" both need the shape
//! below instead:
//!
//! ```json
//! {"timestamp":"2026-09-07T10:15:42.123456Z","level":"INFO","message":"request",
//!  "target":"oxy_telemetry::http_trace","trace_id":"4bf9…","span_id":"00f0…",
//!  "service":"oxy-serve","span.name":"http.server.request","span.http.route":"/api/threads/{id}",
//!  "status":200,"latency_ms":12}
//! ```
//!
//! - Event fields are **flattened** onto the object; `message` is the event's
//!   text. A field that collides with a reserved key loses to the reserved key.
//! - `trace_id` / `span_id` are the W3C ids of the span the event fired in —
//!   its explicit `parent:` when it has one, else the span entered on the
//!   thread — read from the `otel` layer's per-span state. Present on every
//!   server-shaped process, endpoint or not (the layer runs without an
//!   exporter); absent only for an event outside any span, or when
//!   `OTEL_SDK_DISABLED` is set.
//! - `span.name` and `span.<field>` describe the innermost enclosing span only,
//!   not the whole ancestry: with `#[instrument]` on every hop the full list
//!   repeated the SQL text on each nested line, which is how the previous
//!   format got its `.compact()` mode. Flat, dotted keys rather than a nested
//!   object because the cluster's collector parses the line into a
//!   `Map(String, String)` of attributes: a nested object would arrive as one
//!   JSON string, and `span.http.route` would not be a facet.
//! - `file` / `line` are added for `warn` and above, where the site matters
//!   and the volume does not.

use std::fmt;

use opentelemetry::trace::TraceContextExt;
use serde_json::{Map, Value};
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::fmt::format::{JsonFields, Writer};
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields, FormattedFields, MakeWriter};
use tracing_subscriber::registry::LookupSpan;

use crate::with_dispatch::{DispatchHandle, WithDispatch};

/// The event formatter. [`layer`] is the only way to build it: the trace-id
/// lookup needs the [`DispatchHandle`] that function hands back to be bound
/// after the subscriber is installed, and a formatter minted any other way
/// would silently fall back to the thread context — the exact failure the
/// dispatch capture exists to prevent.
#[derive(Debug, Clone)]
pub struct OxyJson {
    /// Stamped on every line as `"service"` — the derived `service.name`, so a
    /// `kubectl logs` capture is attributable without the collector's
    /// resource processor.
    service: Option<String>,
    dispatch: DispatchHandle,
}

/// The stderr JSON layer: [`OxyJson`] over `JsonFields`, writing to `writer`,
/// wrapped so the formatter can resolve trace ids through the subscriber.
/// Apply `.with_filter(..)` to the layer as with any layer, and once the
/// subscriber is installed **bind the returned handle**:
/// `tracing::dispatcher::get_default(|d| handle.bind(d))`. The wrapper also
/// binds itself from `on_register_dispatch`, but a `Vec` of layers does not
/// forward that hook (see `with_dispatch`), and the binary uses one.
pub fn layer<S, W>(
    service: Option<String>,
    writer: W,
) -> (
    WithDispatch<tracing_subscriber::fmt::Layer<S, JsonFields, OxyJson, W>>,
    DispatchHandle,
)
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    W: for<'w> MakeWriter<'w> + 'static,
{
    let dispatch = DispatchHandle::new();
    let format = OxyJson {
        service,
        dispatch: dispatch.clone(),
    };
    let inner = tracing_subscriber::fmt::layer()
        .event_format(format)
        .fmt_fields(JsonFields::new())
        .with_writer(writer);
    (WithDispatch::new(inner, dispatch.clone()), dispatch)
}

/// Collects an event's fields into a JSON map, pulling `message` out.
#[derive(Default)]
struct FieldCollector {
    message: Option<String>,
    fields: Map<String, Value>,
}

impl FieldCollector {
    fn put(&mut self, field: &Field, value: Value) {
        if field.name() == "message" {
            self.message = Some(match value {
                Value::String(s) => s,
                other => other.to_string(),
            });
        } else {
            self.fields.insert(field.name().to_string(), value);
        }
    }
}

impl Visit for FieldCollector {
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.put(field, serde_json::json!(value));
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.put(field, Value::from(value));
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.put(field, Value::from(value));
    }
    fn record_i128(&mut self, field: &Field, value: i128) {
        self.put(field, Value::String(value.to_string()));
    }
    fn record_u128(&mut self, field: &Field, value: u128) {
        self.put(field, Value::String(value.to_string()));
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.put(field, Value::Bool(value));
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        self.put(field, Value::String(value.to_string()));
    }
    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.put(field, Value::String(value.to_string()));
    }
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.put(field, Value::String(format!("{value:?}")));
    }
}

/// The W3C ids this line belongs to, as the lowercase hex a `traceparent`
/// carries. Resolved from the event's parent span through
/// `tracing-opentelemetry`'s per-span state — so `event!(parent: &span, …)`
/// reports *that* span, consistent with `span.name` — and only for a span the
/// `otel` layer never saw (filtered below its level) does it fall back to the
/// context the layer attached to the thread, i.e. the nearest traced ancestor.
/// `None` outside any span, or without the layer.
fn trace_ids_for<S, N>(
    dispatch: &DispatchHandle,
    ctx: &FmtContext<'_, S, N>,
) -> Option<(String, String)>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'w> FormatFields<'w> + 'static,
{
    // Not `tracing::dispatcher::get_default`: inside an event callback the
    // recursion guard would hand back the no-op dispatcher.
    let from_span = dispatch.get().and_then(|dispatch| {
        ctx.event_scope()
            .and_then(|mut scope| scope.next())
            .and_then(|span| tracing_opentelemetry::get_otel_context(&span.id(), &dispatch))
    });
    let cx = from_span.unwrap_or_else(opentelemetry::Context::current);
    let span = cx.span();
    let sc = span.span_context();
    sc.is_valid().then(|| {
        (
            format!("{:032x}", sc.trace_id()),
            format!("{:016x}", sc.span_id()),
        )
    })
}

/// The event's parent span — explicit (`event!(parent: &span, …)`) or the
/// one entered on the thread — written into `line` as `span.name` plus
/// `span.<field>` for each of its fields.
fn insert_span_fields<S, N>(ctx: &FmtContext<'_, S, N>, line: &mut Map<String, Value>)
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'w> FormatFields<'w> + 'static,
{
    let Some(span) = ctx.event_scope().and_then(|mut scope| scope.next()) else {
        return;
    };
    line.insert("span.name".into(), Value::String(span.name().to_string()));
    let extensions = span.extensions();
    if let Some(formatted) = extensions.get::<FormattedFields<N>>()
        && !formatted.fields.is_empty()
    {
        // With `JsonFields` this is a JSON object; with any other field
        // formatter it is `k=v` text, which is kept verbatim under `span.fields`.
        match serde_json::from_str::<Map<String, Value>>(&formatted.fields) {
            Ok(fields) => {
                for (key, value) in fields {
                    line.insert(format!("span.{key}"), value);
                }
            }
            Err(_) => {
                line.insert(
                    "span.fields".into(),
                    Value::String(formatted.fields.clone()),
                );
            }
        }
    }
}

impl<S, N> FormatEvent<S, N> for OxyJson
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'w> FormatFields<'w> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let meta = event.metadata();
        let mut collected = FieldCollector::default();
        event.record(&mut collected);

        // Event fields first, reserved keys after: a field named `level`
        // cannot masquerade as the severity.
        let mut line = collected.fields;
        line.insert(
            "timestamp".into(),
            Value::String(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true)),
        );
        line.insert(
            "level".into(),
            Value::String(meta.level().as_str().to_string()),
        );
        line.insert(
            "message".into(),
            Value::String(collected.message.unwrap_or_else(|| meta.name().to_string())),
        );
        line.insert("target".into(), Value::String(meta.target().to_string()));
        if let Some((trace_id, span_id)) = trace_ids_for(&self.dispatch, ctx) {
            line.insert("trace_id".into(), Value::String(trace_id));
            line.insert("span_id".into(), Value::String(span_id));
        }
        if let Some(service) = &self.service {
            line.insert("service".into(), Value::String(service.clone()));
        }
        insert_span_fields(ctx, &mut line);
        if *meta.level() <= Level::WARN {
            if let Some(file) = meta.file() {
                line.insert("file".into(), Value::String(file.to_string()));
            }
            if let Some(no) = meta.line() {
                line.insert("line".into(), Value::from(no));
            }
        }

        let json = serde_json::to_string(&Value::Object(line)).map_err(|_| fmt::Error)?;
        writer.write_str(&json)?;
        writer.write_char('\n')
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::layer::SubscriberExt;

    #[derive(Clone, Default)]
    struct SharedBuf(Arc<Mutex<Vec<u8>>>);

    impl io::Write for SharedBuf {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for SharedBuf {
        type Writer = SharedBuf;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    impl SharedBuf {
        fn lines(&self) -> Vec<Value> {
            let raw = String::from_utf8(self.0.lock().unwrap().clone()).unwrap();
            raw.lines()
                .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("{l}: {e}")))
                .collect()
        }
    }

    fn fmt_layer<S>(buf: SharedBuf, service: Option<&str>) -> impl tracing_subscriber::Layer<S>
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        // Direct composition: the `on_register_dispatch` path binds the handle.
        layer(service.map(str::to_string), buf).0
    }

    #[test]
    fn flattens_fields_and_reports_the_current_span() {
        let buf = SharedBuf::default();
        let subscriber =
            tracing_subscriber::registry().with(fmt_layer(buf.clone(), Some("oxy-test")));
        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!("http.server.request", http.route = "/api/x/{id}");
            let _g = span.enter();
            tracing::info!(count = 3, ok = true, who = "me", "hello world");
        });

        let lines = buf.lines();
        assert_eq!(lines.len(), 1, "{lines:?}");
        let line = &lines[0];
        assert_eq!(line["level"], "INFO");
        assert_eq!(line["message"], "hello world");
        assert_eq!(line["count"], 3);
        assert_eq!(line["ok"], true);
        assert_eq!(line["who"], "me");
        assert_eq!(line["service"], "oxy-test");
        assert_eq!(line["span.name"], "http.server.request");
        assert_eq!(line["span.http.route"], "/api/x/{id}");
        assert!(line.get("span").is_none(), "span is flattened, not nested");
        assert!(
            line.get("fields").is_none(),
            "fields must be flattened, not nested"
        );
        assert!(
            line.get("trace_id").is_none(),
            "no otel layer → no trace id"
        );
        assert!(
            line.get("file").is_none(),
            "info lines carry no source location"
        );
        assert!(line["timestamp"].as_str().unwrap().ends_with('Z'));
    }

    #[test]
    fn an_explicit_parent_wins_over_the_entered_span() {
        let buf = SharedBuf::default();
        let subscriber = tracing_subscriber::registry().with(fmt_layer(buf.clone(), None));
        tracing::subscriber::with_default(subscriber, || {
            let entered = tracing::info_span!("entered");
            let _g = entered.enter();
            let explicit = tracing::info_span!("explicit", job = "nightly");
            tracing::info!(parent: &explicit, "from a task that was never entered");
            tracing::info!("from the entered span");
        });
        let lines = buf.lines();
        assert_eq!(lines[0]["span.name"], "explicit");
        assert_eq!(lines[0]["span.job"], "nightly");
        assert_eq!(lines[1]["span.name"], "entered");
    }

    #[test]
    fn reserved_keys_win_and_warnings_carry_their_site() {
        let buf = SharedBuf::default();
        let subscriber = tracing_subscriber::registry().with(fmt_layer(buf.clone(), None));
        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!(level = "spoof", "careful");
        });
        let line = &buf.lines()[0];
        assert_eq!(line["level"], "WARN");
        assert_eq!(line["message"], "careful");
        assert!(line["file"].as_str().unwrap().ends_with("json_format.rs"));
        assert!(line["line"].as_u64().unwrap() > 0);
        assert!(line.get("service").is_none());
        assert!(
            line.get("span.name").is_none(),
            "no enclosing span → no span keys"
        );
    }

    #[test]
    fn ids_follow_the_explicit_parent_not_the_entered_span() {
        use opentelemetry::trace::{TraceContextExt, TracerProvider as _};
        use tracing_opentelemetry::OpenTelemetrySpanExt;
        let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder().build();
        let otel = tracing_opentelemetry::layer().with_tracer(provider.tracer("test"));

        let buf = SharedBuf::default();
        let subscriber = tracing_subscriber::registry()
            .with(otel)
            .with(fmt_layer(buf.clone(), None));
        let (explicit_id, entered_id) = tracing::subscriber::with_default(subscriber, || {
            let entered = tracing::info_span!("entered");
            let _g = entered.enter();
            let explicit = tracing::info_span!("explicit");
            tracing::info!(parent: &explicit, "from a task that was never entered");
            let id =
                |s: &tracing::Span| format!("{:016x}", s.context().span().span_context().span_id());
            (id(&explicit), id(&entered))
        });
        assert_ne!(explicit_id, entered_id);
        let line = &buf.lines()[0];
        assert_eq!(line["span.name"], "explicit");
        assert_eq!(line["span_id"], explicit_id, "ids and span.name must agree");
    }

    /// The shipping composition (`logging.rs`) is `Vec<Box<dyn Layer>>` of
    /// `Filtered<WithDispatch<fmt::Layer>>`: three hops that each have to
    /// forward `on_register_dispatch` for the handle to bind. A dropped
    /// forward would not fail loudly — it would silently fall back to the
    /// thread context, which only the explicit-parent case can tell apart.
    #[test]
    fn ids_still_follow_the_explicit_parent_through_the_boxed_filtered_stack() {
        use opentelemetry::trace::{TraceContextExt, TracerProvider as _};
        use tracing_opentelemetry::OpenTelemetrySpanExt;
        use tracing_subscriber::{EnvFilter, Layer, Registry};
        let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder().build();
        let buf = SharedBuf::default();
        let (json, handle) = layer(None, buf.clone());
        let layers: Vec<Box<dyn Layer<Registry> + Send + Sync>> = vec![
            Box::new(tracing_opentelemetry::layer().with_tracer(provider.tracer("test"))),
            Box::new(json.with_filter(EnvFilter::new("info"))),
        ];
        let subscriber = tracing_subscriber::registry().with(layers);
        let (explicit_id, entered_id) = tracing::subscriber::with_default(subscriber, || {
            // What `logging::init` does after `.init()`: `Vec<L>` does not
            // forward `on_register_dispatch`, so bind from outside a callback.
            tracing::dispatcher::get_default(|d| handle.bind(d));
            let entered = tracing::info_span!("entered");
            let _g = entered.enter();
            let explicit = tracing::info_span!("explicit");
            tracing::info!(parent: &explicit, "boxed and filtered");
            let id =
                |s: &tracing::Span| format!("{:016x}", s.context().span().span_context().span_id());
            (id(&explicit), id(&entered))
        });
        assert_ne!(explicit_id, entered_id);
        let line = &buf.lines()[0];
        assert_eq!(line["span.name"], "explicit");
        assert_eq!(
            line["span_id"], explicit_id,
            "the handle must be bound through the real stack: {line}"
        );
    }

    #[test]
    fn carries_the_otel_trace_ids_when_the_export_layer_is_installed() {
        use opentelemetry::trace::TracerProvider as _;
        let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder().build();
        let otel = tracing_opentelemetry::layer().with_tracer(provider.tracer("test"));

        let buf = SharedBuf::default();
        let subscriber = tracing_subscriber::registry()
            .with(otel)
            .with(fmt_layer(buf.clone(), None));
        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!("outer");
            let _g = span.enter();
            tracing::info!("inside");
        });
        tracing::info!("outside");

        let lines = buf.lines();
        let inside = &lines[0];
        let trace_id = inside["trace_id"].as_str().expect("trace id present");
        let span_id = inside["span_id"].as_str().expect("span id present");
        assert_eq!(trace_id.len(), 32);
        assert_eq!(span_id.len(), 16);
        assert!(trace_id.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(trace_id, "0".repeat(32));
        // The second event fired after the subscriber was uninstalled and
        // outside any span; it must not have been captured at all.
        assert_eq!(lines.len(), 1);
    }
}
