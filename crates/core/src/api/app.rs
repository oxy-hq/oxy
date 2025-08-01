use std::path::PathBuf;

use crate::config::ConfigBuilder;
use crate::db::client::get_state_dir;
use crate::execute::types::DataContainer;
use crate::project::resolve_project_path;
use crate::service::{self, app::DisplayWithError};
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

#[derive(Deserialize, Serialize)]
pub struct GetAppDataResponse {
    pub data: DataContainer,
    error: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub struct GetDisplaysResponse {
    pub displays: Vec<DisplayWithError>,
}

fn decode_path(pathb64: &str) -> Result<PathBuf, StatusCode> {
    let decoded_bytes = BASE64_STANDARD.decode(pathb64).map_err(|e| {
        tracing::info!("Base64 decode error: {:?}", e);
        StatusCode::BAD_REQUEST
    })?;

    let path_string = String::from_utf8(decoded_bytes).map_err(|e| {
        tracing::info!("UTF8 conversion error: {:?}", e);
        StatusCode::BAD_REQUEST
    })?;

    Ok(PathBuf::from(path_string))
}

fn create_error_response(error_msg: String) -> GetAppDataResponse {
    GetAppDataResponse {
        data: DataContainer::None,
        error: Some(error_msg),
    }
}

#[utoipa::path(
    method(get),
    path = "/apps",
    responses(
        (status = OK, description = "Success", body = Vec<AppItem>, content_type = "application/json")
    )
)]
pub async fn list_apps() -> Result<extract::Json<Vec<AppItem>>, StatusCode> {
    let project_path = resolve_project_path().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

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

    let app_items: Vec<AppItem> = apps
        .iter()
        .filter_map(|app_path| {
            app_path
                .strip_prefix(&project_path)
                .ok()
                .map(|relative_path| {
                    let name = relative_path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .to_string()
                        .replace(".app.yml", "");

                    AppItem {
                        name,
                        path: relative_path.to_string_lossy().to_string(),
                    }
                })
        })
        .collect();

    Ok(extract::Json(app_items))
}

pub async fn get_displays(
    Path(pathb64): Path<String>,
) -> Result<extract::Json<GetDisplaysResponse>, StatusCode> {
    let path = decode_path(&pathb64)?;

    let displays = match service::app::get_app_displays(&path).await {
        Ok(displays) => displays,
        Err(e) => {
            tracing::debug!("Failed to get app displays: {:?}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    Ok(extract::Json(GetDisplaysResponse { displays }))
}

pub async fn get_app_data(
    Path(pathb64): Path<String>,
) -> Result<extract::Json<GetAppDataResponse>, StatusCode> {
    let path = decode_path(&pathb64)?;

    let app_tasks = match service::app::get_app_tasks(&path).await {
        Ok(tasks) => tasks,
        Err(e) => {
            tracing::debug!("Failed to get app tasks from path: {:?} {}", path, e);
            return Ok(extract::Json(create_error_response(format!(
                "Failed to get app tasks: {}",
                e
            ))));
        }
    };

    if let Some(cached_data) = service::app::try_load_cached_data(&path, &app_tasks) {
        return Ok(extract::Json(GetAppDataResponse {
            data: cached_data,
            error: None,
        }));
    }

    let data = match service::app::run_app(&path).await {
        Ok(data) => data,
        Err(e) => {
            tracing::debug!("Failed to run app: {:?}", e);
            return Ok(extract::Json(create_error_response(format!(
                "Failed to run app: {}",
                e
            ))));
        }
    };

    Ok(extract::Json(GetAppDataResponse { data, error: None }))
}

pub async fn get_data(Path(pathb64): Path<String>) -> impl IntoResponse {
    let path_string = match decode_path(&pathb64) {
        Ok(path) => path.to_string_lossy().to_string(),
        Err(status) => return Err((status, "Invalid path".to_string())),
    };

    let state_path = get_state_dir();
    let full_file_path = state_path.join(path_string);

    let file = match tokio::fs::File::open(full_file_path).await {
        Ok(file) => file,
        Err(err) => return Err((StatusCode::NOT_FOUND, format!("File not found: {err}"))),
    };

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let mut headers = HeaderMap::new();
    headers.insert(
        "Cache-Control",
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );

    Ok((StatusCode::OK, headers, body))
}

pub async fn run_app(
    Path(pathb64): Path<String>,
) -> Result<extract::Json<GetAppDataResponse>, StatusCode> {
    let path = decode_path(&pathb64)?;

    let data = match service::app::run_app(&path).await {
        Ok(data) => data,
        Err(e) => {
            tracing::debug!("Failed to run app: {:?}", e);
            return Ok(extract::Json(create_error_response(format!(
                "Failed to run app: {}",
                e
            ))));
        }
    };

    Ok(extract::Json(GetAppDataResponse { data, error: None }))
}
