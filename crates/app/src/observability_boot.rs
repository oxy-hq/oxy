//! Boot-time observability wiring.
//!
//! Observability needs the tracing subscriber installed *early* (before CLI
//! dispatch) so every span from startup is captured, but the backend store
//! needs its ClickHouse endpoint, which `oxy start` only boots *after*
//! startup begins. We bridge this gap by:
//!
//! 1. In `main.rs`, create the SpanCollectorLayer + its channel and install
//!    the layer into the subscriber. Stash the receiver in [`stash_receiver`].
//! 2. Later, in `serve.rs` — by which point `OXY_CLICKHOUSE_*` is set for both
//!    paths (externally for `oxy serve`, by `oxy start` once its container is
//!    ready) — call [`finalize`] to resolve the backend, spawn the bridge
//!    task, and register the global store.
//!
//! Spans emitted between step 1 and step 2 accumulate in the unbounded channel
//! and get flushed as soon as the bridge spawns.

use std::sync::Arc;
use std::sync::Mutex;

use once_cell::sync::OnceCell;
use oxy::theme::StyledText;
use oxy_observability::{ObservabilityStore, SpanRecord};
use tokio::sync::mpsc::UnboundedReceiver;

static PENDING_RECEIVER: OnceCell<Mutex<Option<UnboundedReceiver<SpanRecord>>>> = OnceCell::new();

/// Stash the `SpanCollectorLayer` receiver created in `main.rs` so the serve
/// path can pick it up once the store is ready.
pub fn stash_receiver(rx: UnboundedReceiver<SpanRecord>) {
    let cell = PENDING_RECEIVER.get_or_init(|| Mutex::new(None));
    let mut guard = cell.lock().expect("observability receiver mutex poisoned");
    if guard.is_some() {
        tracing::warn!("observability receiver already stashed; replacing");
    }
    *guard = Some(rx);
}

/// Take the stashed receiver, if any. Returns `None` when no layer was
/// installed (OXY_OBSERVABILITY_BACKEND was unset at startup) or when the
/// receiver was already taken. Panics on poison, symmetric with
/// [`stash_receiver`] — silently swallowing poison would make `finalize()`
/// a no-op and hide the underlying bug.
fn take_receiver() -> Option<UnboundedReceiver<SpanRecord>> {
    let cell = PENDING_RECEIVER.get()?;
    cell.lock()
        .expect("observability receiver mutex poisoned")
        .take()
}

/// Resolve the observability backend from env. Strictly honors
/// `OXY_OBSERVABILITY_BACKEND` — no default, no silent fallbacks. When the env
/// var is unset, observability is disabled entirely. ClickHouse is the sole
/// backend; removed labels get a migration error via
/// [`oxy_observability::backends::validate_backend_label`].
/// Returns the store + a human-readable status message.
async fn resolve_backend() -> (Option<Arc<dyn ObservabilityStore>>, Option<String>) {
    let Ok(backend) = std::env::var("OXY_OBSERVABILITY_BACKEND") else {
        return (None, None);
    };

    if let Err(e) = oxy_observability::backends::validate_backend_label(&backend) {
        // Deliberately non-fatal: a stale telemetry label should not take the
        // product down. But an explicitly-set-yet-invalid value is a stronger
        // signal than an unset one, so it goes to the structured log (where
        // cloud alerting can see it) as well as stderr.
        tracing::error!(backend = %backend, "{e}");
        eprintln!("{}", e.to_string().error());
        return (None, None);
    }

    open_clickhouse_store().await
}

/// Open the ClickHouse observability store from `OXY_CLICKHOUSE_*` env,
/// ensure its schema, and apply retention TTL. Shared by the serve boot path
/// and standalone CLI commands ([`crate::observability_setup`]). Errors are
/// printed loudly and yield `None` — callers decide whether that is fatal.
pub(crate) async fn open_clickhouse_store() -> (Option<Arc<dyn ObservabilityStore>>, Option<String>)
{
    match oxy_observability::backends::clickhouse::ClickHouseObservabilityStorage::from_env().await
    {
        Ok(storage) => match storage.ensure_schema().await {
            Ok(()) => {
                let retention_days = oxy_observability::RETENTION_DAYS;
                match storage.apply_retention_ttl(retention_days).await {
                    // Retention is ClickHouse's job from here on: the TTL is
                    // enforced by background merges, so there is no purge loop
                    // to run or monitor.
                    Ok(()) => tracing::info!(
                        "Observability retention: {retention_days} days (ClickHouse TTL)"
                    ),
                    Err(e) => eprintln!("{}", format!("ClickHouse TTL apply failed: {e}").error()),
                }
                (
                    Some(Arc::new(storage) as Arc<dyn ObservabilityStore>),
                    Some("Observability: clickhouse (OXY_CLICKHOUSE_URL)".to_string()),
                )
            }
            Err(e) => {
                eprintln!("{}", format!("ClickHouse schema init failed: {e}").error());
                (None, None)
            }
        },
        Err(e) => {
            eprintln!("{}", format!("ClickHouse init failed: {e}").error());
            (None, None)
        }
    }
}

/// Resolve the backend, spawn the bridge task against the stashed receiver,
/// and register the global store.
///
/// Called from `serve.rs` once `OXY_CLICKHOUSE_*` is guaranteed set. Safe to
/// call when no receiver was stashed (OXY_OBSERVABILITY_BACKEND unset) — it
/// becomes a no-op.
///
/// Lifetime contract: if `start_server_and_web_app` bails before reaching
/// this point (e.g. migrations fail), the stashed receiver and tracing
/// sender stay alive for the rest of the process lifetime, buffering spans
/// into an unbounded channel. This is benign in practice because startup
/// failures exit the process quickly; [`shutdown`] explicitly drops the
/// receiver so the accumulated buffer is released on clean exit.
pub async fn finalize() {
    let Some(receiver) = take_receiver() else {
        return;
    };

    let (store, msg) = resolve_backend().await;
    let Some(store) = store else {
        // Backend resolution failed (loud error already printed). Drop the
        // receiver so the unbounded channel stops buffering indefinitely.
        drop(receiver);
        return;
    };

    if let Some(msg) = msg {
        tracing::info!("{msg}");
    }

    oxy_observability::spawn_bridge(receiver, Arc::clone(&store));
    oxy_observability::global::set_global(store);
}

/// Shut down the global observability store, if set. Also drops any
/// receiver left in [`PENDING_RECEIVER`] — this only happens when startup
/// failed before [`finalize`] ran, but we release the buffered channel
/// here so it doesn't outlive the store.
pub async fn shutdown() {
    let _ = take_receiver();
    if let Some(store) = oxy_observability::global::get_global() {
        store.shutdown().await;
    }
    oxy_observability::shutdown();
}
