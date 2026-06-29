//! Semantic-layer endpoints for the IDE:
//!
//! - `GET /semantic/topic/{pathb64}` — parse one `.topic.yml` and hydrate its views.
//! - `GET /semantic/view/{pathb64}` — parse one `.view.yml`.
//! - `POST /semantic/compile` — compile a `{ topic, dimensions, measures, … }`
//!   query into dialect-specific SQL via airlayer.
//! - `POST /semantic` — compile **and execute** the same query, returning
//!   rows (JSON) or a parquet file handle. Used by the IDE's "Run" button.
//!
//! Compile + execute both go through `agentic_automation::semantic_bridge`
//! and `agentic_connector` — same code paths the agentic pipeline's
//! `semantic_query` step uses, so IDE results stay in lockstep with
//! runtime results without re-introducing `oxy-workflow`.

use airlayer::engine::promotions::Promotions;
use axum::{
    extract::{self, Path},
    http::StatusCode,
};
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use oxy_semantic::parse_semantic_layer_from_dir;
use oxy_semantic::parser::{ParserConfig, SemanticLayerParser};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use agentic_semantic::compile::{CompiledQuery, resolve_and_compile};
use agentic_semantic::config::SemanticQueryConfig;
use oxy::adapters::session_filters::SessionFilters;
use oxy::adapters::workspace::manager::WorkspaceManager;
use oxy::config::model::ConnectionOverrides;
use oxy_auth::extractor::AuthenticatedUserExtractor;

use crate::server::api::data::{
    ResultFormat, SQLParams, SemanticQueryResponse, SqlErrorResponse, SqlExecuteError,
    agentic_error_response, run_via_agentic_connector,
};
use crate::server::api::middlewares::workspace_context::{
    EffectiveWorkspaceRole, PreaggCacheCtx, SemanticLayerCacheCtx, WorkspaceManagerExtractor,
};
use crate::server::api::semantic_scan::{self, MaterialisedScan, SemanticEntity};

#[derive(Serialize, ToSchema)]
pub struct ErrorResponse {
    pub message: String,
}

#[derive(Deserialize)]
pub struct ViewPath {
    pub workspace_id: Uuid,
    pub file_path_b64: String,
}

#[derive(Deserialize)]
pub struct TopicPath {
    pub workspace_id: Uuid,
    pub file_path_b64: String,
}

#[derive(Serialize, Deserialize, Clone, ToSchema)]
pub struct ViewResponse {
    pub view_name: String,
    pub name: String,
    pub description: Option<String>,
    pub datasource: Option<String>,
    pub table: Option<String>,
    #[schema(value_type = Vec<Object>)]
    pub dimensions: Vec<serde_json::Value>,
    #[schema(value_type = Vec<Object>)]
    pub measures: Vec<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, ToSchema)]
pub struct TopicResponse {
    pub name: String,
    pub description: Option<String>,
    pub views: Vec<String>,
    pub base_view: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, ToSchema)]
pub struct TopicDetailsResponse {
    pub topic: TopicResponse,
    pub views: Vec<ViewResponse>,
}

/// Decode a base64-encoded workspace-relative file path, accepting both
/// the standard padded form (`...==`) and the unpadded form the IDE
/// historically used. Mirrors the same `path_b64` tolerance the
/// agentic-workflow file route applies — keeps the routes consistent
/// for clients that pick one form or the other.
fn decode_b64_path(
    file_path_b64: &str,
) -> Result<String, (StatusCode, extract::Json<ErrorResponse>)> {
    let decoded = BASE64_STANDARD
        .decode(file_path_b64)
        .or_else(|_| {
            // Tolerate unpadded base64 by retrying with the standard padding rule.
            let pad = (4 - (file_path_b64.len() % 4)) % 4;
            let padded = format!("{file_path_b64}{}", "=".repeat(pad));
            BASE64_STANDARD.decode(padded)
        })
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                extract::Json(ErrorResponse {
                    message: format!("Invalid base64 file path: {e}"),
                }),
            )
        })?;
    String::from_utf8(decoded).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            extract::Json(ErrorResponse {
                message: format!("Invalid UTF-8 in file path: {e}"),
            }),
        )
    })
}

pub async fn get_view_details(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    layer_cache: SemanticLayerCacheCtx,
    Path(ViewPath {
        workspace_id: _,
        file_path_b64,
    }): Path<ViewPath>,
) -> Result<extract::Json<ViewResponse>, (StatusCode, extract::Json<ErrorResponse>)> {
    let file_path_str = decode_b64_path(&file_path_b64)?;
    // Boundary first (a stateless serve replica has no working copy), FS fallback
    // in local / not-yet-promoted mode. `_guard` keeps the materialised tempdir
    // alive until parsing finishes.
    let (scan_path, view_file, _guard) = resolve_semantic_source(
        &workspace_manager,
        SemanticEntity::View,
        &file_path_str,
        "View",
    )
    .await?;

    let parser = SemanticLayerParser::new(ParserConfig::new(&scan_path));
    let view = parser.parse_view_file(&view_file).map_err(|e| {
        semantic_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to parse view file: {e}"),
        )
    })?;

    // Load the full airlayer semantic layer to compute the promotion closure.
    // Induced measures are defined on fine-grain views but become queryable at
    // every coarser-grain ancestor — they don't exist in the single-file YAML.
    // `scan_path` is the whole promoted scan (boundary tempdir or FS fallback),
    // so loading it gives the cross-view graph the promotion closure needs.
    let airlayer_layer = layer_cache
        .get_or_load(scan_path.clone())
        .await
        .map_err(|e| {
            semantic_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to load semantic layer: {e}"),
            )
        })?;
    let promotions = Promotions::build(&airlayer_layer.views).map_err(|e| {
        semantic_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to build promotion closure: {e}"),
        )
    })?;

    let mut measures = view
        .measures
        .map(|ms| {
            json_array(
                &ms.into_iter()
                    .filter(|m| !m.name.starts_with('_'))
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap_or_default();
    append_induced_measures(&mut measures, &view.name, &promotions, &airlayer_layer);

    Ok(extract::Json(ViewResponse {
        view_name: view.name.clone(),
        name: view.name,
        description: view.description,
        datasource: view.datasource,
        table: view.table,
        dimensions: json_array(&view.dimensions),
        measures,
    }))
}

pub async fn get_topic_details(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    layer_cache: SemanticLayerCacheCtx,
    Path(TopicPath {
        workspace_id: _,
        file_path_b64,
    }): Path<TopicPath>,
) -> Result<extract::Json<TopicDetailsResponse>, (StatusCode, extract::Json<ErrorResponse>)> {
    let file_path_str = decode_b64_path(&file_path_b64)?;
    // Boundary first, FS fallback. The WHOLE scan is materialised so the topic's
    // referenced views hydrate from the same dir (`parse_semantic_layer_from_dir`
    // below reads `semantics_path`). `_guard` holds the tempdir until then.
    let (semantics_path, topic_file, _guard) = resolve_semantic_source(
        &workspace_manager,
        SemanticEntity::Topic,
        &file_path_str,
        "Topic",
    )
    .await?;

    let parser = SemanticLayerParser::new(ParserConfig::new(&semantics_path));
    let topic = parser.parse_topic_file(&topic_file).map_err(|e| {
        semantic_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to parse topic file: {e}"),
        )
    })?;

    // Hydrate referenced views from the full semantic layer parse. A
    // missing reference is a real authoring error — return 400 with
    // the offending name rather than silently dropping the view, so the
    // IDE can surface it to the user.
    let parse_result = parse_semantic_layer_from_dir(&semantics_path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            extract::Json(ErrorResponse {
                message: format!("Failed to parse semantic layer: {e}"),
            }),
        )
    })?;

    let airlayer_layer = layer_cache
        .get_or_load(semantics_path.clone())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                extract::Json(ErrorResponse {
                    message: format!("Failed to load semantic layer: {e}"),
                }),
            )
        })?;
    let promotions = Promotions::build(&airlayer_layer.views).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            extract::Json(ErrorResponse {
                message: format!("Failed to build promotion closure: {e}"),
            }),
        )
    })?;

    let mut views_with_data = Vec::with_capacity(topic.views.len());
    for view_name in &topic.views {
        let Some(view) = parse_result
            .semantic_layer
            .views
            .iter()
            .find(|v| v.name == *view_name)
        else {
            return Err((
                StatusCode::BAD_REQUEST,
                extract::Json(ErrorResponse {
                    message: format!("Could not find view {view_name} in semantic layer"),
                }),
            ));
        };
        let visible_measures: Vec<_> = view
            .measures
            .as_ref()
            .map(|ms| ms.iter().filter(|m| !m.name.starts_with('_')).collect())
            .unwrap_or_default();
        let mut measures = json_array(&visible_measures);
        append_induced_measures(&mut measures, view_name, &promotions, &airlayer_layer);
        views_with_data.push(ViewResponse {
            view_name: view_name.clone(),
            name: view.name.clone(),
            description: view.description.clone(),
            datasource: view.datasource.clone(),
            table: view.table.clone(),
            dimensions: json_array(&view.dimensions),
            measures,
        });
    }

    Ok(extract::Json(TopicDetailsResponse {
        topic: TopicResponse {
            name: topic.name,
            description: topic.description,
            views: topic.views,
            base_view: topic.base_view,
        },
        views: views_with_data,
    }))
}

/// Resolve where to read a single view/topic: the compile boundary (materialised
/// into a tempdir) when the workspace is promoted, else the working-copy FS.
/// Returns `(scan_root, file_path, guard)` — HOLD `guard` until parsing finishes
/// (dropping it deletes the materialised tempdir).
async fn resolve_semantic_source(
    workspace_manager: &WorkspaceManager,
    entity: SemanticEntity,
    file_path_str: &str,
    label: &str,
) -> Result<
    (
        std::path::PathBuf,
        std::path::PathBuf,
        Option<MaterialisedScan>,
    ),
    (StatusCode, extract::Json<ErrorResponse>),
> {
    if let Some((scan, file)) = semantic_scan::materialise_semantic_entity(
        workspace_manager.workspace_id,
        entity,
        file_path_str,
    )
    .await
    .map_err(|e| {
        semantic_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to materialise semantic scan: {e}"),
        )
    })? {
        return Ok((scan.scan_path.clone(), file, Some(scan)));
    }

    let full_path_str = workspace_manager
        .config_manager
        .resolve_file(file_path_str)
        .await
        .map_err(|e| {
            semantic_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to resolve file path: {e}"),
            )
        })?;
    let full_path = std::path::PathBuf::from(full_path_str);
    if !full_path.exists() {
        return Err(semantic_err(
            StatusCode::NOT_FOUND,
            format!("{label} file {file_path_str} not found"),
        ));
    }
    Ok((
        workspace_manager.config_manager.semantics_scan_path(),
        full_path,
        None,
    ))
}

fn semantic_err(status: StatusCode, message: String) -> (StatusCode, extract::Json<ErrorResponse>) {
    (status, extract::Json(ErrorResponse { message }))
}

// ── Preagg status ─────────────────────────────────────────────────────────────

#[derive(serde::Deserialize, Serialize, Clone)]
pub struct ManifestMeasure {
    pub name: String,
    #[serde(rename = "type")]
    pub measure_type: String,
}

#[derive(serde::Deserialize)]
struct ManifestRollupEntry {
    view_name: String,
    rollup_name: String,
    file: String,
    #[serde(default)]
    dimensions: Vec<String>,
    #[serde(default)]
    measures: Vec<ManifestMeasure>,
    time_dimension: Option<String>,
    granularity: Option<String>,
    build_date: Option<String>,
    refresh_key_checked_at: Option<String>,
}

#[derive(serde::Deserialize)]
struct LocalManifestJson {
    rollups: Vec<ManifestRollupEntry>,
}

#[derive(Serialize, Clone)]
pub struct PreaggRollupStatus {
    pub view_name: String,
    pub rollup_name: String,
    pub has_parquet: bool,
    pub dimensions: Vec<String>,
    pub measures: Vec<ManifestMeasure>,
    pub time_dimension: Option<String>,
    pub granularity: Option<String>,
    pub build_date: Option<String>,
    pub refresh_key_checked_at: Option<String>,
}

#[derive(Serialize)]
pub struct PreaggStatusResponse {
    pub rollups: Vec<PreaggRollupStatus>,
}

/// Read the manifest and check parquet file presence on disk.
/// Extracted as a pure function for unit testability.
pub(crate) fn build_preagg_status(cache_dir: &std::path::Path) -> PreaggStatusResponse {
    let manifest_path = cache_dir.join("manifest.json");
    let rollups = std::fs::read_to_string(&manifest_path)
        .ok()
        .and_then(|s| serde_json::from_str::<LocalManifestJson>(&s).ok())
        .map(|manifest| {
            manifest
                .rollups
                .into_iter()
                .map(|entry| {
                    let parquet_path = cache_dir.join(&entry.file);
                    PreaggRollupStatus {
                        view_name: entry.view_name,
                        rollup_name: entry.rollup_name,
                        has_parquet: parquet_path.is_file(),
                        dimensions: entry.dimensions,
                        measures: entry.measures,
                        time_dimension: entry.time_dimension,
                        granularity: entry.granularity,
                        build_date: entry.build_date,
                        refresh_key_checked_at: entry.refresh_key_checked_at,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    PreaggStatusResponse { rollups }
}

/// `GET /{workspace_id}/semantic/preagg-status`
///
/// Returns the pre-aggregation cache status by reading `.airlayer/cache/manifest.json`.
/// Missing or unparsable manifest returns an empty rollup list — no error.
pub async fn get_preagg_status(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Path(WorkspacePath {
        workspace_id: _workspace_id,
    }): Path<WorkspacePath>,
) -> extract::Json<PreaggStatusResponse> {
    let workspace_path = workspace_manager
        .config_manager
        .workspace_path()
        .to_path_buf();
    let cache_dir = oxy::state_dir::get_airlayer_cache_dir(&workspace_path);
    extract::Json(build_preagg_status(&cache_dir))
}

#[cfg(test)]
mod preagg_tests {
    use super::*;

    #[test]
    fn manifest_with_existing_parquet_returns_has_parquet_true() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = dir.path().join(".airlayer").join("cache");
        std::fs::create_dir_all(&cache_dir).unwrap();

        let manifest_json = serde_json::json!({
            "pulled_at": "2026-05-11T00:00:00Z",
            "source_database": "test",
            "rollups": [{
                "view_name": "orders",
                "rollup_name": "by_month",
                "rollup_hash": "aabbccdd",
                "file": "orders__aabbccdd.parquet",
                "dimensions": [],
                "measures": [],
                "time_dimension": null,
                "granularity": null,
                "build_date": "2026-05-11"
            }]
        });
        std::fs::write(cache_dir.join("manifest.json"), manifest_json.to_string()).unwrap();
        std::fs::write(cache_dir.join("orders__aabbccdd.parquet"), b"").unwrap();

        let status = build_preagg_status(&cache_dir);
        assert_eq!(status.rollups.len(), 1);
        assert_eq!(status.rollups[0].view_name, "orders");
        assert!(status.rollups[0].has_parquet);
    }

    #[test]
    fn missing_parquet_returns_has_parquet_false() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = dir.path().join(".airlayer").join("cache");
        std::fs::create_dir_all(&cache_dir).unwrap();

        let manifest_json = serde_json::json!({
            "pulled_at": "2026-05-11T00:00:00Z",
            "source_database": "test",
            "rollups": [{
                "view_name": "orders",
                "rollup_name": "by_month",
                "rollup_hash": "aabbccdd",
                "file": "orders__aabbccdd.parquet",
                "dimensions": [],
                "measures": [],
                "time_dimension": null,
                "granularity": null,
                "build_date": "2026-05-11"
            }]
        });
        std::fs::write(cache_dir.join("manifest.json"), manifest_json.to_string()).unwrap();

        let status = build_preagg_status(&cache_dir);
        assert_eq!(status.rollups.len(), 1);
        assert!(!status.rollups[0].has_parquet);
    }

    #[test]
    fn missing_manifest_returns_empty_rollups() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = dir.path().join(".airlayer").join("cache");
        let status = build_preagg_status(&cache_dir);
        assert!(status.rollups.is_empty());
    }
}

/// Append induced (promoted) measures for `view_name` to `measures`.
///
/// Each entry mirrors the source measure's JSON shape with two extra fields:
/// `induced: true` and `promoted_from: "<source_view>"`. The frontend renders
/// them alongside explicit measures; callers can filter on `induced` for badges.
fn append_induced_measures(
    measures: &mut Vec<serde_json::Value>,
    view_name: &str,
    promotions: &Promotions,
    layer: &airlayer::SemanticLayer,
) {
    for im in promotions.induced_for_view(view_name) {
        let mut json = layer
            .views
            .iter()
            .find(|v| v.name == im.source_view)
            .and_then(|v| v.measures.as_ref())
            .and_then(|ms| ms.iter().find(|m| m.name == im.source_measure))
            .and_then(|m| serde_json::to_value(m).ok())
            .unwrap_or_else(|| serde_json::json!({ "name": im.source_measure }));

        if let serde_json::Value::Object(ref mut map) = json {
            map.insert("induced".to_string(), serde_json::Value::Bool(true));
            map.insert(
                "promoted_from".to_string(),
                serde_json::Value::String(im.source_view.clone()),
            );
        }

        measures.push(json);
    }
}

// ── World Model — filter/instance types ──────────────────────────────────────

/// Serialize anything to a `Vec<serde_json::Value>`, treating a
/// non-array serialization as empty. Lets the response shape stay
/// `Vec<Object>` regardless of whether the semantic model exposes the
/// field as an array or an option/scalar.
fn json_array<T: Serialize>(value: &T) -> Vec<serde_json::Value> {
    match serde_json::to_value(value).unwrap_or(serde_json::Value::Null) {
        serde_json::Value::Array(a) => a,
        _ => Vec::new(),
    }
}

// ── World Model — instance / filter endpoints ────────────────────────────────

#[derive(Serialize, ToSchema)]
pub struct SemanticQueryCompileResponse {
    pub sql: String,
}

/// Compile a semantic query into dialect-specific SQL without executing
/// it. Used by the IDE's SQL preview panel — `SemanticExplorerContext`
/// fires this as the user clicks dimensions / measures / filters so the
/// preview stays in sync.
///
/// The request body is the same flat shape the FE has always sent
/// (`{ topic, dimensions, measures, time_dimensions, filters, orders,
/// limit, … }`); extra fields the FE includes for the execute path
/// (`variables`, `session_filters`, `connections`, `result_format`) are
/// tolerated by serde and ignored — compile doesn't need them.
///
/// Compilation goes through `agentic_automation::semantic_bridge`, which
/// drives airlayer end-to-end. Same code path the agentic-pipeline's
/// `semantic_query` step uses, so the IDE preview and runtime stay in
/// lockstep.
pub async fn compile_semantic_query(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Path(WorkspacePath { workspace_id: _ }): Path<WorkspacePath>,
    extract::Json(query): extract::Json<SemanticQueryConfig>,
) -> Result<extract::Json<SemanticQueryCompileResponse>, (StatusCode, extract::Json<ErrorResponse>)>
{
    let scan_path = workspace_manager.config_manager.semantics_scan_path();
    let databases: Vec<airlayer::DatabaseConfig> = workspace_manager
        .config_manager
        .list_databases()
        .iter()
        .map(|db| airlayer::DatabaseConfig {
            name: db.name.clone(),
            db_type: db.database_type.to_string(),
        })
        .collect();

    let compiled = tokio::task::spawn_blocking(move || {
        // Compile to warehouse SQL only (no preagg cache) — IDE preview
        // should always show the human-readable warehouse SQL, not the
        // local-Parquet rewrite.
        resolve_and_compile(&scan_path, &databases, &query, None, 0, None)
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            extract::Json(ErrorResponse {
                message: format!("compile task panicked: {e}"),
            }),
        )
    })?
    .map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            extract::Json(ErrorResponse {
                message: e.to_string(),
            }),
        )
    })?;

    let sql = match compiled {
        CompiledQuery::Warehouse { sql, .. } => sql,
        CompiledQuery::Preaggregation { preagg_sql, .. } => preagg_sql,
    };

    Ok(extract::Json(SemanticQueryCompileResponse { sql }))
}

#[derive(Deserialize)]
pub struct WorkspacePath {
    pub workspace_id: Uuid,
}

// ── POST /semantic ─────────────────────────────────────────────────────────

/// FE request body for the IDE's "Run" button. Mirrors the legacy
/// `SemanticQueryRequest` shape so `executeSemanticQuery` on the FE
/// keeps working untouched.
///
/// The `query` field carries the topic / dimensions / measures / filters
/// just like `/semantic/compile`. `session_filters` / `connections` /
/// `result_format` are the execute-side knobs that `/sql/query` already
/// honors; they get plumbed through to the connector verbatim.
// `SemanticQueryConfig` lives in `agentic-workflow` and doesn't impl
// `ToSchema`, so this request type stays out of the curated OpenAPI
// surface (utoipa). The handler still deserializes it via serde.
#[derive(Deserialize)]
pub struct SemanticQueryExecuteRequest {
    #[serde(flatten)]
    pub query: SemanticQueryConfig,
    #[serde(default)]
    pub session_filters: Option<SessionFilters>,
    #[serde(default)]
    pub connections: Option<ConnectionOverrides>,
    #[serde(default)]
    pub result_format: Option<ResultFormat>,
}

/// Compile a semantic query to SQL, then execute it via the same
/// connector path `/sql/query` uses. Single round-trip from the FE.
///
/// Server-side database selection: the topic's first view with a
/// `datasource:` set wins, so the FE can't accidentally route SQL to
/// the wrong warehouse — `resolve_and_compile` is the source of truth
/// for both the SQL and the database name.
pub async fn execute_semantic_query(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    EffectiveWorkspaceRole(role): EffectiveWorkspaceRole,
    PreaggCacheCtx {
        cache: preagg_cache,
        renewal_threshold_secs,
    }: PreaggCacheCtx,
    Path(WorkspacePath { workspace_id: _ }): Path<WorkspacePath>,
    extract::Json(payload): extract::Json<SemanticQueryExecuteRequest>,
) -> Result<extract::Json<SemanticQueryResponse>, (StatusCode, extract::Json<SqlErrorResponse>)> {
    let scan_path = workspace_manager.config_manager.semantics_scan_path();
    let databases: Vec<airlayer::DatabaseConfig> = workspace_manager
        .config_manager
        .list_databases()
        .iter()
        .map(|db| airlayer::DatabaseConfig {
            name: db.name.clone(),
            db_type: db.database_type.to_string(),
        })
        .collect();

    let query = payload.query;
    // Compile through the same preagg-aware path the automation runtime
    // and analytics solver use. When a preagg cache is attached and a
    // rollup covers the request, `compiled` will be `LocalParquet` and
    // we serve from the on-disk Parquet via DuckDB instead of round-
    // tripping to the warehouse.
    let cache_for_compile = preagg_cache.clone();
    let threshold = renewal_threshold_secs.unwrap_or(0);
    let scan_path_for_compile = scan_path.clone();
    let compiled = tokio::task::spawn_blocking(move || {
        resolve_and_compile(
            &scan_path_for_compile,
            &databases,
            &query,
            cache_for_compile,
            threshold,
            None,
        )
    })
    .await
    .map_err(|e| sql_error_500(format!("compile task panicked: {e}")))?
    .map_err(|e| sql_error_400(e.to_string()))?;

    match compiled {
        CompiledQuery::Warehouse { sql, database_name } => {
            let sql_payload = SQLParams {
                sql,
                database: database_name,
                filters: payload.session_filters,
                connections: payload.connections,
                result_format: payload.result_format,
            };
            run_via_agentic_connector(&workspace_manager, user.id, role, &sql_payload)
                .await
                .map(extract::Json)
                .map_err(|e: SqlExecuteError| agentic_error_response(&sql_payload, e))
        }
        CompiledQuery::Preaggregation {
            preagg_sql,
            parquet_path,
            ..
        } => {
            let started = std::time::Instant::now();
            let want_parquet = matches!(payload.result_format, Some(ResultFormat::Parquet));
            if want_parquet {
                // The IDE Run button always requests Parquet — write the
                // DuckDB reagg result to a file in the workspace results
                // dir and return its handle so the FE can fetch it the
                // same way it does for warehouse queries.
                let results_dir = workspace_manager
                    .config_manager
                    .get_results_dir()
                    .await
                    .map_err(|e| sql_error_500(format!("results dir: {e}")))?;
                tokio::fs::create_dir_all(&results_dir)
                    .await
                    .map_err(|e| sql_error_500(format!("mkdir results dir: {e}")))?;
                let file_name = format!("{}.parquet", uuid::Uuid::new_v4());
                let dest_path = results_dir.join(&file_name);

                tokio::task::spawn_blocking(move || {
                    write_preagg_parquet(&preagg_sql, &parquet_path, &dest_path)
                })
                .await
                .map_err(|e| sql_error_500(format!("preagg task panicked: {e}")))?
                .map_err(sql_error_500)?;

                Ok(extract::Json(SemanticQueryResponse::Parquet {
                    file_name,
                    is_preagg: true,
                    execution_time_ms: started.elapsed().as_millis() as u64,
                    // Preaggregation bypasses the ad-hoc row cap (it reads a
                    // bounded local rollup), so it's never truncated here.
                    truncated: false,
                }))
            } else {
                let result = tokio::task::spawn_blocking(move || {
                    agentic_semantic::preagg::execute_preagg_sql(&preagg_sql, &parquet_path)
                })
                .await
                .map_err(|e| sql_error_500(format!("preagg task panicked: {e}")))?
                .map_err(|e| sql_error_500(e.to_string()))?;

                Ok(extract::Json(preagg_json_to_response(result)))
            }
        }
    }
}

/// Write the result of `preagg_sql` (which `read_parquet(...)`s from the
/// local rollup cache) into `dest_path` as a Parquet file via DuckDB's
/// `COPY ... TO ... (FORMAT PARQUET)`. Keeps every byte inside DuckDB so
/// we never round-trip rows through Rust just to serialize them again.
fn write_preagg_parquet(
    preagg_sql: &str,
    rollup_parquet: &std::path::Path,
    dest_path: &std::path::Path,
) -> Result<(), String> {
    if !rollup_parquet.is_file() {
        return Err(format!(
            "rollup parquet not found: {}",
            rollup_parquet.display()
        ));
    }
    let conn = agentic_semantic::preagg::pooled_duckdb_connection()
        .map_err(|e| format!("preagg duckdb pool: {e}"))?;
    let dest = dest_path
        .to_str()
        .ok_or_else(|| "non-UTF8 dest path".to_string())?
        .replace('\'', "''");
    let trimmed = preagg_sql.trim().trim_end_matches(';');
    let sql = format!("COPY ({trimmed}) TO '{dest}' (FORMAT PARQUET)");
    conn.execute_batch(&sql)
        .map_err(|e| format!("DuckDB parquet write failed: {e}"))?;
    Ok(())
}

/// Convert the JSON blob produced by `execute_preagg_sql` (shape:
/// `{columns: [..], rows: [{col: val, ...}], row_count, truncated}`) into
/// the `SemanticQueryResponse::Json(Vec<Vec<String>>)` shape the IDE
/// frontend already renders for warehouse results.
fn preagg_json_to_response(value: serde_json::Value) -> SemanticQueryResponse {
    let columns: Vec<String> = value
        .get("columns")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .map(|v| v.as_str().unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default();
    // Mirror `typed_stream_to_json_array`: row 0 is the column header,
    // subsequent rows are stringified cell values in column order. The
    // IDE result table reads the header from `data[0]` — omitting it
    // makes the table render "No results to display" even when the
    // payload contains rows.
    let mut out: Vec<Vec<String>> = vec![columns.clone()];
    if let Some(arr) = value.get("rows").and_then(|r| r.as_array()) {
        for row in arr {
            out.push(
                columns
                    .iter()
                    .map(|col| match row.get(col) {
                        Some(serde_json::Value::Null) | None => String::new(),
                        Some(serde_json::Value::String(s)) => s.clone(),
                        Some(other) => other.to_string(),
                    })
                    .collect(),
            );
        }
    }
    SemanticQueryResponse::Json(out)
}

fn sql_error_400(message: String) -> (StatusCode, extract::Json<SqlErrorResponse>) {
    (
        StatusCode::BAD_REQUEST,
        extract::Json(SqlErrorResponse {
            message,
            code: None,
            detail: None,
            hint: None,
            position: None,
            sql: None,
        }),
    )
}

fn sql_error_500(message: String) -> (StatusCode, extract::Json<SqlErrorResponse>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        extract::Json(SqlErrorResponse {
            message,
            code: None,
            detail: None,
            hint: None,
            position: None,
            sql: None,
        }),
    )
}
