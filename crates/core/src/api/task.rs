use crate::{
    adapters::checkpoint::types::RetryStrategy,
    api::middlewares::project::ProjectManagerExtractor,
    dispatcher::run::Dispatcher,
    service::{
        agent::{AgenticRunner, run_agentic_workflow},
        statics::BROADCASTER,
        types::run::RunInfo,
    },
    utils::create_sse_broadcast,
};
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use tokio::sync::Mutex as TokioMutex;
use uuid::Uuid;

use crate::{
    adapters::project::manager::ProjectManager,
    auth::extractor::AuthenticatedUserExtractor,
    errors::OxyError,
    execute::{
        types::{DataAppReference, Event, EventKind, Output, ReferenceKind, Usage},
        writer::EventHandler,
    },
    service::{
        agent::run_agent,
        chat::{ChatExecutionContext, ChatExecutionRequest, ChatHandler, ChatService},
        formatters::streaming_message_persister::StreamingMessagePersister,
        types::{AnswerContent, AnswerStream},
    },
};
use axum::{
    extract::{self, Path, Query},
    http::StatusCode,
    response::IntoResponse,
};
use sea_orm::{ActiveModelTrait, ActiveValue};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;

#[derive(Deserialize)]
pub struct AskTaskRequest {
    pub question: Option<String>,
}

impl ChatExecutionRequest for AskTaskRequest {
    fn get_question(&self) -> Option<String> {
        self.question.clone()
    }
}

struct TaskStream {
    references: Arc<Mutex<Vec<ReferenceKind>>>,
    tx: Sender<AnswerStream>,
    usage: Arc<TokioMutex<Usage>>,
    streaming_message_persister: Arc<StreamingMessagePersister>,
}

impl TaskStream {
    fn new(
        tx: Sender<AnswerStream>,
        streaming_message_persister: Arc<StreamingMessagePersister>,
    ) -> Self {
        TaskStream {
            tx,
            references: Arc::new(Mutex::new(vec![])),
            usage: Arc::new(TokioMutex::new(Usage::new(0, 0))),
            streaming_message_persister,
        }
    }

    async fn update_usage(&self, usage: Usage) -> Result<(), OxyError> {
        let mut usage_lock = self.usage.lock().await;
        usage_lock.input_tokens += usage.input_tokens;
        usage_lock.output_tokens += usage.output_tokens;
        Ok(())
    }
}

#[async_trait::async_trait]
impl EventHandler for TaskStream {
    async fn handle_event(&mut self, event: Event) -> Result<(), OxyError> {
        if let EventKind::Usage { usage } = &event.kind {
            self.update_usage(usage.clone()).await?;
            let message = AnswerStream {
                content: AnswerContent::Usage {
                    usage: usage.clone(),
                },
                references: vec![],
                is_error: false,
                step: event.source.kind.to_string(),
            };
            let _ = self.tx.send(message).await.map_err(|_| ());
        }

        if let EventKind::Updated { chunk } = &event.kind {
            match &chunk.delta {
                Output::Prompt(_) => {
                    let message = AnswerStream {
                        content: AnswerContent::Text {
                            content: "".to_string(),
                        },
                        references: vec![],
                        is_error: false,
                        step: event.source.kind.to_string(),
                    };
                    let _ = self.tx.send(message).await.map_err(|_| ());
                }
                Output::Text(text) => {
                    let content = text.clone();
                    let message = AnswerStream {
                        content: AnswerContent::Text {
                            content: text.to_owned(),
                        },
                        references: vec![],
                        is_error: false,
                        step: event.source.kind.to_string(),
                    };
                    let _ = self.tx.send(message).await.map_err(|_| ());
                    self.streaming_message_persister
                        .append_content(&content)
                        .await?;
                }
                Output::Table(table) => {
                    let reference = table.clone().into_reference();
                    if let Some(r) = reference {
                        self.references.lock().unwrap().push(r);
                        let message = AnswerStream {
                            content: AnswerContent::Text {
                                content: "".to_string(),
                            },
                            references: vec![],
                            is_error: false,
                            step: event.source.kind.to_string(),
                        };
                        let _ = self.tx.send(message).await.map_err(|_| ());
                    }
                }
                _ => {}
            }
        }
        if let EventKind::DataAppCreated { data_app } = &event.kind {
            self.references
                .lock()
                .unwrap()
                .push(ReferenceKind::DataApp(DataAppReference {
                    file_path: data_app.file_path.clone(),
                }));
            let message = AnswerStream {
                content: AnswerContent::DataApp {
                    file_path: data_app.file_path.to_string_lossy().to_string(),
                },
                references: vec![],
                is_error: false,
                step: event.source.kind.to_string(),
            };
            let _ = self.tx.send(message).await.map_err(|_| ());
        }
        Ok(())
    }
}

struct TaskExecutor {
    project_manager: ProjectManager,
}

#[async_trait]
impl ChatHandler for TaskExecutor {
    async fn execute(
        &self,
        context: ChatExecutionContext,
        tx: tokio::sync::mpsc::Sender<AnswerStream>,
    ) -> Result<(String, Usage), OxyError> {
        let connection = context.streaming_persister.get_connection();
        let thread = context.thread.clone();

        let project_manager = self.project_manager.clone();

        let config_manager = project_manager.config_manager.clone();

        let agent_ref = config_manager.get_builder_agent_path().await.map_err(|e| {
            OxyError::RuntimeError(format!("Failed to get builder agent path: {e}"))
        })?;

        let task_stream = TaskStream::new(tx.clone(), context.streaming_persister.clone());
        let references = task_stream.references.clone();
        let usage_arc = task_stream.usage.clone();

        let result = match agent_ref.to_string_lossy().ends_with(".aw.yml") {
            true => {
                run_agentic_workflow(
                    project_manager,
                    &agent_ref,
                    context.user_question.clone(),
                    task_stream,
                    context.memory.clone(),
                )
                .await
            }
            false => {
                run_agent(
                    project_manager,
                    &agent_ref,
                    context.user_question.clone(),
                    task_stream,
                    context.memory.clone(),
                    context.filters.clone(),
                    context.connections.clone(),
                    None, // No globals from task
                    None, // TODO: Support variables from task context
                )
                .await
            }
        };

        match result {
            Ok(output_container) => {
                let output_string = output_container.to_string();
                let references = Arc::try_unwrap(references)
                    .map_err(|_| {
                        OxyError::RuntimeError("Failed to unwrap task references".to_string())
                    })?
                    .into_inner()
                    .map_err(|_| {
                        OxyError::RuntimeError("Failed to lock task references".to_string())
                    })?;

                let mut thread_model: entity::threads::ActiveModel = thread.clone().into();
                for r in references {
                    if let ReferenceKind::DataApp(data_app) = r {
                        let file_path = data_app.file_path.to_string_lossy().to_string();
                        thread_model.source = ActiveValue::Set(file_path.clone());
                    }
                }
                thread_model.output = ActiveValue::Set(output_container.to_string());
                thread_model
                    .update(connection)
                    .await
                    .map_err(|err| OxyError::DBError(format!("Update thread:\n{err}")))?;

                let usage = usage_arc.lock().await.clone();
                Ok((output_string, usage))
            }
            Err(err) => Err(OxyError::RuntimeError(err.to_string())),
        }
    }
}

pub async fn ask_task(
    Path((project_id, id)): Path<(Uuid, String)>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    extract::Json(payload): extract::Json<AskTaskRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let execution_manager = ChatService::new().await?;
    let executor = TaskExecutor { project_manager };

    execution_manager
        .execute_request(id, payload, executor, user.id, project_id)
        .await
}

#[derive(Deserialize)]
pub struct AskAgenticRequest {
    pub agent_ref: String,
    pub question: String,
}

#[derive(Deserialize, Serialize)]
pub struct AskAgenticResponse {
    pub run_info: RunInfo,
    pub message_id: Uuid,
}

pub async fn ask_agentic(
    Path((_project_id, id)): Path<(Uuid, String)>,
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    extract::Json(payload): extract::Json<AskAgenticRequest>,
) -> Result<extract::Json<AskAgenticResponse>, StatusCode> {
    let chat_service = ChatService::new().await?;
    let thread_id = chat_service.parse_thread_id(&id)?;
    chat_service
        .new_user_question(thread_id.clone(), &payload.question)
        .await?;
    let memory = chat_service
        .build_conversation_memory(thread_id.clone())
        .await?;
    let config_manager = project_manager.config_manager.clone();
    let agent_ref = &payload.agent_ref;
    let source_id = agent_ref.to_string();
    let message_id = chat_service.new_agentic_message(thread_id).await?;
    let lookup_id = message_id.clone();

    let run_info = Dispatcher::new(project_manager)
        .dispatch(
            source_id.to_string(),
            RetryStrategy::NoRetry { variables: None },
            AgenticRunner::new(message_id.to_string(), payload.question.clone(), memory),
            Some(lookup_id),
        )
        .await
        .map_err(|err| {
            tracing::error!("Failed to dispatch agentic task: {}", err);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(extract::Json(AskAgenticResponse {
        run_info,
        message_id,
    }))
}

#[derive(Deserialize)]
pub struct AgenticEventsRequest {
    pub lookup_id: String,
}

pub async fn agentic_events(
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Query(request): Query<AgenticEventsRequest>,
) -> Result<impl axum::response::IntoResponse, StatusCode> {
    let runs_manager = project_manager.runs_manager.as_ref().ok_or_else(|| {
        tracing::error!("Failed to initialize RunsManager");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let run_details = runs_manager
        .lookup(&request.lookup_id)
        .await
        .map_err(|e| {
            tracing::error!(
                "Failed to lookup run by lookup_id {}: {:?}",
                request.lookup_id,
                e
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            tracing::warn!("No run found for lookup_id: {}", request.lookup_id);
            StatusCode::NOT_FOUND
        })?;
    let run_info = run_details.run_info;
    let run_info = match run_info.root_ref {
        Some(root_ref) => runs_manager
            .find_run(&root_ref.source_id, root_ref.run_index)
            .await
            .map_err(|e| {
                tracing::error!("Failed to find root run: {:?}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?
            .ok_or(StatusCode::NOT_FOUND)?,
        None => run_info,
    };

    let run_id = run_info.task_id().map_err(|_| StatusCode::BAD_REQUEST)?;
    tracing::info!("Subscribing to events for run ID: {}", run_id);
    let subscribed = BROADCASTER.subscribe(&run_id).await.map_err(|err| {
        tracing::error!("Failed to subscribe to topic {run_id}: {err}");
        Into::<StatusCode>::into(err)
    })?;
    tracing::info!("Subscribed to events for run ID: {}", run_id);
    Ok(axum::response::sse::Sse::new(create_sse_broadcast(
        subscribed.items,
        subscribed.receiver,
    )))
}
