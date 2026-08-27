//! `/api/projects/{project_id}/semantic/metric-tree*` — metric-tree
//! analysis ops for customer-app bundles (drivers / what-if / RCA /
//! opportunity sizing).
//!
//! These mirror the IDE's workspace-scoped `/semantic/metric-tree*`
//! handlers ([`crate::server::api::metric_tree`]) but enter through the
//! customer-app gate and load the semantic layer from the compile boundary
//! (via [`super::semantic_boundary`]) rather than the workspace FS. Request /
//! response shapes are the exact same types the IDE endpoints use — the SDK
//! and the IDE never drift.
//!
//! **Fleet split (see `role_manifest.rs`).** Pure ops (tree, sensitivity,
//! predict, time-dimensions) need only the parsed layer, so they run on the
//! stateless serve fleet where published apps live (`FleetOk`). Query-executing
//! ops (explain, opportunity, distribution) run through [`OxyMetricTreeRunner`]:
//! the scan-path override pins the *layer* to the materialised boundary tempdir,
//! but the warehouse *connector* is still built from the customer-app
//! WorkspaceManager's config (`OxyProjectContext::build_connector_for`), which
//! resolves via the FS fallback — empty `databases: []` on a serve replica with
//! no working copy. So the query-executing ops are pinned `IdeOnly`, same as
//! `/query` and `/semantic-query`. The boundary guard is held for the whole
//! request so the tempdir outlives the run.

use airlayer::schema::models::DimensionType;
use axum::extract::{Path, Query};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use entity::workspace_members::WorkspaceRole;
use std::collections::BTreeMap;
use uuid::Uuid;

use crate::agentic_wiring::metric_tree_runner::OxyMetricTreeRunner;
use crate::server::api::custom_apps_gates::parse_versioned_body;
use crate::server::api::metric_tree::{
    self as mt, DistributionRequest, ExplainRequest, OpportunityRequest, PredictRequest,
    TimeDimensionsResponse, TreeQuery,
};
use crate::server::api::projects::semantic_boundary::{
    SemanticBoundary, cache_lookup, cache_store, enter_semantic_boundary, err_with_code,
    load_layer, wants_refresh,
};

/// Soft cap on explain/distribution — matches the workspace handler. A rich
/// schema can fire 50+ warehouse queries; failing loud beats hanging.
const EXPLAIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);

/// Enter the boundary and parse the layer — the pure-op prelude.
async fn boundary_with_layer(
    headers: &HeaderMap,
    project_id: Uuid,
) -> Result<(SemanticBoundary, airlayer::SemanticLayer), Response> {
    let boundary = enter_semantic_boundary(headers, project_id).await?;
    let layer = load_layer(boundary.scan.path_buf()).await?;
    Ok((boundary, layer))
}

/// Build a read-only runner pinned to the boundary's materialised scan path.
/// The caller must keep `boundary` alive for the whole run (tempdir guard).
fn runner_for(boundary: &SemanticBoundary) -> OxyMetricTreeRunner {
    OxyMetricTreeRunner::new(
        boundary
            .proj_ctx
            .workspace_manager()
            .clone()
            .into_read_only(),
        boundary.app.user.id,
        WorkspaceRole::Viewer,
    )
    .with_scan_path(boundary.scan.path_buf())
}

// ── Pure ops ────────────────────────────────────────────────────────────────

/// `GET .../metric-tree` — the full tree or `?root=<id>` subtree.
pub async fn get_metric_tree(
    Path(project_id): Path<Uuid>,
    Query(q): Query<TreeQuery>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    // Gate first, THEN read the cache: a cached hit must never bypass the
    // customer-app authorization gates.
    let boundary = match enter_semantic_boundary(&headers, project_id).await {
        Ok(b) => b,
        Err(resp) => return resp,
    };
    let key = q.root.clone().unwrap_or_default();
    if let Some(hit) = cache_lookup(project_id, "mt-tree", &key, wants_refresh(uri.query())) {
        return hit;
    }
    let layer = match load_layer(boundary.scan.path_buf()).await {
        Ok(l) => l,
        Err(resp) => return resp,
    };
    let tree = oxy_semantic::build_metric_tree(&layer);
    let out = match q.root {
        Some(root) => match oxy_semantic::subtree(&tree, &root) {
            Some(sub) => sub,
            None => {
                return err_with_code(
                    StatusCode::NOT_FOUND,
                    format!("measure '{root}' not in tree"),
                    "measure_not_found",
                );
            }
        },
        None => tree,
    };
    cache_store(boundary.project_id(), "mt-tree", &key, &out)
}

/// `GET .../metric-tree/{measure_id}/sensitivity` — ranked drivers.
pub async fn get_sensitivity(
    Path((project_id, measure_id)): Path<(Uuid, String)>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    // Gate before cache read (authorization must gate cached hits too).
    let boundary = match enter_semantic_boundary(&headers, project_id).await {
        Ok(b) => b,
        Err(resp) => return resp,
    };
    if let Some(hit) = cache_lookup(
        project_id,
        "mt-sens",
        &measure_id,
        wants_refresh(uri.query()),
    ) {
        return hit;
    }
    let layer = match load_layer(boundary.scan.path_buf()).await {
        Ok(l) => l,
        Err(resp) => return resp,
    };
    let tree = oxy_semantic::build_metric_tree(&layer);
    match oxy_semantic::sensitivity(&tree, &measure_id) {
        Ok(res) => cache_store(boundary.project_id(), "mt-sens", &measure_id, &res),
        Err(e) => err_with_code(StatusCode::BAD_REQUEST, e.to_string(), "sensitivity_failed"),
    }
}

/// `POST .../metric-tree/predict` — propagate hypothetical deltas.
pub async fn post_predict(
    Path(project_id): Path<Uuid>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    // `_boundary` (the tempdir guard) is held until the layer is parsed;
    // `predict` then operates on the in-memory tree, so it may drop after.
    let (_boundary, layer) = match boundary_with_layer(&headers, project_id).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let req: PredictRequest = match parse_versioned_body(&body) {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    let tree = oxy_semantic::build_metric_tree(&layer);
    let changes: Vec<(String, f64)> = req
        .changes
        .into_iter()
        .map(|c| (c.measure, c.delta))
        .collect();
    match oxy_semantic::predict(&tree, &changes) {
        Ok(res) => axum::Json(res).into_response(),
        Err(e) => err_with_code(StatusCode::BAD_REQUEST, e.to_string(), "predict_failed"),
    }
}

/// `GET .../metric-tree/time-dimensions` — valid time dims per view.
pub async fn get_time_dimensions(Path(project_id): Path<Uuid>, headers: HeaderMap) -> Response {
    let (_boundary, layer) = match boundary_with_layer(&headers, project_id).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
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
    axum::Json(TimeDimensionsResponse { by_view }).into_response()
}

// ── Query-executing ops ──────────────────────────────────────────────────────

/// `POST .../metric-tree/explain` — period-over-period root cause.
pub async fn post_explain(
    Path(project_id): Path<Uuid>,
    uri: Uri,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    use agentic_analytics::MetricTreeRunner as _;
    // Gate before cache read so a cached hit can't bypass authorization.
    let boundary = match enter_semantic_boundary(&headers, project_id).await {
        Ok(b) => b,
        Err(resp) => return resp,
    };
    let key = String::from_utf8_lossy(&body).into_owned();
    if let Some(hit) = cache_lookup(project_id, "mt-explain", &key, wants_refresh(uri.query())) {
        return hit;
    }
    let req: ExplainRequest = match parse_versioned_body(&body) {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    let runner = runner_for(&boundary);
    let config = mt::explain_config(req.config);
    let result = tokio::time::timeout(
        EXPLAIN_TIMEOUT,
        runner.run_explain(
            req.target,
            req.time_dimension,
            (req.current_period.0, req.current_period.1),
            (req.previous_period.0, req.previous_period.1),
            // Population-wide: this surface has no segment scope of its own.
            vec![],
            config,
        ),
    )
    .await;
    match result {
        Ok(Ok(r)) => cache_store(boundary.project_id(), "mt-explain", &key, &r),
        Ok(Err(e)) => err_with_code(StatusCode::BAD_REQUEST, e.to_string(), "explain_failed"),
        Err(_) => err_with_code(
            StatusCode::GATEWAY_TIMEOUT,
            format!(
                "explain timed out after {}s — narrow the target measure",
                EXPLAIN_TIMEOUT.as_secs()
            ),
            "explain_timeout",
        ),
    }
}

/// `POST .../metric-tree/opportunity` — segment opportunity sizing.
pub async fn post_opportunity(
    Path(project_id): Path<Uuid>,
    uri: Uri,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    use agentic_analytics::MetricTreeRunner as _;
    // Gate before cache read so a cached hit can't bypass authorization.
    let boundary = match enter_semantic_boundary(&headers, project_id).await {
        Ok(b) => b,
        Err(resp) => return resp,
    };
    let key = String::from_utf8_lossy(&body).into_owned();
    if let Some(hit) = cache_lookup(project_id, "mt-opp", &key, wants_refresh(uri.query())) {
        return hit;
    }
    let req: OpportunityRequest = match parse_versioned_body(&body) {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    // This surface sizes across the population; only the world-model instance
    // panel scopes a scan, and it does not come through here. Refuse rather than
    // drop the scope on the floor — silently returning population numbers to a
    // caller that asked about one instance is worse than saying no.
    if req.instance.is_some() {
        return err_with_code(
            StatusCode::BAD_REQUEST,
            "instance-scoped opportunity sizing is not available on this endpoint".to_string(),
            "opportunity_instance_unsupported",
        );
    }
    let runner = runner_for(&boundary);
    match runner
        .run_opportunity(req.target, req.time_dimension, (req.period.0, req.period.1))
        .await
    {
        Ok(r) => cache_store(boundary.project_id(), "mt-opp", &key, &r),
        Err(e) => err_with_code(StatusCode::BAD_REQUEST, e.to_string(), "opportunity_failed"),
    }
}

/// `POST .../metric-tree/distribution` — single-period distribution
/// (explain against an auto-derived immediately-prior baseline).
pub async fn post_distribution(
    Path(project_id): Path<Uuid>,
    uri: Uri,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    use agentic_analytics::MetricTreeRunner as _;
    // Gate before cache read so a cached hit can't bypass authorization.
    let boundary = match enter_semantic_boundary(&headers, project_id).await {
        Ok(b) => b,
        Err(resp) => return resp,
    };
    let key = String::from_utf8_lossy(&body).into_owned();
    if let Some(hit) = cache_lookup(project_id, "mt-dist", &key, wants_refresh(uri.query())) {
        return hit;
    }
    let req: DistributionRequest = match parse_versioned_body(&body) {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    let baseline = match mt::derive_baseline_period(&req.period.0, &req.period.1) {
        Some(b) => b,
        None => {
            return err_with_code(
                StatusCode::BAD_REQUEST,
                "invalid period dates (expected YYYY-MM-DD)",
                "invalid_period",
            );
        }
    };
    let runner = runner_for(&boundary);
    let result = tokio::time::timeout(
        EXPLAIN_TIMEOUT,
        runner.run_explain(
            req.target,
            req.time_dimension,
            (req.period.0.clone(), req.period.1.clone()),
            baseline,
            // Population-wide: this surface has no segment scope of its own.
            vec![],
            Default::default(),
        ),
    )
    .await;
    match result {
        Ok(Ok(r)) => cache_store(boundary.project_id(), "mt-dist", &key, &r),
        Ok(Err(e)) => err_with_code(
            StatusCode::BAD_REQUEST,
            e.to_string(),
            "distribution_failed",
        ),
        Err(_) => err_with_code(
            StatusCode::GATEWAY_TIMEOUT,
            format!(
                "distribution timed out after {}s",
                EXPLAIN_TIMEOUT.as_secs()
            ),
            "distribution_timeout",
        ),
    }
}
