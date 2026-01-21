//! Metrics Analytics API
//!
//! Provides endpoints for querying metric usage analytics data.

use axum::{
    Router,
    extract::{Json, Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use oxy::metrics::{
    MetricAnalyticsResponse, MetricDetailResponse, MetricStorage, MetricsListResponse,
};
use serde::Deserialize;
use utoipa::IntoParams;
use uuid::Uuid;

use crate::server::router::AppState;

/// Custom error type for metrics endpoints
#[derive(Debug)]
pub enum MetricsError {
    QueryFailed(String),
    NotFound(String),
}

impl IntoResponse for MetricsError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            MetricsError::QueryFailed(err) => {
                tracing::error!("Metrics query failed: {}", err);
                (StatusCode::INTERNAL_SERVER_ERROR, "Failed to query metrics")
            }
            MetricsError::NotFound(metric) => {
                tracing::warn!("Metric not found: {}", metric);
                (StatusCode::NOT_FOUND, "Metric not found")
            }
        };

        (status, message).into_response()
    }
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct MetricsAnalyticsQuery {
    /// Number of days to look back (default: 30)
    #[serde(default = "default_days")]
    pub days: u32,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct MetricsListQuery {
    /// Number of days to look back (default: 30)
    #[serde(default = "default_days")]
    pub days: u32,
    /// Maximum number of metrics to return (default: 20)
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Offset for pagination (default: 0)
    #[serde(default)]
    pub offset: usize,
}

fn default_days() -> u32 {
    30
}

fn default_limit() -> usize {
    20
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct MetricDetailQuery {
    /// Number of days to look back (default: 30)
    #[serde(default = "default_days")]
    pub days: u32,
}

/// Get metric usage analytics summary
///
/// Returns aggregated analytics about metric usage including:
/// - Total queries and unique metrics count
/// - Usage breakdown by source type (agent, workflow, task)
/// - Usage breakdown by context type (SQL, semantic query, question, response)
#[utoipa::path(
    get,
    path = "/api/{project_id}/metrics/analytics",
    params(MetricsAnalyticsQuery),
    responses(
        (status = 200, description = "Metric analytics data", body = MetricAnalyticsResponse),
        (status = 500, description = "Query failed")
    )
)]
pub async fn get_analytics(
    Path(_project_id): Path<Uuid>,
    Query(params): Query<MetricsAnalyticsQuery>,
) -> Result<Json<MetricAnalyticsResponse>, MetricsError> {
    let storage = MetricStorage::from_env();

    let analytics = storage
        .get_analytics(params.days)
        .await
        .map_err(|e| MetricsError::QueryFailed(e.to_string()))?;

    Ok(Json(analytics))
}

/// Get paginated list of metrics
///
/// Returns a paginated list of metrics sorted by usage count
#[utoipa::path(
    get,
    path = "/api/{project_id}/metrics/list",
    params(MetricsListQuery),
    responses(
        (status = 200, description = "Paginated metrics list", body = MetricsListResponse),
        (status = 500, description = "Query failed")
    )
)]
pub async fn get_metrics_list(
    Path(_project_id): Path<Uuid>,
    Query(params): Query<MetricsListQuery>,
) -> Result<Json<MetricsListResponse>, MetricsError> {
    let storage = MetricStorage::from_env();

    let list = storage
        .get_metrics_list(params.days, params.limit, params.offset)
        .await
        .map_err(|e| MetricsError::QueryFailed(e.to_string()))?;

    Ok(Json(list))
}

/// Get detailed information about a specific metric
///
/// Returns detailed usage information for a single metric including:
/// - Total usage count
/// - Usage breakdown by source and context type
/// - Related metrics (often used together)
/// - Usage trend over time
#[utoipa::path(
    get,
    path = "/api/{project_id}/metrics/{metric_name}",
    params(MetricDetailQuery),
    responses(
        (status = 200, description = "Metric detail data", body = MetricDetailResponse),
        (status = 404, description = "Metric not found"),
        (status = 500, description = "Query failed")
    )
)]
pub async fn get_metric_detail(
    Path((_project_id, metric_name)): Path<(Uuid, String)>,
    Query(params): Query<MetricDetailQuery>,
) -> Result<Json<MetricDetailResponse>, MetricsError> {
    let storage = MetricStorage::from_env();

    let detail = storage
        .get_metric_detail(&metric_name, params.days)
        .await
        .map_err(|e| MetricsError::QueryFailed(e.to_string()))?;

    // Check if metric was found (has any usage)
    if detail.total_queries == 0 {
        return Err(MetricsError::NotFound(metric_name));
    }

    Ok(Json(detail))
}

pub fn metrics_routes() -> Router<AppState> {
    Router::new()
        .route("/analytics", get(get_analytics))
        .route("/list", get(get_metrics_list))
        .route("/{metric_name}", get(get_metric_detail))
}
