use std::{path::PathBuf, pin::Pin};

use crate::{
    auth::extractor::AuthenticatedUserExtractor,
    config::{ConfigBuilder, model::AgentConfig},
    errors::OxyError,
    execute::types::{ReferenceKind, Usage},
    project::resolve_project_path,
    service::{
        agent::run_agent,
        chat::{ChatExecutionContext, ChatExecutionRequest, ChatHandler, ChatService},
        formatters::BlockHandler,
        test::run_test as run_agent_test,
        types::{AnswerContent, AnswerStream},
    },
    utils::{create_sse_stream, create_sse_stream_from_stream},
};
use async_stream::stream;
use async_trait::async_trait;
use axum::{
    extract::{self, Path},
    http::StatusCode,
    response::{
        IntoResponse,
        sse::{Event, Sse},
    },
};
use base64::{Engine, prelude::BASE64_STANDARD};
use futures::Stream;
use sea_orm::{ActiveModelTrait, ActiveValue};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Serialize)]
pub struct BuilderAvailabilityResponse {
    pub available: bool,
}

pub async fn check_builder_availability()
-> Result<extract::Json<BuilderAvailabilityResponse>, StatusCode> {
    let project_path = resolve_project_path().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let config_builder = ConfigBuilder::new()
        .with_project_path(&project_path)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let config = config_builder
        .build()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let is_available = config.get_builder_agent_path().await.is_ok();

    Ok(extract::Json(BuilderAvailabilityResponse {
        available: is_available,
    }))
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgentConfigResponse {
    #[serde(flatten)]
    pub config: AgentConfig,
    pub path: String,
}

impl AgentConfigResponse {
    pub fn new(config: AgentConfig, path: String) -> Self {
        Self { config, path }
    }

    pub fn from_config(config: AgentConfig, path: &str) -> Self {
        Self::new(config, path.to_string())
    }
}

#[utoipa::path(
    method(get),
    path = "/agents",
    responses(
        (status = OK, description = "Success", body = Vec<String>, content_type = "application/json")
    )
)]
pub async fn get_agents() -> Result<extract::Json<Vec<AgentConfigResponse>>, StatusCode> {
    let project_path = resolve_project_path().map_err(|e| {
        tracing::error!("Failed to resolve project path: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let config = ConfigBuilder::new()
        .with_project_path(&project_path)
        .map_err(|e| {
            tracing::error!("Failed to create config builder: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .build()
        .await
        .map_err(|e| {
            tracing::error!("Failed to build config: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let agent_paths = config.list_agents().await.map_err(|e| {
        tracing::error!("Failed to list agents: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let agent_relative_paths: Vec<String> = agent_paths
        .iter()
        .filter_map(|agent| {
            agent
                .strip_prefix(&project_path)
                .ok()
                .map(|path| path.to_string_lossy().to_string())
        })
        .collect();

    let agent_futures = agent_relative_paths
        .into_iter()
        .map(|path| {
            let config = &config;
            async move {
                let agent_config = config.resolve_agent(&path).await?;
                Ok::<AgentConfigResponse, anyhow::Error>(AgentConfigResponse::from_config(
                    agent_config,
                    &path,
                ))
            }
        })
        .collect::<Vec<_>>();

    let agents: Vec<AgentConfigResponse> = futures::future::try_join_all(agent_futures)
        .await
        .map_err(|e| {
            tracing::error!("Failed to resolve agent configs: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(extract::Json(agents))
}

pub async fn get_agent(
    Path(pathb64): Path<String>,
) -> Result<extract::Json<AgentConfig>, StatusCode> {
    let decoded_path: Vec<u8> = BASE64_STANDARD.decode(pathb64).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?;
    let path = String::from_utf8(decoded_path).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?;
    let project_path = resolve_project_path().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let config = ConfigBuilder::new()
        .with_project_path(&project_path)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .build()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let agent = config
        .resolve_agent(&path)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(extract::Json(agent))
}
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

    String::from_utf8(decoded_path).map_err(|e| format!("Failed to decode path: {e}"))
}

pub async fn run_test(
    Path((pathb64, test_index)): Path<(String, usize)>,
) -> Result<impl IntoResponse, StatusCode> {
    let path = match decode_path_from_base64(pathb64) {
        Ok(path) => path,
        Err(error) => return Ok(Sse::new(create_error_stream(error))),
    };

    let project_path = match resolve_project_path() {
        Ok(path) => path.to_string_lossy().to_string(),
        Err(e) => {
            let error = format!("Failed to find project path: {e}");
            return Ok(Sse::new(create_error_stream(error)));
        }
    };

    let test_stream = match run_agent_test(project_path, path, test_index).await {
        Ok(stream) => stream,
        Err(e) => {
            let error = format!("Failed to run agent test: {e}");
            return Ok(Sse::new(create_error_stream(error)));
        }
    };

    Ok(Sse::new(Box::pin(create_sse_stream_from_stream(Box::pin(
        test_stream,
    )))))
}

#[derive(Deserialize, ToSchema)]
pub struct AskAgentRequest {
    pub question: String,
}

#[derive(Serialize, ToSchema)]
pub struct AskAgentResponse {
    pub content: String,
    pub references: Vec<ReferenceKind>,
    pub usage: Option<Usage>,
    pub artifacts: Vec<ArtifactInfo>,
}

#[derive(Serialize, ToSchema)]
pub struct ArtifactInfo {
    pub id: String,
    pub title: String,
    pub kind: crate::execute::types::event::ArtifactKind,
    pub is_verified: bool,
}

#[utoipa::path(
    method(post),
    path = "/agents/{pathb64}/ask",
    params(
        ("pathb64" = String, Path, description = "Base64 encoded path to the agent")
    ),
    request_body = AskAgentRequest,
    responses(
        (status = OK, description = "Success", body = AnswerStream, content_type = "text/event-stream")
    )
)]
pub async fn ask_agent_preview(
    Path(pathb64): Path<String>,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    extract::Json(payload): extract::Json<AskAgentRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let decoded_path = BASE64_STANDARD.decode(pathb64).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?;

    let path = String::from_utf8(decoded_path).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?;

    let project_path = resolve_project_path().map_err(|e| {
        tracing::error!("Failed to find project path: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let (tx, rx) = tokio::sync::mpsc::channel(100);

    let _ = tokio::spawn(async move {
        let tx_clone = tx.clone();
        let block_handler = BlockHandler::new(tx);
        let block_handler_reader = block_handler.get_reader();
        let result = run_agent(
            &project_path,
            &PathBuf::from(path),
            payload.question,
            block_handler,
            vec![],
        )
        .await;

        if let Err(err) = result {
            tracing::error!("Error running agent: {}", err);

            let error_message = match block_handler_reader.into_active_models().await {
                Ok((answer_message, _artifacts)) => {
                    let existing_content = match &answer_message.content {
                        ActiveValue::Set(val) => val.clone(),
                        _ => String::new(),
                    };
                    format!("{existing_content}\n 🔴 Error: {err}")
                }
                Err(e) => {
                    tracing::error!("Error reading block handler models: {}", e);
                    format!("🔴 Error: {err}")
                }
            };

            let error_stream = AnswerStream {
                content: AnswerContent::Error {
                    message: error_message,
                },
                references: vec![],
                is_error: true,
                step: String::new(),
            };

            let _ = tx_clone.send(error_stream).await;
        }
    });

    let stream = create_sse_stream(rx);

    Ok(Sse::new(stream))
}

#[utoipa::path(
    method(post),
    path = "/agents/{pathb64}/ask-sync",
    params(
        ("pathb64" = String, Path, description = "Base64 encoded path to the agent")
    ),
    request_body = AskAgentRequest,
    responses(
        (status = OK, description = "Success", body = AskAgentResponse, content_type = "application/json")
    )
)]
pub async fn ask_agent_preview_sync(
    Path(pathb64): Path<String>,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    extract::Json(payload): extract::Json<AskAgentRequest>,
) -> Result<extract::Json<AskAgentResponse>, StatusCode> {
    let decoded_path = BASE64_STANDARD.decode(pathb64).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?;

    let path = String::from_utf8(decoded_path).map_err(|e| {
        tracing::info!("{:?}", e);
        StatusCode::BAD_REQUEST
    })?;

    let project_path = resolve_project_path().map_err(|e| {
        tracing::error!("Failed to find project path: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let (tx, mut rx) = tokio::sync::mpsc::channel(100);

    let task_handle = tokio::spawn(async move {
        let block_handler = BlockHandler::new(tx);
        let block_handler_reader = block_handler.get_reader();
        let result = run_agent(
            &project_path,
            &PathBuf::from(path),
            payload.question,
            block_handler,
            vec![],
        )
        .await;

        if let Err(err) = result {
            tracing::error!("Error running agent: {}", err);

            let error_message = match block_handler_reader.into_active_models().await {
                Ok((answer_message, _artifacts)) => {
                    let existing_content = match &answer_message.content {
                        ActiveValue::Set(val) => val.clone(),
                        _ => String::new(),
                    };
                    format!("{existing_content}\n 🔴 Error: {err}")
                }
                Err(e) => {
                    tracing::error!("Error reading block handler models: {}", e);
                    format!("🔴 Error: {err}")
                }
            };

            return Err(error_message);
        }

        Ok(())
    });

    // Collect all streaming responses
    let mut content = String::new();
    let mut references = Vec::new();
    let mut usage = None;
    let mut artifacts = Vec::new();
    let mut artifact_map = std::collections::HashMap::new();

    while let Some(stream_item) = rx.recv().await {
        match stream_item.content {
            AnswerContent::Text {
                content: text_content,
            } => {
                content.push_str(&text_content);
            }
            AnswerContent::Error { .. } => {
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
            AnswerContent::Usage {
                usage: stream_usage,
            } => {
                usage = Some(stream_usage);
            }
            AnswerContent::ArtifactStarted {
                id,
                title,
                kind,
                is_verified,
            } => {
                let artifact_info = ArtifactInfo {
                    id: id.clone(),
                    title,
                    kind,
                    is_verified,
                };
                artifact_map.insert(id, artifact_info);
            }
            AnswerContent::ArtifactDone { id } => {
                if let Some(artifact) = artifact_map.remove(&id) {
                    artifacts.push(artifact);
                }
            }
            _ => {
                // Handle other content types as needed
            }
        }

        // Collect references from this stream item
        references.extend(stream_item.references);
    }

    // Wait for the task to complete and handle any errors
    match task_handle.await {
        Ok(Ok(())) => {
            // Task completed successfully
        }
        Ok(Err(error_message)) => {
            return Ok(extract::Json(AskAgentResponse {
                content: error_message,
                references: vec![],
                usage: None,
                artifacts: vec![],
            }));
        }
        Err(e) => {
            tracing::error!("Task failed: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    Ok(extract::Json(AskAgentResponse {
        content,
        references,
        usage,
        artifacts,
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct AskThreadRequest {
    pub question: Option<String>,
}

impl ChatExecutionRequest for AskThreadRequest {
    fn get_question(&self) -> Option<String> {
        self.question.clone()
    }
}

struct AgentExecutor;

#[async_trait]
impl ChatHandler for AgentExecutor {
    async fn execute(
        &self,
        context: ChatExecutionContext,
        tx: tokio::sync::mpsc::Sender<AnswerStream>,
    ) -> Result<(String, Usage), OxyError> {
        let thread = context.thread.clone();
        let agent_path = PathBuf::from(thread.source);
        let connection = context.streaming_persister.get_connection();

        let block_handler = BlockHandler::new(tx.clone())
            .with_streaming_persister(context.streaming_persister.clone())
            .with_logs_persister(context.logs_persister.clone());
        let block_handler_reader = block_handler.get_reader();

        let result = run_agent(
            &context.project_path,
            &agent_path,
            context.user_question.clone(),
            block_handler,
            context.memory.clone(),
        )
        .await;

        match result {
            Ok(_output_container) => {
                let (answer_message, artifacts) = block_handler_reader.into_active_models().await?;

                let content = answer_message.content.clone().take().unwrap_or_default();
                let input_tokens = answer_message
                    .input_tokens
                    .clone()
                    .take()
                    .unwrap_or_default();
                let output_tokens = answer_message
                    .output_tokens
                    .clone()
                    .take()
                    .unwrap_or_default();

                for mut artifact in artifacts {
                    artifact.thread_id = ActiveValue::Set(thread.id);
                    artifact.message_id =
                        ActiveValue::Set(context.streaming_persister.get_message_id());

                    artifact
                        .insert(connection)
                        .await
                        .map_err(|e| OxyError::from(anyhow::Error::from(e)))?;
                }

                Ok((
                    content,
                    Usage {
                        input_tokens,
                        output_tokens,
                    },
                ))
            }
            Err(err) => Err(OxyError::RuntimeError(err.to_string())),
        }
    }
}

pub async fn ask_agent(
    Path(id): Path<String>,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    extract::Json(payload): extract::Json<AskThreadRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let execution_manager = ChatService::new().await?;
    let executor = AgentExecutor;

    execution_manager
        .execute_request(id, payload, executor, user.id)
        .await
}

pub async fn ask_agent_sync(
    Path(id): Path<String>,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    extract::Json(payload): extract::Json<AskThreadRequest>,
) -> Result<extract::Json<AskAgentResponse>, StatusCode> {
    let execution_manager = ChatService::new().await?;
    let executor = AgentExecutor;

    // Execute the same logic as the streaming version but collect all responses
    let result = execution_manager
        .execute_request_sync(id, payload, executor, user.id)
        .await?;

    Ok(extract::Json(result))
}
