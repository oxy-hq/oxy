use crate::adapters::connector::{Connector, load_result};
use crate::api::middlewares::project::{ProjectManagerExtractor, ProjectPath};
use crate::execute::types::utils::record_batches_to_2d_array;
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
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path(ProjectPath {
        project_id: _project_id,
    }): Path<ProjectPath>,
    extract::Json(payload): extract::Json<SQLParams>,
) -> Result<extract::Json<Vec<Vec<String>>>, StatusCode> {
    let config_manager = project_manager.config_manager.clone();
    let secrets_manager = project_manager.secrets_manager.clone();
    let connector =
        Connector::from_database(&payload.database, &config_manager, &secrets_manager, None)
            .await?;
    let file_path = connector.run_query(&payload.sql).await?;

    let (batches, schema) =
        load_result(&file_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let data = record_batches_to_2d_array(&batches, &schema)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(extract::Json(data))
}

// TODO: may want to rename this and the `reindex()` function below as we're doing more
//       only conditionally reindexing and doing more than just building embeddings:
//         - constructing retrieval items to store in lancedb
//         - calculating inclusion radius for each retrieval item
//         - caching enum values for each variable so they can be detected at query time
pub async fn build_embeddings(
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path(ProjectPath {
        project_id: _project_id,
    }): Path<ProjectPath>,
) -> Result<extract::Json<EmbeddingsBuildResponse>, StatusCode> {
    let config_manager = project_manager.config_manager;
    let secret_manager = project_manager.secrets_manager;
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
