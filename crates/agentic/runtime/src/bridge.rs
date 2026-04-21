//! Generic event bridge: drain pipeline events into the database and notify
//! subscribers.
//!
//! Both the analytics and builder pipelines use the same pattern:
//! receive events from an mpsc channel, serialize them with a caller-provided
//! function, buffer into batches, flush to postgres on a 20ms tick or on
//! terminal/suspension events, and notify SSE subscribers.

use std::time::Instant;

use agentic_core::{DomainEvents, Event};
use sea_orm::DatabaseConnection;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::crud;
use crate::state::RuntimeState;

/// Callback invoked for each event before it is persisted.
///
/// Receives the event and its serialized `(event_type, payload)`. Can inspect
/// the event to update in-memory state (e.g., marking a run as suspended).
pub type OnEventFn<Ev> = Box<dyn Fn(&Event<Ev>, &str) + Send + Sync>;

/// Run the event bridge loop.
///
/// Drains `event_rx`, serializes each event via [`Event::serialize()`],
/// batches writes to postgres, and notifies `state` subscribers. Injects
/// `duration_ms` into terminal events.
///
/// This function runs until `event_rx` is closed (i.e., the pipeline drops
/// its sender).
pub async fn run_bridge<Ev: DomainEvents>(
    db: &DatabaseConnection,
    state: &RuntimeState,
    run_id: &str,
    mut event_rx: mpsc::Receiver<Event<Ev>>,
    pipeline_start: Instant,
    on_event: Option<OnEventFn<Ev>>,
    attempt: i32,
) {
    let mut seq: i64 = 0;
    let mut buf: Vec<(i64, String, String, i32)> = Vec::new();

    let mut tick = tokio::time::interval(std::time::Duration::from_millis(20));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    tick.tick().await;

    loop {
        tokio::select! {
            maybe = event_rx.recv() => {
                let event = match maybe {
                    Some(e) => e,
                    None => {
                        // Channel closed — flush remaining events.
                        if !buf.is_empty() {
                            do_flush(db, state, run_id, &mut buf).await;
                        }
                        break;
                    }
                };

                let (event_type, mut payload) = event.serialize();

                // Inject duration_ms into terminal events.
                if is_terminal(&event_type) {
                    if let Value::Object(ref mut map) = payload {
                        map.insert(
                            "duration_ms".into(),
                            (pipeline_start.elapsed().as_millis() as u64).into(),
                        );
                    }
                }

                // Let caller inspect events for state updates (e.g. suspension).
                if let Some(ref cb) = on_event {
                    cb(&event, &event_type);
                }

                match event_type.as_str() {
                    "llm_token" | "thinking_token" => {
                        tracing::trace!(run_id = %run_id, seq, %event_type);
                    }
                    _ => {
                        tracing::debug!(
                            run_id = %run_id, seq,
                            event = %event_type,
                            data = %payload,
                        );
                    }
                }

                let flush_now = is_terminal(&event_type)
                    || matches!(event_type.as_str(), "awaiting_input" | "input_resolved");
                buf.push((seq, event_type, payload.to_string(), attempt));
                seq += 1;

                if flush_now {
                    do_flush(db, state, run_id, &mut buf).await;
                }
            }
            _ = tick.tick() => {
                if !buf.is_empty() {
                    do_flush(db, state, run_id, &mut buf).await;
                }
            }
        }
    }

    tracing::debug!(run_id = %run_id, "event bridge closed");
    state.notify(run_id);
}

async fn do_flush(
    db: &DatabaseConnection,
    state: &RuntimeState,
    run_id: &str,
    buf: &mut Vec<(i64, String, String, i32)>,
) {
    if crud::batch_insert_events(db, run_id, buf).await.is_ok() {
        crud::update_run_terminal_from_events(db, run_id, buf)
            .await
            .ok();
    }
    state.notify(run_id);
    buf.clear();
}

fn is_terminal(event_type: &str) -> bool {
    matches!(event_type, "done" | "error")
}
