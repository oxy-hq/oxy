use std::path::PathBuf;

use crate::server::api::middlewares::project::ProjectManagerExtractor;
use crate::server::service::app::{AppService, DisplayWithError, get_app_displays};
use axum::body::Body;
use axum::extract::{self, Path};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use oxy::execute::types::DataContainer;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio_util::io::ReaderStream;
use utoipa::ToSchema;
use uuid::Uuid;

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

/// List all apps in the project
///
/// Retrieves all app configurations available in the project. Returns app metadata
/// including names and relative paths. Apps are YAML-based configurations that define
/// data visualization and dashboard components.
#[utoipa::path(
    method(get),
    path = "/{project_id}/apps",
    params(
        ("project_id" = Uuid, Path, description = "Project UUID")
    ),
    responses(
        (status = OK, description = "Success", body = Vec<AppItem>, content_type = "application/json")
    ),
    security(
        ("ApiKey" = [])
    )
)]
pub async fn list_apps(
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
) -> Result<extract::Json<Vec<AppItem>>, StatusCode> {
    let config_manager = &project_manager.config_manager;
    let project_path = config_manager.project_path();

    let apps = config_manager
        .list_apps()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let app_items: Vec<AppItem> = apps
        .iter()
        .filter_map(|app_path| {
            app_path
                .strip_prefix(project_path)
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
    Path((_project_id, pathb64)): Path<(Uuid, String)>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
) -> Result<extract::Json<GetDisplaysResponse>, StatusCode> {
    let path = decode_path(&pathb64)?;

    let displays = match get_app_displays(project_manager.clone(), &path).await {
        Ok(displays) => displays,
        Err(e) => {
            tracing::debug!("Failed to get app displays: {:?}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    Ok(extract::Json(GetDisplaysResponse { displays }))
}

pub async fn get_app_data(
    Path((_project_id, pathb64)): Path<(Uuid, String)>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
) -> Result<extract::Json<GetAppDataResponse>, StatusCode> {
    let path = decode_path(&pathb64)?;

    let mut app_service = AppService::new(project_manager.clone());

    let app_tasks = match app_service.get_tasks(&path).await {
        Ok(tasks) => tasks,
        Err(e) => {
            tracing::debug!("Failed to get app tasks from path: {:?} {}", path, e);
            return Ok(extract::Json(create_error_response(format!(
                "Failed to get app tasks: {e}"
            ))));
        }
    };

    if let Some(cached_data) = app_service.try_load_cached_data(&path, &app_tasks).await {
        return Ok(extract::Json(GetAppDataResponse {
            data: cached_data,
            error: None,
        }));
    }

    let data = match app_service.run(&path).await {
        Ok(data) => data,
        Err(e) => {
            tracing::debug!("Failed to run app: {:?}", e);
            return Ok(extract::Json(create_error_response(format!(
                "Failed to run app: {e}"
            ))));
        }
    };

    Ok(extract::Json(GetAppDataResponse { data, error: None }))
}

pub async fn get_data(
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path((_project_id, pathb64)): Path<(Uuid, String)>,
) -> impl IntoResponse {
    let path_string = match decode_path(&pathb64) {
        Ok(path) => path.to_string_lossy().to_string(),
        Err(status) => return Err((status, "Invalid path".to_string())),
    };

    let state_path = project_manager
        .config_manager
        .resolve_state_dir()
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to resolve state dir: {e}"),
            )
        })?;
    let full_file_path = state_path.join(path_string);

    print!("Full file path: {:?}", full_file_path);

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
    Path((_project_id, pathb64)): Path<(Uuid, String)>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
) -> Result<extract::Json<GetAppDataResponse>, StatusCode> {
    let path = decode_path(&pathb64)?;

    let mut app_service = AppService::new(project_manager.clone());
    let data = match app_service.run(&path).await {
        Ok(data) => data,
        Err(e) => {
            tracing::debug!("Failed to run app: {:?}", e);
            return Ok(extract::Json(create_error_response(format!(
                "Failed to run app: {e}"
            ))));
        }
    };

    Ok(extract::Json(GetAppDataResponse { data, error: None }))
}
