use crate::config::ConfigBuilder;
use crate::config::model::AgentConfig;
use crate::service::test::{TestStreamMessage, run_test as run_agent_test};
use crate::utils::find_project_path;
use async_stream::stream;
use axum::extract::{self, Path};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum_streams::StreamBodyAs;
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct BuilderAvailabilityResponse {
    pub available: bool,
}

pub async fn check_builder_availability()
-> Result<extract::Json<BuilderAvailabilityResponse>, StatusCode> {
    let project_path = find_project_path().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let config_builder = ConfigBuilder::new()
        .with_project_path(&project_path)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let config = config_builder
        .build()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let is_available = config.get_builder_agent_path().await.is_ok();

    Ok(extract::Json(BuilderAvailabilityResponse {
        available: is_available,
    }))
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgentConfigResponse {
    #[serde(flatten)]
    pub config: AgentConfig,
    pub path: String,
}

impl AgentConfigResponse {
    pub fn new(config: AgentConfig, path: String) -> Self {
        Self { config, path }
    }

    pub fn from_config(config: AgentConfig, path: &str) -> Self {
        Self::new(config, path.to_string())
    }
}

#[utoipa::path(
    method(get),
    path = "/agents",
    responses(
        (status = OK, description = "Success", body = Vec<String>, content_type = "application/json")
    )
)]
pub async fn get_agents() -> Result<extract::Json<Vec<AgentConfigResponse>>, StatusCode> {
    let project_path = find_project_path().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let config = ConfigBuilder::new()
        .with_project_path(&project_path)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .build()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let agent_paths = config
        .list_agents()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let agent_relative_paths: Vec<String> = agent_paths
        .iter()
        .filter_map(|agent| {
            agent
                .strip_prefix(&project_path)
                .ok()
                .map(|path| path.to_string_lossy().to_string())
        })
        .collect();

    let agent_futures = agent_relative_paths
        .into_iter()
        .map(|path| {
            let config = &config;
            async move {
                let agent_config = config.resolve_agent(&path).await?;
                Ok::<AgentConfigResponse, anyhow::Error>(AgentConfigResponse::from_config(
                    agent_config,
                    &path,
                ))
            }
        })
        .collect::<Vec<_>>();

    let agents: Vec<AgentConfigResponse> = futures::future::try_join_all(agent_futures)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(extract::Json(agents))
}

pub async fn get_agent(
    Path(pathb64): Path<String>,
) -> Result<extract::Json<AgentConfig>, StatusCode> {
    let decoded_path: Vec<u8> = BASE64_STANDARD.decode(pathb64).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?;
    let path = String::from_utf8(decoded_path).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?;
    let project_path = find_project_path().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let config = ConfigBuilder::new()
        .with_project_path(&project_path)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .build()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let agent = config
        .resolve_agent(&path)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(extract::Json(agent))
}

pub async fn run_test(
    Path((pathb64, test_index)): Path<(String, usize)>,
) -> Result<impl IntoResponse, StatusCode> {
    let decoded_path: Vec<u8> = match BASE64_STANDARD.decode(pathb64) {
        Ok(decoded_path) => decoded_path,
        Err(e) => {
            return Ok(StreamBodyAs::json_nl(stream! {
                yield TestStreamMessage {
                    error: Some(format!("Failed to decode path: {}", e)),
                    event: None,
                };
            }));
        }
    };
    let path = match String::from_utf8(decoded_path) {
        Ok(path) => path,
        Err(e) => {
            return Ok(StreamBodyAs::json_nl(stream! {
                yield TestStreamMessage {
                    error: Some(format!("Failed to decode path: {}", e)),
                    event: None,
                };
            }));
        }
    };

    let project_path = match find_project_path() {
        Ok(path) => path.to_string_lossy().to_string(),
        Err(e) => {
            return Ok(StreamBodyAs::json_nl(stream! {
                yield TestStreamMessage {
                    error: Some(format!("Failed to find project path: {}", e)),
                    event: None,
                };
            }));
        }
    };

    let stream = run_agent_test(project_path, path, test_index).await?;
    Ok(StreamBodyAs::json_nl(stream))
}
