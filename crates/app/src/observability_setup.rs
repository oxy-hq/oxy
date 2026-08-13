//! Observability store access for standalone CLI commands (e.g.
//! `oxy intent cluster`) that need the `ObservabilityStore` without a
//! running server.
//!
//! ClickHouse is the sole backend; resolution rules live in
//! [`crate::observability_boot`] so CLI commands stay aligned with server
//! behaviour.

use std::env;
use std::sync::Arc;

use oxy_shared::errors::OxyError;

/// Resolve the observability store (ClickHouse from `OXY_CLICKHOUSE_*` env).
///
/// Mirrors the serve path: observability is opt-in, so an *unset*
/// `OXY_OBSERVABILITY_BACKEND` is "not configured" rather than a dial of
/// ClickHouse's default endpoint that fails with a connection error. A legacy
/// value (`duckdb` / `postgres` / `airhouse`) yields the migration error.
///
/// Every failure is returned rather than printed — the CLI command that asked
/// for the store surfaces it once, so there is no doubled message.
pub async fn resolve_observability_backend()
-> Result<Arc<dyn oxy_observability::ObservabilityStore>, OxyError> {
    let backend = env::var("OXY_OBSERVABILITY_BACKEND").map_err(|_| {
        OxyError::ConfigurationError(
            "Observability is not configured: OXY_OBSERVABILITY_BACKEND is not set. \
             Set it to clickhouse (and OXY_CLICKHOUSE_URL) to use this command."
                .into(),
        )
    })?;
    oxy_observability::backends::validate_backend_label(&backend)?;

    // Detail of an init failure (bad URL, unreachable host, schema error) is
    // printed by the shared constructor; this is the actionable summary.
    let (store, _msg) = crate::observability_boot::open_clickhouse_store().await;
    store.ok_or_else(|| {
        OxyError::RuntimeError(
            "Could not initialize observability storage (check OXY_CLICKHOUSE_URL / \
             OXY_CLICKHOUSE_USER / OXY_CLICKHOUSE_PASSWORD / OXY_CLICKHOUSE_DATABASE)"
                .into(),
        )
    })
}

/// Ensure the global `ObservabilityStore` is initialized. No-op if it is
/// already set (e.g. when running inside `oxy serve --enterprise`).
///
/// Standalone CLI commands that query the store (intent classification,
/// metric analytics) call this before touching `oxy_observability::global`.
pub async fn ensure_global_store_initialized() -> Result<(), OxyError> {
    if oxy_observability::global::get_global().is_some() {
        return Ok(());
    }
    oxy_observability::global::set_global(resolve_observability_backend().await?);
    Ok(())
}
