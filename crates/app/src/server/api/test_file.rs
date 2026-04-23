use crate::{
    api::middlewares::role_guards::WorkspaceEditor,
    api::middlewares::workspace_context::WorkspaceManagerExtractor,
    server::service::test::TestCasePersistContext, server::service::test::run_test,
    server::service::test_runs::TestRunsManager,
};
use axum::{
    extract::{self, Path, Query},
    http::StatusCode,
    response::{
        IntoResponse,
        sse::{Event, Sse},
    },
};
use base64::{Engine, prelude::BASE64_STANDARD};
use futures::Stream;
use oxy::utils::create_sse_stream_from_stream;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use uuid::Uuid;

use async_stream::stream;

type EventStream = Pin<Box<dyn Stream<Item = Result<Event, axum::Error>> + Send>>;

fn create_error_stream(error_message: String) -> EventStream {
    Box::pin(stream! {
        let error_msg = serde_json::json!({
            "error": error_message,
            "event": null
        });
        yield Ok::<_, axum::Error>(
            Event::default()
                .event("error")
                .data(error_msg.to_string())
        );
    })
}

fn decode_path_from_base64(pathb64: String) -> Result<String, String> {
    let decoded_path = BASE64_STANDARD
        .decode(pathb64)
        .map_err(|e| format!("Failed to decode path: {e}"))?;

    let path =
        String::from_utf8(decoded_path).map_err(|e| format!("Failed to decode path: {e}"))?;

    if path.contains("..") {
        return Err("Invalid path".to_string());
    }

    Ok(path)
}

#[derive(Serialize)]
pub struct TestFileSummary {
    pub path: String,
    pub name: Option<String>,
    pub target: Option<String>,
    pub case_count: usize,
}

pub async fn list_test_files(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
) -> Result<extract::Json<Vec<TestFileSummary>>, StatusCode> {
    let paths = workspace_manager
        .config_manager
        .list_tests()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut summaries = Vec::new();
    for path in paths {
        let path_str = path.to_string_lossy().to_string();
        match workspace_manager.config_manager.resolve_test(&path).await {
            Ok(config) => {
                summaries.push(TestFileSummary {
                    path: path_str,
                    name: config.name,
                    target: config.target,
                    case_count: config.cases.len(),
                });
            }
            Err(e) => {
                tracing::warn!("Skipping test file {path_str}: failed to parse config: {e}");
            }
        }
    }

    Ok(extract::Json(summaries))
}

pub async fn get_test_file(
    Path((_workspace_id, pathb64)): Path<(Uuid, String)>,
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
) -> Result<impl IntoResponse, StatusCode> {
    let path = decode_path_from_base64(pathb64).map_err(|_| StatusCode::BAD_REQUEST)?;

    let config = workspace_manager
        .config_manager
        .resolve_test(&path)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    Ok(extract::Json(config))
}

#[derive(Deserialize)]
pub struct RunTestCaseQuery {
    pub run_index: Option<i32>,
}

pub async fn run_test_case(
    _: WorkspaceEditor,
    Path((workspace_id, pathb64, case_index)): Path<(Uuid, String, usize)>,
    Query(query): Query<RunTestCaseQuery>,
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
) -> Result<impl IntoResponse, StatusCode> {
    let path = match decode_path_from_base64(pathb64) {
        Ok(path) => path,
        Err(error) => return Ok(Sse::new(create_error_stream(error))),
    };

    // Build persist context if a run_index was provided
    let persist = if let Some(run_index) = query.run_index {
        // Load test config to get prompt/expected for this case
        let config = match workspace_manager
            .config_manager
            .resolve_test(std::path::Path::new(&path))
            .await
        {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Failed to load test config for persistence: {e}");
                return Ok(Sse::new(create_error_stream(format!(
                    "Failed to load test config: {e}"
                ))));
            }
        };

        let (prompt, expected) = config
            .cases
            .get(case_index)
            .map(|c| (c.prompt.clone(), c.expected.clone()))
            .unwrap_or_default();

        // Look up the test_run_id from source_id + run_index
        match TestRunsManager::new(workspace_id).await {
            Ok(manager) => match manager.get_run(&path, run_index).await {
                Ok(Some(run_with_cases)) => Some(TestCasePersistContext {
                    workspace_id,
                    test_run_id: run_with_cases.run.id,
                    case_index,
                    prompt,
                    expected,
                }),
                Ok(None) => {
                    tracing::warn!(
                        "Test run not found for source_id={path}, run_index={run_index}"
                    );
                    None
                }
                Err(e) => {
                    tracing::warn!("Failed to look up test run for persistence: {e}");
                    None
                }
            },
            Err(e) => {
                tracing::warn!("Failed to create TestRunsManager: {e}");
                None
            }
        }
    } else {
        None
    };

    let test_stream = match run_test(workspace_manager, path, case_index, persist).await {
        Ok(stream) => stream,
        Err(e) => {
            let error = format!("Failed to run test case: {e}");
            return Ok(Sse::new(create_error_stream(error)));
        }
    };

    Ok(Sse::new(Box::pin(create_sse_stream_from_stream(Box::pin(
        test_stream,
    )))))
}
