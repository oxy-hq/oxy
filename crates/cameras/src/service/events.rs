//! Process-wide sink for camera domain events.
//!
//! The cameras crate can't depend on the app crate, so live fan-out (the
//! world-model SSE bus) is bridged through a registered callback: the app
//! crate calls [`set_sink`] once at startup; producers here call [`emit`].
//! No sink registered → no-op.

use std::sync::OnceLock;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum CameraDomainEvent {
    /// A compliance report was ingested (one per report, violations and
    /// clean verdicts alike — consumers filter on `violation`).
    ComplianceReport {
        camera_id: Uuid,
        report_id: Uuid,
        violation: bool,
        missing_items: Vec<String>,
        confidence: Option<f64>,
        segment_start: chrono::DateTime<chrono::Utc>,
        segment_end: chrono::DateTime<chrono::Utc>,
    },
    /// Camera health transition observed by the alerter tick.
    HealthTransition {
        camera_id: Uuid,
        from: String,
        to: String,
        reason: String,
    },
}

pub type EventSink = Box<dyn Fn(Uuid, CameraDomainEvent) + Send + Sync>;

static SINK: OnceLock<EventSink> = OnceLock::new();

/// Register the process-wide sink. First caller wins; later calls no-op.
pub fn set_sink(sink: EventSink) {
    let _ = SINK.set(sink);
}

pub fn emit(workspace_id: Uuid, event: CameraDomainEvent) {
    if let Some(sink) = SINK.get() {
        sink(workspace_id, event);
    }
}
