use crate::server::api::middlewares::project::ProjectManagerExtractor;
use axum::{http::StatusCode, response::Json};
use oxy::config::model::IntegrationType;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use oxy_looker::MetadataStorage;
use serde::Serialize;

#[derive(Serialize)]
pub struct LookerExploreInfo {
    pub model: String,
    pub name: String,
    pub description: Option<String>,
    pub fields: Vec<String>,
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
                        let fields = storage
                            .load_merged_metadata(&explore.model, &explore.name)
                            .map(|metadata| {
                                let mut field_names = metadata
                                    .views
                                    .into_iter()
                                    .flat_map(|view| {
                                        view.dimensions
                                            .into_iter()
                                            .chain(view.measures.into_iter())
                                            .map(|field| field.name)
                                    })
                                    .collect::<Vec<_>>();
                                field_names.sort();
                                field_names.dedup();
                                field_names
                            })
                            .unwrap_or_else(|error| {
                                tracing::debug!(
                                    integration = integration.name,
                                    model = explore.model,
                                    explore = explore.name,
                                    error = %error,
                                    "No synced Looker metadata found for explore"
                                );
                                Vec::new()
                            });

                        LookerExploreInfo {
                            model: explore.model.clone(),
                            name: explore.name.clone(),
                            description: explore.description.clone(),
                            fields,
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
