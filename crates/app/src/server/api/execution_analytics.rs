//! Execution Analytics API
//!
//! Provides endpoints for querying execution analytics data,
//! tracking verified vs generated executions across different tool types.

use axum::{
    Router,
    extract::{Json, Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use oxy::execution_analytics::{
    AgentExecutionStats, ExecutionAnalyticsStorage, ExecutionListResponse, ExecutionSummary,
    ExecutionTimeBucket,
};
use serde::Deserialize;
use utoipa::IntoParams;
use uuid::Uuid;

use crate::server::router::AppState;

#[derive(Debug)]
pub enum ExecutionAnalyticsError {
    QueryFailed(String),
}

impl IntoResponse for ExecutionAnalyticsError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            ExecutionAnalyticsError::QueryFailed(err) => {
                tracing::error!("Execution analytics query failed: {}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to query execution analytics",
                )
            }
        };

        (status, message).into_response()
    }
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct SummaryQuery {
    /// Number of days to look back (default: 7)
    #[serde(default = "default_days")]
    pub days: u32,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct TimeSeriesQuery {
    /// Number of days to look back (default: 7)
    #[serde(default = "default_days")]
    pub days: u32,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct AgentStatsQuery {
    /// Number of days to look back (default: 7)
    #[serde(default = "default_days")]
    pub days: u32,
    /// Maximum number of agents to return (default: 10)
    #[serde(default = "default_agent_limit")]
    pub limit: usize,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ExecutionsQuery {
    /// Number of days to look back (default: 7)
    #[serde(default = "default_days")]
    pub days: u32,
    /// Maximum number of executions to return (default: 50)
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Offset for pagination (default: 0)
    #[serde(default)]
    pub offset: usize,
    /// Filter by execution type (semantic_query, omni_query, sql_generated, workflow, agent_tool)
    pub execution_type: Option<String>,
    /// Filter by verified status
    pub is_verified: Option<bool>,
    /// Filter by source reference (agent or workflow ref)
    pub source_ref: Option<String>,
    /// Filter by status (success, error)
    pub status: Option<String>,
}

fn default_days() -> u32 {
    7
}

fn default_limit() -> usize {
    50
}

fn default_agent_limit() -> usize {
    10
}

/// Get execution analytics summary
///
/// Returns aggregated statistics about verified vs generated executions
#[utoipa::path(
    get,
    path = "/api/{project_id}/execution-analytics/summary",
    params(SummaryQuery),
    responses(
        (status = 200, description = "Execution analytics summary", body = ExecutionSummary),
        (status = 500, description = "Query failed")
    )
)]
pub async fn get_summary(
    Path(_project_id): Path<Uuid>,
    Query(params): Query<SummaryQuery>,
) -> Result<Json<ExecutionSummary>, ExecutionAnalyticsError> {
    let storage = ExecutionAnalyticsStorage::from_env();

    let summary = storage
        .get_summary(params.days)
        .await
        .map_err(|e| ExecutionAnalyticsError::QueryFailed(e.to_string()))?;

    Ok(Json(summary))
}

/// Get execution analytics time series
///
/// Returns execution counts bucketed by day
#[utoipa::path(
    get,
    path = "/api/{project_id}/execution-analytics/time-series",
    params(TimeSeriesQuery),
    responses(
        (status = 200, description = "Execution time series data", body = Vec<ExecutionTimeBucket>),
        (status = 500, description = "Query failed")
    )
)]
pub async fn get_time_series(
    Path(_project_id): Path<Uuid>,
    Query(params): Query<TimeSeriesQuery>,
) -> Result<Json<Vec<ExecutionTimeBucket>>, ExecutionAnalyticsError> {
    let storage = ExecutionAnalyticsStorage::from_env();

    let time_series = storage
        .get_time_series(params.days)
        .await
        .map_err(|e| ExecutionAnalyticsError::QueryFailed(e.to_string()))?;

    Ok(Json(time_series))
}

/// Get per-agent execution statistics
///
/// Returns execution statistics grouped by agent
#[utoipa::path(
    get,
    path = "/api/{project_id}/execution-analytics/agents",
    params(AgentStatsQuery),
    responses(
        (status = 200, description = "Agent execution statistics", body = Vec<AgentExecutionStats>),
        (status = 500, description = "Query failed")
    )
)]
pub async fn get_agent_stats(
    Path(_project_id): Path<Uuid>,
    Query(params): Query<AgentStatsQuery>,
) -> Result<Json<Vec<AgentExecutionStats>>, ExecutionAnalyticsError> {
    let storage = ExecutionAnalyticsStorage::from_env();

    let agent_stats = storage
        .get_agent_stats(params.days, params.limit)
        .await
        .map_err(|e| ExecutionAnalyticsError::QueryFailed(e.to_string()))?;

    Ok(Json(agent_stats))
}

/// Get paginated execution details
///
/// Returns detailed execution records with filtering and pagination
#[utoipa::path(
    get,
    path = "/api/{project_id}/execution-analytics/executions",
    params(ExecutionsQuery),
    responses(
        (status = 200, description = "Paginated execution details", body = ExecutionListResponse),
        (status = 500, description = "Query failed")
    )
)]
pub async fn get_executions(
    Path(_project_id): Path<Uuid>,
    Query(params): Query<ExecutionsQuery>,
) -> Result<Json<ExecutionListResponse>, ExecutionAnalyticsError> {
    let storage = ExecutionAnalyticsStorage::from_env();

    let executions = storage
        .get_executions(
            params.days,
            params.limit,
            params.offset,
            params.execution_type.as_deref(),
            params.is_verified,
            params.source_ref.as_deref(),
            params.status.as_deref(),
        )
        .await
        .map_err(|e| ExecutionAnalyticsError::QueryFailed(e.to_string()))?;

    Ok(Json(executions))
}

pub fn execution_analytics_routes() -> Router<AppState> {
    Router::new()
        .route("/summary", get(get_summary))
        .route("/time-series", get(get_time_series))
        .route("/agents", get(get_agent_stats))
        .route("/executions", get(get_executions))
}
