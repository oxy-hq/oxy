use axum::{
    extract::{self, Path},
    http::StatusCode,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    api::middlewares::project::ProjectManagerExtractor,
    server::service::test_runs::{TestProjectRunInfo, TestRunsManager},
};

#[derive(Deserialize)]
pub struct CreateProjectRunBody {
    pub name: Option<String>,
}

pub async fn create_project_run(
    Path(project_id): Path<Uuid>,
    ProjectManagerExtractor(_pm): ProjectManagerExtractor,
    extract::Json(body): extract::Json<CreateProjectRunBody>,
) -> Result<extract::Json<TestProjectRunInfo>, StatusCode> {
    let manager = TestRunsManager::new(project_id).await.map_err(|e| {
        tracing::error!("Failed to create TestRunsManager: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let run = manager.new_project_run(body.name).await.map_err(|e| {
        tracing::error!("Failed to create project run: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(extract::Json(run))
}

pub async fn list_project_runs(
    Path(project_id): Path<Uuid>,
    ProjectManagerExtractor(_pm): ProjectManagerExtractor,
) -> Result<extract::Json<Vec<TestProjectRunInfo>>, StatusCode> {
    let manager = TestRunsManager::new(project_id).await.map_err(|e| {
        tracing::error!("Failed to create TestRunsManager: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let runs = manager.list_project_runs().await.map_err(|e| {
        tracing::error!("Failed to list project runs: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(extract::Json(runs))
}

pub async fn delete_project_run(
    Path((project_id, project_run_id)): Path<(Uuid, Uuid)>,
    ProjectManagerExtractor(_pm): ProjectManagerExtractor,
) -> Result<StatusCode, StatusCode> {
    let manager = TestRunsManager::new(project_id).await.map_err(|e| {
        tracing::error!("Failed to create TestRunsManager: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    manager
        .delete_project_run(project_run_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete project run: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(StatusCode::NO_CONTENT)
}
