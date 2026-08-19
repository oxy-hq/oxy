//! Metric Tree API — tree structure and analysis ops over the semantic layer.
//!
//! The tree is built per request from the workspace's semantic layer, resolved
//! through the same scan source `execute_semantic_query` uses —
//! [`resolve_query_scan_source`]: the compile boundary first, the working copy
//! second. Every route here is `FleetOk`, so reading the working copy directly
//! is not an option: a stateless serve replica has none, and scanning the
//! missing directory failed every call with a flat 500 (oxy-hq/oxygen#878).
//!
//! The pure ops — tree, `sensitivity`, `predict` — need no database access.
//! The query-executing ops (`explain`, `opportunity`) run airlayer's
//! algorithms against a `QueryExecutor` bridged to Oxy's connector: airlayer
//! compiles each `QueryRequest` to SQL, and `run_via_agentic_connector`
//! executes it.

use std::collections::BTreeMap;

use axum::{
    extract::{Json, Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use airlayer::engine::metric_tree::MetricTree;
use airlayer::engine::metric_tree_ops::{ExplainConfig, ExplainResult, OpportunityResult};
use oxy::adapters::workspace::manager::WorkspaceManager;
use oxy_auth::extractor::AuthenticatedUserExtractor;

use crate::agentic_wiring::metric_tree_runner::{
    OxyMetricTreeRunner, build_drill_query_executor, build_query_executor,
};
use crate::server::api::middlewares::workspace_context::{
    EffectiveWorkspaceRole, PreaggCacheCtx, WorkspaceManagerExtractor,
};
use crate::server::api::semantic::{QueryScanSource, resolve_query_scan_source};

#[derive(Debug)]
pub enum MetricTreeError {
    LayerLoad(String),
    /// No semantic layer is reachable on this node: the workspace has no
    /// compiled revision and this replica has no working copy to fall back to.
    /// Retryable — a compile is enqueued on the way out — so it must not be
    /// flattened into the generic 500 above.
    ScanUnavailable(String),
    NotFound(String),
    Op(String),
}

impl IntoResponse for MetricTreeError {
    fn into_response(self) -> Response {
        let mut retry_after = false;
        let (status, msg) = match &self {
            MetricTreeError::LayerLoad(e) => {
                tracing::error!("metric-tree layer load failed: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to load semantic layer".to_string(),
                )
            }
            MetricTreeError::ScanUnavailable(m) => {
                // Logged, not silent: the "no compiled revision at all" branch
                // leaves no breadcrumb anywhere else (`resolve_query_scan_source`
                // only warns when a materialise actually failed), so a workspace
                // stuck un-compiled would be visible to the client and to nobody
                // operating the fleet.
                tracing::warn!("metric-tree scan unavailable: {m}");
                retry_after = true;
                (StatusCode::SERVICE_UNAVAILABLE, m.clone())
            }
            MetricTreeError::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            MetricTreeError::Op(e) => {
                tracing::error!("metric-tree op failed: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Metric tree operation failed".to_string(),
                )
            }
        };
        if retry_after {
            // A compile is enqueued on the way out; give the client a concrete
            // interval instead of leaving the retry cadence to guesswork.
            return (status, [(axum::http::header::RETRY_AFTER, "5")], msg).into_response();
        }
        (status, msg).into_response()
    }
}

/// Resolve the scan root for this request: compile boundary first, working copy
/// second — the same resolution `execute_semantic_query` performs.
///
/// The returned [`QueryScanSource`] owns the materialised tempdir, so callers
/// must keep it alive until every read of `scan_path` has finished. Every such
/// read happens in the async half of a handler — parsing the layer, or a
/// runner's `snapshot_for_blocking` — deliberately: a `spawn_blocking` task
/// outlives handler cancellation (client disconnect, `EXPLAIN_TIMEOUT`), which
/// drops the guard and deletes the directory underneath it. Keep it that way;
/// the invariant "hold the guard across the blocking task" is not one the
/// handler can actually enforce.
async fn resolve_scan(
    workspace_manager: &WorkspaceManager,
) -> Result<QueryScanSource, MetricTreeError> {
    resolve_query_scan_source(workspace_manager)
        .await
        .map_err(|e| MetricTreeError::ScanUnavailable(e.message()))
}

/// Parse the workspace's semantic layer from an already-resolved scan root.
fn load_layer_at(scan_path: &std::path::Path) -> Result<airlayer::SemanticLayer, MetricTreeError> {
    OxyMetricTreeRunner::load_layer_at(scan_path)
        .map_err(|e| MetricTreeError::LayerLoad(e.to_string()))
}

/// Build the metric tree from an already-resolved scan root.
fn load_tree_at(scan_path: &std::path::Path) -> Result<MetricTree, MetricTreeError> {
    let layer = load_layer_at(scan_path)?;
    Ok(oxy_semantic::build_metric_tree(&layer))
}

/// The airlayer database configs for the workspace.
fn workspace_databases(workspace_manager: &WorkspaceManager) -> Vec<airlayer::DatabaseConfig> {
    OxyMetricTreeRunner::list_databases_sync(workspace_manager)
}

// ── Pure ops: tree, sensitivity, predict ────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct TreeQuery {
    /// Optional measure id to root the returned subtree at.
    pub root: Option<String>,
}

/// `GET .../semantic/metric-tree` — the full tree, or `?root=<id>` subtree.
pub async fn get_metric_tree(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Query(q): Query<TreeQuery>,
) -> Result<Json<MetricTree>, MetricTreeError> {
    let source = resolve_scan(&workspace_manager).await?;
    let tree = load_tree_at(&source.scan_path)?;
    match q.root {
        Some(root) => oxy_semantic::subtree(&tree, &root)
            .map(Json)
            .ok_or_else(|| MetricTreeError::NotFound(format!("measure '{root}' not in tree"))),
        None => Ok(Json(tree)),
    }
}

/// `GET .../semantic/metric-tree/{measure_id}/sensitivity` — ranked drivers.
pub async fn get_sensitivity(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Path((_workspace_id, measure_id)): Path<(Uuid, String)>,
) -> Result<Json<airlayer::engine::metric_tree_ops::SensitivityResult>, MetricTreeError> {
    let source = resolve_scan(&workspace_manager).await?;
    let tree = load_tree_at(&source.scan_path)?;
    oxy_semantic::sensitivity(&tree, &measure_id)
        .map(Json)
        .map_err(|e| MetricTreeError::Op(e.to_string()))
}

#[derive(Debug, Deserialize)]
pub struct PredictRequest {
    pub changes: Vec<PredictChange>,
}

#[derive(Debug, Deserialize)]
pub struct PredictChange {
    pub measure: String,
    pub delta: f64,
}

/// `POST .../semantic/metric-tree/predict` — propagate hypothetical deltas.
pub async fn post_predict(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Json(req): Json<PredictRequest>,
) -> Result<Json<airlayer::engine::metric_tree_ops::PredictResult>, MetricTreeError> {
    let source = resolve_scan(&workspace_manager).await?;
    let tree = load_tree_at(&source.scan_path)?;
    let changes: Vec<(String, f64)> = req
        .changes
        .into_iter()
        .map(|c| (c.measure, c.delta))
        .collect();
    oxy_semantic::predict(&tree, &changes)
        .map(Json)
        .map_err(|e| MetricTreeError::Op(e.to_string()))
}

// ── Executor bridge ─────────────────────────────────────────────────────────
//
// The executor closure that compiles airlayer `QueryRequest`s and runs them
// through Oxy's connector pool lives in [`crate::agentic_wiring::metric_tree_runner`]
// so both this HTTP handler and the agentic analytics tools route through the
// same code path. See `build_query_executor` there.

/// Build the airlayer `SemanticEngine` for the workspace, with dialects
/// resolved from the configured databases.
fn build_engine(
    layer: airlayer::SemanticLayer,
    databases: &[airlayer::DatabaseConfig],
) -> Result<airlayer::SemanticEngine, MetricTreeError> {
    let dialects = airlayer::DatasourceDialectMap::from_config_databases(databases);
    airlayer::SemanticEngine::from_semantic_layer(layer, dialects)
        .map_err(|e| MetricTreeError::Op(e.to_string()))
}

// ── Query-executing ops: explain, opportunity ───────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ExplainRequest {
    pub target: String,
    pub time_dimension: String,
    /// `[start, end]` inclusive date strings.
    pub current_period: (String, String),
    pub previous_period: (String, String),
    /// Optional `ExplainConfig` override; `None` uses airlayer defaults.
    pub config: Option<ExplainConfigOverride>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ExplainConfigOverride {
    pub deep: Option<bool>,
    pub max_depth: Option<usize>,
    pub coverage_threshold: Option<f64>,
}

pub(crate) fn explain_config(over: Option<ExplainConfigOverride>) -> ExplainConfig {
    let mut config = ExplainConfig::default();
    if let Some(o) = over {
        if let Some(d) = o.deep {
            config.deep = d;
        }
        if let Some(d) = o.max_depth {
            config.max_depth = d;
        }
        if let Some(c) = o.coverage_threshold {
            config.coverage_threshold = c;
        }
    }
    config
}

/// Soft cap on how long an explain call can run before the handler bails.
/// Explain can fire 50+ warehouse queries on rich schemas; the UI just
/// shows a spinner the whole time, so failing loud after the cap is
/// kinder than hanging indefinitely.
const EXPLAIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);

/// `POST .../semantic/metric-tree/explain` — period-over-period root cause.
///
/// Goes through `OxyMetricTreeRunner::run_explain` so the HTTP path and
/// the agentic path apply the same dim-pruning policy (the runner strips
/// high-cardinality numeric dims before passing the layer to airlayer).
pub async fn post_explain(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    EffectiveWorkspaceRole(role): EffectiveWorkspaceRole,
    preagg_ctx: PreaggCacheCtx,
    Json(req): Json<ExplainRequest>,
) -> Result<Json<ExplainResult>, MetricTreeError> {
    use agentic_analytics::MetricTreeRunner as _;
    // `source` owns the materialised tempdir — it must outlive `runner`, which
    // re-parses the layer from `scan_path` once per run it performs (in the
    // async `snapshot_for_blocking`, not inside the blocking task).
    let source = resolve_scan(&workspace_manager).await?;
    let runner = OxyMetricTreeRunner::new(workspace_manager, user.id, role)
        .with_scan_path(source.scan_path.clone())
        .with_preagg(
            preagg_ctx.cache,
            preagg_ctx.renewal_threshold_secs.unwrap_or(120),
        );
    let config = explain_config(req.config);

    let result = tokio::time::timeout(
        EXPLAIN_TIMEOUT,
        runner.run_explain(
            req.target,
            req.time_dimension,
            (req.current_period.0, req.current_period.1),
            (req.previous_period.0, req.previous_period.1),
            vec![],
            config,
        ),
    )
    .await
    .map_err(|_| {
        MetricTreeError::Op(format!(
            "explain timed out after {}s — consider narrowing the target measure or filtering dims",
            EXPLAIN_TIMEOUT.as_secs()
        ))
    })?
    .map_err(|e| MetricTreeError::Op(e.to_string()))?;

    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
pub struct OpportunityRequest {
    pub target: String,
    pub time_dimension: String,
    /// `[start, end]` inclusive date strings.
    pub period: (String, String),
    /// Narrow the scan to one world-model instance. `None` sizes across the
    /// whole population.
    pub instance: Option<OpportunityInstance>,
}

/// A world-model instance to scope a scan to, addressed the way the rest of the
/// world-model API addresses one: entity name plus instance key.
#[derive(Debug, Deserialize)]
pub struct OpportunityInstance {
    pub entity: String,
    /// JSON array for a composite key, else a bare scalar.
    pub key: String,
}

/// Resolve an [`OpportunityRequest`]'s optional instance into the engine scope.
///
/// No instance → an empty scope, i.e. size the whole population.
///
/// An instance that cannot be resolved is an error rather than an ignored
/// scope: silently sizing the population under a request that asked for one
/// instance is how a panel ends up reporting company-wide numbers under an
/// instance header.
fn opportunity_scope(
    layer: &airlayer::SemanticLayer,
    req: &OpportunityRequest,
) -> Result<Vec<airlayer::engine::query::QueryFilter>, MetricTreeError> {
    let Some(instance) = req.instance.as_ref() else {
        return Ok(Vec::new());
    };
    crate::server::api::world_model_graph::instance_scope_filters(
        layer,
        &instance.entity,
        &instance.key,
    )
    .ok_or_else(|| {
        MetricTreeError::NotFound(format!(
            "cannot scope '{}' to '{}': no entity named '{}' in the semantic layer",
            req.target, instance.key, instance.entity
        ))
    })
}

/// Airlayer's sizing result, plus the denominator that makes its rates legible.
#[derive(Debug, Serialize)]
pub struct OpportunityResponse {
    #[serde(flatten)]
    pub result: OpportunityResult,
    /// The `count` measure whose value divides the target to form the per-unit
    /// rates, as a `view.measure` id — set only in `weight_basis: "rows"` mode,
    /// the only mode that forms rates.
    ///
    /// Without it a rate is an unlabelled number: the panel can only say
    /// "533.9 vs 801.6", and a reader has no way to learn that means revenue per
    /// order, nor *which* count it divided by. That last part is not pedantry —
    /// the engine takes the FIRST declared `count` measure, so a view declaring
    /// a filtered one (`completed_orders`) silently rates against a different
    /// denominator than a reader would assume. Naming it is how a modeller can
    /// notice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_denominator: Option<String>,
}

/// First `type: count` measure declared on `view_name`, as a `view.measure` id.
///
/// Mirrors airlayer's private `discover_count_measure` — same "first Count
/// measure wins" rule, so the id we report is the one the engine actually
/// divided by. Kept in sync by hand: if the crate ever surfaces the denominator
/// on `OpportunityResult` itself, delete this and read it from there.
fn count_measure_id(layer: &airlayer::SemanticLayer, view_name: &str) -> Option<String> {
    let view = layer.views.iter().find(|v| v.name == view_name)?;
    view.measures_list()
        .iter()
        .find(|m| m.measure_type == airlayer::schema::models::MeasureType::Count)
        .map(|m| format!("{}.{}", view_name, m.name))
}

/// Whether `target` (a `view.measure` id) is something the drill sizes on a
/// per-unit rate (`filtered_sum / count`).
///
/// airlayer no longer hard-gates rate mode to `type: sum` — an eligible additive
/// composite (e.g. a `type: number | custom` root whose refs flatten to a single
/// same-view additive expression) is rate-sized too. `supports_rate_basis` is
/// airlayer's own authoritative answer to this question; call it rather than
/// re-deriving eligibility from `measure_type` alone, which is how this handler
/// used to accept only `type: sum` and silently omit `rate_denominator` for an
/// accepted composite. Mirrors that rule handler-side, since `DrillResult` does
/// not surface the weight basis the way `OpportunityResult` does.
fn target_supports_rate_basis(layer: &airlayer::SemanticLayer, target: &str) -> bool {
    airlayer::engine::metric_tree_ops::supports_rate_basis(layer, target)
}

/// `POST .../semantic/metric-tree/opportunity` — segment opportunity sizing.
pub async fn post_opportunity(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    EffectiveWorkspaceRole(role): EffectiveWorkspaceRole,
    preagg_ctx: PreaggCacheCtx,
    Json(req): Json<OpportunityRequest>,
) -> Result<Json<OpportunityResponse>, MetricTreeError> {
    // `source` owns the materialised tempdir; hold it until the layer below is
    // parsed. Nothing inside the blocking task reads it — the executor derives
    // its own (different) pre-aggregation cache root from the workspace manager.
    let source = resolve_scan(&workspace_manager).await?;
    let mut layer = load_layer_at(&source.scan_path)?;
    // Order is load-bearing. The tree is built from the clean layer: the
    // dispersion measure below is a pass-through, and pass-throughs read as
    // composite nodes, so a tree built after it would sprout an internal
    // `__opp_stddev__…` node into the world-model graph. The engine, by
    // contrast, must be built from the augmented layer — it resolves measure
    // names against its own copy, and would reject a measure it had never seen.
    let tree = oxy_semantic::build_metric_tree(&layer);
    airlayer::engine::metric_tree_ops::augment_layer_for_opportunity(&mut layer, &req.target);
    let scope = opportunity_scope(&layer, &req)?;
    // Resolved here because `layer` and `req` are both moved into the blocking
    // task below. Read from the augmented layer deliberately, but this is not
    // shadow-proof: `augment_layer_for_opportunity` also installs a
    // `MeasureType::Count` companion (`__opp_n__<measure>`) when the target
    // carries `.filters`, and `count_measure_id` — like the engine's own
    // `discover_count_measure` — takes the FIRST `Count` measure on the view.
    // On a view with a natural `count` measure, that one is declared first and
    // wins, so the synthetic companion never shows up here. On a view with NO
    // natural count measure, the synthetic companion becomes the only `Count`
    // found and gets reported as the denominator — naming a measure that
    // doesn't exist on the layer the analyst actually sees.
    let denominator = req
        .target
        .split_once('.')
        .and_then(|(view, _)| count_measure_id(&layer, view));
    let databases = workspace_databases(&workspace_manager);
    let engine = build_engine(layer.clone(), &databases)?;
    let handle = tokio::runtime::Handle::current();
    let preagg_cache = preagg_ctx.cache;
    let preagg_renewal_threshold_secs = preagg_ctx.renewal_threshold_secs.unwrap_or(120);

    let result = tokio::task::spawn_blocking(move || {
        let executor = build_query_executor(
            &req.target,
            engine,
            databases,
            workspace_manager,
            user.id,
            role,
            handle,
            preagg_cache,
            preagg_renewal_threshold_secs,
        );
        airlayer::engine::metric_tree_ops::opportunity(
            &tree,
            &layer,
            &req.target,
            &req.time_dimension,
            (req.period.0.as_str(), req.period.1.as_str()),
            &scope,
            &executor,
        )
    })
    .await
    .map_err(|e| MetricTreeError::Op(format!("opportunity task panicked: {e}")))?
    .map_err(|e| MetricTreeError::Op(e.to_string()))?;

    // Only "rows" mode divides by the count to form rates; reporting a
    // denominator for a scan that never used one would invite the reader to
    // read `current_value` as a rate when it is a raw measure value.
    let rate_denominator = (result.weight_basis == "rows")
        .then_some(denominator)
        .flatten();
    Ok(Json(OpportunityResponse {
        result,
        rate_denominator,
    }))
}

// ── Query-executing op: opportunity drill (recursive decomposition) ──────────

#[derive(Debug, Deserialize)]
pub struct DrillRequest {
    pub target: String,
    pub time_dimension: String,
    /// `[start, end]` inclusive date strings.
    pub period: (String, String),
    /// Narrow the scan to one world-model instance. `None` drills across the
    /// whole population.
    pub instance: Option<OpportunityInstance>,
    /// Optional overrides; defaults to airlayer's `DrillConfig::default()`
    /// (max_depth 5, alpha = the single-scan significance budget).
    pub max_depth: Option<usize>,
    pub alpha: Option<f64>,
    /// Root the drill at a specific row of the root scan instead of its
    /// top-ranked one. The merged panel sends this when an analyst expands a
    /// row that is not the engine's own pick.
    pub root: Option<airlayer::engine::metric_tree_ops::DrillRoot>,
}

/// The drill tree, plus the denominator that makes its rates legible (same role
/// as [`OpportunityResponse::rate_denominator`]). `result` is null when the root
/// scan found nothing to drill (`Ok(None)`).
#[derive(Debug, Serialize)]
pub struct DrillResponse {
    #[serde(flatten)]
    pub result: Option<airlayer::engine::metric_tree_ops::DrillResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_denominator: Option<String>,
}

/// Resolve a [`DrillRequest`]'s optional instance into the engine scope.
///
/// Mirrors [`opportunity_scope`] against `DrillRequest`: no instance → an empty
/// scope (drill the whole population); an instance that cannot be resolved is an
/// error rather than a silently-ignored scope.
fn opportunity_scope_from_drill(
    layer: &airlayer::SemanticLayer,
    req: &DrillRequest,
) -> Result<Vec<airlayer::engine::query::QueryFilter>, MetricTreeError> {
    let Some(instance) = req.instance.as_ref() else {
        return Ok(Vec::new());
    };
    crate::server::api::world_model_graph::instance_scope_filters(
        layer,
        &instance.entity,
        &instance.key,
    )
    .ok_or_else(|| {
        MetricTreeError::NotFound(format!(
            "cannot scope '{}' to '{}': no entity named '{}' in the semantic layer",
            req.target, instance.key, instance.entity
        ))
    })
}

/// `POST .../semantic/metric-tree/drill` — recursive gap decomposition.
///
/// Mirrors [`post_opportunity`] with three deltas: the augmented layer is shared
/// via `Arc<RwLock<>>`, the executor rebuilds its engine per query from that
/// shared layer (so the drill's mid-recursion synthetic measures compile), and
/// it calls `opportunity_drill` instead of `opportunity`.
pub async fn post_opportunity_drill(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    EffectiveWorkspaceRole(role): EffectiveWorkspaceRole,
    preagg_ctx: PreaggCacheCtx,
    Json(req): Json<DrillRequest>,
) -> Result<Json<DrillResponse>, MetricTreeError> {
    // `source` owns the materialised tempdir; hold it until the layer below is
    // parsed. Nothing inside the blocking task reads it — the executor derives
    // its own (different) pre-aggregation cache root from the workspace manager.
    let source = resolve_scan(&workspace_manager).await?;
    let mut clean_layer = load_layer_at(&source.scan_path)?;
    // Tree from the CLEAN layer (same reason as post_opportunity: augmentation
    // adds pass-through measures that would sprout graph nodes).
    let tree = oxy_semantic::build_metric_tree(&clean_layer);
    // Augment the ROOT target before sharing — the drill's children augment
    // themselves via dimension_candidates.
    airlayer::engine::metric_tree_ops::augment_layer_for_opportunity(&mut clean_layer, &req.target);
    // opportunity_scope + denominator read from the augmented layer, same as
    // post_opportunity.
    let scope = opportunity_scope_from_drill(&clean_layer, &req)?;
    let denominator = req
        .target
        .split_once('.')
        .and_then(|(view, _)| count_measure_id(&clean_layer, view));
    // Read the target's rate-basis eligibility before the layer is moved into
    // the Arc, so the response can gate the rate denominator on rows mode
    // (sum, or an eligible additive composite).
    let target_supports_rate_basis = target_supports_rate_basis(&clean_layer, &req.target);

    let databases = workspace_databases(&workspace_manager);
    let dialects = airlayer::DatasourceDialectMap::from_config_databases(&databases);
    let shared: airlayer::engine::metric_tree_ops::SharedLayer =
        std::sync::Arc::new(std::sync::RwLock::new(clean_layer));
    let handle = tokio::runtime::Handle::current();
    let preagg_cache = preagg_ctx.cache;
    let preagg_renewal_threshold_secs = preagg_ctx.renewal_threshold_secs.unwrap_or(120);
    let default_config = airlayer::engine::metric_tree_ops::DrillConfig::default();
    let config = airlayer::engine::metric_tree_ops::DrillConfig {
        max_depth: req.max_depth.unwrap_or(default_config.max_depth),
        alpha: req.alpha.unwrap_or(default_config.alpha),
        root: req.root.clone(),
    };

    let shared_for_exec = shared.clone();
    let result = tokio::task::spawn_blocking(move || {
        let executor = build_drill_query_executor(
            shared_for_exec,
            dialects,
            databases,
            workspace_manager,
            user.id,
            role,
            handle,
            preagg_cache,
            preagg_renewal_threshold_secs,
        );
        airlayer::engine::metric_tree_ops::opportunity_drill(
            &tree,
            &shared,
            &req.target,
            &req.time_dimension,
            (req.period.0.as_str(), req.period.1.as_str()),
            &scope,
            &executor,
            &config,
        )
    })
    .await
    .map_err(|e| MetricTreeError::Op(format!("drill task panicked: {e}")))?
    .map_err(|e| MetricTreeError::Op(e.to_string()))?;

    // A rate denominator only makes sense when the drill actually formed per-unit
    // rates — i.e. rows mode, which airlayer gates to a `supports_rate_basis`
    // target (a sum, or an eligible additive composite). `opportunity_drill`
    // returns `Some` for any additive root (its dimensions are populated in
    // value_share/equal mode too), so gating on `Some` alone would mislabel a raw
    // value as a rate. Require BOTH a result and a rate-basis target, mirroring
    // `post_opportunity`'s `weight_basis == "rows"` gate.
    let rate_denominator = result
        .as_ref()
        .filter(|_| target_supports_rate_basis)
        .and(denominator);
    Ok(Json(DrillResponse {
        result,
        rate_denominator,
    }))
}

// ── Discovery: time dimensions per view ─────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct TimeDimensionsResponse {
    /// Map of view name → fully-qualified time-dimension ids
    /// (e.g. `orders.order_date`). Includes every `date` and `datetime`
    /// dimension declared on the view. Empty for views without one.
    pub by_view: BTreeMap<String, Vec<String>>,
}

/// `GET .../semantic/metric-tree/time-dimensions` — list valid time
/// dimensions per view. Lets clients (the metric-tree UI in particular)
/// drop hardcoded curated maps and discover what's actually queryable.
pub async fn get_time_dimensions(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
) -> Result<Json<TimeDimensionsResponse>, MetricTreeError> {
    use airlayer::schema::models::DimensionType;

    let source = resolve_scan(&workspace_manager).await?;
    let layer = load_layer_at(&source.scan_path)?;
    let mut by_view: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for view in &layer.views {
        let mut dims: Vec<String> = view
            .dimensions
            .iter()
            .filter(|d| {
                matches!(
                    d.dimension_type,
                    DimensionType::Date | DimensionType::Datetime
                )
            })
            .map(|d| format!("{}.{}", view.name, d.name))
            .collect();
        dims.sort();
        by_view.insert(view.name.clone(), dims);
    }
    Ok(Json(TimeDimensionsResponse { by_view }))
}

// ── Distribution: structural decomposition at a single period ───────────────

#[derive(Debug, Deserialize)]
pub struct DistributionRequest {
    pub target: String,
    pub time_dimension: String,
    /// `[start, end]` inclusive — the period to value the tree at.
    pub period: (String, String),
}

/// `POST .../semantic/metric-tree/distribution` — distribution view of a
/// measure. Returns an `ExplainResult`-shaped payload (so the same client
/// renderers work), but with the baseline auto-derived as the equal-length
/// window immediately before `period`. Callers only specify the period of
/// interest; ignore the delta fields when rendering a pure distribution.
pub async fn post_distribution(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    EffectiveWorkspaceRole(role): EffectiveWorkspaceRole,
    preagg_ctx: PreaggCacheCtx,
    Json(req): Json<DistributionRequest>,
) -> Result<Json<ExplainResult>, MetricTreeError> {
    use agentic_analytics::MetricTreeRunner as _;

    let baseline = derive_baseline_period(&req.period.0, &req.period.1)
        .ok_or_else(|| MetricTreeError::Op("invalid period dates (expected YYYY-MM-DD)".into()))?;

    // `source` owns the materialised tempdir; keep it alive for the whole run.
    let source = resolve_scan(&workspace_manager).await?;
    let runner = OxyMetricTreeRunner::new(workspace_manager, user.id, role)
        .with_scan_path(source.scan_path.clone())
        .with_preagg(
            preagg_ctx.cache,
            preagg_ctx.renewal_threshold_secs.unwrap_or(120),
        );
    let config = ExplainConfig::default();

    let result = tokio::time::timeout(
        EXPLAIN_TIMEOUT,
        runner.run_explain(
            req.target,
            req.time_dimension,
            (req.period.0.clone(), req.period.1.clone()),
            baseline,
            vec![],
            config,
        ),
    )
    .await
    .map_err(|_| {
        MetricTreeError::Op(format!(
            "distribution timed out after {}s — try narrowing the target measure",
            EXPLAIN_TIMEOUT.as_secs()
        ))
    })?
    .map_err(|e| MetricTreeError::Op(e.to_string()))?;

    Ok(Json(result))
}

/// Compute an `(start, end)` window of the same inclusive day-count as
/// `period`, ending the day before `period.0`. Returns `None` if either
/// bound is not a valid `YYYY-MM-DD` date.
pub(crate) fn derive_baseline_period(start: &str, end: &str) -> Option<(String, String)> {
    use chrono::{Duration, NaiveDate};
    let start = NaiveDate::parse_from_str(start, "%Y-%m-%d").ok()?;
    let end = NaiveDate::parse_from_str(end, "%Y-%m-%d").ok()?;
    if end < start {
        return None;
    }
    let duration_days = (end - start).num_days();
    let baseline_end = start - Duration::days(1);
    let baseline_start = baseline_end - Duration::days(duration_days);
    Some((baseline_start.to_string(), baseline_end.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The handler source, excluding this test module — which necessarily
    /// names the very symbols the tests below assert are absent.
    fn handler_src() -> &'static str {
        include_str!("metric_tree.rs")
            .split("\n#[cfg(test)]")
            .next()
            .expect("source splits on its own test-module attribute")
    }

    /// The handlers must resolve their scan root through the compile boundary,
    /// never from the workspace working copy — a stateless serve replica has
    /// none, and reading it there 500'd every metric-tree call on every
    /// workspace (oxy-hq/oxygen#878). `load_layer_sync` is gone, but
    /// `config_manager.semantics_scan_path()` is still one autocomplete away,
    /// and it compiles fine — it just fails in cloud. So guard the shape, in
    /// the spirit of `tests/authz/authz_boundaries.rs`.
    #[test]
    fn handlers_never_scan_the_working_copy() {
        assert!(
            !handler_src().contains("semantics_scan_path"),
            "metric_tree.rs must resolve its scan root via resolve_query_scan_source \
             (compile boundary first), not config_manager.semantics_scan_path()"
        );
    }

    /// Parsed from YAML rather than built field-by-field, so the fixture is
    /// bound to the real view schema instead of a hand-rolled approximation.
    fn layer(yaml: &str) -> airlayer::SemanticLayer {
        serde_yaml::from_str(yaml).expect("fixture layer should parse")
    }

    #[test]
    fn count_measure_id_names_the_denominator_rates_are_formed_against() {
        let l = layer(
            r#"
views:
  - name: orders
    measures:
      - name: gross_revenue
        type: sum
        expr: total_amount
      - name: total_orders
        type: count
"#,
        );

        // The sum is skipped: only a `count` can serve as the volume denominator.
        assert_eq!(
            count_measure_id(&l, "orders").as_deref(),
            Some("orders.total_orders")
        );
    }

    #[test]
    fn count_measure_id_takes_the_first_count_as_the_engine_does() {
        // The engine divides by the FIRST declared count, so reporting any other
        // would name a denominator it never used — the exact confusion this
        // field exists to prevent.
        let l = layer(
            r#"
views:
  - name: orders
    measures:
      - name: completed_orders
        type: count
      - name: total_orders
        type: count
"#,
        );

        assert_eq!(
            count_measure_id(&l, "orders").as_deref(),
            Some("orders.completed_orders")
        );
    }

    #[test]
    fn count_measure_id_is_absent_when_a_view_declares_no_count() {
        // Mirrors the engine refusing to size: no count, no rate, no denominator
        // to name.
        let l = layer(
            r#"
views:
  - name: orders
    measures:
      - name: gross_revenue
        type: sum
        expr: total_amount
"#,
        );

        assert_eq!(count_measure_id(&l, "orders"), None);
        assert_eq!(count_measure_id(&l, "nonexistent_view"), None);
    }

    fn sized_result(weight_basis: &str) -> OpportunityResult {
        OpportunityResult {
            target: "orders.gross_revenue".into(),
            period: ("2026-04-15".into(), "2026-07-13".into()),
            overall_value: 640_000.0,
            weight_basis: weight_basis.into(),
            dimensions: vec![],
            skipped_dimensions: vec![],
            downstream: vec![],
        }
    }

    #[test]
    fn response_flattens_the_denominator_alongside_the_engine_result() {
        // The client reads `rate_denominator` as a sibling of `weight_basis`; a
        // nested `result` object would silently break every field of the panel.
        let json = serde_json::to_value(OpportunityResponse {
            result: sized_result("rows"),
            rate_denominator: Some("orders.total_orders".into()),
        })
        .unwrap();

        assert_eq!(json["weight_basis"], "rows");
        assert_eq!(json["rate_denominator"], "orders.total_orders");
        assert_eq!(json["target"], "orders.gross_revenue");
        assert!(json.get("result").is_none(), "must not nest under `result`");
    }

    #[test]
    fn response_omits_the_denominator_when_no_rate_was_formed() {
        // In value_share/equal mode `current_value` is a raw measure value, not a
        // rate. Naming a denominator would invite reading it as one.
        let json = serde_json::to_value(OpportunityResponse {
            result: sized_result("equal"),
            rate_denominator: None,
        })
        .unwrap();

        assert!(json.get("rate_denominator").is_none());
    }

    #[test]
    fn drill_response_wire_format_is_stable() {
        use airlayer::engine::metric_tree_ops::{
            CandidateKind, DrillCandidate, DrillLevel, DrillResult, StopReason,
        };
        let resp = DrillResponse {
            result: Some(DrillResult {
                target: "orders.revenue".into(),
                root_gap: 500.0,
                root_upside: 276_000.0,
                benchmark_filter: vec![],
                levels: vec![DrillLevel {
                    measure: "orders.revenue".into(),
                    segment_filter: vec![],
                    gap: 500.0,
                    root_share: 1.0,
                    candidates: vec![DrillCandidate {
                        kind: CandidateKind::Dimension {
                            dimension: "orders.category".into(),
                            value: "sides".into(),
                        },
                        concentration: 0.9,
                        gap: 450.0,
                        gated: true,
                    }],
                    stop_reason: Some(StopReason::NoCandidates),
                }],
            }),
            rate_denominator: Some("orders.total_orders".into()),
        };
        let j = serde_json::to_value(&resp).unwrap();
        assert_eq!(j["rate_denominator"], "orders.total_orders");
        assert_eq!(j["target"], "orders.revenue"); // flattened, not nested under "result"
        let cand = &j["levels"][0]["candidates"][0];
        assert_eq!(cand["kind"]["Dimension"]["dimension"], "orders.category"); // externally tagged
        assert_eq!(cand["kind"]["Dimension"]["value"], "sides");
        assert_eq!(cand["gated"], true);
        assert_eq!(j["levels"][0]["stop_reason"], "NoCandidates"); // unit variant → string
    }

    #[test]
    fn drill_request_accepts_an_optional_root() {
        // Absent -> None: the top-pick path the panel uses before any row is expanded.
        let without: DrillRequest = serde_json::from_value(serde_json::json!({
            "target": "orders.revenue",
            "time_dimension": "orders.created_at",
            "period": ["2024-01-01", "2024-03-31"],
        }))
        .expect("root is optional");
        assert!(without.root.is_none());

        // Present -> the named row, verbatim.
        let with: DrillRequest = serde_json::from_value(serde_json::json!({
            "target": "orders.revenue",
            "time_dimension": "orders.created_at",
            "period": ["2024-01-01", "2024-03-31"],
            "root": { "dimension": "orders.channel", "segment": "mobile_app" },
        }))
        .expect("root must deserialize");
        let root = with.root.expect("root present");
        assert_eq!(root.dimension, "orders.channel");
        assert_eq!(root.segment, "mobile_app");
    }

    #[test]
    fn drill_response_none_serializes_without_result_fields() {
        // The frontend treats "no `levels` key" as nothing-to-drill. A None
        // result must flatten to an object carrying NEITHER the DrillResult
        // fields NOR rate_denominator, so the panel's empty-state branch fires.
        let resp = DrillResponse {
            result: None,
            rate_denominator: None,
        };
        let j = serde_json::to_value(&resp).unwrap();
        assert!(
            j.get("levels").is_none(),
            "None must not emit `levels`: {j}"
        );
        assert!(
            j.get("target").is_none(),
            "None must not emit `target`: {j}"
        );
        assert!(
            j.get("rate_denominator").is_none(),
            "a None denominator must be skipped: {j}"
        );
    }
}
