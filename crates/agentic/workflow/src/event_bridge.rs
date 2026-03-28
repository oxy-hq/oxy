//! Bridge: translates oxy workflow execution events into analytics events.
//!
//! [`WorkflowEventBridge`] implements the oxy [`EventHandler`] trait so it can
//! be passed directly to [`WorkflowLauncher::launch`] in place of
//! [`NoopHandler`].  It selectively translates oxy task-level lifecycle events
//! into [`AnalyticsEvent::ProcedureStepStarted`] /
//! [`AnalyticsEvent::ProcedureStepCompleted`] and forwards them on the
//! caller-supplied [`EventStream`].
//!
//! All other oxy event kinds (workflow-level bookkeeping, consistency checks,
//! concurrency signals, etc.) are silently dropped so the analytics event
//! stream stays clean.
//!
//! # Wiring
//!
//! ```rust,no_run
//! use agentic_core::events::EventStream;
//! use agentic_analytics::AnalyticsEvent;
//! use agentic_workflow::WorkflowEventBridge;
//! use tokio::sync::mpsc;
//!
//! let (tx, _rx): (EventStream<AnalyticsEvent>, _) = mpsc::channel(256);
//! let bridge = WorkflowEventBridge::new(tx);
//! // pass `bridge` to WorkflowLauncher::launch(…, bridge, …)
//! ```

use std::collections::HashMap;

use agentic_analytics::AnalyticsEvent;
use agentic_core::events::{Event, EventStream};
use oxy::config::constants::TASK_SOURCE;
use oxy::execute::{
    types::{Event as OxyEvent, EventKind},
    writer::EventHandler,
};
use oxy_shared::errors::OxyError;

// ---------------------------------------------------------------------------
// WorkflowEventBridge
// ---------------------------------------------------------------------------

/// Bridges oxy workflow execution events into an [`EventStream<AnalyticsEvent>`].
///
/// Implements [`EventHandler`] so it can be passed to
/// [`WorkflowLauncher::launch`].  Only task-scoped lifecycle events are
/// translated; all other oxy events are dropped.
///
/// The bridge is stateful: it tracks the display name of each in-flight task
/// (keyed by `source.id`) so that completion events can carry the same
/// human-readable `step` string as the corresponding start event.
pub struct WorkflowEventBridge {
    tx: EventStream<AnalyticsEvent>,
    /// `source.id` → task display name, populated on `Started`, removed on `Finished` / `Error`.
    task_names: HashMap<String, String>,
}

impl WorkflowEventBridge {
    /// Create a bridge that forwards translated events on `tx`.
    pub fn new(tx: EventStream<AnalyticsEvent>) -> Self {
        Self {
            tx,
            task_names: HashMap::new(),
        }
    }

    async fn emit(&self, event: AnalyticsEvent) {
        // Non-blocking send: drop silently if the channel is full or closed.
        let _ = self.tx.send(Event::Domain(event)).await;
    }
}

#[async_trait::async_trait]
impl EventHandler for WorkflowEventBridge {
    async fn handle_event(&mut self, event: OxyEvent) -> Result<(), OxyError> {
        // Only bridge task-scoped events; everything else is dropped.
        if event.source.kind != TASK_SOURCE {
            return Ok(());
        }

        match event.kind {
            EventKind::Started { name, .. } => {
                // Record the human-readable name so we can echo it on completion.
                self.task_names
                    .insert(event.source.id.clone(), name.clone());
                self.emit(AnalyticsEvent::ProcedureStepStarted { step: name })
                    .await;
            }

            EventKind::Finished { error, .. } => {
                let step = self
                    .task_names
                    .remove(&event.source.id)
                    .unwrap_or_else(|| event.source.id.clone());
                self.emit(AnalyticsEvent::ProcedureStepCompleted {
                    step,
                    success: error.is_none(),
                    error,
                })
                .await;
            }

            EventKind::Error { message } => {
                let step = self
                    .task_names
                    .remove(&event.source.id)
                    .unwrap_or_else(|| event.source.id.clone());
                self.emit(AnalyticsEvent::ProcedureStepCompleted {
                    step,
                    success: false,
                    error: Some(message),
                })
                .await;
            }

            // All other task-scoped event kinds (Updated, Progress, Message,
            // SQLQueryGenerated, …) are intentionally dropped — they carry
            // workflow-internal detail that has no clean analytics semantic.
            _ => {}
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use agentic_core::events::Event;
    use oxy::config::constants::TASK_SOURCE;
    use oxy::execute::types::{Event as OxyEvent, EventKind, Source};
    use std::collections::HashMap;
    use tokio::sync::mpsc;

    // ── construction helpers ────────────────────────────────────────────────

    fn task_source(id: &str) -> Source {
        Source {
            id: id.to_string(),
            kind: TASK_SOURCE.to_string(),
            parent_id: None,
        }
    }

    fn other_source(id: &str) -> Source {
        Source {
            id: id.to_string(),
            kind: "workflow".to_string(),
            parent_id: None,
        }
    }

    fn started(source: Source, name: &str) -> OxyEvent {
        OxyEvent {
            source,
            kind: EventKind::Started {
                name: name.to_string(),
                attributes: HashMap::new(),
            },
        }
    }

    fn finished(source: Source, error: Option<&str>) -> OxyEvent {
        OxyEvent {
            source,
            kind: EventKind::Finished {
                attributes: HashMap::new(),
                message: String::new(),
                error: error.map(str::to_string),
            },
        }
    }

    fn error_ev(source: Source, message: &str) -> OxyEvent {
        OxyEvent {
            source,
            kind: EventKind::Error {
                message: message.to_string(),
            },
        }
    }

    /// Drain all Domain analytics events currently in the channel (non-blocking).
    fn drain(rx: &mut mpsc::Receiver<Event<AnalyticsEvent>>) -> Vec<AnalyticsEvent> {
        let mut out = Vec::new();
        while let Ok(e) = rx.try_recv() {
            if let Event::Domain(ev) = e {
                out.push(ev);
            }
        }
        out
    }

    // ── tests ───────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn started_emits_procedure_step_started() {
        let (tx, mut rx) = mpsc::channel(16);
        let mut bridge = WorkflowEventBridge::new(tx);

        bridge
            .handle_event(started(task_source("t1"), "Run sales report"))
            .await
            .unwrap();

        let events = drain(&mut rx);
        assert_eq!(events.len(), 1);
        assert!(
            matches!(&events[0], AnalyticsEvent::ProcedureStepStarted { step } if step == "Run sales report"),
            "unexpected event: {:?}",
            events[0]
        );
    }

    #[tokio::test]
    async fn finished_success_emits_completed_with_name() {
        let (tx, mut rx) = mpsc::channel(16);
        let mut bridge = WorkflowEventBridge::new(tx);

        bridge
            .handle_event(started(task_source("t1"), "Run sales report"))
            .await
            .unwrap();
        bridge
            .handle_event(finished(task_source("t1"), None))
            .await
            .unwrap();

        let events = drain(&mut rx);
        assert_eq!(events.len(), 2);
        assert!(
            matches!(&events[1], AnalyticsEvent::ProcedureStepCompleted { step, success: true, error: None } if step == "Run sales report"),
            "unexpected event: {:?}",
            events[1]
        );
    }

    #[tokio::test]
    async fn finished_with_error_emits_failed() {
        let (tx, mut rx) = mpsc::channel(16);
        let mut bridge = WorkflowEventBridge::new(tx);

        bridge
            .handle_event(started(task_source("t1"), "Step A"))
            .await
            .unwrap();
        bridge
            .handle_event(finished(task_source("t1"), Some("query failed")))
            .await
            .unwrap();

        let events = drain(&mut rx);
        assert!(
            matches!(&events[1], AnalyticsEvent::ProcedureStepCompleted { success: false, error: Some(e), .. } if e == "query failed"),
            "unexpected event: {:?}",
            events[1]
        );
    }

    #[tokio::test]
    async fn error_event_emits_failed_with_message() {
        let (tx, mut rx) = mpsc::channel(16);
        let mut bridge = WorkflowEventBridge::new(tx);

        bridge
            .handle_event(started(task_source("t1"), "Step A"))
            .await
            .unwrap();
        bridge
            .handle_event(error_ev(task_source("t1"), "connection refused"))
            .await
            .unwrap();

        let events = drain(&mut rx);
        assert!(
            matches!(&events[1], AnalyticsEvent::ProcedureStepCompleted { success: false, error: Some(e), .. } if e == "connection refused"),
            "unexpected event: {:?}",
            events[1]
        );
    }

    #[tokio::test]
    async fn non_task_source_events_are_dropped() {
        let (tx, mut rx) = mpsc::channel(16);
        let mut bridge = WorkflowEventBridge::new(tx);

        bridge
            .handle_event(started(other_source("wf-1"), "Workflow step"))
            .await
            .unwrap();
        bridge
            .handle_event(finished(other_source("wf-1"), None))
            .await
            .unwrap();

        assert!(drain(&mut rx).is_empty());
    }

    #[tokio::test]
    async fn name_is_recovered_for_opaque_source_id() {
        // The bridge must store name on Started and echo it on Finished, even
        // when source.id is an opaque internal identifier.
        let (tx, mut rx) = mpsc::channel(16);
        let mut bridge = WorkflowEventBridge::new(tx);

        bridge
            .handle_event(started(
                task_source("opaque-id-9999"),
                "Human readable step",
            ))
            .await
            .unwrap();
        bridge
            .handle_event(finished(task_source("opaque-id-9999"), None))
            .await
            .unwrap();

        let events = drain(&mut rx);
        match &events[1] {
            AnalyticsEvent::ProcedureStepCompleted { step, .. } => {
                assert_eq!(step, "Human readable step");
            }
            other => panic!("expected ProcedureStepCompleted, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn multiple_concurrent_steps_tracked_independently() {
        let (tx, mut rx) = mpsc::channel(16);
        let mut bridge = WorkflowEventBridge::new(tx);

        bridge
            .handle_event(started(task_source("t1"), "Step One"))
            .await
            .unwrap();
        bridge
            .handle_event(started(task_source("t2"), "Step Two"))
            .await
            .unwrap();
        bridge
            .handle_event(finished(task_source("t1"), None))
            .await
            .unwrap();
        bridge
            .handle_event(finished(task_source("t2"), Some("failed")))
            .await
            .unwrap();

        let events = drain(&mut rx);
        assert_eq!(events.len(), 4);
        assert!(
            matches!(&events[2], AnalyticsEvent::ProcedureStepCompleted { step, success: true, .. } if step == "Step One")
        );
        assert!(
            matches!(&events[3], AnalyticsEvent::ProcedureStepCompleted { step, success: false, .. } if step == "Step Two")
        );
    }

    #[tokio::test]
    async fn other_task_event_kinds_are_dropped() {
        let (tx, mut rx) = mpsc::channel(16);
        let mut bridge = WorkflowEventBridge::new(tx);

        let ev = OxyEvent {
            source: task_source("t1"),
            kind: EventKind::Message {
                message: "progress update".to_string(),
            },
        };
        bridge.handle_event(ev).await.unwrap();

        assert!(drain(&mut rx).is_empty());
    }
}
