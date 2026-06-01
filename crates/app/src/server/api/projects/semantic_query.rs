//! `POST /api/projects/{project_id}/semantic-query` — semantic-layer
//! proxy for customer-app bundles.
//!
//! Bundle authors stop hand-rolling raw SQL against view-defined
//! measures. Instead they reference the topic + dimensions + measures
//! and let airlayer compile to dialect-specific SQL. When the data
//! team refactors the SQL behind a measure, the bundle picks up the
//! change without an edit.
//!
//! Pipeline:
//!   1. Shared customer-app gates (auth → origin → workspace → org).
//!   2. Versioned body parse (`v: 1` honored; absent = v1 backcompat).
//!   3. Airlayer compile via `agentic_semantic::resolve_and_compile` —
//!      same code path the IDE's `/semantic/compile` uses, so bundles
//!      stay in lockstep with what the rest of oxy renders.
//!   4. Execute through the same connector layer as `/query` (row cap,
//!      typed-stream conversion).
//!
//! Body shape matches `agentic_semantic::SemanticQueryConfig`:
//! `{ topic, dimensions[], measures[], filters[], time_dimensions[],
//!    orders[], limit?, offset? }`. We deliberately reuse the existing
//! type rather than declare a parallel one — keeping the bundle's
//! semantic-query shape identical to the IDE's prevents the surface
//! from drifting.

use std::sync::Arc;

use agentic_connector::{ConnectorError, DatabaseConnector};
use agentic_semantic::compile::{CompiledQuery, resolve_and_compile};
use agentic_semantic::config::SemanticQueryConfig;
use axum::Json;
use axum::extract::{Path, Query as AxumQuery};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use oxy_shared::errors::OxyError;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tracing::{error, instrument, warn};
use uuid::Uuid;

use crate::server::api::customer_apps_gates::{check_customer_app_gates, parse_versioned_body};
use crate::server::api::projects::query::{QueryResponse, json_objects_to_table};
use crate::server::api::typed_stream::typed_stream_to_json_objects;

const MAX_ROWS: usize = 10_000;

#[derive(Serialize)]
struct ApiErr {
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'static str>,
}

fn err(status: StatusCode, msg: impl Into<String>) -> Response {
    (
        status,
        Json(ApiErr {
            message: msg.into(),
            code: None,
        }),
    )
        .into_response()
}

fn err_with_code(status: StatusCode, msg: impl Into<String>, code: &'static str) -> Response {
    (
        status,
        Json(ApiErr {
            message: msg.into(),
            code: Some(code),
        }),
    )
        .into_response()
}

#[derive(Debug, Deserialize, Default)]
pub struct DebugQuery {
    /// When `1`, response includes the compiled SQL string. Off by
    /// default so production responses don't leak warehouse SQL
    /// shape to the browser (compile output may include column
    /// expressions that an operator hasn't seen). Bundle authors
    /// flip this on while debugging.
    #[serde(default)]
    debug: Option<u8>,
}

/// Customer-app `/semantic-query` response. Extends `QueryResponse`
/// with an optional `sql` field that's only populated when
/// `?debug=1` is passed.
#[derive(Debug, Serialize)]
pub struct SemanticQueryResponse {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<JsonValue>>,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql: Option<String>,
}

#[instrument(skip_all, fields(project_id = %project_id))]
pub async fn run_semantic_query(
    Path(project_id): Path<Uuid>,
    AxumQuery(debug): AxumQuery<DebugQuery>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    // 1. Shared gates.
    let ctx = match check_customer_app_gates(&headers, project_id).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    // 2. Parse versioned body.
    let req: SemanticQueryConfig = match parse_versioned_body(&body) {
        Ok(r) => r,
        Err(resp) => return resp,
    };

    // 3. Body validation. The compile step would also reject these
    //    but we want sharper error codes the SDK can pattern-match.
    if req.topic.as_deref().unwrap_or("").trim().is_empty() {
        return err_with_code(
            StatusCode::BAD_REQUEST,
            "`topic` is required",
            "semantic_topic_missing",
        );
    }
    if req.dimensions.is_empty() && req.measures.is_empty() && req.time_dimensions.is_empty() {
        return err_with_code(
            StatusCode::BAD_REQUEST,
            "at least one of dimensions, measures, or time_dimensions must be non-empty",
            "semantic_selection_empty",
        );
    }

    // 4. Build workspace context — needed for the semantic scan path
    //    and the database connector.
    let proj_ctx = match ctx.build_project_context().await {
        Ok(ctx) => ctx,
        Err(resp) => return resp,
    };

    // 5. Compile via airlayer. Off-thread because compile is
    //    blocking-CPU work (parses every .view.yml / .topic.yml under
    //    the workspace scan path); same pattern the IDE handler uses.
    let scan_path = proj_ctx
        .workspace_manager()
        .config_manager
        .semantics_scan_path();
    let databases: Vec<airlayer::DatabaseConfig> = proj_ctx
        .workspace_manager()
        .config_manager
        .list_databases()
        .iter()
        .map(|db| airlayer::DatabaseConfig {
            name: db.name.clone(),
            db_type: db.database_type.to_string(),
        })
        .collect();

    let req_clone = req;
    let compiled = match tokio::task::spawn_blocking(move || {
        resolve_and_compile(&scan_path, &databases, &req_clone, None, 0)
    })
    .await
    {
        Ok(Ok(c)) => c,
        Ok(Err(e)) => {
            // Airlayer compile errors are caller-input problems —
            // unknown topic, dimension typo, malformed filter. Map
            // to 400 with the airlayer message in the detail so the
            // bundle author sees exactly what to fix.
            warn!(error = %e, "semantic-query compile failed");
            return err_with_code(
                StatusCode::BAD_REQUEST,
                e.to_string(),
                "semantic_compile_failed",
            );
        }
        Err(e) => {
            error!("semantic compile task panicked: {e}");
            return err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "semantic compile task panicked",
            );
        }
    };

    // We pass `preagg=None` to `resolve_and_compile` above, so we
    // should only ever see the `Warehouse` arm here. Treat
    // `Preaggregation` defensively (fall back to the warehouse SQL
    // it carries) in case airlayer ever decides to short-circuit
    // anyway — bundles should always see the warehouse path.
    let (sql, database_name) = match compiled {
        CompiledQuery::Warehouse { sql, database_name } => (sql, database_name),
        CompiledQuery::Preaggregation {
            warehouse_sql,
            warehouse_database,
            ..
        } => (warehouse_sql, warehouse_database),
    };

    // 6. Resolve connector for the database the topic compiled
    //    against. airlayer's resolver may pick a different db than
    //    the project's default if the topic's first view declares
    //    `datasource:` — honor that decision.
    let connector = match proj_ctx.build_connector_for(&database_name).await {
        Ok(c) => c,
        Err(OxyError::ConfigurationError(msg)) => {
            return err(StatusCode::BAD_REQUEST, msg);
        }
        Err(e) => {
            error!("connector build failed: {e}");
            return err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("could not resolve database '{database_name}'"),
            );
        }
    };

    // 7. Execute. Same outer-LIMIT wrap pattern as `/query` so a
    //    semantic query that produces millions of rows still respects
    //    the row cap.
    let limited_sql = wrap_with_limit(&sql, MAX_ROWS);
    let response = match execute_compiled_sql(connector, &limited_sql).await {
        Ok(r) => r,
        Err(resp) => return resp,
    };

    // Airlayer emits column aliases as `view__member` (dot in the
    // member path → double underscore in the result column).
    // Bundle authors think in member names (`store_id`,
    // `total_store_sales`), not view-prefixed names. Strip the
    // `view__` prefix when doing so doesn't introduce a collision
    // with another column in the same row. Multi-view queries that
    // would collide keep the qualified form so the bundle never sees
    // ambiguous data.
    let (columns, rows) = strip_view_prefix(response.columns, response.rows);

    let include_sql = matches!(debug.debug, Some(1));
    let semantic_response = SemanticQueryResponse {
        columns,
        rows,
        truncated: response.truncated,
        sql: if include_sql { Some(sql) } else { None },
    };

    Json(semantic_response).into_response()
}

/// Rewrite columns that look like `view__member` to bare `member`,
/// unless renaming would collide with another column. Bundle author
/// gets `row.store_id` instead of `row.store_performance__store_id`
/// in the common single-view case; multi-view collisions keep the
/// qualified form. Rows are remapped in lockstep so column index
/// invariants stay intact.
fn strip_view_prefix(
    columns: Vec<String>,
    rows: Vec<Vec<JsonValue>>,
) -> (Vec<String>, Vec<Vec<JsonValue>>) {
    // First pass: count how many times each candidate bare name
    // would appear. A bare name with >1 contributor means at least
    // two columns would collide — keep both qualified.
    let mut bare_counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for col in &columns {
        let bare = bare_member_name(col).unwrap_or(col.as_str());
        *bare_counts.entry(bare).or_insert(0) += 1;
    }

    let new_columns: Vec<String> = columns
        .iter()
        .map(|col| match bare_member_name(col) {
            Some(bare) if bare_counts.get(bare).copied().unwrap_or(0) == 1 => bare.to_string(),
            _ => col.clone(),
        })
        .collect();
    (new_columns, rows)
}

/// Parse an airlayer column alias of the form `view__member` and
/// return the `member` part. Returns `None` when the column doesn't
/// match the pattern (e.g. it was already bare, or contains multiple
/// `__` segments — which airlayer doesn't produce, but we're
/// defensive).
fn bare_member_name(col: &str) -> Option<&str> {
    let (_view, member) = col.split_once("__")?;
    // If the member half itself contains `__`, abort the strip —
    // the column shape doesn't match the expected `view__member`
    // layout and we'd be guessing.
    if member.contains("__") {
        return None;
    }
    Some(member)
}

/// Wrap compiled SQL in `SELECT * FROM (...) LIMIT N`. Same pattern
/// `query.rs` uses — keeps the row cap consistent across endpoints
/// without forcing airlayer to know about it.
fn wrap_with_limit(sql: &str, max_rows: usize) -> String {
    let sql_trimmed = sql.trim_end().trim_end_matches(';').trim_end();
    format!("SELECT * FROM (\n{sql_trimmed}\n) AS oxy_semantic_query LIMIT {max_rows}")
}

async fn execute_compiled_sql(
    connector: Arc<dyn DatabaseConnector>,
    sql: &str,
) -> Result<QueryResponse, Response> {
    let stream = match connector.execute_query_full(sql).await {
        Ok(s) => s,
        Err(ConnectorError::QueryFailed(detail)) => {
            warn!(detail = ?detail.message, "semantic-query: warehouse query failed");
            return Err(err(
                StatusCode::BAD_REQUEST,
                "semantic query failed; see server logs for details",
            ));
        }
        Err(ConnectorError::ConnectionError(msg)) => {
            error!(msg = ?msg, "semantic-query: warehouse connection failed");
            return Err(err(StatusCode::BAD_GATEWAY, "warehouse connection failed"));
        }
        Err(ConnectorError::Other(msg)) => {
            error!(msg = ?msg, "semantic-query: execution error");
            return Err(err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "semantic query execution failed",
            ));
        }
    };

    let objects = match typed_stream_to_json_objects(stream).await {
        Ok(rows) => rows,
        Err(e) => {
            error!("row conversion failed: {e}");
            return Err(err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to convert query results",
            ));
        }
    };

    let truncated = objects.len() == MAX_ROWS;
    Ok(json_objects_to_table(objects, truncated))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_with_limit_appends_outer_limit() {
        let wrapped = wrap_with_limit("SELECT 1", 100);
        assert!(wrapped.starts_with("SELECT * FROM (\nSELECT 1\n)"));
        assert!(wrapped.ends_with("LIMIT 100"));
    }

    #[test]
    fn wrap_with_limit_strips_trailing_semicolon() {
        let wrapped = wrap_with_limit("SELECT 1;", 100);
        assert!(!wrapped.contains("SELECT 1;"));
    }

    #[test]
    fn strip_view_prefix_simplifies_single_view() {
        let cols = vec![
            "store_performance__store_id".to_string(),
            "store_performance__total_sales".to_string(),
        ];
        let rows = vec![vec![JsonValue::from(1), JsonValue::from(100.0)]];
        let (new_cols, new_rows) = strip_view_prefix(cols, rows);
        assert_eq!(new_cols, vec!["store_id", "total_sales"]);
        // Rows are unchanged in order; column positions match.
        assert_eq!(new_rows[0][0], JsonValue::from(1));
        assert_eq!(new_rows[0][1], JsonValue::from(100.0));
    }

    #[test]
    fn strip_view_prefix_keeps_collisions_qualified() {
        // Two views both contribute a `store_id` dimension → keep
        // both qualified so the bundle gets unambiguous data.
        let cols = vec![
            "sales__store_id".to_string(),
            "inventory__store_id".to_string(),
        ];
        let rows = vec![vec![JsonValue::from(1), JsonValue::from(2)]];
        let (new_cols, _) = strip_view_prefix(cols.clone(), rows);
        assert_eq!(new_cols, cols, "collisions must keep view prefix");
    }

    #[test]
    fn strip_view_prefix_leaves_already_bare_alone() {
        let cols = vec!["count".to_string(), "id".to_string()];
        let rows = vec![vec![JsonValue::from(1), JsonValue::from(2)]];
        let (new_cols, _) = strip_view_prefix(cols.clone(), rows);
        assert_eq!(new_cols, cols);
    }

    #[test]
    fn strip_view_prefix_mixed_strips_only_uncollided() {
        let cols = vec![
            "sales__amount".to_string(),   // bare `amount` available, no collision
            "sales__store_id".to_string(), // bare `store_id` would collide with next
            "inventory__store_id".to_string(),
        ];
        let rows = vec![vec![
            JsonValue::from(10),
            JsonValue::from(1),
            JsonValue::from(2),
        ]];
        let (new_cols, _) = strip_view_prefix(cols, rows);
        assert_eq!(
            new_cols,
            vec!["amount", "sales__store_id", "inventory__store_id"]
        );
    }

    #[test]
    fn bare_member_name_handles_unexpected_shapes() {
        assert_eq!(bare_member_name("view__member"), Some("member"));
        assert_eq!(bare_member_name("no_underscore"), None);
        // Defensive: airlayer doesn't emit triple-underscored names,
        // but if it ever does, the parser bails out rather than guess.
        assert_eq!(bare_member_name("view__weird__name"), None);
    }
}
