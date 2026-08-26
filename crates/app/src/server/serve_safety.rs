//! Per-workspace "serve-safe" predicate for the conditional `/analytics` un-pin.
//!
//! A workspace is *serve-safe* when every configured database can be queried
//! WITHOUT the workspace working copy — so the stateless serve fleet can run the
//! analytics agent itself instead of reverse-proxying to the ide. The check is a
//! CONSERVATIVE allowlist: anything we can't positively classify as FS-free (a
//! raw local DuckDB, a local key file) is treated as NOT serve-safe, so the
//! worst case is an unnecessary proxy to the ide (always correct) — never a run
//! mis-served on a node that lacks the file.
//!
//! Gated by `OXY_ANALYTICS_FLEET_UNPIN` (default off) at the middleware call
//! site; this module is pure classification + the compile-boundary read.

use oxy::config::model::{Config, Database, DatabaseType, DuckDBOptions, SnowflakeAuthType};
use uuid::Uuid;

/// Whether the conditional `/analytics` fleet un-pin is enabled. Default OFF —
/// when unset the serve fleet proxies every `/analytics` request to the ide,
/// exactly as before, so enabling/rolling back is a single env flag.
pub fn analytics_fleet_unpin_enabled() -> bool {
    std::env::var("OXY_ANALYTICS_FLEET_UNPIN")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Extract the workspace id from an `/api/{workspace_id}/analytics/...` path, or
/// `None` for any other path. Scopes the un-pin to analytics routes only —
/// every other IdeOnly route (and the similarly-named `/analytics-workflows`)
/// keeps proxying.
pub fn analytics_workspace_id(path: &str) -> Option<Uuid> {
    let rest = path.strip_prefix("/api/")?;
    let (ws, tail) = rest.split_once('/')?;
    if tail != "analytics" && !tail.starts_with("analytics/") {
        return None;
    }
    Uuid::parse_str(ws).ok()
}

/// True when querying `db` needs no workspace working copy on the serving node.
/// Exhaustive over `DatabaseType` (no wildcard) so a new database kind forces an
/// explicit serve-safety decision here rather than defaulting silently.
fn database_is_serve_safe(db: &Database) -> bool {
    match &db.database_type {
        // Remote / managed connectors: nothing lives on the local working copy.
        DatabaseType::Postgres(_)
        | DatabaseType::Redshift(_)
        | DatabaseType::Mysql(_)
        | DatabaseType::ClickHouse(_)
        | DatabaseType::DOMO(_)
        | DatabaseType::MotherDuck(_)
        | DatabaseType::Airhouse(_)
        | DatabaseType::AirhouseManaged(_)
        // Per-org OLTP: a remote managed Postgres, resolved from `oltp_tenants`.
        // Nothing about it lives on the working copy.
        | DatabaseType::PostgresManaged(_) => true,
        // DuckDB: serve-safe with a compiler-injected S3 mirror, or a natively
        // S3-backed DuckLake. A raw Local/File DuckDB without a mirror needs the
        // working tree's data files.
        DatabaseType::DuckDB(d) => {
            d.s3_mirror.is_some() || matches!(d.options, DuckDBOptions::DuckLake(_))
        }
        // BigQuery: a local key file pins to the FS; a key_path_var (secret) or
        // ambient credentials are serve-safe.
        DatabaseType::Bigquery(b) => b.key_path.is_none(),
        // Snowflake: a PrivateKey path is a local file; password / var / browser
        // auth are serve-safe.
        DatabaseType::Snowflake(s) => !matches!(s.auth_type, SnowflakeAuthType::PrivateKey { .. }),
    }
}

/// True when every database in `config` is serve-safe. An agent with no
/// databases has nothing to execute against and is trivially serve-safe.
fn config_is_serve_safe(config: &Config) -> bool {
    config.databases.iter().all(database_is_serve_safe)
}

/// True when the workspace's promoted compiled config has only serve-safe
/// databases — the fleet can run its analytics agent locally instead of proxying
/// to the ide. Reads the compile boundary; any miss / undeserialisable config /
/// DB error resolves to `false` (proxy — the always-correct default).
pub async fn workspace_is_serve_safe(workspace_id: Uuid) -> bool {
    match crate::server::api::compiled_reader::resolve_workspace_config(workspace_id, None).await {
        Ok(Some(value)) => match serde_json::from_value::<Config>(value) {
            Ok(config) => config_is_serve_safe(&config),
            Err(e) => {
                tracing::warn!(
                    workspace_id = %workspace_id, error = ?e,
                    "serve_safety: compiled config did not deserialise; not serve-safe (proxy)"
                );
                false
            }
        },
        // Not promoted / not compiled / transient DB error — can't prove
        // serve-safe, so proxy.
        Ok(None) => false,
        Err(e) => {
            tracing::warn!(
                workspace_id = %workspace_id, error = ?e,
                "serve_safety: compiled config lookup failed; not serve-safe (proxy)"
            );
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxy::config::model::DuckDB;

    #[test]
    fn analytics_workspace_id_matches_only_analytics_paths() {
        let ws = "11111111-1111-1111-1111-111111111111";
        let parsed = Uuid::parse_str(ws).unwrap();
        // Analytics routes (bare + nested) yield the workspace id.
        assert_eq!(
            analytics_workspace_id(&format!("/api/{ws}/analytics/runs")),
            Some(parsed)
        );
        assert_eq!(
            analytics_workspace_id(&format!("/api/{ws}/analytics")),
            Some(parsed)
        );
        assert_eq!(
            analytics_workspace_id(&format!("/api/{ws}/analytics/runs/abc/answer")),
            Some(parsed)
        );
        // Non-analytics routes → None (they keep proxying), incl. the
        // similarly-named /analytics-workflows and a non-UUID segment.
        assert_eq!(analytics_workspace_id(&format!("/api/{ws}/threads")), None);
        assert_eq!(
            analytics_workspace_id(&format!("/api/{ws}/analytics-workflows/x")),
            None
        );
        assert_eq!(
            analytics_workspace_id("/api/not-a-uuid/analytics/runs"),
            None
        );
        assert_eq!(analytics_workspace_id("/healthz"), None);
    }

    #[test]
    fn local_duckdb_without_mirror_is_not_serve_safe() {
        let db = Database {
            name: "local".to_string(),
            database_type: DatabaseType::DuckDB(DuckDB {
                options: DuckDBOptions::Local {
                    file_search_path: ".db/".to_string(),
                },
                s3_mirror: None,
            }),
        };
        assert!(
            !database_is_serve_safe(&db),
            "a raw local DuckDB without an S3 mirror needs the working copy"
        );
    }
}
