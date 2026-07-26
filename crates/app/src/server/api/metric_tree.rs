//! Metric Tree API — tree structure and analysis ops over the semantic layer.
//!
//! The tree is built per request from the workspace's semantic layer (the
//! same scan path `execute_semantic_query` uses). The pure ops — tree,
//! `sensitivity`, `predict` — need no database access. The query-executing
//! ops (`explain`, `opportunity`) run airlayer's algorithms against a
//! `QueryExecutor` bridged to Oxy's connector: airlayer compiles each
//! `QueryRequest` to SQL, and `run_via_agentic_connector` executes it.

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

use crate::agentic_wiring::metric_tree_runner::{OxyMetricTreeRunner, build_query_executor};
use crate::server::api::middlewares::workspace_context::{
    EffectiveWorkspaceRole, PreaggCacheCtx, WorkspaceManagerExtractor,
};

#[derive(Debug)]
pub enum MetricTreeError {
    LayerLoad(String),
    NotFound(String),
    Op(String),
}

impl IntoResponse for MetricTreeError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            MetricTreeError::LayerLoad(e) => {
                tracing::error!("metric-tree layer load failed: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to load semantic layer".to_string(),
                )
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
        (status, msg).into_response()
    }
}

/// Load the workspace's semantic layer from disk.
fn load_layer(
    workspace_manager: &WorkspaceManager,
) -> Result<airlayer::SemanticLayer, MetricTreeError> {
    OxyMetricTreeRunner::load_layer_sync(workspace_manager)
        .map_err(|e| MetricTreeError::LayerLoad(e.to_string()))
}

/// Build the metric tree for the workspace's semantic layer.
fn load_tree(workspace_manager: &WorkspaceManager) -> Result<MetricTree, MetricTreeError> {
    let layer = load_layer(workspace_manager)?;
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
    let tree = load_tree(&workspace_manager)?;
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
    let tree = load_tree(&workspace_manager)?;
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
    let tree = load_tree(&workspace_manager)?;
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

fn explain_config(over: Option<ExplainConfigOverride>) -> ExplainConfig {
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
    let runner = OxyMetricTreeRunner::new(workspace_manager, user.id, role).with_preagg(
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
}

/// `POST .../semantic/metric-tree/opportunity` — segment opportunity sizing.
pub async fn post_opportunity(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    EffectiveWorkspaceRole(role): EffectiveWorkspaceRole,
    preagg_ctx: PreaggCacheCtx,
    Json(req): Json<OpportunityRequest>,
) -> Result<Json<OpportunityResult>, MetricTreeError> {
    let layer = load_layer(&workspace_manager)?;
    let tree = oxy_semantic::build_metric_tree(&layer);
    let databases = workspace_databases(&workspace_manager);
    let engine = build_engine(layer.clone(), &databases)?;
    let handle = tokio::runtime::Handle::current();
    let scan_path = workspace_manager
        .config_manager
        .semantics_scan_path()
        .to_path_buf();
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
            scan_path,
            preagg_cache,
            preagg_renewal_threshold_secs,
        );
        airlayer::engine::metric_tree_ops::opportunity(
            &tree,
            &layer,
            &req.target,
            &req.time_dimension,
            (req.period.0.as_str(), req.period.1.as_str()),
            &executor,
        )
    })
    .await
    .map_err(|e| MetricTreeError::Op(format!("opportunity task panicked: {e}")))?
    .map_err(|e| MetricTreeError::Op(e.to_string()))?;

    Ok(Json(result))
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

    let layer = load_layer(&workspace_manager)?;
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

    let runner = OxyMetricTreeRunner::new(workspace_manager, user.id, role).with_preagg(
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
fn derive_baseline_period(start: &str, end: &str) -> Option<(String, String)> {
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
