use axum::extract::Path as AxumPath;
use axum::http::StatusCode;
use axum::response::sse::Sse;
use axum::routing::{get, post};
use axum::{Json, Router};
use oxy::utils::create_sse_stream;
use tokio::sync::mpsc;
use uuid::Uuid;

use oxy_airform::service::{self, AirformService};
use oxy_airform::types::{
    AnalyzeOutput, ColumnLineageOutput, CompileOutput, DbtProjectInfo, LineageOutput, NodeSummary,
    RunOutput, RunRequest, RunStreamEvent, SeedOutput, TestOutput,
};

use crate::server::api::middlewares::workspace_context::WorkspaceManagerExtractor;
use crate::server::router::AppState;

pub fn build_modeling_routes() -> Router<AppState> {
    let project_routes = Router::new()
        .route("/", get(get_project_info))
        .route("/nodes", get(list_nodes))
        .route("/compile", post(compile_project))
        .route("/compile/{model_name}", post(compile_model))
        .route("/run", post(run_models))
        .route("/run/stream", post(run_models_stream))
        .route("/test", post(run_tests))
        .route("/analyze", post(analyze_project))
        .route("/seed", post(seed_project))
        .route("/lineage", get(get_lineage))
        .route("/lineage/columns", get(get_column_lineage));

    Router::new()
        .route("/", get(list_projects))
        .nest("/{project_name}", project_routes)
}

fn root_path(wm: &oxy::adapters::workspace::manager::WorkspaceManager) -> std::path::PathBuf {
    wm.config_manager.workspace_path().to_path_buf()
}

fn validate_project_name(name: &str) -> Result<(), StatusCode> {
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        Err(StatusCode::BAD_REQUEST)
    } else {
        Ok(())
    }
}

fn make_service(
    wm: &oxy::adapters::workspace::manager::WorkspaceManager,
    project_name: &str,
) -> Result<AirformService, StatusCode> {
    validate_project_name(project_name)?;
    let project_dir = root_path(wm).join("modeling").join(project_name);
    Ok(AirformService::new(project_dir)
        .with_oxy_context(wm.config_manager.clone(), wm.secrets_manager.clone()))
}

pub async fn list_projects(
    WorkspaceManagerExtractor(wm): WorkspaceManagerExtractor,
) -> Json<Vec<DbtProjectInfo>> {
    let root = root_path(&wm);
    Json(service::list_projects(&root))
}

pub async fn get_project_info(
    WorkspaceManagerExtractor(wm): WorkspaceManagerExtractor,
    AxumPath((_pid, project_name)): AxumPath<(Uuid, String)>,
) -> Result<Json<DbtProjectInfo>, StatusCode> {
    make_service(&wm, &project_name)?
        .get_project_info()
        .map(Json)
        .map_err(|e| {
            tracing::warn!("modeling project_info {}: {}", project_name, e);
            StatusCode::NOT_FOUND
        })
}

pub async fn list_nodes(
    WorkspaceManagerExtractor(wm): WorkspaceManagerExtractor,
    AxumPath((_pid, project_name)): AxumPath<(Uuid, String)>,
) -> Result<Json<Vec<NodeSummary>>, StatusCode> {
    let svc = make_service(&wm, &project_name)?;
    tokio::task::spawn_blocking(move || svc.list_nodes())
        .await
        .map_err(|e| {
            tracing::error!("modeling list_nodes {} panicked: {}", project_name, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map(Json)
        .map_err(|e| {
            tracing::error!("modeling list_nodes {}: {}", project_name, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn compile_project(
    WorkspaceManagerExtractor(wm): WorkspaceManagerExtractor,
    AxumPath((_pid, project_name)): AxumPath<(Uuid, String)>,
) -> Result<Json<CompileOutput>, StatusCode> {
    let svc = make_service(&wm, &project_name)?;
    tokio::task::spawn_blocking(move || svc.compile_project())
        .await
        .map_err(|e| {
            tracing::error!("modeling compile {} panicked: {}", project_name, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map(Json)
        .map_err(|e| {
            tracing::error!("modeling compile {}: {}", project_name, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn compile_model(
    WorkspaceManagerExtractor(wm): WorkspaceManagerExtractor,
    AxumPath((_pid, project_name, model_name)): AxumPath<(Uuid, String, String)>,
) -> Result<Json<String>, StatusCode> {
    let svc = make_service(&wm, &project_name)?;
    let mn = model_name.clone();
    tokio::task::spawn_blocking(move || svc.compile_model(&mn))
        .await
        .map_err(|e| {
            tracing::warn!(
                "modeling compile_model {}/{} panicked: {}",
                project_name,
                model_name,
                e
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map(Json)
        .map_err(|e| {
            tracing::warn!(
                "modeling compile_model {}/{}: {}",
                project_name,
                model_name,
                e
            );
            StatusCode::NOT_FOUND
        })
}

pub async fn run_models(
    WorkspaceManagerExtractor(wm): WorkspaceManagerExtractor,
    AxumPath((_pid, project_name)): AxumPath<(Uuid, String)>,
    Json(req): Json<RunRequest>,
) -> Result<Json<RunOutput>, (StatusCode, String)> {
    make_service(&wm, &project_name)
        .map_err(|s| (s, "bad request".into()))?
        .run(req.selector.as_deref())
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("modeling run {}: {}", project_name, e);
            modeling_error(e)
        })
}

pub async fn run_models_stream(
    WorkspaceManagerExtractor(wm): WorkspaceManagerExtractor,
    AxumPath((_pid, project_name)): AxumPath<(Uuid, String)>,
    Json(req): Json<RunRequest>,
) -> Result<
    Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, axum::Error>>>,
    StatusCode,
> {
    let svc = make_service(&wm, &project_name)?;
    let (tx, rx) = mpsc::channel::<RunStreamEvent>(64);

    tokio::spawn(async move {
        if let Err(e) = svc.run_streaming(req.selector.as_deref(), tx.clone()).await {
            tracing::error!("modeling run_stream {}: {}", project_name, e);
            let _ = tx
                .send(RunStreamEvent::Error {
                    message: e.to_string(),
                })
                .await;
        }
    });

    Ok(Sse::new(create_sse_stream(rx)))
}

pub async fn run_tests(
    WorkspaceManagerExtractor(wm): WorkspaceManagerExtractor,
    AxumPath((_pid, project_name)): AxumPath<(Uuid, String)>,
    Json(req): Json<RunRequest>,
) -> Result<Json<TestOutput>, (StatusCode, String)> {
    make_service(&wm, &project_name)
        .map_err(|s| (s, "bad request".into()))?
        .test(req.selector.as_deref())
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("modeling test {}: {}", project_name, e);
            modeling_error(e)
        })
}

fn modeling_error(e: oxy_airform::error::AirformIntegrationError) -> (StatusCode, String) {
    let status = match &e {
        oxy_airform::error::AirformIntegrationError::MissingOxyConfig(_) => {
            StatusCode::UNPROCESSABLE_ENTITY
        }
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, e.to_string())
}

pub async fn analyze_project(
    WorkspaceManagerExtractor(wm): WorkspaceManagerExtractor,
    AxumPath((_pid, project_name)): AxumPath<(Uuid, String)>,
) -> Result<Json<AnalyzeOutput>, StatusCode> {
    let svc = make_service(&wm, &project_name)?;
    let project_name2 = project_name.clone();
    tokio::task::spawn(async move { svc.analyze().await })
        .await
        .map_err(|e| {
            tracing::error!("modeling analyze {} panicked: {}", project_name, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map(Json)
        .map_err(|e| {
            tracing::error!("modeling analyze {}: {:#}", project_name2, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn seed_project(
    WorkspaceManagerExtractor(wm): WorkspaceManagerExtractor,
    AxumPath((_pid, project_name)): AxumPath<(Uuid, String)>,
) -> Result<Json<SeedOutput>, (StatusCode, String)> {
    let svc = make_service(&wm, &project_name).map_err(|s| (s, "bad request".into()))?;
    let project_name2 = project_name.clone();
    tokio::task::spawn(async move { svc.seed().await })
        .await
        .map_err(|e| {
            tracing::error!("modeling seed {} panicked: {}", project_name, e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?
        .map(Json)
        .map_err(|e| {
            tracing::error!("modeling seed {}: {:#}", project_name2, e);
            modeling_error(e)
        })
}

pub async fn get_lineage(
    WorkspaceManagerExtractor(wm): WorkspaceManagerExtractor,
    AxumPath((_pid, project_name)): AxumPath<(Uuid, String)>,
) -> Result<Json<LineageOutput>, StatusCode> {
    let svc = make_service(&wm, &project_name)?;
    tokio::task::spawn_blocking(move || svc.get_lineage())
        .await
        .map_err(|e| {
            tracing::error!("modeling lineage {} panicked: {}", project_name, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map(Json)
        .map_err(|e| {
            tracing::error!("modeling lineage {}: {}", project_name, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn get_column_lineage(
    WorkspaceManagerExtractor(wm): WorkspaceManagerExtractor,
    AxumPath((_pid, project_name)): AxumPath<(Uuid, String)>,
) -> Result<Json<ColumnLineageOutput>, StatusCode> {
    let svc = make_service(&wm, &project_name)?;
    tokio::task::spawn_blocking(move || svc.get_column_lineage())
        .await
        .map_err(|e| {
            tracing::error!("modeling column_lineage {} panicked: {}", project_name, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map(Json)
        .map_err(|e| {
            tracing::error!("modeling column_lineage {}: {}", project_name, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}
