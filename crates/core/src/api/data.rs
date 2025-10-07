use crate::adapters::connector::{Connector, load_result};
use crate::adapters::project::manager::ProjectManager;
use crate::api::middlewares::project::{ProjectManagerExtractor, ProjectPath};
use crate::errors::OxyError;
use crate::execute::types::utils::record_batches_to_2d_array;
use crate::service::retrieval::{ReindexInput, reindex};
use crate::theme::StyledText;
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
    handle_omni_sync(&project_manager).await?;
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

async fn handle_omni_sync(project: &ProjectManager) -> Result<(), OxyError> {
    use crate::service::omni_sync::OmniSyncService;
    use omni::{OmniApiClient, OmniError as AdapterOmniError};

    let project_path = project.config_manager.project_path();

    let config = project.config_manager.clone();

    // Get all Omni integration configurations - if none found, skip silently
    let omni_integrations: Vec<_> = config
        .get_config()
        .integrations
        .iter()
        .filter_map(|integration| match &integration.integration_type {
            crate::config::model::IntegrationType::Omni(omni_integration) => {
                Some((integration.name.clone(), omni_integration.clone()))
            }
        })
        .collect();

    if omni_integrations.is_empty() {
        // No Omni integrations configured, skip silently
        return Ok(());
    }

    println!(
        "🔗 Synchronizing {} Omni integration(s)...",
        omni_integrations.len()
    );

    let mut all_sync_results = Vec::new();
    let mut total_successful_topics = Vec::new();

    for (integration_name, omni_integration) in omni_integrations {
        println!("\n🔗 Processing integration: {}", integration_name);

        // Resolve API key from environment variable
        let api_key = project
            .secrets_manager
            .resolve_secret(&omni_integration.api_key_var)
            .await?
            .unwrap();
        let base_url = omni_integration.base_url.clone();
        let topics = omni_integration.topics.clone();

        // Sync all configured topics for this integration
        println!("🔄 Synchronizing Omni metadata for {} topics", topics.len());
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
        let sync_service = OmniSyncService::new(api_client, project_path, integration_name.clone());

        // Perform synchronization for each topic in this integration
        println!("📥 Fetching metadata from Omni API...");

        let mut integration_results = Vec::new();
        for topic in &topics_to_sync {
            println!(
                "  📋 Syncing topic: {} (model: {})",
                topic.name, topic.model_id
            );
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

    // Display overall results
    println!("\n{}", "🎉 Omni synchronization completed!".success());

    if !all_sync_results.is_empty() {
        let overall_success = all_sync_results.iter().all(|r| r.is_success());
        let partial_success = all_sync_results.iter().any(|r| r.is_partial_success());

        if overall_success {
            println!(
                "{}",
                "All integrations synchronized successfully.".success()
            );
        } else if partial_success {
            println!(
                "{}",
                "Partial synchronization completed with some errors.".warning()
            );
            // Show error summaries from failed integrations
            for sync_result in &all_sync_results {
                if let Some(error_summary) = sync_result.error_summary() {
                    println!("\n{}", "Errors encountered:".warning());
                    println!("{}", error_summary.error());
                }
            }
        } else {
            println!("{}", "Some integrations failed to synchronize.".error());
            for sync_result in &all_sync_results {
                if let Some(error_summary) = sync_result.error_summary() {
                    println!("\n{}", "Errors encountered:".error());
                    println!("{}", error_summary.error());
                }
            }
            return Err(OxyError::RuntimeError(
                "Some Omni sync operations failed".to_string(),
            ));
        }

        // Show all successful topics across all integrations
        if !total_successful_topics.is_empty() {
            println!("\n{}", "Successfully synchronized topics:".success());
            for topic in &total_successful_topics {
                println!("  ✅ {}", topic);
            }
        }
    }

    Ok(())
}
