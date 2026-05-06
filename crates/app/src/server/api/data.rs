use crate::agentic_wiring::OxyProjectContext;
use crate::server::api::middlewares::workspace_context::{
    WorkspaceManagerExtractor, WorkspacePath,
};
use crate::server::api::semantic::{ErrorResponse, ResultFormat, SemanticQueryResponse};
use crate::server::api::typed_stream::{
    EMPTY_RESULT_SENTINEL, typed_stream_to_json_array, typed_stream_to_parquet,
};
use crate::server::service::retrieval::{ReindexInput, reindex};
use agentic_pipeline::platform::ProjectContext;
use axum::extract::{self, Path};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use oxy::adapters::{session_filters::SessionFilters, workspace::manager::WorkspaceManager};
use oxy::config::model::ConnectionOverrides;
use oxy::execute::types::utils::record_batches_to_2d_array;
use oxy_shared::errors::OxyError;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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

/// Render an `OxyError` from [`run_via_agentic_connector`] as the (status,
/// body) tuple the handlers return on failure.
fn agentic_error_response(
    payload: &SQLParams,
    err: OxyError,
) -> (StatusCode, extract::Json<ErrorResponse>) {
    tracing::error!(
        database = %payload.database,
        sql = %truncate_sql_for_log(&payload.sql),
        error.debug = ?err,
        "SQL query execution failed"
    );
    (
        StatusCode::BAD_REQUEST,
        extract::Json(ErrorResponse {
            message: user_facing_query_error(&err),
        }),
    )
}

/// Extract a clean user-facing message from a connector error.
///
/// Strips the internal `\nSQL: …` suffix (redundant — the user can see what
/// they typed) and the `"query failed: db error: ERROR: "` prefix chain that
/// tokio-postgres wraps around server errors.
fn user_facing_query_error(err: &OxyError) -> String {
    let raw = err.to_string();
    // Drop the internal SQL echo that connector errors append.
    let without_sql = raw.split("\nSQL:").next().unwrap_or(&raw);
    // Unwrap the prefix chain added by tokio-postgres / connector wrappers.
    let msg = without_sql
        .trim_start_matches("query failed: db error: ERROR: ")
        .trim_start_matches("db error: ERROR: ")
        .trim_start_matches("query failed: ")
        .trim();
    msg.to_string()
}

/// Execute a SQL query through `agentic-connector` and shape the response
/// according to the requested `result_format`. Every `DatabaseType` in
/// `oxy::config::model` has a landing spot in `OxyProjectContext`, so this
/// is now the single path for every Dev Portal query.
async fn run_via_agentic_connector(
    workspace_manager: &WorkspaceManager,
    payload: &SQLParams,
) -> Result<SemanticQueryResponse, OxyError> {
    let ctx = OxyProjectContext::new(workspace_manager.clone());
    let connector = ctx.build_connector_for(&payload.database).await?;

    let stream = connector
        .execute_query_full(&payload.sql)
        .await
        .map_err(|e| OxyError::DBError(e.to_string()))?;

    let result_format = payload
        .result_format
        .as_ref()
        .unwrap_or(&ResultFormat::Json);
    match result_format {
        ResultFormat::Parquet => {
            let file_name = typed_stream_to_parquet(stream, workspace_manager).await?;
            if file_name == EMPTY_RESULT_SENTINEL {
                // DDL/DML or zero-column result — return empty JSON so the
                // frontend shows an empty table instead of a broken Parquet read.
                Ok(SemanticQueryResponse::Json(vec![]))
            } else {
                Ok(SemanticQueryResponse::Parquet { file_name })
            }
        }
        ResultFormat::Json => {
            let data = typed_stream_to_json_array(stream).await?;
            Ok(SemanticQueryResponse::Json(data))
        }
    }
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
    Path(WorkspacePath {
        workspace_id: _workspace_id,
    }): Path<WorkspacePath>,
    extract::Json(payload): extract::Json<SQLParams>,
) -> Result<extract::Json<SemanticQueryResponse>, (StatusCode, extract::Json<ErrorResponse>)> {
    run_via_agentic_connector(&workspace_manager, &payload)
        .await
        .map(extract::Json)
        .map_err(|e| agentic_error_response(&payload, e))
}

pub async fn execute_sql_query(
    extractor: WorkspaceManagerExtractor,
    path: Path<WorkspacePath>,
    payload: extract::Json<SQLParams>,
) -> Result<extract::Json<SemanticQueryResponse>, (StatusCode, extract::Json<ErrorResponse>)> {
    execute_sql(extractor, path, payload).await
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
