use std::collections::HashMap;
use std::time::Instant;

use tokio::sync::mpsc;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use uuid::Uuid;

use crate::types::SpanRecord;

/// Internal data attached to each span via extensions.
struct SpanData {
    trace_id: String,
    span_id: String,
    start: Instant,
    start_wall: chrono::DateTime<chrono::Utc>,
    attributes: HashMap<String, String>,
    events: Vec<EventRecord>,
    service_name: String,
    has_error_event: bool,
}

/// A single event captured within a span.
struct EventRecord {
    name: String,
    attributes: HashMap<String, String>,
}

/// Generate a 32-character hex trace ID from a UUID v4.
fn new_trace_id() -> String {
    let uuid = Uuid::new_v4();
    let bytes = uuid.as_bytes();
    hex::encode(bytes)
}

/// Generate a 16-character hex span ID from the first 8 bytes of a UUID v4.
fn new_span_id() -> String {
    let uuid = Uuid::new_v4();
    let bytes = uuid.as_bytes();
    hex::encode(&bytes[..8])
}

// ---------------------------------------------------------------------------
// Visitors for extracting field values from spans and events
// ---------------------------------------------------------------------------

/// Visitor that collects all fields into a HashMap<String, String>.
struct FieldVisitor {
    fields: HashMap<String, String>,
}

impl FieldVisitor {
    fn new() -> Self {
        Self {
            fields: HashMap::new(),
        }
    }
}

impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.fields
            .insert(field.name().to_string(), format!("{:?}", value));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }
}

/// Visitor for events that also extracts the event name.
struct EventVisitor {
    name: Option<String>,
    fields: HashMap<String, String>,
}

impl EventVisitor {
    fn new() -> Self {
        Self {
            name: None,
            fields: HashMap::new(),
        }
    }
}

impl Visit for EventVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let key = field.name();
        let val = format!("{:?}", value);
        if key == "name" {
            self.name = Some(val.trim_matches('"').to_string());
        } else {
            self.fields.insert(key.to_string(), val);
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        let key = field.name();
        if key == "name" {
            self.name = Some(value.to_string());
        } else {
            self.fields.insert(key.to_string(), value.to_string());
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }
}

// ---------------------------------------------------------------------------
// SpanCollectorLayer
// ---------------------------------------------------------------------------

/// A custom `tracing_subscriber::Layer` that captures span lifecycle events
/// and sends completed `SpanRecord`s to the observability store via an unbounded channel.
pub struct SpanCollectorLayer {
    sender: mpsc::UnboundedSender<SpanRecord>,
    service_name: String,
}

impl SpanCollectorLayer {
    /// Create a new `SpanCollectorLayer`.
    ///
    /// - `sender`: channel sender for completed span records.
    /// - `service_name`: the service name to attach to every span record.
    pub fn new(sender: mpsc::UnboundedSender<SpanRecord>, service_name: String) -> Self {
        Self {
            sender,
            service_name,
        }
    }
}

impl<S> Layer<S> for SpanCollectorLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let span = match ctx.span(id) {
            Some(s) => s,
            None => return,
        };

        // Inherit trace_id from parent, or generate a new one.
        let trace_id = if let Some(parent) = span.parent() {
            let extensions = parent.extensions();
            extensions
                .get::<SpanData>()
                .map(|d| d.trace_id.clone())
                .unwrap_or_else(new_trace_id)
        } else {
            new_trace_id()
        };

        // Collect initial span attributes.
        let mut visitor = FieldVisitor::new();
        attrs.record(&mut visitor);

        let data = SpanData {
            trace_id,
            span_id: new_span_id(),
            start: Instant::now(),
            start_wall: chrono::Utc::now(),
            attributes: visitor.fields,
            events: Vec::new(),
            service_name: self.service_name.clone(),
            has_error_event: false,
        };

        span.extensions_mut().insert(data);
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        let span = match ctx.span(id) {
            Some(s) => s,
            None => return,
        };

        let mut visitor = FieldVisitor::new();
        values.record(&mut visitor);

        let mut extensions = span.extensions_mut();
        if let Some(data) = extensions.get_mut::<SpanData>() {
            data.attributes.extend(visitor.fields);
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        // Attach the event to the current span, if any.
        let span = match ctx.current_span().id().and_then(|id| ctx.span(id)) {
            Some(s) => s,
            None => return,
        };

        let mut visitor = EventVisitor::new();
        event.record(&mut visitor);

        // Determine the event name: prefer the explicit "name" field, fall back to metadata name.
        let event_name = visitor
            .name
            .unwrap_or_else(|| event.metadata().name().to_string());

        let is_error = *event.metadata().level() == Level::ERROR;

        let record = EventRecord {
            name: event_name,
            attributes: visitor.fields,
        };

        let mut extensions = span.extensions_mut();
        if let Some(data) = extensions.get_mut::<SpanData>() {
            data.events.push(record);
            if is_error {
                data.has_error_event = true;
            }
        }
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        let span = match ctx.span(&id) {
            Some(s) => s,
            None => return,
        };

        // Determine parent_span_id before borrowing extensions.
        let parent_span_id = span
            .parent()
            .and_then(|p| {
                let ext = p.extensions();
                ext.get::<SpanData>().map(|d| d.span_id.clone())
            })
            .unwrap_or_default();

        let mut extensions = span.extensions_mut();
        let data = match extensions.remove::<SpanData>() {
            Some(d) => d,
            None => return,
        };

        let duration_ns = data.start.elapsed().as_nanos() as i64;

        let status_code = if data.has_error_event {
            "ERROR".to_string()
        } else {
            "OK".to_string()
        };

        // Build status_message from the first error event, if any.
        let status_message = if data.has_error_event {
            data.events
                .iter()
                .find(|e| {
                    e.attributes.contains_key("message")
                        || e.attributes.contains_key("error")
                        || e.attributes.contains_key("error.message")
                })
                .and_then(|e| {
                    e.attributes
                        .get("message")
                        .or_else(|| e.attributes.get("error"))
                        .or_else(|| e.attributes.get("error.message"))
                        .cloned()
                })
                .unwrap_or_default()
        } else {
            String::new()
        };

        // Serialize attributes to JSON.
        let span_attributes =
            serde_json::to_string(&data.attributes).unwrap_or_else(|_| "{}".to_string());

        // Serialize events to JSON array.
        let event_data: Vec<serde_json::Value> = data
            .events
            .iter()
            .map(|e| {
                serde_json::json!({
                    "name": e.name,
                    "attributes": e.attributes,
                })
            })
            .collect();
        let event_data_str =
            serde_json::to_string(&event_data).unwrap_or_else(|_| "[]".to_string());

        // Use the `oxy.name` attribute as span name if present (set by
        // #[tracing::instrument(fields(oxy.name = "..."))]).
        // Otherwise fall back to the tracing metadata name (function name).
        let span_name = data
            .attributes
            .get("oxy.name")
            .cloned()
            .unwrap_or_else(|| span.name().to_string());

        let record = SpanRecord {
            trace_id: data.trace_id,
            span_id: data.span_id,
            parent_span_id,
            span_name,
            service_name: data.service_name,
            span_attributes,
            duration_ns,
            status_code,
            status_message,
            event_data: event_data_str,
            timestamp: data.start_wall.to_rfc3339(),
        };

        // Send to writer; ignore errors (receiver dropped means shutdown).
        let _ = self.sender.send(record);
    }
}

// ---------------------------------------------------------------------------
// Public helper
// ---------------------------------------------------------------------------

/// Extract the `trace_id` from the current span's extensions.
///
/// Walks up the span hierarchy starting from `tracing::Span::current()`.
/// Returns `None` if there is no active span or the span was not created
/// by a `SpanCollectorLayer`.
pub fn current_trace_id() -> Option<String> {
    let span = tracing::Span::current();
    span.with_subscriber(|(id, subscriber)| {
        let registry = subscriber.downcast_ref::<tracing_subscriber::Registry>()?;

        // scope() yields an iterator from the current span upward through all ancestors.
        let span_ref = registry.span(id)?;
        for ancestor in span_ref.scope() {
            let extensions = ancestor.extensions();
            if let Some(data) = extensions.get::<SpanData>() {
                return Some(data.trace_id.clone());
            }
        }
        None
    })?
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;
    use tracing_subscriber::layer::SubscriberExt;

    #[test]
    fn test_trace_id_format() {
        let tid = new_trace_id();
        assert_eq!(tid.len(), 32);
        assert!(tid.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_span_id_format() {
        let sid = new_span_id();
        assert_eq!(sid.len(), 16);
        assert!(sid.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_ids_are_unique() {
        let ids: Vec<String> = (0..100).map(|_| new_trace_id()).collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique.len());
    }

    #[tokio::test]
    async fn test_span_record_sent_on_close() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let layer = SpanCollectorLayer::new(tx, "test-service".to_string());

        let subscriber = tracing_subscriber::registry().with(layer);
        let _guard = tracing::subscriber::set_default(subscriber);

        {
            let span = tracing::info_span!("test_span", key = "value");
            let _enter = span.enter();
            tracing::info!(name = "test.event", detail = "hello");
        }

        let record = rx.try_recv().expect("should receive a SpanRecord");
        assert_eq!(record.span_name, "test_span");
        assert_eq!(record.service_name, "test-service");
        assert_eq!(record.status_code, "OK");
        assert_eq!(record.trace_id.len(), 32);
        assert_eq!(record.span_id.len(), 16);
        assert!(record.parent_span_id.is_empty());
        assert!(record.duration_ns >= 0);

        // Verify attributes contain the span field.
        let attrs: HashMap<String, String> = serde_json::from_str(&record.span_attributes).unwrap();
        assert_eq!(attrs.get("key").map(|s| s.as_str()), Some("value"));

        // Verify event data.
        let events: Vec<serde_json::Value> = serde_json::from_str(&record.event_data).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["name"], "test.event");
    }

    #[tokio::test]
    async fn test_parent_child_trace_id_inheritance() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let layer = SpanCollectorLayer::new(tx, "test-service".to_string());

        let subscriber = tracing_subscriber::registry().with(layer);
        let _guard = tracing::subscriber::set_default(subscriber);

        {
            let parent = tracing::info_span!("parent_span");
            let _parent_enter = parent.enter();
            {
                let child = tracing::info_span!("child_span");
                let _child_enter = child.enter();
            }
        }

        // Child closes first, then parent.
        let child_record = rx.try_recv().expect("child record");
        let parent_record = rx.try_recv().expect("parent record");

        assert_eq!(child_record.span_name, "child_span");
        assert_eq!(parent_record.span_name, "parent_span");
        assert_eq!(child_record.trace_id, parent_record.trace_id);
        assert_eq!(child_record.parent_span_id, parent_record.span_id);
    }

    #[tokio::test]
    async fn test_error_event_sets_status() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let layer = SpanCollectorLayer::new(tx, "test-service".to_string());

        let subscriber = tracing_subscriber::registry().with(layer);
        let _guard = tracing::subscriber::set_default(subscriber);

        {
            let span = tracing::info_span!("error_span");
            let _enter = span.enter();
            tracing::error!(message = "something went wrong");
        }

        let record = rx.try_recv().expect("should receive a SpanRecord");
        assert_eq!(record.status_code, "ERROR");
        assert_eq!(record.status_message, "something went wrong");
    }

    #[tokio::test]
    async fn test_on_record_merges_attributes() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let layer = SpanCollectorLayer::new(tx, "test-service".to_string());

        let subscriber = tracing_subscriber::registry().with(layer);
        let _guard = tracing::subscriber::set_default(subscriber);

        {
            let span = tracing::info_span!("record_span", initial = "first");
            let _enter = span.enter();
            span.record("initial", "updated");
        }

        let record = rx.try_recv().expect("should receive a SpanRecord");
        let attrs: HashMap<String, String> = serde_json::from_str(&record.span_attributes).unwrap();
        assert_eq!(attrs.get("initial").map(|s| s.as_str()), Some("updated"));
    }
}
