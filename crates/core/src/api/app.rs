use std::path::PathBuf;

use crate::config::ConfigBuilder;
use crate::config::model::Display;
use crate::db::client::get_state_dir;
use crate::execute::types::DataContainer;
use crate::service;
use crate::utils::find_project_path;
use axum::body::Body;
use axum::extract::{self, Path};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio_util::io::ReaderStream;
use utoipa::ToSchema;

#[derive(Deserialize, Serialize, JsonSchema, ToSchema)]
pub struct AppItem {
    pub name: String,
    pub path: String,
}

#[utoipa::path(
    method(get),
    path = "/apps",
    responses(
        (status = OK, description = "Success", body = Vec<AppItem>, content_type = "application/json")
    )
)]
pub async fn list_apps() -> Result<extract::Json<Vec<AppItem>>, StatusCode> {
    let project_path = find_project_path().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let config_builder = ConfigBuilder::new()
        .with_project_path(&project_path)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let config = config_builder
        .build()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let apps = config
        .list_apps()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(extract::Json(
        apps.iter()
            .filter_map(|agent| {
                agent.strip_prefix(&project_path).ok().map(|path| AppItem {
                    name: path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .to_string()
                        .replace(".app.yml", ""),
                    path: path.to_string_lossy().to_string(),
                })
            })
            .collect(),
    ))
}

#[derive(Deserialize, Serialize)]
pub struct GetAppResponse {
    pub data: DataContainer,
    pub displays: Vec<Display>,
}

pub async fn get_app(
    Path(pathb64): Path<String>,
) -> Result<extract::Json<GetAppResponse>, StatusCode> {
    let decoded_path: Vec<u8> = BASE64_STANDARD.decode(pathb64).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?;
    let path = PathBuf::from(String::from_utf8(decoded_path).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?);

    let app_config = service::app::get_app(&path.to_owned()).await.map_err(|e| {
        tracing::debug!(
            "Failed to get app config from path: {:?} {}",
            path.to_owned(),
            e
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if let Some(cached_data) = service::app::try_load_cached_data(&path) {
        return Ok(extract::Json(GetAppResponse {
            data: cached_data,
            displays: app_config.display,
        }));
    }

    // write the data to the file
    let data = service::app::run_app(&path).await.map_err(|e| {
        tracing::debug!("Failed to run app: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(extract::Json(GetAppResponse {
        data,
        displays: app_config.display,
    }))
}

pub async fn get_data(Path(pathb64): Path<String>) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    let state_path = get_state_dir();
    let decoded_path: Vec<u8> = BASE64_STANDARD.decode(pathb64).map_err(|e| {
        tracing::info!("{:?}", e);
        (StatusCode::BAD_REQUEST, "Invalid path".to_string())
    })?;
    let path = String::from_utf8(decoded_path).map_err(|e| {
        tracing::info!("{:?}", e);
        (StatusCode::BAD_REQUEST, "Invalid path".to_string())
    })?;
    let full_file_path = state_path.join(path);
    let file = match tokio::fs::File::open(full_file_path).await {
        Ok(file) => file,
        Err(err) => return Err((StatusCode::NOT_FOUND, format!("File not found: {}", err))),
    };
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    headers.insert(
        "Cache-Control",
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    Ok((StatusCode::OK, headers, body))
}

pub async fn run_app(
    Path(pathb64): Path<String>,
) -> Result<extract::Json<GetAppResponse>, StatusCode> {
    let decoded_path: Vec<u8> = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = PathBuf::from(String::from_utf8(decoded_path).map_err(|_| StatusCode::BAD_REQUEST)?);
    let app_config = service::app::get_app(&path).await.map_err(|e| {
        tracing::debug!(
            "Failed to get app config from path: {:?} {}",
            path.to_owned(),
            e
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let rs = service::app::run_app(&path).await.map_err(|e| {
        tracing::debug!("Failed to run app: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(extract::Json(GetAppResponse {
        data: rs,
        displays: app_config.display,
    }))
}
