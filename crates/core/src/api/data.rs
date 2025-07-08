use crate::adapters::connector::{Connector, load_result};
use crate::auth::extractor::AuthenticatedUserExtractor;
use crate::config::ConfigBuilder;
use crate::execute::types::utils::record_batches_to_2d_array;
use crate::project::resolve_project_path;
use crate::service::retrieval::{ReindexInput, reindex};
use axum::extract::{self, Path};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, Clone, ToSchema)]
pub struct SQLParams {
    pub sql: String,
    pub database: String,
}

#[derive(Serialize, ToSchema)]
pub struct EmbeddingsBuildResponse {
    pub success: bool,
    pub message: String,
}

pub async fn execute_sql(
    Path(_pathb64): Path<String>,
    extract::Json(payload): extract::Json<SQLParams>,
) -> Result<extract::Json<Vec<Vec<String>>>, StatusCode> {
    let project_path = resolve_project_path().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let config_builder = ConfigBuilder::new()
        .with_project_path(&project_path)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let config = config_builder
        .build()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let connector = Connector::from_database(&payload.database, &config, None).await?;
    let file_path = connector.run_query(&payload.sql).await?;

    let (batches, schema) =
        load_result(&file_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let data = record_batches_to_2d_array(&batches, &schema)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(extract::Json(data))
}

pub async fn build_embeddings(
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
) -> Result<extract::Json<EmbeddingsBuildResponse>, StatusCode> {
    let project_path = resolve_project_path().map_err(|e| {
        tracing::error!("Failed to find project path: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let drop_all_tables = true; // Always drop all tables

    match reindex(ReindexInput {
        project_path: project_path.to_string_lossy().to_string(),
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
