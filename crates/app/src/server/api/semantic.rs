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

/// Where a semantic **query** (compile / execute) reads the layer from.
///
/// The query-shaped counterpart to [`resolve_semantic_source`], which does the
/// same for a single-file read. `_guard` owns the materialised tempdir — hold it
/// until compilation finishes or the scan root is deleted out from under it.
pub(crate) struct QueryScanSource {
    pub(crate) scan_path: std::path::PathBuf,
    _guard: Option<MaterialisedScan>,
}

/// The workspace has no compiled semantic layer and this node has no working
/// copy to fall back to.
pub(crate) struct ScanUnavailable {
    workspace_id: Uuid,
}

impl ScanUnavailable {
    pub(crate) fn message(&self) -> String {
        format!(
            "workspace {} has no compiled semantic layer available on this stateless \
             replica; a (re)compile has been enqueued — retry shortly",
            self.workspace_id
        )
    }
}

/// Resolve the scan root for a semantic query — compile boundary first, working
/// copy second.
///
/// Both of this module's query handlers used to read `semantics_scan_path()`
/// unconditionally, which is the workspace working copy. A stateless `serve`
/// replica has no working copy, and scanning a directory that isn't there
/// produced an EMPTY semantic layer rather than an error — surfacing as
/// `Topic 'x' not found. Available: []`, i.e. a modelling mistake for what is
/// really a missing directory. Both routes are (correctly) classified `FleetOk`
/// in `role_manifest`, so they must serve from Postgres like their siblings:
/// `projects::semantic_query` (custom-app data plane) and
/// `resolve_semantic_source` (single-file IDE reads) already do exactly this.
///
/// The workspace-surface metric-tree handlers (`api::metric_tree`) are the
/// other caller, for the same reason and with a louder symptom: scanning the
/// missing directory failed outright there, 500ing every metric-tree call on
/// every workspace in cloud (oxy-hq/oxygen#878).
///
/// Branch semantics come for free: `workspace_middleware` pins the request to
/// one revision via `compiled_reader::resolve_request_revision`, which yields
/// `None` for a non-default branch on a node that HAS a working copy. The IDE
/// previewing uncommitted edits on a feature branch therefore still reads the
/// FS, exactly as before.
pub(crate) async fn resolve_query_scan_source(
    workspace_manager: &WorkspaceManager,
) -> Result<QueryScanSource, ScanUnavailable> {
    let workspace_id = workspace_manager.workspace_id;
    let materialised = match semantic_scan::materialise_semantic_scan(workspace_id).await {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(
                workspace_id = %workspace_id,
                error = ?e,
                "semantic query: materialise failed; falling through to FS"
            );
            None
        }
    };

    if let Some(scan) = materialised {
        return Ok(QueryScanSource {
            scan_path: scan.scan_path.clone(),
            _guard: Some(scan),
        });
    }

    // Stateless-fleet guard, mirroring `projects::semantic_query`: refuse the FS
    // fallback on a node that has no working copy and enqueue a deduped compile
    // so the next request succeeds without operator action. Note
    // `materialise_semantic_scan` downgrades real DB errors to `None`, so this
    // also covers a transient DB failure — a retry is the right answer there too.
    if crate::server::role_manifest::current_process_role()
        == crate::server::role_manifest::Role::Serve
    {
        if let Ok(db) = oxy::database::client::establish_connection().await {
            crate::server::api::middlewares::workspace_context::enqueue_lazy_compile(
                &db,
                workspace_id,
            )
            .await;
        }
        return Err(ScanUnavailable { workspace_id });
    }

    Ok(QueryScanSource {
        scan_path: workspace_manager.config_manager.semantics_scan_path(),
        _guard: None,
    })
}

// ── Preagg status ─────────────────────────────────────────────────────────────

#[derive(Serialize, Clone)]
pub struct PreaggMeasure {
    pub name: String,
    /// Serializes as the snake_case YAML form (`sum`, `count_distinct`, …) —
    /// the shape `web-app/src/services/api/semantic.ts` reads.
    pub measure_type: airlayer::schema::models::MeasureType,
}

/// The subset of a local-manifest entry this endpoint needs: which rollup shape
/// it describes, which file the worker wrote, and when it last looked.
#[derive(serde::Deserialize)]
struct ManifestRollupEntry {
    rollup_hash: String,
    file: String,
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
    pub measures: Vec<PreaggMeasure>,
    pub time_dimension: Option<String>,
    pub granularity: Option<String>,
    pub build_date: Option<String>,
    pub refresh_key_checked_at: Option<String>,
}

#[derive(Serialize)]
pub struct PreaggStatusResponse {
    pub rollups: Vec<PreaggRollupStatus>,
}

/// What the refresh worker has actually materialised for one declared rollup.
/// Defaulted (all-empty) when the worker has never built it — a declared rollup
/// still lists, as "Not cached".
#[derive(Default, Clone)]
pub(crate) struct RollupBuildState {
    has_parquet: bool,
    build_date: Option<String>,
    refresh_key_checked_at: Option<String>,
}

/// Build state indexed by **rollup hash**.
///
/// The hash, not `(view_name, rollup_name)`: `preagg_rebuild` upserts manifest
/// entries by `rollup_hash` and never prunes stale ones, so one `(view, name)`
/// pair can own several entries at different shapes. Collapsing those by name
/// would attach an arbitrary one to the current declaration — reporting another
/// shape's `build_date`, or "Cached" off a parquet built for a shape that no
/// longer exists.
///
/// Hash identity answers *this panel's* question — "has the current declaration
/// been built" — and is deliberately stricter than the serve path, which picks a
/// rollup by **coverage**, not hash (`check_coverage` over every manifest entry,
/// in `agentic_semantic::compile::try_resolve_local_parquet`). So the two can
/// disagree, and legitimately: a query a stale shape still covers is served from
/// that parquet, showing the **Pre-aggregated** badge, while this panel reports
/// the current declaration as "Not cached". Both answers are correct — they are
/// answers to different questions.
pub(crate) type RollupBuildStates = std::collections::HashMap<String, RollupBuildState>;

/// Read the worker's `.airlayer/cache/manifest.json` and index build state by
/// rollup hash.
///
/// Blocking fs (`read_to_string` + one `is_file` per rollup) — call it from
/// `spawn_blocking`. A missing or unparsable manifest is not an error: it means
/// nothing has been built on this node yet.
///
/// **This annotation is node-local.** The manifest and its parquet live in the
/// state dir of whichever instance ran the worker, while the route is `FleetOk`
/// — so a stateless `serve` replica answers "Not cached" for rollups the
/// singleton-worker node has built. Only that node's answer is authoritative;
/// treat the rest as "this replica cannot see a build", not as absence of one.
pub(crate) fn load_rollup_build_states(cache_dir: &std::path::Path) -> RollupBuildStates {
    let manifest_path = cache_dir.join("manifest.json");
    std::fs::read_to_string(&manifest_path)
        .ok()
        .and_then(|s| serde_json::from_str::<LocalManifestJson>(&s).ok())
        .map(|manifest| {
            manifest
                .rollups
                .into_iter()
                .map(|entry| {
                    (
                        entry.rollup_hash,
                        RollupBuildState {
                            has_parquet: cache_dir.join(&entry.file).is_file(),
                            build_date: entry.build_date,
                            refresh_key_checked_at: entry.refresh_key_checked_at,
                        },
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// List the rollups the semantic layer **declares**, annotated with what the
/// worker has built.
///
/// The declaration is the spine, not the manifest. This endpoint used to read
/// its whole list out of `manifest.json`, which is worker output: a rollup
/// appeared in the IDE only once the worker had already materialised it, so the
/// "Not cached" state the panel renders was unreachable, a rollup whose parquet
/// was evicted or whose build kept failing silently vanished instead of showing
/// as stale, and an entry left behind by a since-deleted `pre_aggregations:`
/// block kept showing up. Reading the declaration also makes the route honest on
/// a stateless replica, where there is no local cache dir at all.
///
/// `resolve_rollups` is the same resolution the worker itself enumerates
/// (`preagg_executor`), so the list matches what will be built — including the
/// implicit `default` rollup a view with no `pre_aggregations:` block gets.
/// Both of the worker's skip gates are mirrored here, because listing a rollup
/// the worker will never touch promises a build that never comes: a rollup with
/// no refresh key (`rollup_refresh_key`), and one whose datasource isn't
/// configured in this workspace (normal on a fresh multi-tenant workspace whose
/// seed views reference datasources nobody has connected).
///
/// `database_override` is `config.yml`'s `pre_aggregations.database`, and
/// `is_database_configured` is injected rather than read from a `ConfigManager`
/// so this stays unit-testable without a workspace on disk.
pub(crate) fn build_preagg_status(
    layer: &airlayer::SemanticLayer,
    build_states: &RollupBuildStates,
    database_override: Option<&str>,
    is_database_configured: &dyn Fn(&str) -> bool,
) -> PreaggStatusResponse {
    let mut rollups = Vec::new();
    for view in &layer.views {
        // Mirrors `preagg_executor::load_view_files_sync`'s datasource resolution.
        let database = database_override
            .map(str::to_string)
            .or_else(|| view.datasource.clone())
            .unwrap_or_else(|| "default".to_string());
        if !is_database_configured(&database) {
            continue;
        }
        for rollup in airlayer::preagg::resolve_rollups(view) {
            if crate::server::preagg_executor::rollup_refresh_key(&rollup, view).is_none() {
                continue;
            }
            let state = build_states.get(&rollup.hash).cloned().unwrap_or_default();
            rollups.push(PreaggRollupStatus {
                view_name: view.name.clone(),
                rollup_name: rollup.name,
                has_parquet: state.has_parquet,
                dimensions: rollup.dimensions,
                measures: rollup
                    .measures
                    .into_iter()
                    .map(|m| PreaggMeasure {
                        name: m.name,
                        measure_type: m.measure_type,
                    })
                    .collect(),
                time_dimension: rollup.time_dimension,
                granularity: rollup.granularity,
                build_date: state.build_date,
                refresh_key_checked_at: state.refresh_key_checked_at,
            });
        }
    }
    PreaggStatusResponse { rollups }
}

/// `GET /{workspace_id}/semantic/preagg-status`
///
/// Lists the pre-aggregation rollups declared in the semantic layer, each
/// annotated with the local cache state from `.airlayer/cache/manifest.json`.
/// An unreadable layer or manifest returns an empty rollup list — no error, so
/// the panel degrades to "nothing declared" rather than breaking the view. Each
/// fallback logs, because that empty list otherwise conflates three different
/// situations: nothing declared, not compiled yet (retryable), and a layer that
/// failed to parse.
pub async fn get_preagg_status(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    layer_cache: SemanticLayerCacheCtx,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
) -> extract::Json<PreaggStatusResponse> {
    let empty = || {
        extract::Json(PreaggStatusResponse {
            rollups: Vec::new(),
        })
    };

    // Compile boundary first, working copy second — same source the IDE's other
    // semantic reads use, so the panel reflects the branch the request is pinned to.
    let scan = match resolve_query_scan_source(&workspace_manager).await {
        Ok(scan) => scan,
        Err(unavailable) => {
            tracing::warn!(
                workspace_id = %workspace_id,
                "preagg status: {} — returning no rollups",
                unavailable.message()
            );
            return empty();
        }
    };
    let layer = match layer_cache.get_or_load(scan.scan_path.clone()).await {
        Ok(layer) => layer,
        Err(e) => {
            tracing::warn!(
                workspace_id = %workspace_id,
                error = ?e,
                "preagg status: failed to load semantic layer — returning no rollups"
            );
            return empty();
        }
    };

    let cache_dir =
        oxy::state_dir::get_airlayer_cache_dir(workspace_manager.config_manager.workspace_path());
    let build_states = tokio::task::spawn_blocking(move || load_rollup_build_states(&cache_dir))
        .await
        .unwrap_or_default();

    let config_manager = workspace_manager.config_manager.clone();
    let database_override = config_manager
        .get_config()
        .pre_aggregations
        .as_ref()
        .and_then(|p| p.database.clone());

    extract::Json(build_preagg_status(
        &layer,
        &build_states,
        database_override.as_deref(),
        &|name| config_manager.resolve_database(name).is_ok(),
    ))
}

#[cfg(test)]
mod preagg_tests {
    use super::*;

    /// Every datasource resolves — the common case; the datasource gate has its
    /// own test below.
    fn all_databases_configured(_: &str) -> bool {
        true
    }

    fn status_for(
        layer: &airlayer::SemanticLayer,
        build_states: &RollupBuildStates,
    ) -> PreaggStatusResponse {
        build_preagg_status(layer, build_states, None, &all_databases_configured)
    }

    fn view_from(yaml: &str) -> airlayer::View {
        serde_yaml::from_str(yaml).unwrap()
    }

    /// A view declaring one rollup with an `every:` refresh key.
    fn layer_with_declared_rollup() -> airlayer::SemanticLayer {
        airlayer::SemanticLayer::new(
            vec![view_from(
                r#"
name: orders
datasource: warehouse
table: orders
dimensions:
  - name: status
    type: string
    expr: status
  - name: ordered_at
    type: datetime
    expr: ordered_at
measures:
  - name: revenue
    type: sum
    expr: amount
pre_aggregations:
  - name: by_month
    dimensions: [status]
    measures: [revenue]
    time_dimension: ordered_at
    granularity: month
    refresh_key:
      every: 6h
"#,
            )],
            None,
        )
    }

    /// The hash the worker would key this layer's single rollup by.
    fn declared_hash(layer: &airlayer::SemanticLayer) -> String {
        airlayer::preagg::resolve_rollups(&layer.views[0])[0]
            .hash
            .clone()
    }

    /// Write a manifest holding one entry per `(hash, has_parquet)` pair. The
    /// entries share a view/rollup name on purpose — that is exactly the
    /// stale-shape situation `preagg_rebuild` leaves behind.
    fn write_manifest(cache_dir: &std::path::Path, entries: &[(&str, bool)]) {
        std::fs::create_dir_all(cache_dir).unwrap();
        let rollups: Vec<_> = entries
            .iter()
            .map(|(hash, with_parquet)| {
                let file = format!("orders__{hash}.parquet");
                if *with_parquet {
                    std::fs::write(cache_dir.join(&file), b"").unwrap();
                }
                serde_json::json!({
                    "view_name": "orders",
                    "rollup_name": "by_month",
                    "rollup_hash": hash,
                    "file": file,
                    "dimensions": [],
                    "measures": [],
                    "time_dimension": null,
                    "granularity": null,
                    "build_date": format!("2026-05-{hash}"),
                    "refresh_key_checked_at": "2026-05-11T00:00:00Z"
                })
            })
            .collect();
        let manifest_json = serde_json::json!({
            "pulled_at": "2026-05-11T00:00:00Z",
            "source_database": "test",
            "rollups": rollups,
        });
        std::fs::write(cache_dir.join("manifest.json"), manifest_json.to_string()).unwrap();
    }

    fn cache_dir_in(dir: &tempfile::TempDir) -> std::path::PathBuf {
        dir.path().join(".airlayer").join("cache")
    }

    #[test]
    fn declared_rollup_with_built_parquet_reports_cached() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = cache_dir_in(&dir);
        let layer = layer_with_declared_rollup();
        let hash = declared_hash(&layer);
        write_manifest(&cache_dir, &[(&hash, true)]);

        let status = status_for(&layer, &load_rollup_build_states(&cache_dir));
        assert_eq!(status.rollups.len(), 1);
        let rollup = &status.rollups[0];
        assert_eq!(rollup.view_name, "orders");
        assert_eq!(rollup.rollup_name, "by_month");
        assert!(rollup.has_parquet);
        assert_eq!(rollup.build_date, Some(format!("2026-05-{hash}")));
        assert_eq!(rollup.dimensions, vec!["status".to_string()]);
        assert_eq!(rollup.measures[0].name, "revenue");
        assert_eq!(
            rollup.measures[0].measure_type,
            airlayer::schema::models::MeasureType::Sum
        );
        assert_eq!(rollup.time_dimension.as_deref(), Some("ordered_at"));
        assert_eq!(rollup.granularity.as_deref(), Some("month"));
    }

    /// The wire shape the frontend reads (`measure_type: "sum"`), pinned — it
    /// silently serialized as `type` before this endpoint was rewritten, so the
    /// panel's aggregation badge never rendered.
    #[test]
    fn measure_type_serializes_as_the_snake_case_yaml_form() {
        let measure = PreaggMeasure {
            name: "revenue".to_string(),
            measure_type: airlayer::schema::models::MeasureType::CountDistinct,
        };
        assert_eq!(
            serde_json::to_value(&measure).unwrap(),
            serde_json::json!({ "name": "revenue", "measure_type": "count_distinct" })
        );
    }

    #[test]
    fn declared_rollup_with_missing_parquet_reports_not_cached() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = cache_dir_in(&dir);
        let layer = layer_with_declared_rollup();
        write_manifest(&cache_dir, &[(&declared_hash(&layer), false)]);

        let status = status_for(&layer, &load_rollup_build_states(&cache_dir));
        assert_eq!(status.rollups.len(), 1);
        assert!(!status.rollups[0].has_parquet);
    }

    /// The regression this endpoint was rewritten for: the declaration is the
    /// spine, so a rollup the worker has never touched still lists.
    #[test]
    fn declared_rollup_lists_with_no_manifest_at_all() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = cache_dir_in(&dir);

        let layer = layer_with_declared_rollup();
        let status = status_for(&layer, &load_rollup_build_states(&cache_dir));
        assert_eq!(status.rollups.len(), 1);
        assert_eq!(status.rollups[0].rollup_name, "by_month");
        assert!(!status.rollups[0].has_parquet);
        assert!(status.rollups[0].build_date.is_none());
    }

    /// The mirror case: a manifest entry whose declaration is gone must not
    /// resurrect the rollup in the UI.
    #[test]
    fn manifest_entry_without_a_declaration_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = cache_dir_in(&dir);
        write_manifest(&cache_dir, &[("aabbccdd", true)]);

        let empty_layer = airlayer::SemanticLayer::new(vec![], None);
        let status = status_for(&empty_layer, &load_rollup_build_states(&cache_dir));
        assert!(status.rollups.is_empty());
    }

    /// `preagg_rebuild` upserts by `rollup_hash` and never prunes, so editing a
    /// rollup's shape leaves the old hash in the manifest under the same
    /// view/rollup name. Keying build state by name would attach that stale
    /// row — claiming "Cached", with the wrong build date, off a parquet built
    /// for a shape that no longer exists.
    #[test]
    fn stale_hash_under_the_same_name_does_not_mark_the_rollup_cached() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = cache_dir_in(&dir);
        let layer = layer_with_declared_rollup();
        let current = declared_hash(&layer);
        write_manifest(&cache_dir, &[("staleaaa", true), ("stalebbb", true)]);
        assert_ne!(current, "staleaaa");

        let status = status_for(&layer, &load_rollup_build_states(&cache_dir));
        assert_eq!(status.rollups.len(), 1);
        assert!(!status.rollups[0].has_parquet);
        assert!(status.rollups[0].build_date.is_none());
    }

    /// The worker skips a rollup with no refresh key, so listing it would
    /// promise a build that never comes.
    #[test]
    fn rollup_without_a_refresh_key_is_not_listed() {
        let layer = airlayer::SemanticLayer::new(
            vec![view_from(
                r#"
name: orders
table: orders
dimensions:
  - name: status
    type: string
    expr: status
measures:
  - name: revenue
    type: sum
    expr: amount
pre_aggregations:
  - name: by_status
    dimensions: [status]
    measures: [revenue]
"#,
            )],
            None,
        );
        let status = status_for(&layer, &RollupBuildStates::new());
        assert!(status.rollups.is_empty());
    }

    /// A view with no `pre_aggregations:` block still gets the implicit
    /// `default` rollup — but only when a view-level `refresh_key:` gives the
    /// worker something to build on, which is the same gate the worker applies.
    #[test]
    fn implicit_default_rollup_lists_only_with_a_view_level_refresh_key() {
        const VIEW: &str = r#"
name: orders
table: orders
dimensions:
  - name: status
    type: string
    expr: status
  - name: ordered_at
    type: datetime
    expr: ordered_at
measures:
  - name: revenue
    type: sum
    expr: amount
"#;

        let bare = airlayer::SemanticLayer::new(vec![view_from(VIEW)], None);
        assert!(
            status_for(&bare, &RollupBuildStates::new())
                .rollups
                .is_empty()
        );

        let keyed = airlayer::SemanticLayer::new(
            vec![view_from(&format!("{VIEW}refresh_key:\n  every: 6h\n"))],
            None,
        );
        let status = status_for(&keyed, &RollupBuildStates::new());
        assert_eq!(status.rollups.len(), 1);
        assert_eq!(status.rollups[0].rollup_name, "default");
        assert!(!status.rollups[0].has_parquet);
    }

    /// The worker's other skip gate: a view whose datasource isn't configured
    /// in this workspace never gets built, so it must not list as pending. This
    /// is normal on a fresh multi-tenant workspace whose seed views reference
    /// datasources nobody has connected.
    #[test]
    fn rollup_on_an_unconfigured_datasource_is_not_listed() {
        let layer = layer_with_declared_rollup();
        let states = RollupBuildStates::new();

        let listed = build_preagg_status(&layer, &states, None, &|name| name == "warehouse");
        assert_eq!(listed.rollups.len(), 1);

        let hidden = build_preagg_status(&layer, &states, None, &|name| name != "warehouse");
        assert!(hidden.rollups.is_empty());
    }

    /// `config.yml`'s `pre_aggregations.database` overrides each view's own
    /// `datasource`, so the gate must test the override, not the view.
    #[test]
    fn database_override_is_what_the_datasource_gate_checks() {
        let layer = layer_with_declared_rollup();
        let states = RollupBuildStates::new();

        let listed = build_preagg_status(&layer, &states, Some("override_db"), &|name| {
            name == "override_db"
        });
        assert_eq!(listed.rollups.len(), 1);

        let hidden = build_preagg_status(&layer, &states, Some("override_db"), &|name| {
            name == "warehouse"
        });
        assert!(hidden.rollups.is_empty());
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
    // Compile boundary first — this route is FleetOk and must not depend on a
    // working copy. `source` owns the materialised tempdir; keep it alive until
    // the blocking compile below has finished reading from it.
    let source = resolve_query_scan_source(&workspace_manager)
        .await
        .map_err(|e| semantic_err(StatusCode::SERVICE_UNAVAILABLE, e.message()))?;
    let scan_path = source.scan_path.clone();
    let databases: Vec<airlayer::DatabaseConfig> = workspace_manager
        .config_manager
        .list_databases()
        .iter()
        .map(|db| airlayer::DatabaseConfig {
            name: db.name.clone(),
            // `dialect()`, not the raw type name: airhouse and motherduck
            // speak an engine their `type:` string does not name, and
            // airlayer drops a datasource it cannot classify -- silently
            // inheriting whichever dialect config.yml lists first.
            db_type: db.dialect(),
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
    // Compile boundary first — see `resolve_query_scan_source`. `source` owns the
    // materialised tempdir and must outlive the blocking compile below.
    let source = resolve_query_scan_source(&workspace_manager)
        .await
        .map_err(|e| sql_error_503(e.message()))?;
    let scan_path = source.scan_path.clone();
    let databases: Vec<airlayer::DatabaseConfig> = workspace_manager
        .config_manager
        .list_databases()
        .iter()
        .map(|db| airlayer::DatabaseConfig {
            name: db.name.clone(),
            // `dialect()`, not the raw type name: airhouse and motherduck
            // speak an engine their `type:` string does not name, and
            // airlayer drops a datasource it cannot classify -- silently
            // inheriting whichever dialect config.yml lists first.
            db_type: db.dialect(),
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
                untyped: false,
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

/// Retryable: the compiled semantic layer isn't available on this replica yet.
/// Distinct from 400 (the caller's query is wrong) and 500 (we broke) — the
/// caller should retry rather than change anything.
fn sql_error_503(message: String) -> (StatusCode, extract::Json<SqlErrorResponse>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
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
