//! Backend implementations of [`crate::store::ObservabilityStore`].
//!
//! ClickHouse is the sole supported backend. Observability telemetry is a
//! serving-type workload (continuous append ingest, fixed query shapes,
//! concurrent reads, TTL retention) and must live on an independent failure
//! domain from the data planes it observes — see the 2026-07-06 incident,
//! where the DuckDB-based backend's engine instance was poisoned by a single
//! query and took the debugging surface down with it.
//!
//! Former backends (`duckdb`, `postgres`, `airhouse`) were removed in favor
//! of ClickHouse; [`validate_backend_label`] turns their labels into
//! actionable migration errors.

use oxy_shared::errors::OxyError;

pub mod clickhouse;

/// The sole supported observability backend label for
/// `OXY_OBSERVABILITY_BACKEND`.
pub const BACKEND_CLICKHOUSE: &str = "clickhouse";

/// Backends that once existed and were removed in favor of ClickHouse.
const REMOVED_BACKENDS: &[&str] = &["duckdb", "postgres", "airhouse"];

/// Validate an `OXY_OBSERVABILITY_BACKEND` label.
///
/// Returns `Ok(())` only for [`BACKEND_CLICKHOUSE`]. Removed backends get a
/// migration message naming the replacement; anything else gets an
/// unknown-label message. Callers treat an unset variable as "observability
/// disabled" before reaching this function.
pub fn validate_backend_label(label: &str) -> Result<(), OxyError> {
    if label == BACKEND_CLICKHOUSE {
        return Ok(());
    }
    if REMOVED_BACKENDS.contains(&label) {
        return Err(OxyError::ConfigurationError(format!(
            "OXY_OBSERVABILITY_BACKEND='{label}' was removed; observability now runs on \
             ClickHouse only. Set OXY_OBSERVABILITY_BACKEND=clickhouse and configure \
             OXY_CLICKHOUSE_URL / OXY_CLICKHOUSE_USER / OXY_CLICKHOUSE_PASSWORD / \
             OXY_CLICKHOUSE_DATABASE (local `oxy start` provisions the container \
             automatically)."
        )));
    }
    Err(OxyError::ConfigurationError(format!(
        "Unknown OXY_OBSERVABILITY_BACKEND='{label}'. The only supported backend is \
         'clickhouse'; unset the variable to disable observability."
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clickhouse_is_accepted() {
        assert!(validate_backend_label("clickhouse").is_ok());
    }

    #[test]
    fn removed_backends_get_migration_guidance() {
        for removed in ["duckdb", "postgres", "airhouse"] {
            let err = validate_backend_label(removed)
                .expect_err("removed backend label must be rejected")
                .to_string();
            assert!(
                err.contains(removed) && err.contains("removed") && err.contains("clickhouse"),
                "error for '{removed}' must name the label, say it was removed, \
                 and point at clickhouse, got: {err}"
            );
            assert!(
                err.contains("OXY_CLICKHOUSE_URL"),
                "error for '{removed}' must name the config needed to migrate, got: {err}"
            );
        }
    }

    #[test]
    fn unknown_labels_are_rejected() {
        let err = validate_backend_label("bogus")
            .expect_err("unknown label must be rejected")
            .to_string();
        assert!(
            err.contains("bogus") && err.contains("clickhouse"),
            "unknown-label error must echo the label and name the supported backend, got: {err}"
        );
    }

    #[test]
    fn labels_are_exact_match() {
        // Selection has always been exact/lowercase; "ClickHouse" is a typo,
        // not a synonym.
        assert!(validate_backend_label("ClickHouse").is_err());
    }
}
