use std::path::PathBuf;

use std::fs::File;
use std::sync::Arc;
use std::sync::Mutex;
use tokio_stream::StreamExt;
use tokio_util::io::ReaderStream;

use crate::config::model::Workflow;
use crate::config::{load_config, ConfigBuilder};
use crate::errors::OnyxError;
use crate::execute::core::event::Dispatcher;
use crate::execute::core::run;
use crate::execute::workflow::LogItem;
use crate::execute::workflow::{
    WorkflowAPILogger, WorkflowExporter, WorkflowInput, WorkflowReceiver,
};
use crate::service::workflow::get_workflow;
use crate::utils::find_project_path;
use crate::workflow::executor::WorkflowExecutor;
use axum::extract::{self, Json, Path};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum_streams::StreamBodyAs;
use futures::Stream;
use minijinja::Value;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use tokio::sync::mpsc;

#[derive(Serialize)]
pub struct GetWorkflowResponse {
    data: Workflow,
}

pub async fn list() -> impl IntoResponse {
    match crate::service::workflow::list_workflows().await {
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
    let decoded_path = base64::decode(pathb64).unwrap();
    let path = String::from_utf8(decoded_path).unwrap();
    match get_workflow(PathBuf::from(path)).await {
        Ok(workflow) => Ok(extract::Json(GetWorkflowResponse { data: workflow })),
        Err(e) => Err(StatusCode::NOT_FOUND),
    }
}

#[derive(Serialize)]
pub struct GetLogsResponse {
    logs: Vec<LogItem>,
}

pub async fn get_logs(
    Path(pathb64): Path<String>,
) -> Result<extract::Json<GetLogsResponse>, StatusCode> {
    let decoded_path = base64::decode(pathb64).unwrap();
    let path = String::from_utf8(decoded_path).unwrap();
    let project_path = find_project_path().unwrap();
    let full_workflow_path = project_path.join(&PathBuf::from(path));
    let full_workflow_path_b64: String = base64::encode(full_workflow_path.to_str().unwrap());
    let log_file_path = format!("/var/tmp/onyx-{}.log.json", full_workflow_path_b64);
    println!("{:?}", log_file_path);
    let content = std::fs::read_to_string(log_file_path);
    println!("{:?}", content);
    match content {
        Ok(content) => {
            let mut logs: Vec<LogItem> = vec![];
            let lines = content.lines();
            for line in lines {
                let log_item: LogItem = serde_json::from_str(line).unwrap();
                logs.push(log_item);
            }
            return Ok(extract::Json(GetLogsResponse { logs }));
        }
        Err(_) => return Err(StatusCode::NOT_FOUND),
    }
}

pub async fn run_workflow(
    Path(pathb64): Path<String>,
    Json(payload): Json<RunPayload>,
) -> impl IntoResponse {
    let decoded_path = base64::decode(pathb64).unwrap();
    let path = String::from_utf8(decoded_path).unwrap();
    let project_path = find_project_path().unwrap();

    let config = ConfigBuilder::new()
        .with_project_path(find_project_path().unwrap())
        .unwrap()
        .build()
        .await
        .unwrap();
    let full_workflow_path = project_path.join(&PathBuf::from(path));
    let workflow = config.resolve_workflow(&full_workflow_path).await.unwrap();

    let full_workflow_path_b64: String = base64::encode(full_workflow_path.to_str().unwrap());
    // Create a channel to send logs to the client
    let (sender, receiver) = mpsc::channel(100);
    let log_file_path = format!("/var/tmp/onyx-{}.log.json", full_workflow_path_b64);
    File::create(log_file_path.clone()).unwrap();
    let file = OpenOptions::new()
        .write(true)
        .append(true)
        .open(log_file_path)
        .unwrap();
    let pipe_logger: WorkflowAPILogger =
        WorkflowAPILogger::new(sender, Some(Arc::new(Mutex::new(file))));

    let dispatcher = Dispatcher::new(vec![
        Box::new(WorkflowReceiver::new(Box::new(pipe_logger))),
        Box::new(WorkflowExporter),
    ]);
    let executor = WorkflowExecutor::new(workflow.clone());
    let ctx = Value::from_serialize(&workflow.variables);
    tokio::spawn(async move {
        run(
            &executor,
            WorkflowInput,
            Arc::new(config),
            ctx,
            Some(&workflow),
            dispatcher,
        )
        .await
        .unwrap();
    });
    use tokio_stream::wrappers::ReceiverStream;
    let stream = ReceiverStream::new(receiver);
    return StreamBodyAs::json_nl(stream);
}

// Define a struct to represent the request payload
#[derive(Deserialize)]
pub struct RunPayload {
    project_path: String,
}

#[derive(Serialize)]
pub struct RunResponse {
    output: String,
}
