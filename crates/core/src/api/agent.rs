use crate::config::ConfigBuilder;
use crate::config::model::AgentConfig;
use crate::service;
use crate::service::agent::{AskRequest, Message};
use crate::utils::find_project_path;
use axum::extract::{self, Path};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum_streams::StreamBodyAs;
use base64::Engine;
use base64::prelude::BASE64_STANDARD;

#[utoipa::path(
    method(post),
    path = "/ask",
    responses(
        (status = OK, description = "Success", body = Message, content_type = "application/x-ndjson")
    )
)]
pub async fn ask(
    extract::Json(payload): extract::Json<AskRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let s = service::agent::ask(payload).await?;
    Ok(StreamBodyAs::json_nl(s))
}

#[utoipa::path(
    method(get),
    path = "/agents",
    responses(
        (status = OK, description = "Success", body = Vec<String>, content_type = "application/json")
    )
)]
pub async fn get_agents() -> Result<extract::Json<Vec<String>>, StatusCode> {
    let project_path = find_project_path().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let config_builder = ConfigBuilder::new()
        .with_project_path(&project_path)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let config = config_builder
        .build()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let agents = config
        .list_agents()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(extract::Json(
        agents
            .iter()
            .filter_map(|agent| {
                agent
                    .strip_prefix(&project_path)
                    .ok()
                    .map(|path| path.to_string_lossy().to_string())
            })
            .collect(),
    ))
}

pub async fn get_agent(
    Path(pathb64): Path<String>,
) -> Result<extract::Json<AgentConfig>, StatusCode> {
    let decoded_path: Vec<u8> = BASE64_STANDARD.decode(pathb64).map_err(|e| {
        log::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?;
    let path = String::from_utf8(decoded_path).map_err(|e| {
        log::info!("{:?}", e);
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

pub async fn run_test(Path((pathb64, test_index)): Path<(String, usize)>) -> impl IntoResponse {
    service::test::run_test(pathb64, test_index).await
}
