use crate::agentic_wiring::OxyProjectContext;
use crate::server::api::middlewares::workspace_context::{
    EffectiveWorkspaceRole, WorkspaceManagerExtractor, WorkspacePath,
};
use crate::server::api::typed_stream::{
    EMPTY_RESULT_SENTINEL, typed_stream_to_json_array, typed_stream_to_parquet,
};

// `ResultFormat` and `SemanticQueryResponse` previously lived in the
// retired `crate::server::api::semantic` module. They're inlined here
// because data.rs is now their only consumer.
#[derive(Serialize, Deserialize, Clone, Debug, ToSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum ResultFormat {
    Parquet,
    #[default]
    Json,
}

#[derive(Serialize, ToSchema)]
#[serde(untagged)]
pub enum SemanticQueryResponse {
    Json(Vec<Vec<String>>),
    Parquet {
        file_name: String,
        is_preagg: bool,
        execution_time_ms: u64,
    },
}
use crate::server::service::retrieval::{ReindexInput, reindex};
use agentic_connector::{ConnectorError, DatabaseConnector, QueryFailedDetails};
use axum::extract::{self, Path};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use entity::workspace_members::WorkspaceRole;
use oxy::adapters::{session_filters::SessionFilters, workspace::manager::WorkspaceManager};
use oxy::config::model::ConnectionOverrides;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use oxy_shared::errors::OxyError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Clone, ToSchema)]
pub struct SQLParams {
    pub sql: String,
    pub database: String,

    #[serde(default)]
    pub filters: Option<SessionFilters>,

    #[serde(default)]
    #[schema(value_type = Object)]
    pub connections: Option<ConnectionOverrides>,

    #[serde(default)]
    pub result_format: Option<ResultFormat>,
}

#[derive(Serialize, ToSchema)]
pub struct EmbeddingsBuildResponse {
    pub success: bool,
    pub message: String,
}

/// Structured error body returned by the SQL execute endpoints.
///
/// `message` is always populated. The remaining fields are surfaced when the
/// underlying connector returned a `ConnectorError::QueryFailed` whose driver
/// exposes vendor metadata (Postgres SQLSTATE / DETAIL / HINT / POSITION). The
/// IDE renders these as a structured block; older clients can keep displaying
/// `message` alone.
#[derive(Serialize, ToSchema)]
pub struct SqlErrorResponse {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<u32>,
    /// The SQL the connector reported as failing. May differ from the input
    /// (e.g. agentic temp-table wrapping); kept here so the IDE can highlight
    /// the right span when `position` is set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql: Option<String>,
}

/// Internal error type that preserves connector-level structure on the way
/// from `run_via_agentic_connector` to `agentic_error_response`. Exposed
/// crate-wide so the `semantic` module can compose compile → execute
/// without re-implementing the connector error mapping.
pub(crate) enum SqlExecuteError {
    Connector(ConnectorError),
    Other(OxyError),
}

impl From<OxyError> for SqlExecuteError {
    fn from(e: OxyError) -> Self {
        Self::Other(e)
    }
}

impl SqlExecuteError {
    pub(crate) fn debug_string(&self) -> String {
        match self {
            Self::Connector(e) => format!("{e:?}"),
            Self::Other(e) => format!("{e:?}"),
        }
    }
}

/// Shape an `SqlExecuteError` into a (status, body) pair. Structured fields
/// from `ConnectorError::QueryFailed(details)` propagate; everything else
/// degrades to a single-line `message`.
pub(crate) fn agentic_error_response(
    payload: &SQLParams,
    err: SqlExecuteError,
) -> (StatusCode, extract::Json<SqlErrorResponse>) {
    tracing::error!(
        database = %payload.database,
        sql = %truncate_sql_for_log(&payload.sql),
        error.debug = %err.debug_string(),
        "SQL query execution failed"
    );

    // Status: 400 only for genuine user-side query errors (bad SQL,
    // missing columns, etc.). Upstream-unreachable (`ConnectionError`)
    // becomes 502 so the IDE shows a "warehouse is down" surface
    // distinguishable from "your SQL is wrong"; everything else
    // (decoder errors, internal driver bugs) is 500.
    let (status, body) = match err {
        SqlExecuteError::Connector(ConnectorError::QueryFailed(d)) => {
            let QueryFailedDetails {
                sql,
                message,
                code,
                detail,
                hint,
                position,
            } = d;
            (
                StatusCode::BAD_REQUEST,
                SqlErrorResponse {
                    message,
                    code,
                    detail,
                    hint,
                    position,
                    // Echo SQL only when it differs from what the user submitted —
                    // most of the time it's identical and would be noise.
                    sql: if sql != payload.sql { Some(sql) } else { None },
                },
            )
        }
        SqlExecuteError::Connector(ConnectorError::ConnectionError(msg)) => (
            StatusCode::BAD_GATEWAY,
            SqlErrorResponse {
                message: format!("connection error: {msg}"),
                code: None,
                detail: None,
                hint: None,
                position: None,
                sql: None,
            },
        ),
        SqlExecuteError::Connector(ConnectorError::Other(msg)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            SqlErrorResponse {
                message: msg,
                code: None,
                detail: None,
                hint: None,
                position: None,
                sql: None,
            },
        ),
        SqlExecuteError::Other(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            SqlErrorResponse {
                message: e.to_string(),
                code: None,
                detail: None,
                hint: None,
                position: None,
                sql: None,
            },
        ),
    };

    (status, extract::Json(body))
}

/// Execute a SQL query through `agentic-connector` and shape the response
/// according to the requested `result_format`. Every `DatabaseType` in
/// `oxy::config::model` has a landing spot in `OxyProjectContext`, so this
/// is now the single path for every Dev Portal query.
pub(crate) async fn run_via_agentic_connector(
    workspace_manager: &WorkspaceManager,
    user_id: Uuid,
    role: WorkspaceRole,
    payload: &SQLParams,
) -> Result<SemanticQueryResponse, SqlExecuteError> {
    let ctx = OxyProjectContext::new(workspace_manager.clone())
        .with_subject(user_id)
        .with_role(role);
    let connector = ctx.build_connector_for(&payload.database).await?;

    let query_start = std::time::Instant::now();
    let stream = connector
        .execute_query_full(&payload.sql)
        .await
        .map_err(SqlExecuteError::Connector)?;
    let execution_time_ms = query_start.elapsed().as_millis() as u64;

    let result_format = payload
        .result_format
        .as_ref()
        .unwrap_or(&ResultFormat::Json);
    match result_format {
        ResultFormat::Parquet => {
            let file_name = typed_stream_to_parquet(stream, workspace_manager)
                .await
                .map_err(SqlExecuteError::Other)?;
            if file_name == EMPTY_RESULT_SENTINEL {
                // DDL/DML or zero-column result — return empty JSON so the
                // frontend shows an empty table instead of a broken Parquet read.
                Ok(SemanticQueryResponse::Json(vec![]))
            } else {
                Ok(SemanticQueryResponse::Parquet {
                    file_name,
                    is_preagg: false,
                    execution_time_ms,
                })
            }
        }
        ResultFormat::Json => {
            let data = typed_stream_to_json_array(stream)
                .await
                .map_err(SqlExecuteError::Other)?;
            Ok(SemanticQueryResponse::Json(data))
        }
    }
}

/// Execute SQL against an already-built connector, returning JSON rows.
/// Use this when running multiple queries against the same database in one
/// handler — build the connector once with `OxyProjectContext::build_connector_for`
/// and pass it here to avoid paying the initialization cost per query.
/// Execute SQL against an already-built connector, returning data rows (header
/// row 0 stripped). The header is consistent with what `run_via_agentic_connector`
/// returns — callers that previously did `.skip(1)` after `run_via_agentic_connector`
/// can use this directly with `.first()` / `.next()`.
pub(crate) async fn run_with_connector(
    connector: &Arc<dyn DatabaseConnector>,
    sql: &str,
    _workspace_manager: &WorkspaceManager,
) -> Vec<Vec<String>> {
    match connector.execute_query_full(sql).await {
        Ok(stream) => typed_stream_to_json_array(stream)
            .await
            .unwrap_or_default()
            .into_iter()
            .skip(1)
            .collect(),
        Err(e) => {
            tracing::warn!(error = %e, sql, "run_with_connector: query failed, returning empty result");
            vec![]
        }
    }
}

/// Build a connector for the given database name, scoped to `user_id`/`role`.
pub(crate) async fn build_connector(
    workspace_manager: &WorkspaceManager,
    user_id: Uuid,
    role: WorkspaceRole,
    database: &str,
) -> Result<Arc<dyn DatabaseConnector>, OxyError> {
    let ctx = OxyProjectContext::new(workspace_manager.clone())
        .with_subject(user_id)
        .with_role(role);
    ctx.build_connector_for(database).await
}

/// Cap SQL length in structured log fields so one bad query doesn't flood the
/// log pipeline. The error response keeps the database name intact; the SQL
/// preview is just for operator triage.
fn truncate_sql_for_log(sql: &str) -> String {
    const MAX: usize = 500;
    if sql.len() <= MAX {
        sql.to_string()
    } else {
        // Find the largest char boundary at or below MAX so we don't split a
        // multi-byte UTF-8 sequence.
        let boundary = (0..=MAX)
            .rev()
            .find(|i| sql.is_char_boundary(*i))
            .unwrap_or(0);
        format!(
            "{}… [truncated, {} bytes total]",
            &sql[..boundary],
            sql.len()
        )
    }
}

pub async fn execute_sql(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    EffectiveWorkspaceRole(role): EffectiveWorkspaceRole,
    Path(WorkspacePath {
        workspace_id: _workspace_id,
    }): Path<WorkspacePath>,
    extract::Json(payload): extract::Json<SQLParams>,
) -> Result<extract::Json<SemanticQueryResponse>, (StatusCode, extract::Json<SqlErrorResponse>)> {
    run_via_agentic_connector(&workspace_manager, user.id, role, &payload)
        .await
        .map(extract::Json)
        .map_err(|e| agentic_error_response(&payload, e))
}

pub async fn execute_sql_query(
    workspace: WorkspaceManagerExtractor,
    user: AuthenticatedUserExtractor,
    role: EffectiveWorkspaceRole,
    path: Path<WorkspacePath>,
    payload: extract::Json<SQLParams>,
) -> Result<extract::Json<SemanticQueryResponse>, (StatusCode, extract::Json<SqlErrorResponse>)> {
    execute_sql(workspace, user, role, path, payload).await
}

// TODO: may want to rename this and the `reindex()` function below as we're doing more
//       only conditionally reindexing and doing more than just building embeddings:
//         - constructing retrieval items to store in lancedb
//         - calculating inclusion radius for each retrieval item
//         - caching enum values for each variable so they can be detected at query time
pub async fn build_embeddings(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Path(WorkspacePath {
        workspace_id: _workspace_id,
    }): Path<WorkspacePath>,
) -> Result<extract::Json<EmbeddingsBuildResponse>, Response> {
    handle_omni_sync(&workspace_manager)
        .await
        .map_err(|e| StatusCode::from(e).into_response())?;
    let config_manager = workspace_manager.config_manager;
    let secret_manager = workspace_manager.secrets_manager;
    let drop_all_tables = false;

    match reindex(ReindexInput {
        config: config_manager,
        secrets_manager: secret_manager,
        drop_all_tables,
    })
    .await
    {
        Ok(_) => Ok(extract::Json(EmbeddingsBuildResponse {
            success: true,
            message: "Embeddings built successfully".to_string(),
        })),
        Err(e) => {
            tracing::error!("Embeddings build failed: {}", e);
            Ok(extract::Json(EmbeddingsBuildResponse {
                success: false,
                message: format!("Embeddings build failed: {e}"),
            }))
        }
    }
}

async fn handle_omni_sync(workspace: &WorkspaceManager) -> Result<(), OxyError> {
    use crate::server::service::omni_sync::OmniSyncService;
    use omni::{OmniApiClient, OmniError as AdapterOmniError};

    let workspace_path = workspace.config_manager.workspace_path();

    let config = workspace.config_manager.clone();

    // Get all Omni integration configurations - if none found, skip silently
    let omni_integrations: Vec<_> = config
        .get_config()
        .integrations
        .iter()
        .filter_map(|integration| match &integration.integration_type {
            oxy::config::model::IntegrationType::Omni(omni_integration) => {
                Some((integration.name.clone(), omni_integration.clone()))
            }
            _ => None,
        })
        .collect();

    if omni_integrations.is_empty() {
        // No Omni integrations configured, skip silently
        return Ok(());
    }

    tracing::info!(
        "Synchronizing {} Omni integration(s)",
        omni_integrations.len()
    );

    let mut all_sync_results = Vec::new();
    let mut total_successful_topics = Vec::new();

    for (integration_name, omni_integration) in omni_integrations {
        tracing::info!(integration = %integration_name, "Processing Omni integration");

        // Resolve API key from environment variable
        let api_key = workspace
            .secrets_manager
            .resolve_secret(&omni_integration.api_key_var)
            .await?
            .unwrap();
        let base_url = omni_integration.base_url.clone();
        let topics = omni_integration.topics.clone();

        // Sync all configured topics for this integration
        tracing::debug!(integration = %integration_name, topic_count = topics.len(), "Synchronizing Omni metadata");
        let topics_to_sync: Vec<_> = topics.iter().collect();

        // Create API client
        let api_client =
            OmniApiClient::new(base_url.clone(), api_key.clone()).map_err(|e| match e {
                AdapterOmniError::ConfigError(msg) => {
                    OxyError::ConfigurationError(format!("Omni configuration error: {}", msg))
                }
                _ => OxyError::RuntimeError(format!("Failed to create Omni API client: {}", e)),
            })?;

        // Create sync service
        let sync_service =
            OmniSyncService::new(api_client, workspace_path, integration_name.clone());

        tracing::debug!("Fetching metadata from Omni API");

        let mut integration_results = Vec::new();
        for topic in &topics_to_sync {
            tracing::debug!(topic = %topic.name, model = %topic.model_id, "Syncing Omni topic");
            let sync_result = sync_service
                .sync_metadata(&topic.model_id, &topic.name)
                .await
                .map_err(|e| {
                    OxyError::RuntimeError(format!(
                        "Sync operation failed for topic '{}' (model '{}'): {}",
                        topic.name, topic.model_id, e
                    ))
                })?;
            integration_results.push(sync_result);
        }

        // Collect results for this integration
        if let Some(first_result) = integration_results.into_iter().next() {
            total_successful_topics.extend(first_result.successful_topics.clone());
            all_sync_results.push(first_result);
        }
    }

    tracing::info!("Omni synchronization completed");

    if !all_sync_results.is_empty() {
        let overall_success = all_sync_results.iter().all(|r| r.is_success());
        let partial_success = all_sync_results.iter().any(|r| r.is_partial_success());

        if overall_success {
            tracing::info!("All integrations synchronized successfully");
        } else if partial_success {
            tracing::warn!("Partial synchronization completed with some errors");
            for sync_result in &all_sync_results {
                if let Some(error_summary) = sync_result.error_summary() {
                    tracing::warn!(error = %error_summary, "Omni sync errors encountered");
                }
            }
        } else {
            tracing::error!("Some integrations failed to synchronize");
            for sync_result in &all_sync_results {
                if let Some(error_summary) = sync_result.error_summary() {
                    tracing::error!(error = %error_summary, "Omni sync errors encountered");
                }
            }
            return Err(OxyError::RuntimeError(
                "Some Omni sync operations failed".to_string(),
            ));
        }

        if !total_successful_topics.is_empty() {
            tracing::info!(topics = ?total_successful_topics, "Successfully synchronized topics");
        }
    }

    Ok(())
}
