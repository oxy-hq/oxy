use std::sync::Arc;

use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt, util::SubscriberInitExt};

use crate::layer::SpanCollectorLayer;
use crate::store::ObservabilityStore;
use crate::types::SpanRecord;

const DEFAULT_LOG_LEVEL: &str = "warn";
const DEFAULT_OBSERVABILITY_LOG_LEVEL: &str = "debug";

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

/// Build an EnvFilter for observability spans. Uses `OXY_OBSERVABILITY_LOG_LEVEL`
/// env var, defaults to "debug" to capture all traces.
fn build_observability_filter() -> EnvFilter {
    let level = std::env::var("OXY_OBSERVABILITY_LOG_LEVEL")
        .unwrap_or_else(|_| DEFAULT_OBSERVABILITY_LOG_LEVEL.to_string());

    match EnvFilter::try_new(&level) {
        Ok(filter) => filter.add_directive("deser_incomplete=off".parse().unwrap()),
        Err(_) => {
            eprintln!(
                "Warning: Invalid observability log level '{}', falling back to '{}'",
                level, DEFAULT_OBSERVABILITY_LOG_LEVEL
            );
            EnvFilter::try_new(DEFAULT_OBSERVABILITY_LOG_LEVEL)
                .unwrap()
                .add_directive("deser_incomplete=off".parse().unwrap())
        }
    }
}

/// Build just the `SpanCollectorLayer` and its receiver. No store, no bridge.
///
/// Use this when you need to install the tracing layer early (e.g. before the
/// database is ready) and defer the store wiring. Spans emitted before the
/// bridge is spawned accumulate in the unbounded channel — call
/// [`spawn_bridge`] later with the matching receiver to drain them.
pub fn build_layer_and_receiver() -> (
    SpanCollectorLayer,
    tokio::sync::mpsc::UnboundedReceiver<SpanRecord>,
) {
    let (span_tx, span_rx) = tokio::sync::mpsc::unbounded_channel::<SpanRecord>();
    let service_name = std::env::var("OXY_SERVICE_NAME").unwrap_or_else(|_| "oxy".to_string());
    let layer = SpanCollectorLayer::new(span_tx, service_name);
    (layer, span_rx)
}

/// Spawn the batching bridge task that drains `receiver` into `store`.
///
/// Uses a short interval (1s) and small batch (100) to keep latency low while
/// still amortizing write overhead for Postgres. These values match the DuckDB
/// writer's internal buffer so the two don't stack latency.
pub fn spawn_bridge(
    mut receiver: tokio::sync::mpsc::UnboundedReceiver<SpanRecord>,
    store: Arc<dyn ObservabilityStore>,
) {
    tokio::spawn(async move {
        let mut buffer = Vec::with_capacity(100);
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        interval.tick().await; // consume first immediate tick

        loop {
            tokio::select! {
                msg = receiver.recv() => {
                    match msg {
                        Some(record) => {
                            buffer.push(record);
                            if buffer.len() >= 100 {
                                let batch = std::mem::take(&mut buffer);
                                if let Err(e) = store.insert_spans(batch).await {
                                    tracing::error!("Failed to insert spans: {}", e);
                                }
                            }
                        }
                        None => {
                            // Channel closed — flush remaining and exit.
                            if !buffer.is_empty() {
                                let batch = std::mem::take(&mut buffer);
                                let _ = store.insert_spans(batch).await;
                            }
                            break;
                        }
                    }
                }
                _ = interval.tick() => {
                    if !buffer.is_empty() {
                        let batch = std::mem::take(&mut buffer);
                        if let Err(e) = store.insert_spans(batch).await {
                            tracing::error!("Failed to insert spans: {}", e);
                        }
                    }
                }
            }
        }
    });
}

/// Build a `SpanCollectorLayer` wired to the observability store and spawn the
/// batching bridge task. Convenience combiner over [`build_layer_and_receiver`]
/// + [`spawn_bridge`] for callers that have a store ready at subscriber-install
/// time.
pub fn build_observability_layer(store: Arc<dyn ObservabilityStore>) -> SpanCollectorLayer {
    let (layer, receiver) = build_layer_and_receiver();
    spawn_bridge(receiver, store);
    layer
}

/// Build the EnvFilter used for the observability layer. Exported so callers
/// composing their own subscriber can attach it alongside the layer returned
/// by [`build_observability_layer`].
pub fn observability_filter() -> EnvFilter {
    build_observability_filter()
}

/// Spawn a background task that periodically deletes observability event
/// data older than [`crate::duration::RETENTION_DAYS`]. Runs every 6 hours.
///
/// The retention window is derived from the longest finite duration the UI
/// exposes (see `crate::duration`), so the UI and retention stay in lockstep
/// automatically — there is no separate env var to keep in sync.
pub fn spawn_retention_cleanup(store: Arc<dyn ObservabilityStore>) {
    let retention_days = crate::duration::RETENTION_DAYS;

    tracing::info!(
        "Observability retention: {} days (cleanup every 6h)",
        retention_days
    );

    tokio::spawn(async move {
        // First cleanup after 60s — gives the app time to finish startup so
        // the DELETE doesn't race with migrations or schema setup.
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;

        let mut interval = tokio::time::interval(std::time::Duration::from_secs(6 * 3600));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            // `tokio::time::interval` fires its first tick immediately, which
            // acts as our "run now" signal after the 60s startup sleep. On
            // subsequent iterations this waits the full 6h period. Placing
            // the tick at the top of the loop is the standard Tokio periodic-
            // task pattern — putting the work first would cause a double
            // execution because the immediate first tick would resolve right
            // after the first purge completes.
            interval.tick().await;

            match store.purge_older_than(retention_days).await {
                Ok(0) => tracing::debug!("Retention cleanup: no rows purged"),
                Ok(n) => tracing::info!(
                    "Retention cleanup: purged {} observability rows older than {}d",
                    n,
                    retention_days
                ),
                Err(e) => tracing::warn!("Retention cleanup failed: {}", e),
            }
        }
    });
}

/// Initialize observability with a backend-agnostic store.
///
/// Sets up a tracing subscriber with:
/// - A console `fmt::layer()` filtered by `OXY_LOG_LEVEL`
/// - A `SpanCollectorLayer` filtered by `OXY_OBSERVABILITY_LOG_LEVEL` that writes
///   span records to the observability store.
///
/// Use [`build_observability_layer`] if you need to compose with other layers
/// (e.g. Sentry, file appender).
pub fn init_observability(store: Arc<dyn ObservabilityStore>) {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_filter(build_env_filter()))
        .with(build_observability_layer(store).with_filter(build_observability_filter()))
        .init();

    tracing::debug!("Observability initialized");
}

/// Initialize stdout logging only (no observability export)
pub fn init_stdout() {
    tracing_subscriber::registry()
        .with(build_env_filter())
        .with(tracing_subscriber::fmt::layer())
        .init();
}

/// Shutdown observability.
///
/// The actual flush/shutdown is handled by `ObservabilityStore::shutdown()`
/// called from the application entrypoint.
pub fn shutdown() {
    tracing::debug!("Observability shutdown requested");
}
