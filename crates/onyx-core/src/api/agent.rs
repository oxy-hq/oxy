use crate::config::ConfigBuilder;
use crate::service;
use crate::service::agent::AskRequest;
use crate::utils::find_project_path;
use axum::extract;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum_streams::StreamBodyAs;

pub async fn ask(extract::Json(payload): extract::Json<AskRequest>) -> impl IntoResponse {
    let s = service::agent::ask(payload).await;
    StreamBodyAs::json_nl(s)
}

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
