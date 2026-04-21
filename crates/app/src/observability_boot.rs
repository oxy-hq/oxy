//! Boot-time observability wiring.
//!
//! Observability needs the tracing subscriber installed *early* (before CLI
//! dispatch) so every span from startup is captured, but the backend store
//! needs the database URL which `oxy start` only sets *after* it boots its
//! Postgres container. We bridge this gap by:
//!
//! 1. In `main.rs`, create the SpanCollectorLayer + its channel and install
//!    the layer into the subscriber. Stash the receiver in [`stash_receiver`].
//! 2. Later, in `serve.rs` — by which point `OXY_DATABASE_URL` is set for both
//!    `oxy start` and `oxy serve` paths — call [`finalize`] to resolve the
//!    backend, spawn the bridge task, register the global store, and start
//!    retention cleanup.
//!
//! Spans emitted between step 1 and step 2 accumulate in the unbounded channel
//! and get flushed as soon as the bridge spawns.

use std::sync::Arc;
use std::sync::Mutex;

use once_cell::sync::OnceCell;
use oxy::state_dir::get_state_dir;
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
/// var is unset, observability is disabled entirely.
/// Returns the store + a human-readable status message.
async fn resolve_backend() -> (Option<Arc<dyn ObservabilityStore>>, Option<String>) {
    let Ok(backend) = std::env::var("OXY_OBSERVABILITY_BACKEND") else {
        return (None, None);
    };

    match backend.as_str() {
        "duckdb" => {
            let db_path = get_state_dir().join("observability.duckdb");
            match oxy_observability::backends::duckdb::DuckDBStorage::open(&db_path) {
                Ok(storage) => (
                    Some(Arc::new(storage) as Arc<dyn ObservabilityStore>),
                    Some(format!("Observability: duckdb ({})", db_path.display())),
                ),
                Err(e) => {
                    eprintln!(
                        "{}",
                        format!("Failed to open DuckDB observability: {e}").error()
                    );
                    (None, None)
                }
            }
        }
        "clickhouse" => {
            match oxy_observability::backends::clickhouse::ClickHouseObservabilityStorage::from_env(
            )
            .await
            {
                Ok(storage) => match storage.ensure_schema().await {
                    Ok(()) => {
                        if let Err(e) = storage
                            .apply_retention_ttl(oxy_observability::RETENTION_DAYS)
                            .await
                        {
                            eprintln!("{}", format!("ClickHouse TTL apply failed: {e}").error());
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
        "postgres" => {
            if std::env::var("OXY_DATABASE_URL").is_err() {
                eprintln!(
                    "{}",
                    "Observability: postgres backend selected but OXY_DATABASE_URL is not set. \
                     Set OXY_DATABASE_URL or choose a different backend via \
                     OXY_OBSERVABILITY_BACKEND=duckdb|clickhouse."
                        .error()
                );
                return (None, None);
            }
            match oxy_observability::backends::postgres::PostgresObservabilityStorage::from_env()
                .await
            {
                Ok(storage) => (
                    Some(Arc::new(storage) as Arc<dyn ObservabilityStore>),
                    Some("Observability: postgres (OXY_DATABASE_URL)".to_string()),
                ),
                Err(e) => {
                    eprintln!(
                        "{}",
                        format!("Failed to initialize Postgres observability: {e}").error()
                    );
                    (None, None)
                }
            }
        }
        other => {
            eprintln!(
                "{}",
                format!(
                    "Unknown OXY_OBSERVABILITY_BACKEND='{other}'. \
                     Valid values: duckdb, postgres, clickhouse."
                )
                .error()
            );
            (None, None)
        }
    }
}

/// Resolve the backend, spawn the bridge task against the stashed receiver,
/// register the global store, and kick off retention cleanup.
///
/// Called from `serve.rs` once `OXY_DATABASE_URL` is guaranteed set. Safe to
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
    oxy_observability::global::set_global(Arc::clone(&store));
    oxy_observability::spawn_retention_cleanup(store);
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
