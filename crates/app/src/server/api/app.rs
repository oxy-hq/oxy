use std::collections::HashMap;
use std::path::PathBuf;

use crate::cli::commands::export_chart::export_charts_to_dir;
use crate::server::api::middlewares::project::ProjectManagerExtractor;
use crate::server::service::app::{
    AppResultData, AppService, DisplayWithError, GetAppResultResponse, TaskResult, get_app_displays,
};
use axum::body::Body;
use axum::extract::{self, Path};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use oxy::config::model::Display;
use oxy::execute::types::DataContainer;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
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

/// Error response wrapper
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ApiErrorResponse {
    pub error: String,
}

#[derive(Deserialize, Serialize)]
pub struct GetDisplaysResponse {
    pub displays: Vec<DisplayWithError>,
}

type ApiError = (StatusCode, extract::Json<ApiErrorResponse>);

fn api_error(status: StatusCode, msg: impl ToString) -> ApiError {
    (
        status,
        extract::Json(ApiErrorResponse {
            error: msg.to_string(),
        }),
    )
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

    let apps = config_manager.list_apps().await.map_err(|e| {
        tracing::error!("Failed to list apps: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

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
    let full_file_path = state_path
        .join(&path_string)
        .canonicalize()
        .map_err(|e| (StatusCode::NOT_FOUND, format!("File not found: {e}")))?;

    if !full_file_path.starts_with(&state_path) {
        return Err((StatusCode::FORBIDDEN, "Access denied".to_string()));
    }

    let file = match tokio::fs::File::open(&full_file_path).await {
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

/// Execute data app and get combined results (tasks + displays)
///
/// Executes a data app and returns both task execution results and display configurations.
/// This endpoint combines task outputs with their display representations, allowing consumers
/// to access both the raw data and its visual presentation.
#[derive(Deserialize, JsonSchema, ToSchema)]
pub struct AppResultQuery {
    /// When false (default), return cached result if available. When true, re-execute the app.
    #[serde(default)]
    pub refresh: bool,
}

fn get_result_cache_filename(app_path: &PathBuf) -> Option<String> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    app_path.hash(&mut hasher);
    let hash = hasher.finish();
    Some(format!("{:x}.app.result.yml", hash))
}

#[utoipa::path(
    method(post),
    path = "/{project_id}/apps/{pathb64}/result",
    params(
        ("project_id" = Uuid, Path, description = "Project UUID"),
        ("pathb64" = String, Path, description = "Base64-encoded path to data app file"),
        ("refresh" = Option<bool>, Query, description = "Re-execute app instead of returning cached result (defaults to false)")
    ),
    responses(
        (status = OK, description = "Execution completed successfully", body = GetAppResultResponse, content_type = "application/json"),
        (status = BAD_REQUEST, description = "Invalid request parameters"),
        (status = UNAUTHORIZED, description = "Invalid or missing API key"),
        (status = NOT_FOUND, description = "Data app not found"),
        (status = INTERNAL_SERVER_ERROR, description = "Execution failed or server error", body = ApiErrorResponse, content_type = "application/json")
    ),
    security(
        ("ApiKey" = [])
    )
)]
pub async fn get_app_result(
    Path((_project_id, pathb64)): Path<(Uuid, String)>,
    extract::Query(query): extract::Query<AppResultQuery>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
) -> (StatusCode, extract::Json<GetAppResultResponse>) {
    let path = match decode_path(&pathb64) {
        Ok(p) => p,
        Err(status) => {
            return (
                status,
                extract::Json(GetAppResultResponse {
                    success: false,
                    error_message: Some("Invalid base64 path encoding".to_string()),
                    result: None,
                }),
            );
        }
    };

    // Try to load cached result if not refreshing
    if !query.refresh {
        if let Some(cached) = load_cached_result(&project_manager, &path).await {
            return (StatusCode::OK, extract::Json(cached));
        }
    }

    // Execute the app to get task results
    let mut app_service = AppService::new(project_manager.clone());

    // Get task names first (needed for response even if execution fails)
    let task_configs = match app_service.get_tasks(&path).await {
        Ok(tasks) => tasks,
        Err(e) => {
            tracing::debug!("Failed to get app tasks: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                extract::Json(GetAppResultResponse {
                    success: false,
                    error_message: Some(format!("Failed to get app tasks: {e}")),
                    result: None,
                }),
            );
        }
    };

    // Execute the app
    let execution_result = app_service.run(&path).await;

    // Transform execution results into TaskResult objects
    let (tasks, execution_succeeded): (Vec<TaskResult>, bool) = match execution_result {
        Ok(DataContainer::Map(results)) => {
            // Convert the results map into TaskResult objects
            let tasks = task_configs
                .iter()
                .map(|task_config| {
                    let task_name = task_config.name.clone();
                    // Try to find the result for this task and extract JSON data
                    let output = results.get(&task_name).and_then(|data| {
                        let value = serde_json::to_value(data).ok()?;
                        // If the result is a table (has file_path + json), extract the json data
                        if let Some(json_str) = value.get("json").and_then(|v| v.as_str()) {
                            serde_json::from_str(json_str).ok()
                        } else {
                            Some(value)
                        }
                    });

                    TaskResult {
                        task_name,
                        output,
                        error: None,
                    }
                })
                .collect();
            (tasks, true)
        }
        Err(e) => {
            // If execution failed, return tasks with error
            let error_msg = e.to_string();
            let tasks = task_configs
                .iter()
                .map(|task_config| TaskResult {
                    task_name: task_config.name.clone(),
                    output: None,
                    error: Some(error_msg.clone()),
                })
                .collect();
            (tasks, false)
        }
        _ => {
            // Unexpected data container type
            let tasks = task_configs
                .iter()
                .map(|task_config| TaskResult {
                    task_name: task_config.name.clone(),
                    output: None,
                    error: Some("Unexpected output format".to_string()),
                })
                .collect();
            (tasks, false)
        }
    };

    // Build a map of task_name -> output data for resolving display references
    let task_data_map: HashMap<String, JsonValue> = tasks
        .iter()
        .filter_map(|t| t.output.as_ref().map(|o| (t.task_name.clone(), o.clone())))
        .collect();

    // Get typed displays
    let typed_displays = match get_app_displays(project_manager.clone(), &path).await {
        Ok(displays) => displays,
        Err(e) => {
            tracing::debug!("Failed to get app displays: {:?}", e);
            vec![]
        }
    };

    // Check if there are any chart displays that need PNG export
    let has_charts = typed_displays.iter().any(|d| {
        matches!(
            d,
            DisplayWithError::Display(Display::LineChart(_))
                | DisplayWithError::Display(Display::BarChart(_))
                | DisplayWithError::Display(Display::PieChart(_))
        )
    });

    // Export charts to PNG if needed
    let mut chart_export_error: Option<String> = None;
    let chart_file_map: HashMap<i64, String> = if has_charts {
        let charts_dir = project_manager
            .config_manager
            .get_charts_dir()
            .await
            .unwrap_or_default();
        let app_path_str = path.to_string_lossy().to_string();
        match export_charts_to_dir(&app_path_str, &charts_dir).await {
            Ok(map) => map,
            Err(e) => {
                tracing::warn!("Failed to export charts: {:?}", e);
                chart_export_error = Some(e.to_string());
                HashMap::new()
            }
        }
    } else {
        HashMap::new()
    };

    // Build displays JSON with proper types and resolved data
    let displays: Vec<JsonValue> = typed_displays
        .into_iter()
        .enumerate()
        .filter_map(|(i, d)| match d {
            DisplayWithError::Display(display) => Some(match display {
                Display::LineChart(chart) => {
                    let file_path = chart_file_map.get(&(i as i64)).cloned();
                    let mut obj = serde_json::json!({
                        "type": "line_chart",
                        "file_name": file_path,
                        "title": chart.title,
                    });
                    if file_path.is_none() {
                        if let Some(err) = &chart_export_error {
                            obj["error"] = serde_json::json!(err);
                        }
                    }
                    obj
                }
                Display::BarChart(chart) => {
                    let file_path = chart_file_map.get(&(i as i64)).cloned();
                    let mut obj = serde_json::json!({
                        "type": "bar_chart",
                        "file_name": file_path,
                        "title": chart.title,
                    });
                    if file_path.is_none() {
                        if let Some(err) = &chart_export_error {
                            obj["error"] = serde_json::json!(err);
                        }
                    }
                    obj
                }
                Display::PieChart(chart) => {
                    let file_path = chart_file_map.get(&(i as i64)).cloned();
                    let mut obj = serde_json::json!({
                        "type": "pie_chart",
                        "file_name": file_path,
                        "title": chart.title,
                    });
                    if file_path.is_none() {
                        if let Some(err) = &chart_export_error {
                            obj["error"] = serde_json::json!(err);
                        }
                    }
                    obj
                }
                Display::Table(table) => {
                    let data = task_data_map.get(&table.data).cloned();
                    serde_json::json!({
                        "type": "table",
                        "data": data,
                        "title": table.title,
                    })
                }
                Display::Markdown(md) => {
                    serde_json::json!({
                        "type": "markdown",
                        "content": md.content,
                    })
                }
            }),
            DisplayWithError::Error(_) => None,
        })
        .collect();

    let response = GetAppResultResponse {
        success: execution_succeeded,
        error_message: None,
        result: Some(AppResultData { tasks, displays }),
    };

    // Only cache successful results to avoid permanently caching errors
    if execution_succeeded {
        save_cached_result(&project_manager, &path, &response).await;
    }

    (StatusCode::OK, extract::Json(response))
}

async fn load_cached_result(
    project_manager: &oxy::adapters::project::manager::ProjectManager,
    app_path: &PathBuf,
) -> Option<GetAppResultResponse> {
    let cache_name = get_result_cache_filename(app_path)?;
    let results_dir = project_manager
        .config_manager
        .get_app_results_dir()
        .await
        .ok()?;
    let cache_path = results_dir.join(cache_name);
    tokio::task::spawn_blocking(move || {
        if !cache_path.exists() {
            return None;
        }
        let file = std::fs::File::open(&cache_path).ok()?;
        let reader = std::io::BufReader::new(file);
        match serde_yaml::from_reader(reader) {
            Ok(data) => Some(data),
            Err(e) => {
                tracing::warn!("Failed to parse cached app result: {}", e);
                None
            }
        }
    })
    .await
    .ok()
    .flatten()
}

async fn save_cached_result(
    project_manager: &oxy::adapters::project::manager::ProjectManager,
    app_path: &PathBuf,
    response: &GetAppResultResponse,
) {
    let Some(cache_name) = get_result_cache_filename(app_path) else {
        return;
    };
    let Ok(results_dir) = project_manager.config_manager.get_app_results_dir().await else {
        return;
    };
    let cache_path = results_dir.join(cache_name);
    let response = response.clone();
    let _ = tokio::task::spawn_blocking(move || {
        let tmp_path = cache_path.with_extension("tmp");
        match std::fs::File::create(&tmp_path) {
            Ok(file) => {
                let writer = std::io::BufWriter::new(file);
                if let Err(e) = serde_yaml::to_writer(writer, &response) {
                    tracing::warn!("Failed to write cached app result: {}", e);
                    let _ = std::fs::remove_file(&tmp_path);
                    return;
                }
                if let Err(e) = std::fs::rename(&tmp_path, &cache_path) {
                    tracing::warn!("Failed to rename cache temp file: {}", e);
                    let _ = std::fs::remove_file(&tmp_path);
                }
            }
            Err(e) => {
                tracing::warn!("Failed to create cache temp file: {}", e);
            }
        }
    })
    .await;
}

/// Fetch chart image by file path
///
/// Retrieves a rendered chart image (PNG) by its file path. The file_path is returned
/// in chart display items (line_chart, bar_chart, pie_chart) from the result endpoint.
/// This endpoint serves the pre-rendered chart images for visualization.
#[utoipa::path(
    method(get),
    path = "/{project_id}/apps/{pathb64}/charts/{chart_path}",
    params(
        ("project_id" = Uuid, Path, description = "Project UUID"),
        ("pathb64" = String, Path, description = "Base64-encoded path to data app file"),
        ("chart_path" = String, Path, description = "File path of the chart image (from display item)")
    ),
    responses(
        (status = OK, description = "Image returned successfully", content_type = "image/png"),
        (status = UNAUTHORIZED, description = "Invalid or missing API key"),
        (status = NOT_FOUND, description = "Chart image not found"),
        (status = NOT_IMPLEMENTED, description = "Chart rendering not yet implemented")
    ),
    security(
        ("ApiKey" = [])
    )
)]
pub async fn get_chart_image(
    Path((_project_id, pathb64, chart_path)): Path<(Uuid, String, String)>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
) -> Result<impl IntoResponse, StatusCode> {
    let _app_path = decode_path(&pathb64)?;

    // Get charts directory
    let charts_dir = project_manager
        .config_manager
        .get_charts_dir()
        .await
        .map_err(|e| {
            tracing::error!("Failed to get charts directory: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let full_chart_path = charts_dir.join(&chart_path).canonicalize().map_err(|e| {
        tracing::debug!("Chart file not found: {:?} - {}", chart_path, e);
        StatusCode::NOT_FOUND
    })?;

    if !full_chart_path.starts_with(&charts_dir) {
        return Err(StatusCode::FORBIDDEN);
    }

    // Read the PNG file
    let file = tokio::fs::File::open(&full_chart_path).await.map_err(|e| {
        tracing::debug!("Chart file not found: {:?} - {}", full_chart_path, e);
        StatusCode::NOT_FOUND
    })?;

    let reader_stream = ReaderStream::new(file);
    let body = Body::from_stream(reader_stream);

    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("image/png"));
    headers.insert(
        "Cache-Control",
        HeaderValue::from_static("public, max-age=3600"),
    );

    Ok((StatusCode::OK, headers, body))
}
