use axum::{
    extract::{self, Path},
    http::StatusCode,
};
use base64::{Engine, prelude::BASE64_STANDARD};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    api::middlewares::project::ProjectManagerExtractor,
    server::service::test_runs::{
        HumanVerdictInfo, TestRunInfo, TestRunWithCases, TestRunsManager,
    },
};

fn decode_path_from_base64(pathb64: String) -> Result<String, StatusCode> {
    let decoded = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let path = String::from_utf8(decoded).map_err(|_| StatusCode::BAD_REQUEST)?;
    if path.contains("..") {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(path)
}

#[derive(Deserialize)]
pub struct CreateRunBody {
    pub name: Option<String>,
    pub project_run_id: Option<Uuid>,
}

pub async fn create_run(
    Path((project_id, pathb64)): Path<(Uuid, String)>,
    ProjectManagerExtractor(_pm): ProjectManagerExtractor,
    extract::Json(body): extract::Json<CreateRunBody>,
) -> Result<extract::Json<TestRunInfo>, StatusCode> {
    let source_id = decode_path_from_base64(pathb64)?;
    let manager = TestRunsManager::new(project_id).await.map_err(|e| {
        tracing::error!("Failed to create TestRunsManager: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let run = manager
        .new_run(&source_id, body.name, body.project_run_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create test run: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(extract::Json(run))
}

pub async fn list_runs(
    Path((project_id, pathb64)): Path<(Uuid, String)>,
    ProjectManagerExtractor(_pm): ProjectManagerExtractor,
) -> Result<extract::Json<Vec<TestRunInfo>>, StatusCode> {
    let source_id = decode_path_from_base64(pathb64)?;
    let manager = TestRunsManager::new(project_id).await.map_err(|e| {
        tracing::error!("Failed to create TestRunsManager: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let runs = manager.list_runs(&source_id).await.map_err(|e| {
        tracing::error!("Failed to list test runs: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(extract::Json(runs))
}

pub async fn get_run(
    Path((project_id, pathb64, run_index)): Path<(Uuid, String, i32)>,
    ProjectManagerExtractor(_pm): ProjectManagerExtractor,
) -> Result<extract::Json<TestRunWithCases>, StatusCode> {
    let source_id = decode_path_from_base64(pathb64)?;
    let manager = TestRunsManager::new(project_id).await.map_err(|e| {
        tracing::error!("Failed to create TestRunsManager: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let run = manager
        .get_run(&source_id, run_index)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get test run: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(extract::Json(run))
}

pub async fn delete_run(
    Path((project_id, pathb64, run_index)): Path<(Uuid, String, i32)>,
    ProjectManagerExtractor(_pm): ProjectManagerExtractor,
) -> Result<StatusCode, StatusCode> {
    let source_id = decode_path_from_base64(pathb64)?;
    let manager = TestRunsManager::new(project_id).await.map_err(|e| {
        tracing::error!("Failed to create TestRunsManager: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    manager
        .delete_run(&source_id, run_index)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete test run: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct SetHumanVerdictBody {
    pub verdict: Option<String>,
}

pub async fn set_human_verdict(
    Path((project_id, pathb64, run_index, case_index)): Path<(Uuid, String, i32, i32)>,
    ProjectManagerExtractor(_pm): ProjectManagerExtractor,
    extract::Json(body): extract::Json<SetHumanVerdictBody>,
) -> Result<extract::Json<Option<HumanVerdictInfo>>, StatusCode> {
    if let Some(ref v) = body.verdict
        && v != "pass"
        && v != "fail"
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let source_id = decode_path_from_base64(pathb64)?;
    let manager = TestRunsManager::new(project_id).await.map_err(|e| {
        tracing::error!("Failed to create TestRunsManager: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let result = manager
        .set_human_verdict(&source_id, run_index, case_index, body.verdict)
        .await
        .map_err(|e| {
            tracing::error!("Failed to set human verdict: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(extract::Json(result))
}

pub async fn list_human_verdicts(
    Path((project_id, pathb64, run_index)): Path<(Uuid, String, i32)>,
    ProjectManagerExtractor(_pm): ProjectManagerExtractor,
) -> Result<extract::Json<Vec<HumanVerdictInfo>>, StatusCode> {
    let source_id = decode_path_from_base64(pathb64)?;
    let manager = TestRunsManager::new(project_id).await.map_err(|e| {
        tracing::error!("Failed to create TestRunsManager: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let verdicts = manager
        .list_human_verdicts(&source_id, run_index)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list human verdicts: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(extract::Json(verdicts))
}
