//! Airway worker — runs a parsed [`AirwayPipelineSpec`] end-to-end and
//! bridges engine events onto an [`ExecutingTask`] channel pair the
//! agentic runtime can consume.
//!
//! Pattern B subsystem: one queue row → one engine run → done/failed.
//! No per-step decisions, no fan-out at the coordinator. Within a run,
//! resource-level fan-out happens inside [`airway::Pipeline::extract_source`]
//! via `extract_workers`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use agentic_core::delegation::TaskOutcome;
use agentic_runtime::orchestrator::worker::ExecutingTask;
use airway::Pipeline;
use airway::airstack::{AirappEventHandler, EventBus, PipelineEvent};
use airway::state::StateStore;
use async_trait::async_trait;
use chrono::Utc;
use sea_orm::DatabaseConnection;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use crate::boxed::{BoxedDestination, BoxedSourceConnector};
use crate::config::AirwayPipelineSpec;
use crate::destination_factory::build_destination;
use crate::error::AirwayError;
use crate::events::AirwayEvent;
use crate::source_factory::build_source_connector;
use crate::state_store::{AirwayPgStateStore, AirwayRunScopedStateStore};

/// Buffer for engine→runtime event forwarding. Sized so a burst of
/// resource completions in a wide pipeline doesn't make the engine
/// backpressure on the EventBus itself.
const EVENT_BUFFER: usize = 64;

/// Buffer for outcomes. Airway only ever produces a single terminal
/// outcome per run, but the runtime channel type wants a non-zero
/// capacity.
const OUTCOME_BUFFER: usize = 4;

/// Builds and drives a single airway pipeline run.
///
/// Construct once per dispatch (`agentic-pipeline`'s `TaskExecutor`
/// arm) and call [`AirwayWorker::execute`]. The returned
/// [`ExecutingTask`] mirrors what `agentic-runtime` expects — the
/// coordinator owns the channels from here.
#[derive(Clone)]
pub struct AirwayWorker {
    db: Arc<DatabaseConnection>,
    /// Optional OAuth refresh-token write-back sink, supplied by the
    /// host for sources that rotate refresh tokens (QuickBooks). `None`
    /// for every other source.
    refresh_sink: Option<Arc<dyn crate::RefreshTokenSink>>,
    /// Optional credential provider, supplied by the host for
    /// `airhouse_managed` destinations so the destination re-mints a fresh
    /// (non-expired) ephemeral credential on every (re)connect. `None` for
    /// every other destination.
    credential_provider: Option<Arc<dyn crate::CredentialProvider>>,
}

impl AirwayWorker {
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Self {
            db,
            refresh_sink: None,
            credential_provider: None,
        }
    }

    /// Construct a worker that hands `sink` to the source factory so a
    /// rotated OAuth refresh token can be persisted to the host's secret
    /// store. Used by the executor for `quickbooks` pipelines.
    pub fn with_refresh_sink(
        db: Arc<DatabaseConnection>,
        sink: Arc<dyn crate::RefreshTokenSink>,
    ) -> Self {
        Self {
            db,
            refresh_sink: Some(sink),
            credential_provider: None,
        }
    }

    /// Attach a [`crate::CredentialProvider`] handed to the destination factory
    /// so an `airhouse_managed` destination re-mints fresh credentials on every
    /// (re)connect. Chainable with the constructors above.
    pub fn with_credential_provider(
        mut self,
        provider: Arc<dyn crate::CredentialProvider>,
    ) -> Self {
        self.credential_provider = Some(provider);
        self
    }

    /// Start the airway run for `spec`. Returns immediately with
    /// receiver halves for events and outcomes; the actual extract /
    /// normalize / load runs on a spawned task.
    ///
    /// Errors are surfaced via [`TaskOutcome::Failed`] on the outcomes
    /// channel rather than as the function's return type — this keeps
    /// the worker shape uniform with how other domain executors plug
    /// into the runtime.
    /// `resume_run_id`: when `Some`, this run uses a RUN-SCOPED state store
    /// keyed by that run_id (persisting the cursor to
    /// `airway_run_extensions.resume_state`) instead of the pipeline-global
    /// store — set for resumable backfills so a reset-in-place retry resumes
    /// mid-window and the live pipeline cursor is never touched. `None` = normal
    /// run against the pipeline-global store.
    pub fn execute(
        &self,
        spec: AirwayPipelineSpec,
        resume_run_id: Option<String>,
    ) -> ExecutingTask {
        let (event_tx, event_rx) = mpsc::channel::<(String, Value)>(EVENT_BUFFER);
        let (outcome_tx, outcome_rx) = mpsc::channel::<TaskOutcome>(OUTCOME_BUFFER);
        let cancel = CancellationToken::new();

        let db = self.db.clone();
        let refresh_sink = self.refresh_sink.clone();
        let credential_provider = self.credential_provider.clone();
        let cancel_clone = cancel.clone();
        // If `drive` panics its JoinHandle is normally dropped and the
        // panic is swallowed: no `TaskOutcome` is ever sent and the
        // coordinator waits on a dead channel until cancel. Watch the
        // handle and synthesize a terminal `Failed` on panic/abort so
        // the run always reaches a terminal state.
        let outcome_tx_watch = outcome_tx.clone();
        tokio::spawn(async move {
            let handle = tokio::spawn(drive(
                spec,
                resume_run_id,
                db,
                refresh_sink,
                credential_provider,
                event_tx,
                cancel_clone,
                outcome_tx,
            ));
            if let Err(join_err) = handle.await {
                let msg = if join_err.is_panic() {
                    "airway worker panicked (internal error)".to_string()
                } else {
                    "airway worker task aborted".to_string()
                };
                // No-op if `drive` already sent an outcome before the
                // failure; this only fires when it never did.
                let _ = outcome_tx_watch.send(TaskOutcome::Failed(msg)).await;
            }
        });

        ExecutingTask {
            events: event_rx,
            outcomes: outcome_rx,
            cancel,
            // Airway never suspends mid-run, so no resume channel.
            answers: None,
        }
    }
}

/// Top-level driver spawned by [`AirwayWorker::execute`]. Owns every
/// piece of state for the run (no borrowed parameters), so the
/// returned future is straightforwardly `Send + 'static` for
/// `tokio::spawn`. Translates terminal outcome into the runtime's
/// [`TaskOutcome`] shape.
async fn drive(
    spec: AirwayPipelineSpec,
    resume_run_id: Option<String>,
    db: Arc<DatabaseConnection>,
    refresh_sink: Option<Arc<dyn crate::RefreshTokenSink>>,
    credential_provider: Option<Arc<dyn crate::CredentialProvider>>,
    event_tx: mpsc::Sender<(String, Value)>,
    cancel: CancellationToken,
    outcome_tx: mpsc::Sender<TaskOutcome>,
) {
    let pipeline_name = spec.name.clone();
    // Set by the event forwarder once the engine emits its own
    // `pipeline_error`. Lets us tell "airway already reported the
    // failure on the stream" from "failed before any engine event"
    // (connector/destination build, secret resolution, state store) —
    // the latter would otherwise flip the run to failed with nothing
    // on the SSE stream, so the UI shows a status change but no cause.
    let saw_error = Arc::new(AtomicBool::new(false));
    let outcome = match run_pipeline(
        spec,
        resume_run_id,
        db,
        refresh_sink,
        credential_provider,
        event_tx.clone(),
        cancel,
        saw_error.clone(),
    )
    .await
    {
        Ok(()) => TaskOutcome::Done {
            answer: String::new(),
            metadata: None,
        },
        Err(err) => {
            if !saw_error.load(Ordering::Relaxed) {
                let domain = AirwayEvent::PipelineError {
                    pipeline_name,
                    load_id: None,
                    error: err.to_string(),
                };
                if let Ok(value) = serde_json::to_value(&domain) {
                    let _ = event_tx.send(("pipeline_error".to_string(), value)).await;
                }
            }
            TaskOutcome::Failed(err.to_string())
        }
    };
    let _ = outcome_tx.send(outcome).await;
}

/// Build the airway [`Pipeline`] from a spec and drive it to
/// completion. Pure airway-side; the runtime bridge lives in
/// [`AirwayWorker::execute`].
async fn run_pipeline(
    spec: AirwayPipelineSpec,
    resume_run_id: Option<String>,
    db: Arc<DatabaseConnection>,
    refresh_sink: Option<Arc<dyn crate::RefreshTokenSink>>,
    credential_provider: Option<Arc<dyn crate::CredentialProvider>>,
    event_tx: mpsc::Sender<(String, Value)>,
    cancel: CancellationToken,
    saw_error: Arc<AtomicBool>,
) -> Result<(), AirwayError> {
    // ── Build pluggable parts ──────────────────────────────────────────────
    let connector = build_source_connector(&spec.source, refresh_sink)?;
    let destination = build_destination(spec.destination.as_inline()?, credential_provider)?;

    let mut source = airway::Source::from_connector(BoxedSourceConnector(connector));
    if !spec.resources.is_empty() {
        let names: Vec<&str> = spec.resources.iter().map(String::as_str).collect();
        source = source.with_resources(&names);
    }

    let state_store: Arc<dyn StateStore> = match resume_run_id {
        // Resumable backfill: run-scoped store — cursor → `resume_state` keyed by
        // run_id, schema + audit delegated to the pipeline-global row, live
        // cursor never touched.
        Some(run_id) => Arc::new(AirwayRunScopedStateStore::new(
            db,
            run_id,
            spec.name.clone(),
        )),
        None => Arc::new(AirwayPgStateStore::new(db, spec.name.clone())),
    };

    // ── Event bridge ──────────────────────────────────────────────────────
    let mut bus = EventBus::new();
    bus.subscribe(EventForwarder {
        tx: event_tx,
        saw_error,
    });
    let bus = Arc::new(bus);

    // ── Compose pipeline ──────────────────────────────────────────────────
    //
    // `spec.concurrency` is threaded via `with_extract_workers`: airway
    // extracts resources sequentially when it's 1 and via
    // `buffer_unordered` otherwise (see `airway::Pipeline::extract_source`).
    let mut pipeline = Pipeline::new(spec.name.clone(), BoxedDestination(destination))
        .with_state_store(state_store)
        .with_event_bus(bus)
        .with_cancellation_token(cancel)
        .with_extract_workers(spec.concurrency)
        .with_streaming(spec.streaming);
    if let Some(cap) = spec.channel_capacity {
        pipeline = pipeline.with_channel_capacity(cap);
    }

    pipeline.run_source(source).await?;
    Ok(())
}

/// Subscriber on airway's `EventBus` that forwards every
/// [`PipelineEvent`] to the runtime event channel as a pre-serialised
/// `(event_type, payload)` pair.
///
/// Translates through [`AirwayEvent`] so the serialization contract
/// stays under oxy's control — the `event_type` discriminator and
/// payload field names are stable even if the engine struct evolves.
struct EventForwarder {
    tx: mpsc::Sender<(String, Value)>,
    /// Flipped once a `pipeline_error` is forwarded, so the driver
    /// knows the engine already reported the failure and doesn't
    /// double-emit a synthetic one.
    saw_error: Arc<AtomicBool>,
}

#[async_trait]
impl AirappEventHandler for EventForwarder {
    async fn handle_event(&self, event: PipelineEvent) -> Result<(), airway::AirwayError> {
        let domain = AirwayEvent::from(event);
        match serde_json::to_value(&domain) {
            Ok(mut value) => {
                // Stamp emit time once, here (this handler fires when
                // the engine emits). It's persisted in the event
                // payload, so replay returns the same value — the
                // frontend reducer stays pure/idempotent and can build
                // a real time-axis run timeline. Injected at the
                // envelope level so all variants get it without
                // touching every `AirwayEvent` struct.
                if let Value::Object(map) = &mut value {
                    map.insert("ts".into(), Value::String(Utc::now().to_rfc3339()));
                } else {
                    // Every `AirwayEvent` variant is `#[serde(tag=...)]`
                    // so serializes as an object. If a future variant
                    // breaks that, the `ts` stamp (and the timeline)
                    // silently degrade — surface it instead of hiding
                    // behind the `unwrap_or("airway_event")` fallback.
                    debug_assert!(false, "AirwayEvent serialized as non-object: {value}");
                    warn!(value = %value, "AirwayEvent serialized as non-object; `ts` not stamped");
                }
                let event_type = value
                    .get("event_type")
                    .and_then(Value::as_str)
                    .unwrap_or("airway_event")
                    .to_string();
                if event_type == "pipeline_error" {
                    self.saw_error.store(true, Ordering::Relaxed);
                }
                if let Err(e) = self.tx.send((event_type, value)).await {
                    // Subscriber gone — the runtime stopped consuming
                    // events (cancellation, downstream drop). Log and
                    // continue; the pipeline shouldn't fail because of
                    // a closed SSE.
                    warn!(error = %e, "airway event channel closed; dropping events");
                }
            }
            Err(e) => {
                warn!(error = %e, "failed to serialize AirwayEvent");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn forwarder(tx: mpsc::Sender<(String, Value)>) -> (EventForwarder, Arc<AtomicBool>) {
        let saw_error = Arc::new(AtomicBool::new(false));
        (
            EventForwarder {
                tx,
                saw_error: saw_error.clone(),
            },
            saw_error,
        )
    }

    #[tokio::test]
    async fn forwarder_pushes_serialised_event_type() {
        let (tx, mut rx) = mpsc::channel::<(String, Value)>(4);
        let (forwarder, saw_error) = forwarder(tx);
        forwarder
            .handle_event(PipelineEvent::LoadStarted {
                pipeline_name: "p".into(),
                load_id: "l".into(),
            })
            .await
            .expect("forward");

        let (event_type, payload) = rx.recv().await.expect("event");
        assert_eq!(event_type, "load_started");
        assert_eq!(payload["pipeline_name"], json!("p"));
        assert_eq!(payload["load_id"], json!("l"));
        assert!(
            !saw_error.load(Ordering::Relaxed),
            "non-error must not set saw_error"
        );
    }

    #[tokio::test]
    async fn forwarder_flags_saw_error_on_pipeline_error() {
        let (tx, mut rx) = mpsc::channel::<(String, Value)>(4);
        let (forwarder, saw_error) = forwarder(tx);
        forwarder
            .handle_event(PipelineEvent::PipelineError {
                pipeline_name: "p".into(),
                load_id: None,
                error: "boom".into(),
            })
            .await
            .expect("forward");
        let (event_type, _) = rx.recv().await.expect("event");
        assert_eq!(event_type, "pipeline_error");
        assert!(
            saw_error.load(Ordering::Relaxed),
            "pipeline_error must flip saw_error so drive() doesn't double-emit"
        );
    }

    #[tokio::test]
    async fn forwarder_silently_drops_on_closed_channel() {
        let (tx, rx) = mpsc::channel::<(String, Value)>(4);
        drop(rx); // close the receiver
        let (forwarder, _saw_error) = forwarder(tx);
        // Should not panic / not return Err — closed channel is logged
        // and the pipeline keeps running.
        forwarder
            .handle_event(PipelineEvent::PipelineError {
                pipeline_name: "p".into(),
                load_id: None,
                error: "boom".into(),
            })
            .await
            .expect("handle_event must not error on closed channel");
    }
}
