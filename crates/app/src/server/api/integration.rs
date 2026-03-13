use crate::server::api::middlewares::project::ProjectManagerExtractor;
use crate::server::api::result_files::store_result_file;
use axum::{extract, http::StatusCode, response::Json};
use oxy::adapters::project::manager::ProjectManager;
use oxy::config::model::{IntegrationType, LookerQueryParams, LookerSortField};
use oxy::execute::renderer::Renderer;
use oxy::execute::types::{Output, Source};
use oxy::execute::{Executable, ExecutionContext};
use oxy::tools::looker::executable::LookerQueryExecutable;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use oxy_looker::MetadataStorage;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::mpsc;

#[derive(Serialize)]
pub struct LookerExploreInfo {
    pub model: String,
    pub name: String,
    pub description: Option<String>,
    pub dimensions: Vec<String>,
    pub measures: Vec<String>,
}

#[derive(Serialize)]
pub struct LookerIntegrationInfo {
    pub name: String,
    pub explores: Vec<LookerExploreInfo>,
}

pub async fn list_looker_integrations(
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
) -> Result<Json<Vec<LookerIntegrationInfo>>, StatusCode> {
    let config_manager = &project_manager.config_manager;
    let state_dir = config_manager.resolve_state_dir().await.map_err(|error| {
        tracing::error!(error = %error, "Failed to resolve state directory for Looker metadata");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let project_path = config_manager.project_path();

    let integrations = config_manager
        .list_looker_integrations()
        .into_iter()
        .filter_map(|integration| {
            if let IntegrationType::Looker(looker) = &integration.integration_type {
                let storage = MetadataStorage::new(
                    state_dir.join(".looker"),
                    project_path.join("looker"),
                    integration.name.clone(),
                );

                let explores = looker
                    .explores
                    .iter()
                    .map(|explore| {
                        let (dimensions, measures) = storage
                            .load_merged_metadata(&explore.model, &explore.name)
                            .map(|metadata| {
                                let mut dims = metadata
                                    .views
                                    .iter()
                                    .flat_map(|view| view.dimensions.iter().map(|f| f.name.clone()))
                                    .collect::<Vec<_>>();
                                let mut meas = metadata
                                    .views
                                    .iter()
                                    .flat_map(|view| view.measures.iter().map(|f| f.name.clone()))
                                    .collect::<Vec<_>>();
                                dims.sort();
                                dims.dedup();
                                meas.sort();
                                meas.dedup();
                                (dims, meas)
                            })
                            .unwrap_or_else(|error| {
                                tracing::debug!(
                                    integration = integration.name,
                                    model = explore.model,
                                    explore = explore.name,
                                    error = %error,
                                    "No synced Looker metadata found for explore"
                                );
                                (Vec::new(), Vec::new())
                            });

                        LookerExploreInfo {
                            model: explore.model.clone(),
                            name: explore.name.clone(),
                            description: explore.description.clone(),
                            dimensions,
                            measures,
                        }
                    })
                    .collect();
                Some(LookerIntegrationInfo {
                    name: integration.name.clone(),
                    explores,
                })
            } else {
                None
            }
        })
        .collect();

    Ok(Json(integrations))
}

#[derive(Deserialize)]
pub struct LookerQueryRequest {
    pub integration: String,
    pub model: String,
    pub explore: String,
    pub fields: Vec<String>,
    #[serde(default)]
    pub filters: Option<HashMap<String, String>>,
    #[serde(default)]
    pub sorts: Option<Vec<LookerSortField>>,
    #[serde(default)]
    pub limit: Option<i64>,
}

fn build_execution_context(project_manager: ProjectManager) -> ExecutionContext {
    let (tx, _rx) = mpsc::channel(100);
    let renderer = Renderer::new(minijinja::Value::default());
    ExecutionContext {
        source: Source {
            id: "api".to_string(),
            kind: "api".to_string(),
            parent_id: None,
        },
        writer: tx,
        renderer,
        project: project_manager,
        checkpoint: None,
        filters: None,
        connections: None,
        sandbox_info: None,
        user_id: None,
        metric_context: None,
        data_app_file_path: None,
    }
}

pub async fn compile_looker_query(
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    extract::Json(payload): extract::Json<LookerQueryRequest>,
) -> Result<Json<String>, (StatusCode, String)> {
    let execution_context = build_execution_context(project_manager);

    let params = LookerQueryParams {
        fields: payload.fields,
        filters: payload.filters,
        filter_expression: None,
        sorts: payload.sorts,
        limit: payload.limit,
    };

    let executable = LookerQueryExecutable::new();
    let sql = executable
        .get_sql(
            &execution_context,
            &params,
            &payload.integration,
            &payload.model,
            &payload.explore,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to compile Looker query: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;

    Ok(Json(sql))
}

#[derive(Serialize)]
pub struct LookerQueryResponse {
    pub file_name: String,
}

pub async fn execute_looker_query(
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    extract::Json(payload): extract::Json<LookerQueryRequest>,
) -> Result<Json<LookerQueryResponse>, (StatusCode, String)> {
    let execution_context = build_execution_context(project_manager.clone());

    let params = LookerQueryParams {
        fields: payload.fields,
        filters: payload.filters,
        filter_expression: None,
        sorts: payload.sorts,
        limit: payload.limit,
    };

    let output = LookerQueryExecutable::new()
        .execute_query(
            &execution_context,
            &params,
            &payload.integration,
            &payload.model,
            &payload.explore,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to execute Looker query: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;

    match output {
        Output::Table(table) => {
            let file_name = store_result_file(&project_manager, &table.file_path)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to store Looker query result: {}", e);
                    (StatusCode::INTERNAL_SERVER_ERROR, e)
                })?;

            Ok(Json(LookerQueryResponse { file_name }))
        }
        _ => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unexpected output type from Looker query".to_string(),
        )),
    }
}
