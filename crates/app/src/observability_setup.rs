//! Observability backend resolution, shared by the `oxy serve`/`oxy start`
//! entry points and standalone CLI commands (e.g. `oxy intent cluster`)
//! that need to reach the `ObservabilityStore` without a running server.
//!
//! Keeps the resolution rules in one place so CLI commands stay aligned
//! with server behaviour (DuckDB by default, Postgres when
//! `OXY_DATABASE_URL` is set, ClickHouse on explicit opt-in).

use std::env;
use std::sync::Arc;

use oxy::state_dir::get_state_dir;
use oxy::theme::StyledText;

/// Resolve the observability backend, open the store, and return a
/// status message describing which backend was actually selected
/// (useful to print once the tracing subscriber is installed).
pub async fn resolve_observability_backend() -> (
    Option<Arc<dyn oxy_observability::ObservabilityStore>>,
    Option<String>,
) {
    let requested = env::var("OXY_OBSERVABILITY_BACKEND").ok();
    let has_db_url = env::var("OXY_DATABASE_URL").is_ok();
    let backend = requested.clone().unwrap_or_else(|| {
        if has_db_url {
            "postgres".to_string()
        } else {
            "duckdb".to_string()
        }
    });

    match backend.as_str() {
        "duckdb" => {
            let db_path = get_state_dir().join("observability.duckdb");
            match oxy_observability::backends::duckdb::DuckDBStorage::open(&db_path) {
                Ok(storage) => (
                    Some(Arc::new(storage)),
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
                            Some(
                                Arc::new(storage) as Arc<dyn oxy_observability::ObservabilityStore>
                            ),
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
        _ => {
            if has_db_url {
                match oxy_observability::backends::postgres::PostgresObservabilityStorage::from_env(
                )
                .await
                {
                    Ok(storage) => (
                        Some(Arc::new(storage) as Arc<dyn oxy_observability::ObservabilityStore>),
                        Some("Observability: postgres (OXY_DATABASE_URL)".to_string()),
                    ),
                    Err(e) => {
                        eprintln!(
                            "{}",
                            format!("Postgres observability failed: {e}. Falling back to DuckDB.")
                                .error()
                        );
                        fallback_to_duckdb()
                    }
                }
            } else {
                let label = if requested.as_deref() == Some("postgres") {
                    "Observability: postgres → duckdb (OXY_DATABASE_URL not set)"
                } else {
                    "Observability: duckdb (no OXY_DATABASE_URL)"
                };
                let (store, _) = fallback_to_duckdb();
                (store, Some(label.to_string()))
            }
        }
    }
}

fn fallback_to_duckdb() -> (
    Option<Arc<dyn oxy_observability::ObservabilityStore>>,
    Option<String>,
) {
    let db_path = get_state_dir().join("observability.duckdb");
    match oxy_observability::backends::duckdb::DuckDBStorage::open(&db_path) {
        Ok(s) => (
            Some(Arc::new(s) as Arc<dyn oxy_observability::ObservabilityStore>),
            None,
        ),
        Err(e) => {
            eprintln!("{}", format!("DuckDB fallback failed: {e}").error());
            (None, None)
        }
    }
}

/// Ensure the global `ObservabilityStore` is initialized. No-op if it is
/// already set (e.g. when running inside `oxy serve --enterprise`).
///
/// Standalone CLI commands that query the store (intent classification,
/// metric analytics) call this before touching `oxy_observability::global`.
pub async fn ensure_global_store_initialized() -> Result<(), oxy_shared::errors::OxyError> {
    if oxy_observability::global::get_global().is_some() {
        return Ok(());
    }
    let (store, _msg) = resolve_observability_backend().await;
    let store = store.ok_or_else(|| {
        oxy_shared::errors::OxyError::RuntimeError(
            "Could not initialize observability storage (check OXY_DATABASE_URL / \
             OXY_OBSERVABILITY_BACKEND / OXY_CLICKHOUSE_URL)"
                .into(),
        )
    })?;
    oxy_observability::global::set_global(store);
    Ok(())
}
