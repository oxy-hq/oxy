use std::collections::HashMap;
use std::path::PathBuf;

use crate::cli::commands::export_chart::export_charts_to_dir;
use crate::server::api::middlewares::workspace_context::WorkspaceManagerExtractor;
use crate::server::service::app::{
    AppResultChartDisplay, AppResultData, AppResultDisplay, AppResultMarkdownDisplay,
    AppResultTableDisplay, AppService, DisplayWithError, GetAppResultResponse, TaskKind,
    TaskOutput, TaskResult, get_app_displays, render_control_default,
};
use axum::body::Body;
use axum::extract::{self, Path};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use oxy::config::model::{
    AppTaskMode, ControlConfig, DatabaseType, Display, DuckDBOptions, SQL, TaskType,
};
use oxy::execute::types::{Data, DataContainer};
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

/// Per-task information exposed to the frontend for client-side execution.
#[derive(Deserialize, Serialize)]
pub struct TaskClientInfo {
    /// Raw SQL template (may contain Jinja syntax like `{{ controls.x }}`).
    pub sql: String,
    /// Where to execute this task when controls change. `client` = DuckDB WASM (default),
    /// `server` = backend round-trip (needed for Snowflake, BigQuery, etc.).
    pub mode: AppTaskMode,
    /// Project-relative file paths that the SQL reads (e.g. `oxymart.csv`).
    /// The frontend downloads these once and registers them in DuckDB WASM so the
    /// original SQL runs unchanged.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_files: Vec<String>,
}

#[derive(Deserialize, Serialize)]
pub struct GetDisplaysResponse {
    pub displays: Vec<DisplayWithError>,
    pub controls: Vec<ControlConfig>,
    /// SQL templates and execution modes for each task, keyed by task name.
    /// Only `execute_sql` tasks with inline `sql_query` are included.
    pub tasks: HashMap<String, TaskClientInfo>,
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
    path = "/{workspace_id}/apps",
    params(
        ("workspace_id" = Uuid, Path, description = "Workspace UUID")
    ),
    responses(
        (status = OK, description = "Success", body = Vec<AppItem>, content_type = "application/json")
    ),
    security(
        ("ApiKey" = [])
    )
)]
pub async fn list_apps(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
) -> Result<extract::Json<Vec<AppItem>>, StatusCode> {
    let config_manager = &workspace_manager.config_manager;
    let workspace_path = config_manager.workspace_path();

    let apps = config_manager.list_apps().await.map_err(|e| {
        tracing::error!("Failed to list apps: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let app_items: Vec<AppItem> = apps
        .iter()
        .filter_map(|app_path| {
            app_path
                .strip_prefix(workspace_path)
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

/// Extract all single-quoted file paths (ending in .csv, .parquet, .json) from a SQL string.
/// These are project-relative source files the browser needs to download before running the
/// query in DuckDB WASM.
fn extract_sql_source_files(sql: &str) -> Vec<String> {
    let mut files = Vec::new();
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\'' {
            let start = i + 1;
            let mut j = start;
            while j < chars.len() && chars[j] != '\'' {
                j += 1;
            }
            let content: String = chars[start..j].iter().collect();
            let lower = content.to_lowercase();
            if lower.ends_with(".csv") || lower.ends_with(".parquet") || lower.ends_with(".json") {
                files.push(content);
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }
    files.sort();
    files.dedup();
    files
}

pub async fn get_displays(
    Path((_workspace_id, pathb64)): Path<(Uuid, String)>,
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
) -> Result<extract::Json<GetDisplaysResponse>, StatusCode> {
    let path = decode_path(&pathb64)?;

    let (displays, controls) = match get_app_displays(workspace_manager.clone(), &path).await {
        Ok(result) => result,
        Err(e) => {
            tracing::debug!("Failed to get app displays: {:?}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Collect SQL templates for execute_sql tasks so the frontend can run them
    // client-side in DuckDB WASM without a server round-trip on control changes.
    let databases = workspace_manager.config_manager.list_databases();
    let app_service = AppService::new(workspace_manager.clone());
    let tasks: HashMap<String, TaskClientInfo> = app_service
        .get_config(&path)
        .await
        .map(|c| c.tasks)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|task| {
            let sql_task = match &task.task_type {
                TaskType::ExecuteSQL(t) => t,
                _ => return None,
            };
            let sql = match &sql_task.sql {
                SQL::Query { sql_query } => sql_query.clone(),
                // sql_file tasks can't run in the browser without extra setup
                SQL::File { .. } => return None,
            };

            // DuckLake databases use a PostgreSQL catalog + object-storage data files that
            // are inaccessible from the browser — force server mode regardless of what the
            // task YAML declares.
            let is_ducklake = databases.iter().any(|db| {
                db.name == sql_task.database
                    && matches!(
                        &db.database_type,
                        DatabaseType::DuckDB(d) if matches!(&d.options, DuckDBOptions::DuckLake(_))
                    )
            });
            let source_files = extract_sql_source_files(&sql);

            // DuckLake and explicit server-mode tasks always run on the backend.
            // Tasks with source files can run client-side — the browser downloads a Parquet
            // version of each source file via /apps/source/ and re-runs the SQL in DuckDB WASM.
            let effective_mode = if is_ducklake || task.mode == AppTaskMode::Server {
                AppTaskMode::Server
            } else {
                task.mode.clone()
            };

            Some((
                task.name.clone(),
                TaskClientInfo {
                    sql,
                    mode: effective_mode,
                    source_files,
                },
            ))
        })
        .collect();

    // Render Jinja expressions in control defaults and options
    // (e.g. `default: "{{ now(fmt='%Y-%m-%d') }}"` or `options: ["{{ now(fmt='%Y') }}"]`)
    // so the frontend initialises widgets with computed values, not raw templates.
    let controls = controls
        .into_iter()
        .map(|mut c| {
            c.default = c.default.map(render_control_default);
            c.options = c
                .options
                .map(|opts| opts.into_iter().map(render_control_default).collect());
            c
        })
        .collect();

    Ok(extract::Json(GetDisplaysResponse {
        displays,
        controls,
        tasks,
    }))
}

pub async fn get_app_data(
    Path((_workspace_id, pathb64)): Path<(Uuid, String)>,
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
) -> Result<extract::Json<GetAppDataResponse>, StatusCode> {
    let path = decode_path(&pathb64)?;

    let mut app_service = AppService::new(workspace_manager.clone());

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

    let data = match app_service.run(&path, HashMap::new()).await {
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
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Path((_workspace_id, pathb64)): Path<(Uuid, String)>,
) -> impl IntoResponse {
    let path_string = match decode_path(&pathb64) {
        Ok(path) => path.to_string_lossy().to_string(),
        Err(status) => return Err((status, "Invalid path".to_string())),
    };

    let state_path = workspace_manager
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
        HeaderValue::from_static("private, max-age=31536000, immutable"),
    );

    Ok((StatusCode::OK, headers, body))
}

/// Serve a project source file as Parquet so the browser can register it in DuckDB WASM
/// and re-run SQL client-side. The server reads the file via DuckDB (handling CSV, JSON,
/// Parquet, etc.) and re-serializes as Parquet — smaller and faster to parse than CSV.
///
/// Search order: (1) project root, (2) each local DuckDB database's file_search_path.
pub async fn get_source_file(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Path((_workspace_id, pathb64)): Path<(Uuid, String)>,
) -> impl IntoResponse {
    let path_string = match decode_path(&pathb64) {
        Ok(path) => path.to_string_lossy().to_string(),
        Err(status) => return Err((status, "Invalid path".to_string())),
    };

    let workspace_path = workspace_manager
        .config_manager
        .workspace_path()
        .to_path_buf();

    // Build candidate search directories.
    let mut search_dirs: Vec<PathBuf> = vec![workspace_path.clone()];
    for db in workspace_manager.config_manager.list_databases() {
        if let DatabaseType::DuckDB(duckdb) = &db.database_type
            && let DuckDBOptions::Local { file_search_path } = &duckdb.options
        {
            search_dirs.push(workspace_path.join(file_search_path));
        }
    }

    // Find the file under one of the search directories.
    // The canonicalized path is safe to use directly in SQL — the starts_with
    // check ensures it stays inside the project root.
    let full_path = search_dirs
        .iter()
        .find_map(|dir| {
            let candidate = dir.join(&path_string);
            candidate
                .canonicalize()
                .ok()
                .filter(|p| p.starts_with(&workspace_path))
        })
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("File not found: {path_string}"),
            )
        })?;

    // Use DuckDB to read the file and re-serialize as Parquet bytes.
    // This is done on a blocking thread because DuckDB is synchronous.
    let parquet_bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
        use duckdb::Connection;
        use parquet::arrow::arrow_writer::ArrowWriter;

        let conn = Connection::open_in_memory().map_err(|e| e.to_string())?;
        // Use the canonicalized absolute path so subdirectory references
        // (e.g. 'data/sales.csv') work correctly regardless of DuckDB's
        // default search path.
        let full_path_escaped = full_path.to_string_lossy().replace('\'', "''");

        let mut stmt = conn
            .prepare(&format!("SELECT * FROM '{full_path_escaped}'"))
            .map_err(|e| e.to_string())?;
        let arrow_stream = stmt.query_arrow([]).map_err(|e| e.to_string())?;
        let schema = arrow_stream.get_schema();
        let batches: Vec<_> = arrow_stream.collect();

        let mut buf = Vec::new();
        let mut writer = ArrowWriter::try_new(&mut buf, schema, None).map_err(|e| e.to_string())?;
        for batch in batches {
            writer.write(&batch).map_err(|e| e.to_string())?;
        }
        writer.close().map_err(|e| e.to_string())?;
        Ok(buf)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let mut headers = HeaderMap::new();
    headers.insert(
        "Content-Type",
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(
        "Cache-Control",
        HeaderValue::from_static("private, max-age=3600"),
    );

    Ok((StatusCode::OK, headers, Body::from(parquet_bytes)))
}

#[derive(Deserialize, Default)]
pub struct RunAppBody {
    #[serde(default)]
    pub params: HashMap<String, JsonValue>,
}

pub async fn run_app(
    Path((_workspace_id, pathb64)): Path<(Uuid, String)>,
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    body: Option<extract::Json<RunAppBody>>,
) -> Result<extract::Json<GetAppDataResponse>, StatusCode> {
    let path = decode_path(&pathb64)?;
    let params = body.map(|b| b.0.params).unwrap_or_default();

    let mut app_service = AppService::new(workspace_manager.clone());
    let data = match app_service.run(&path, params).await {
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

fn get_result_cache_filename(app_path: &PathBuf) -> String {
    use xxhash_rust::xxh3::xxh3_64;
    let path_bytes = app_path.to_string_lossy();
    let hash = xxh3_64(path_bytes.as_bytes());
    format!("{hash:x}.app.result.yml")
}

#[utoipa::path(
    method(post),
    path = "/{workspace_id}/apps/{pathb64}/result",
    params(
        ("workspace_id" = Uuid, Path, description = "Workspace UUID"),
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
    Path((_workspace_id, pathb64)): Path<(Uuid, String)>,
    extract::Query(query): extract::Query<AppResultQuery>,
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
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
    if !query.refresh
        && let Some(cached) = load_cached_result(&workspace_manager, &path).await
    {
        return (StatusCode::OK, extract::Json(cached));
    }

    // Execute the app to get task results
    let mut app_service = AppService::new(workspace_manager.clone());

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
    let execution_result = app_service.run(&path, HashMap::new()).await;

    // Transform execution results into TaskResult objects
    let (tasks, execution_succeeded): (Vec<TaskResult>, bool) = match execution_result {
        Ok(DataContainer::Map(results)) => {
            // Convert the results map into TaskResult objects
            let tasks = task_configs
                .iter()
                .map(|task_config| {
                    let task_name = task_config.name.clone();
                    let task_type = TaskKind::from(task_config.kind());
                    let output = results.get(&task_name).and_then(data_container_to_output);

                    TaskResult {
                        task_name,
                        task_type,
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
                    task_type: TaskKind::from(task_config.kind()),
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
                    task_type: TaskKind::from(task_config.kind()),
                    output: None,
                    error: Some("Unexpected output format".to_string()),
                })
                .collect();
            (tasks, false)
        }
    };

    // Build a map of task_name -> output data for resolving display references
    let task_data_map: HashMap<String, TaskOutput> = tasks
        .iter()
        .filter_map(|t| t.output.as_ref().map(|o| (t.task_name.clone(), o.clone())))
        .collect();

    // Get typed displays
    let typed_displays = match get_app_displays(workspace_manager.clone(), &path).await {
        Ok((displays, _controls)) => displays,
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
        let charts_dir = workspace_manager
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

    // Build typed displays with resolved data
    let displays: Vec<AppResultDisplay> = typed_displays
        .into_iter()
        .enumerate()
        .filter_map(|(i, d)| match d {
            DisplayWithError::Display(display) => Some(match display {
                Display::LineChart(chart) => {
                    let file_path = chart_file_map.get(&(i as i64)).cloned();
                    AppResultDisplay::LineChart(AppResultChartDisplay {
                        file_name: file_path.clone(),
                        title: chart.title,
                        error: if file_path.is_none() {
                            chart_export_error.clone()
                        } else {
                            None
                        },
                    })
                }
                Display::BarChart(chart) => {
                    let file_path = chart_file_map.get(&(i as i64)).cloned();
                    AppResultDisplay::BarChart(AppResultChartDisplay {
                        file_name: file_path.clone(),
                        title: chart.title,
                        error: if file_path.is_none() {
                            chart_export_error.clone()
                        } else {
                            None
                        },
                    })
                }
                Display::PieChart(chart) => {
                    let file_path = chart_file_map.get(&(i as i64)).cloned();
                    AppResultDisplay::PieChart(AppResultChartDisplay {
                        file_name: file_path.clone(),
                        title: chart.title,
                        error: if file_path.is_none() {
                            chart_export_error.clone()
                        } else {
                            None
                        },
                    })
                }
                Display::Table(table) => {
                    let data = task_data_map
                        .get(&table.data)
                        .and_then(|o| serde_json::to_value(o).ok());
                    AppResultDisplay::Table(AppResultTableDisplay {
                        data,
                        title: table.title,
                    })
                }
                Display::Markdown(md) => AppResultDisplay::Markdown(AppResultMarkdownDisplay {
                    content: md.content,
                }),
                Display::Row(_) | Display::Controls(_) | Display::Control(_) => return None,
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
        save_cached_result(&workspace_manager, &path, &response).await;
    }

    (StatusCode::OK, extract::Json(response))
}

fn data_container_to_output(data: &DataContainer) -> Option<TaskOutput> {
    match data {
        DataContainer::Single(Data::Bool(b)) => Some(TaskOutput::Bool(*b)),
        DataContainer::Single(Data::Text(s)) => Some(TaskOutput::Text(s.clone())),
        DataContainer::Single(Data::Table(table_data)) => {
            if let Some(json_str) = table_data.json.as_deref() {
                match serde_json::from_str(json_str) {
                    Ok(value) => Some(TaskOutput::Table(value)),
                    Err(_) => serde_json::to_value(table_data).ok().map(TaskOutput::Table),
                }
            } else {
                serde_json::to_value(table_data).ok().map(TaskOutput::Table)
            }
        }
        DataContainer::Single(Data::None) | DataContainer::None => None,
        DataContainer::List(items) => {
            let outputs: Vec<Box<TaskOutput>> = items
                .iter()
                .map(|item| Box::new(data_container_to_output(item).unwrap_or(TaskOutput::None)))
                .collect();
            if outputs.is_empty() {
                None
            } else {
                Some(TaskOutput::List(outputs))
            }
        }
        DataContainer::Map(map) => {
            let outputs: HashMap<String, Box<TaskOutput>> = map
                .iter()
                .filter_map(|(k, v)| data_container_to_output(v).map(|o| (k.clone(), Box::new(o))))
                .collect();
            if outputs.is_empty() {
                None
            } else {
                Some(TaskOutput::Map(outputs))
            }
        }
    }
}

async fn load_cached_result(
    workspace_manager: &oxy::adapters::workspace::manager::WorkspaceManager,
    app_path: &PathBuf,
) -> Option<GetAppResultResponse> {
    let cache_name = get_result_cache_filename(app_path);
    let results_dir = workspace_manager
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
    workspace_manager: &oxy::adapters::workspace::manager::WorkspaceManager,
    app_path: &PathBuf,
    response: &GetAppResultResponse,
) {
    let cache_name = get_result_cache_filename(app_path);
    let Ok(results_dir) = workspace_manager.config_manager.get_app_results_dir().await else {
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
    path = "/{workspace_id}/apps/{pathb64}/charts/{chart_path}",
    params(
        ("workspace_id" = Uuid, Path, description = "Workspace UUID"),
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
    Path((_workspace_id, pathb64, chart_path)): Path<(Uuid, String, String)>,
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
) -> Result<impl IntoResponse, StatusCode> {
    let _app_path = decode_path(&pathb64)?;

    // Get charts directory
    let charts_dir = workspace_manager
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
        HeaderValue::from_static("private, max-age=3600"),
    );

    Ok((StatusCode::OK, headers, body))
}

// ── App-builder run → save as app file ──────────────────────────────────────

#[derive(Serialize)]
pub struct SaveAppBuilderRunResponse {
    /// Base64-encoded app file path for use with AppPreview.
    pub app_path64: String,
    /// Project-relative path of the saved file.
    pub app_path: String,
}

/// Save a completed app-builder run's generated YAML as an `.app.yml` file
/// in the project directory and return the base64-encoded path for AppPreview.
///
/// The file is written to `generated/{run_id}.app.yml` within the project root.
pub async fn save_app_builder_run(
    Path((_workspace_id, run_id)): Path<(Uuid, String)>,
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
) -> Result<extract::Json<SaveAppBuilderRunResponse>, StatusCode> {
    use agentic_db::entity::agentic_run;
    use sea_orm::EntityTrait;

    let db = oxy::database::client::establish_connection()
        .await
        .map_err(|e| {
            tracing::error!("db connect failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let run = agentic_run::Entity::find_by_id(&run_id)
        .one(&db)
        .await
        .map_err(|e| {
            tracing::error!("db query failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let yaml = run.answer.ok_or(StatusCode::CONFLICT)?;

    // Write to {workspace_path}/generated/{run_id}.app.yml
    let workspace_path = workspace_manager.config_manager.workspace_path();
    let generated_dir = workspace_path.join("generated");
    tokio::fs::create_dir_all(&generated_dir)
        .await
        .map_err(|e| {
            tracing::error!("create generated dir: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let file_name = format!("{run_id}.app.yml");
    let full_path = generated_dir.join(&file_name);
    tokio::fs::write(&full_path, &yaml).await.map_err(|e| {
        tracing::error!("write app file: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let relative = format!("generated/{file_name}");
    let app_path64 = BASE64_STANDARD.encode(&relative);

    Ok(extract::Json(SaveAppBuilderRunResponse {
        app_path64,
        app_path: relative,
    }))
}
