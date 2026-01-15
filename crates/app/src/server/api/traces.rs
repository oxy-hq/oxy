use axum::{
    Router,
    extract::{self, Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use clickhouse::Row;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::server::router::AppState;
use oxy::storage::clickhouse::ClickHouseStorage;

/// Custom error type for trace endpoints
#[derive(Debug)]
pub enum TracesError {
    QueryFailed(String),
    NotFound(String),
}

impl IntoResponse for TracesError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            TracesError::QueryFailed(err) => {
                tracing::error!("ClickHouse query failed: {}", err);
                (StatusCode::INTERNAL_SERVER_ERROR, "Failed to query traces")
            }
            TracesError::NotFound(trace_id) => {
                tracing::warn!("Trace not found: {}", trace_id);
                (StatusCode::NOT_FOUND, "Trace not found")
            }
        };

        (status, message).into_response()
    }
}

#[derive(Debug, Serialize, Deserialize, IntoParams)]
pub struct TraceListQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    pub agent_ref: Option<String>,
    pub status: Option<String>,
    /// Duration filter: 1h, 24h, 7d, 30d, or all
    pub duration: Option<String>,
}

fn default_limit() -> i64 {
    50
}

#[derive(Debug, Serialize, Deserialize, Row, ToSchema)]
pub struct TraceResponse {
    #[serde(rename = "traceId")]
    pub trace_id: String,
    #[serde(rename = "spanId")]
    pub span_id: String,
    pub timestamp: String,
    #[serde(rename = "spanName")]
    pub span_name: String,
    #[serde(rename = "serviceName")]
    pub service_name: String,
    #[serde(rename = "durationNs")]
    pub duration_ns: i64,
    #[serde(rename = "statusCode")]
    pub status_code: String,
    #[serde(rename = "statusMessage")]
    pub status_message: String,
    #[serde(rename = "spanKind")]
    pub span_kind: String,
    #[serde(rename = "spanAttributes")]
    pub span_attributes: Vec<(String, String)>,
    #[serde(rename = "eventsAttributes")]
    pub events_attributes: Vec<Vec<(String, String)>>,
    #[serde(rename = "promptTokens")]
    pub prompt_tokens: i64,
    #[serde(rename = "completionTokens")]
    pub completion_tokens: i64,
    #[serde(rename = "totalTokens")]
    pub total_tokens: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PaginatedTraceResponse {
    pub items: Vec<TraceResponse>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

/// Full trace span detail response (all columns from otel_traces table)
#[derive(Debug, Serialize, Deserialize, Row, ToSchema)]
pub struct TraceDetailSpan {
    #[serde(rename = "timestamp")]
    pub timestamp: String,
    #[serde(rename = "traceId")]
    pub trace_id: String,
    #[serde(rename = "spanId")]
    pub span_id: String,
    #[serde(rename = "parentSpanId")]
    pub parent_span_id: String,
    #[serde(rename = "spanName")]
    pub span_name: String,
    #[serde(rename = "spanKind")]
    pub span_kind: String,
    #[serde(rename = "serviceName")]
    pub service_name: String,
    #[serde(rename = "spanAttributes")]
    pub span_attributes: Vec<(String, String)>,
    #[serde(rename = "duration")]
    pub duration: i64,
    #[serde(rename = "statusCode")]
    pub status_code: String,
    #[serde(rename = "statusMessage")]
    pub status_message: String,
    #[serde(rename = "eventsName")]
    pub events_name: Vec<String>,
    #[serde(rename = "eventsAttributes")]
    pub events_attributes: Vec<Vec<(String, String)>>,
}

/// Helper function to get ClickHouse storage client
fn get_clickhouse_storage() -> ClickHouseStorage {
    ClickHouseStorage::from_env()
}

/// List recent traces
#[utoipa::path(
    get,
    path = "/api/traces",
    responses(
        (status = 200, description = "List of traces", body = PaginatedTraceResponse)
    ),
    params(TraceListQuery)
)]
pub async fn list_traces(
    Query(params): Query<TraceListQuery>,
) -> Result<extract::Json<PaginatedTraceResponse>, TracesError> {
    let storage = get_clickhouse_storage();

    // Build WHERE clause for both count and data queries
    // Collect only root traces (no parent span) for workflow.run_workflow or agent.run_agent
    let mut where_clause = String::from(
        "WHERE (t.SpanName = 'workflow.run_workflow' OR t.SpanName = 'agent.run_agent') AND t.ParentSpanId = ''",
    );

    if let Some(ref agent_ref) = params.agent_ref {
        where_clause.push_str(&format!(
            " AND t.SpanAttributes['agent.ref'] = '{}'",
            agent_ref.replace("'", "''")
        ));
    }

    if let Some(ref status) = params.status {
        where_clause.push_str(&format!(
            " AND t.StatusCode = '{}'",
            status.replace("'", "''")
        ));
    }

    // Apply duration filter
    if let Some(ref duration) = params.duration {
        let interval = match duration.as_str() {
            "1h" => "1 HOUR",
            "24h" => "24 HOUR",
            "7d" => "7 DAY",
            "30d" => "30 DAY",
            _ => "", // "all" or any other value - no filter
        };
        if !interval.is_empty() {
            where_clause.push_str(&format!(
                " AND t.Timestamp >= now() - INTERVAL {}",
                interval
            ));
        }
    }

    // Count query
    let count_query = format!(
        "SELECT count() as cnt FROM otel.otel_traces t {}",
        where_clause
    );

    #[derive(Row, Deserialize)]
    struct CountResult {
        cnt: u64,
    }

    let count_result = storage
        .client()
        .query(&count_query)
        .fetch_one::<CountResult>()
        .await
        .map_err(|e| TracesError::QueryFailed(e.to_string()))?;

    let total = count_result.cnt as i64;

    // Data query
    let data_query = format!(
        "SELECT 
            t.TraceId as trace_id,
            t.SpanId as span_id,
            toString(t.Timestamp) as timestamp,
            t.SpanName as span_name,
            t.ServiceName as service_name,
            t.Duration as duration_ns,
            t.StatusCode as status_code,
            t.StatusMessage as status_message,
            t.SpanKind as span_kind,
            t.SpanAttributes as span_attributes,
            t.Events.Attributes as events_attributes,
            COALESCE(u.prompt_tokens, 0) as prompt_tokens,
            COALESCE(u.completion_tokens, 0) as completion_tokens,
            COALESCE(u.total_tokens, 0) as total_tokens
        FROM otel.otel_traces t
        LEFT JOIN (
            SELECT 
                TraceId,
                SUM(arraySum(arrayMap(
                    attrs -> if(attrs['name'] = 'llm.usage', toInt64OrZero(attrs['prompt_tokens']), 0),
                    Events.Attributes
                ))) as prompt_tokens,
                SUM(arraySum(arrayMap(
                    attrs -> if(attrs['name'] = 'llm.usage', toInt64OrZero(attrs['completion_tokens']), 0),
                    Events.Attributes
                ))) as completion_tokens,
                SUM(arraySum(arrayMap(
                    attrs -> if(attrs['name'] = 'llm.usage', toInt64OrZero(attrs['total_tokens']), 0),
                    Events.Attributes
                ))) as total_tokens
            FROM otel.otel_traces
            GROUP BY TraceId
        ) u ON t.TraceId = u.TraceId
        {}
        ORDER BY t.Timestamp DESC LIMIT {} OFFSET {}",
        where_clause, params.limit, params.offset
    );

    let traces = storage
        .client()
        .query(&data_query)
        .fetch_all::<TraceResponse>()
        .await
        .map_err(|e| TracesError::QueryFailed(e.to_string()))?;

    Ok(extract::Json(PaginatedTraceResponse {
        items: traces,
        total,
        limit: params.limit,
        offset: params.offset,
    }))
}

/// Get trace detail by TraceId
#[utoipa::path(
    get,
    path = "/api/traces/{trace_id}",
    responses(
        (status = 200, description = "Trace detail with all spans", body = Vec<TraceDetailSpan>),
        (status = 404, description = "Trace not found")
    ),
    params(
        ("trace_id" = String, Path, description = "The trace ID to retrieve")
    )
)]
pub async fn get_trace_detail(
    Path((_project_id, trace_id)): Path<(Uuid, String)>,
) -> Result<extract::Json<Vec<TraceDetailSpan>>, TracesError> {
    let storage = get_clickhouse_storage();

    let query = format!(
        "SELECT 
            toString(Timestamp) as timestamp,
            TraceId as trace_id,
            SpanId as span_id,
            ParentSpanId as parent_span_id,
            SpanName as span_name,
            SpanKind as span_kind,
            ServiceName as service_name,
            SpanAttributes as span_attributes,
            Duration as duration,
            StatusCode as status_code,
            StatusMessage as status_message,
            Events.Name as events_name,
            Events.Attributes as events_attributes
        FROM otel.otel_traces
        WHERE TraceId = '{}'
        ORDER BY Timestamp ASC",
        trace_id.replace("'", "''")
    );

    let spans = storage
        .client()
        .query(&query)
        .fetch_all::<TraceDetailSpan>()
        .await
        .map_err(|e| {
            tracing::error!("ClickHouse query error: {:?}", e);
            TracesError::QueryFailed(e.to_string())
        })?;

    if spans.is_empty() {
        return Err(TracesError::NotFound(trace_id));
    }

    Ok(extract::Json(spans))
}

// ============================================================================
// Cluster Map API - 2D visualization of intent clusters
// ============================================================================

/// A point in the cluster map visualization
#[derive(Debug, Serialize, ToSchema)]
pub struct ClusterMapPoint {
    /// Unique trace ID
    #[serde(rename = "traceId")]
    pub trace_id: String,
    /// The question/prompt text
    pub question: String,
    /// X coordinate (2D projection)
    pub x: f32,
    /// Y coordinate (2D projection)
    pub y: f32,
    /// Cluster ID (-1 for outliers)
    #[serde(rename = "clusterId")]
    pub cluster_id: i32,
    /// Intent name (or "outlier" for unclustered)
    #[serde(rename = "intentName")]
    pub intent_name: String,
    /// Classification confidence
    pub confidence: f32,
    /// Timestamp of the trace
    pub timestamp: String,
    /// Agent response/output (if available)
    pub output: Option<String>,
    /// Duration in milliseconds
    #[serde(rename = "durationMs")]
    pub duration_ms: Option<f64>,
}

/// Cluster summary for the legend
#[derive(Debug, Serialize, ToSchema)]
pub struct ClusterSummary {
    /// Cluster ID
    #[serde(rename = "clusterId")]
    pub cluster_id: i32,
    /// Intent name
    #[serde(rename = "intentName")]
    pub intent_name: String,
    /// Intent description
    pub description: String,
    /// Number of points in this cluster
    pub count: usize,
    /// Color for this cluster (hex)
    pub color: String,
    /// Sample questions
    #[serde(rename = "sampleQuestions")]
    pub sample_questions: Vec<String>,
}

/// Response for the cluster map visualization
#[derive(Debug, Serialize, ToSchema)]
pub struct ClusterMapResponse {
    /// All points in the visualization
    pub points: Vec<ClusterMapPoint>,
    /// Summary of each cluster
    pub clusters: Vec<ClusterSummary>,
    /// Total number of points
    #[serde(rename = "totalPoints")]
    pub total_points: usize,
    /// Number of outliers
    #[serde(rename = "outlierCount")]
    pub outlier_count: usize,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ClusterMapQuery {
    /// Maximum number of points to return
    #[serde(default = "default_cluster_map_limit")]
    pub limit: usize,
    /// Number of days to look back
    #[serde(default = "default_cluster_map_days")]
    pub days: u32,
    /// Filter by source (e.g., agent ref)
    pub source: Option<String>,
}

fn default_cluster_map_limit() -> usize {
    500
}

fn default_cluster_map_days() -> u32 {
    30
}

/// Row for reading embeddings with classification data
#[derive(Debug, Row, Deserialize)]
struct EmbeddingWithClassification {
    #[serde(rename = "TraceId")]
    trace_id: String,
    #[serde(rename = "Question")]
    question: String,
    #[serde(rename = "Embedding")]
    embedding: Vec<f32>,
    #[serde(rename = "ClusterId")]
    cluster_id: u32,
    #[serde(rename = "IntentName")]
    intent_name: String,
    #[serde(rename = "Confidence")]
    confidence: f32,
    #[serde(rename = "classified_at_str")]
    classified_at: String,
    #[serde(rename = "Source")]
    source: String,
}

/// Row for reading cluster info
#[derive(Debug, Row, Deserialize)]
struct ClusterInfoRow {
    #[serde(rename = "ClusterId")]
    cluster_id: u32,
    #[serde(rename = "IntentName")]
    intent_name: String,
    #[serde(rename = "IntentDescription")]
    intent_description: String,
    #[serde(rename = "SampleQuestions")]
    sample_questions: Vec<String>,
}

/// Get cluster map data for visualization
#[utoipa::path(
    get,
    path = "/clusters/map",
    params(ClusterMapQuery),
    responses(
        (status = 200, description = "Cluster map data", body = ClusterMapResponse),
        (status = 503, description = "ClickHouse unavailable")
    )
)]
pub async fn get_cluster_map(
    Query(query): Query<ClusterMapQuery>,
) -> Result<extract::Json<ClusterMapResponse>, TracesError> {
    let storage = get_clickhouse_storage();

    // Check if intent_classifications table exists first
    let table_exists_query = "SELECT count() FROM system.tables WHERE database = 'otel' AND name = 'intent_classifications'";

    #[derive(Debug, Row, Deserialize)]
    struct TableExistsResult {
        #[serde(rename = "count()")]
        count: u64,
    }

    let table_exists = storage
        .client()
        .query(table_exists_query)
        .fetch_one::<TableExistsResult>()
        .await
        .map(|result| result.count > 0)
        .unwrap_or(false);

    if !table_exists {
        // Return empty response if tables don't exist yet
        return Ok(extract::Json(ClusterMapResponse {
            points: vec![],
            clusters: vec![],
            total_points: 0,
            outlier_count: 0,
        }));
    }

    // Fetch embeddings with their classifications
    let mut where_conditions = vec![format!(
        "ClassifiedAt >= now64() - INTERVAL {} DAY",
        query.days
    )];

    if let Some(ref source) = query.source {
        where_conditions.push(format!("Source = '{}'", source.replace("'", "''")));
    }

    let embeddings_query = format!(
        r#"
        SELECT
            TraceId,
            Question,
            Embedding,
            ClusterId,
            IntentName,
            Confidence,
            toString(ClassifiedAt) as classified_at_str,
            Source
        FROM intent_classifications
        WHERE {}
        ORDER BY ClassifiedAt DESC
        LIMIT {}
        "#,
        where_conditions.join(" AND "),
        query.limit
    );

    let embeddings: Vec<EmbeddingWithClassification> = storage
        .client()
        .query(&embeddings_query)
        .fetch_all()
        .await
        .map_err(|e| TracesError::QueryFailed(e.to_string()))?;

    if embeddings.is_empty() {
        return Ok(extract::Json(ClusterMapResponse {
            points: vec![],
            clusters: vec![],
            total_points: 0,
            outlier_count: 0,
        }));
    }

    // Fetch cluster info
    let clusters_query = r#"
        SELECT 
            ClusterId,
            IntentName,
            IntentDescription,
            SampleQuestions
        FROM intent_clusters
        ORDER BY ClusterId
    "#;

    let cluster_infos: Vec<ClusterInfoRow> = storage
        .client()
        .query(clusters_query)
        .fetch_all()
        .await
        .unwrap_or_default();

    // Project embeddings to 2D using simple PCA-like approach
    let points_2d = project_to_2d(&embeddings);

    // Generate colors for clusters and build a color map by cluster_id
    let cluster_colors = generate_cluster_colors(cluster_infos.len() + 1); // +1 for outliers
    let mut cluster_color_map: std::collections::HashMap<i32, String> =
        std::collections::HashMap::new();
    cluster_color_map.insert(-1, cluster_colors[0].clone()); // Outliers get first color
    for (i, c) in cluster_infos.iter().enumerate() {
        cluster_color_map.insert(
            c.cluster_id as i32,
            cluster_colors
                .get(i + 1)
                .cloned()
                .unwrap_or_else(|| "#6b7280".to_string()),
        );
    }

    // Build points
    let mut points: Vec<ClusterMapPoint> = Vec::with_capacity(embeddings.len());
    let mut cluster_counts: std::collections::HashMap<i32, usize> =
        std::collections::HashMap::new();

    for (i, emb) in embeddings.iter().enumerate() {
        let (x, y) = points_2d[i];
        let cluster_id = if emb.intent_name == "unknown" {
            -1
        } else {
            emb.cluster_id as i32
        };

        *cluster_counts.entry(cluster_id).or_insert(0) += 1;

        points.push(ClusterMapPoint {
            trace_id: emb.trace_id.clone(),
            question: emb.question.clone(),
            x,
            y,
            cluster_id,
            intent_name: emb.intent_name.clone(),
            confidence: emb.confidence,
            timestamp: emb.classified_at.clone(),
            output: None, // Could be enriched from traces if needed
            duration_ms: None,
        });
    }

    // Build cluster summaries
    let mut clusters: Vec<ClusterSummary> = cluster_infos
        .iter()
        .map(|c| ClusterSummary {
            cluster_id: c.cluster_id as i32,
            intent_name: c.intent_name.clone(),
            description: c.intent_description.clone(),
            count: *cluster_counts.get(&(c.cluster_id as i32)).unwrap_or(&0),
            color: cluster_color_map
                .get(&(c.cluster_id as i32))
                .cloned()
                .unwrap_or_else(|| "#6b7280".to_string()),
            sample_questions: c.sample_questions.clone(),
        })
        .filter(|c| c.count > 0)
        .collect();

    // Add outlier "cluster" if there are any
    let outlier_count = *cluster_counts.get(&-1).unwrap_or(&0);
    if outlier_count > 0 {
        clusters.insert(
            0,
            ClusterSummary {
                cluster_id: -1,
                intent_name: "Outliers".to_string(),
                description: "Questions that don't fit any cluster".to_string(),
                count: outlier_count,
                color: cluster_color_map
                    .get(&-1)
                    .cloned()
                    .unwrap_or_else(|| "#6b7280".to_string()),
                sample_questions: vec![],
            },
        );
    }

    let total_points = points.len();

    Ok(extract::Json(ClusterMapResponse {
        points,
        clusters,
        total_points,
        outlier_count,
    }))
}

/// Fast 2D projection using PCA (Principal Component Analysis)
/// PCA is O(n) and provides instant results for real-time API queries
/// For better cluster visualization, consider pre-computing t-SNE/UMAP
/// coordinates during intent classification and storing in ClickHouse
fn project_to_2d(embeddings: &[EmbeddingWithClassification]) -> Vec<(f32, f32)> {
    use linfa::prelude::*;
    use linfa_reduction::Pca;
    use ndarray::{Array2, Axis};

    if embeddings.is_empty() {
        return vec![];
    }

    let n = embeddings.len();
    let dims = embeddings[0].embedding.len();

    if dims == 0 {
        return vec![(0.0, 0.0); n];
    }

    if n < 2 {
        return vec![(0.0, 0.0); n];
    }

    // Convert embeddings to ndarray matrix (n_samples x n_features)
    let mut data = Array2::<f64>::zeros((n, dims));
    for (i, emb) in embeddings.iter().enumerate() {
        for (j, &val) in emb.embedding.iter().enumerate() {
            data[[i, j]] = val as f64;
        }
    }

    // Create a Dataset from the array
    let dataset = DatasetBase::from(data);

    // Fit PCA to reduce to 2 dimensions - this is very fast O(n*d)
    let pca = match Pca::params(2).fit(&dataset) {
        Ok(pca) => pca,
        Err(e) => {
            tracing::warn!("PCA fitting failed: {}", e);
            return vec![(0.0, 0.0); n];
        }
    };

    // Transform the data to 2D
    let projected = pca.transform(dataset);

    // Extract 2D coordinates and normalize to display range
    let mut positions: Vec<(f64, f64)> = projected
        .records()
        .axis_iter(Axis(0))
        .map(|row| (row[0], row[1]))
        .collect();

    // Normalize to display range [-400, 400] x [-300, 300]
    normalize_positions(&mut positions);

    positions
        .iter()
        .map(|(x, y)| (*x as f32, *y as f32))
        .collect()
}

/// Normalize positions to display range [-400, 400] x [-300, 300]
fn normalize_positions(positions: &mut [(f64, f64)]) {
    if positions.is_empty() {
        return;
    }

    let (min_x, max_x, min_y, max_y) = positions.iter().fold(
        (f64::MAX, f64::MIN, f64::MAX, f64::MIN),
        |(min_x, max_x, min_y, max_y), (x, y)| {
            (min_x.min(*x), max_x.max(*x), min_y.min(*y), max_y.max(*y))
        },
    );

    let range_x = (max_x - min_x).max(0.001);
    let range_y = (max_y - min_y).max(0.001);

    for (x, y) in positions.iter_mut() {
        *x = (*x - min_x) / range_x * 800.0 - 400.0;
        *y = (*y - min_y) / range_y * 600.0 - 300.0;
    }
}

/// Generate distinct colors for clusters
fn generate_cluster_colors(n: usize) -> Vec<String> {
    // Predefined palette of distinct colors
    let palette = [
        "#9ca3af", // Gray for outliers
        "#3b82f6", // Blue
        "#ef4444", // Red
        "#22c55e", // Green
        "#f59e0b", // Amber
        "#8b5cf6", // Purple
        "#06b6d4", // Cyan
        "#ec4899", // Pink
        "#f97316", // Orange
        "#84cc16", // Lime
        "#14b8a6", // Teal
        "#6366f1", // Indigo
        "#a855f7", // Violet
        "#0ea5e9", // Sky
        "#10b981", // Emerald
        "#eab308", // Yellow
    ];

    (0..n)
        .map(|i| palette[i % palette.len()].to_string())
        .collect()
}

pub fn traces_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_traces))
        .route("/{trace_id}", get(get_trace_detail))
        .route("/clusters/map", get(get_cluster_map))
}
