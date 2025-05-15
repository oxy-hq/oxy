use base64::prelude::*;
use std::path::PathBuf;
use utoipa::ToSchema;

use std::fs::File;
use std::sync::Arc;
use std::sync::Mutex;

use crate::config::model::Workflow;
use crate::service::workflow as service;
use crate::service::workflow::WorkflowInfo;
use crate::service::workflow::get_workflow;
use crate::utils::find_project_path;
use crate::workflow::loggers::api::WorkflowAPILogger;
use crate::workflow::loggers::types::LogItem;
use axum::extract::{self, Path};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum_streams::StreamBodyAs;
use serde::Serialize;
use std::fs::OpenOptions;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

#[derive(Serialize)]
pub struct GetWorkflowResponse {
    data: Workflow,
}

#[utoipa::path(
    method(get),
    path = "/workflows",
    responses(
        (status = 200, description = "Success", body = Vec<WorkflowInfo>, content_type = "application/json")
    )
)]
pub async fn list() -> impl IntoResponse {
    match crate::service::workflow::list_workflows(None).await {
        Ok(workflows) => {
            let response = serde_json::to_string(&workflows).unwrap();
            (StatusCode::OK, response)
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn get(
    Path(pathb64): Path<String>,
) -> Result<extract::Json<GetWorkflowResponse>, StatusCode> {
    let decoded_path = BASE64_STANDARD.decode(pathb64).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?;
    let path = String::from_utf8(decoded_path).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?;
    match get_workflow(PathBuf::from(path), None).await {
        Ok(workflow) => Ok(extract::Json(GetWorkflowResponse { data: workflow })),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

#[derive(Serialize, ToSchema)]
pub struct GetLogsResponse {
    logs: Vec<LogItem>,
}

#[utoipa::path(
    method(get),
    path = "/workflows/{pathb64}/logs",
    responses(
        (status = 200, description = "Success", body = GetLogsResponse, content_type = "application/json")
    )
)]
pub async fn get_logs(
    Path(pathb64): Path<String>,
) -> Result<extract::Json<GetLogsResponse>, StatusCode> {
    let path = PathBuf::from(
        String::from_utf8(BASE64_STANDARD.decode(pathb64).map_err(|e| {
            tracing::info!("{:?}", e);
            StatusCode::BAD_REQUEST
        })?)
        .map_err(|e| {
            tracing::info!("{:?}", e);
            StatusCode::BAD_REQUEST
        })?,
    );
    let logs = service::get_workflow_logs(&path).await?;
    Ok(extract::Json(GetLogsResponse { logs }))
}

pub async fn build_workflow_api_logger(
    full_workflow_path: &PathBuf,
) -> (WorkflowAPILogger, mpsc::Receiver<LogItem>) {
    let full_workflow_path_b64: String =
        BASE64_STANDARD.encode(full_workflow_path.to_str().unwrap());
    // Create a channel to send logs to the client
    let (sender, receiver) = mpsc::channel(100);
    let log_file_path = format!("/var/tmp/oxy-{}.log.json", full_workflow_path_b64);
    File::create(log_file_path.clone()).unwrap();
    let file = OpenOptions::new()
        .append(true)
        .open(log_file_path)
        .map_err(|e| {
            tracing::error!("{:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
        .unwrap();
    let api_logger: WorkflowAPILogger =
        WorkflowAPILogger::new(sender, Some(Arc::new(Mutex::new(file))));
    (api_logger, receiver)
}

#[utoipa::path(
    method(post),
    path = "/workflows/{pathb64}/run",
    responses(
        (status = 200, description = "Success", body = (), content_type = "application/json")
    )
)]
pub async fn run_workflow(Path(pathb64): Path<String>) -> Result<impl IntoResponse, StatusCode> {
    let decoded_path = BASE64_STANDARD.decode(pathb64).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?;
    let path = PathBuf::from(String::from_utf8(decoded_path).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?);
    let project_path = find_project_path()?;

    let full_workflow_path = project_path.join(&path);
    let (logger, receiver) = build_workflow_api_logger(&full_workflow_path).await;
    let _ = tokio::spawn(async move { service::run_workflow(&path, logger, false, None).await });
    let stream = ReceiverStream::new(receiver);
    Ok(StreamBodyAs::json_nl(stream))
}

#[derive(Serialize)]
pub struct RunResponse {
    output: String,
}
