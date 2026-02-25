//! Streaming message handling logic for A2A message/stream method.
//!
//! This module contains the implementation of streaming message handling,
//! which converts A2A messages to Oxy format, executes the agent via
//! ChatService with streaming support, and returns an SSE stream of artifacts.

use a2a::{
    error::A2aError,
    server::SseStream,
    storage::TaskStorage,
    streaming::SseEventType,
    types::{
        Artifact, ArtifactUpdateKind, DataPart, Message, Part, StatusUpdateKind, Task,
        TaskArtifactUpdateEvent, TaskState, TaskStatus, TaskStatusUpdateEvent, TextPart,
    },
};
use chrono::Utc;
use futures::stream::StreamExt;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    integrations::a2a::storage::OxyTaskStorage,
    server::service::agent::{Message as AgentMessage, run_agent, run_agentic_workflow},
};
use oxy::{
    adapters::project::manager::ProjectManager,
    execute::{
        types::{Event, EventKind},
        writer::EventHandler,
    },
};
use oxy_shared::errors::OxyError;

/// Shared state for streaming persistence.
struct StreamingState {
    /// Collection of artifacts created during streaming
    artifacts: Vec<Artifact>,
}

/// Handle a streaming message/stream request via ChatService.
///
/// This function:
/// 1. Converts the A2A message to ChatService request format
/// 2. Creates a task with unique ID
/// 3. Sends task.created event
/// 4. Executes the agent via ChatService streaming endpoint
/// 5. Converts ChatService SSE events to A2A format in real-time
/// 6. Sends task.completed or task.failed event at the end
/// 7. Returns the SSE stream
///
/// # Arguments
///
/// * `agent_name` - The name of the agent to execute
/// * `agent_ref` - The path to the agent configuration file
/// * `message` - The A2A message to send
/// * `project_manager` - Project manager for agent execution
///
/// # Returns
///
/// An SSE stream of StreamEvents (task.created, artifact.update, task.completed/failed).
pub async fn handle_send_streaming_message(
    agent_name: &str,
    agent_ref: String,
    message: Message,
    project_manager: &ProjectManager,
    storage: Arc<OxyTaskStorage>,
    metadata: Option<HashMap<String, Value>>,
) -> Result<SseStream, A2aError> {
    tracing::info!(
        "Handling streaming message for agent '{}' via ChatService",
        agent_name
    );

    // Generate task ID and context upfront
    let task_id = Uuid::new_v4().to_string();

    // Use provided context_id or generate a new one per A2A Message spec
    let context_id = message
        .context_id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    // Generate or extract thread ID for conversation continuity
    let thread_id = context_id.clone();

    tracing::debug!(
        "A2A streaming context for agent '{}': task_id={}, thread_id={}, context_id={}",
        agent_name,
        task_id,
        thread_id,
        context_id
    );

    // Fetch prior history for this context before persisting the new task so we don't
    // re-inject the inbound message into agent memory.
    let history_messages = storage.get_messages_by_context_id(&context_id).await?;

    // Tag inbound message with task/context for persistence
    let mut inbound_message = message.clone();
    inbound_message.task_id = Some(task_id.clone());
    inbound_message.context_id = Some(context_id.clone());
    if let Some(meta) = metadata.clone() {
        let mut merged = inbound_message.metadata.unwrap_or_default();
        merged.extend(meta);
        inbound_message.metadata = Some(merged);
    }

    // Create initial task and persist before streaming starts
    let task = Task {
        id: task_id.clone(),
        context_id: context_id.clone(),
        status: TaskStatus {
            state: TaskState::Working,
            message: None,
            timestamp: Some(Utc::now().to_rfc3339()),
        },
        history: Some(vec![inbound_message.clone()]),
        artifacts: None,
        metadata: metadata.clone(),
        kind: a2a::types::TaskKind::Task,
    };

    storage
        .create_task(task.clone())
        .await
        .map_err(|e| A2aError::ServerError(format!("Failed to persist initial task: {}", e)))?;

    // Create a channel for streaming events
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<a2a::streaming::SseEvent>();

    // Send task.created event (after persistence)
    let task_json = serde_json::to_string(&task)
        .map_err(|e| A2aError::ServerError(format!("Failed to serialize task: {}", e)))?;
    if let Err(e) = tx.send(a2a::streaming::SseEvent::with_type(
        SseEventType::TaskCreated,
        task_json,
    )) {
        tracing::warn!("Failed to send task.created SSE event: {}", e);
    }
    tracing::debug!(
        "Fetched {} history messages for context_id={}",
        history_messages.len(),
        context_id
    );

    // Convert history to AgentMessage
    let memory: Vec<AgentMessage> = history_messages
        .into_iter()
        .map(|m| {
            let content = super::super::mapper::a2a_message_to_prompt(&m).unwrap_or_default();
            AgentMessage {
                content,
                is_human: m.role == a2a::types::MessageRole::User,
                created_at: chrono::Utc::now().into(),
            }
        })
        .collect();

    // Create event handler
    let shared_state = Arc::new(Mutex::new(StreamingState {
        artifacts: Vec::new(),
    }));

    let event_handler = A2aSseEventHandler {
        tx: tx.clone(),
        task_id: task_id.clone(),
        context_id: context_id.clone(),
        state: shared_state.clone(),
        metadata: metadata.clone(),
    };

    // Spawn execution
    let project_manager = project_manager.clone();
    let prompt = super::super::mapper::a2a_message_to_prompt(&message)?;
    let task_id_clone = task_id.clone();
    let tx_clone = tx.clone();
    let agent_ref_clone = agent_ref.clone();
    let context_id_clone = context_id.clone();
    let storage_clone = storage.clone();
    let state_clone = shared_state.clone();
    let _inbound_message_clone = inbound_message.clone();

    tokio::spawn(async move {
        // Determine agent type and execute accordingly
        let is_agentic = project_manager
            .config_manager
            .resolve_agentic_workflow(&agent_ref_clone)
            .await
            .is_ok();

        let result = if is_agentic {
            run_agentic_workflow(
                project_manager,
                agent_ref_clone,
                prompt,
                event_handler,
                memory,
            )
            .await
        } else {
            run_agent(
                project_manager,
                agent_ref_clone,
                prompt,
                event_handler,
                memory,
                None,
                None,
                None,
                None,
                Some(crate::service::agent::ExecutionSource::A2a {
                    task_id: task_id_clone.clone(),
                    context_id: context_id_clone.clone(),
                    thread_id: context_id_clone.clone(), // In A2A, thread_id is the context_id
                }),
                None,
                None, // No data_app_file_path from A2A streaming
            )
            .await
        };

        match result {
            Ok(_output) => {
                // Collect all artifacts accumulated during streaming
                let final_artifacts = {
                    let state = state_clone.lock().await;
                    state.artifacts.clone()
                };

                // Build final agent message from all artifact parts
                let all_parts: Vec<Part> = final_artifacts
                    .iter()
                    .flat_map(|a| a.parts.clone())
                    .collect();

                let parts_for_message = if all_parts.is_empty() {
                    vec![Part::Text(TextPart::new(String::new()))]
                } else {
                    all_parts
                };

                let mut final_message = Message::new_agent(parts_for_message);
                final_message.task_id = Some(task_id_clone.clone());
                final_message.context_id = Some(context_id_clone.clone());
                if let Some(meta) = metadata.clone() {
                    let mut merged = final_message.metadata.unwrap_or_default();
                    merged.extend(meta.clone());
                    final_message.metadata = Some(merged);
                }

                // Persist final status with all artifacts and agent message
                let final_task = Task {
                    id: task_id_clone.clone(),
                    context_id: context_id_clone.clone(),
                    status: TaskStatus {
                        state: TaskState::Completed,
                        message: Some(final_message.clone()),
                        timestamp: Some(Utc::now().to_rfc3339()),
                    },
                    history: Some(vec![final_message.clone()]),
                    artifacts: if final_artifacts.is_empty() {
                        None
                    } else {
                        Some(final_artifacts)
                    },
                    metadata: metadata.clone(),
                    kind: a2a::types::TaskKind::Task,
                };

                if let Err(err) = storage_clone.update_task(final_task).await {
                    tracing::error!("Failed to persist completed streaming task: {}", err);
                }

                // Send status-update event (final=true) per A2A spec
                let status_event = TaskStatusUpdateEvent {
                    task_id: task_id_clone.clone(),
                    context_id: context_id_clone.clone(),
                    kind: StatusUpdateKind::StatusUpdate,
                    status: TaskStatus {
                        state: TaskState::Completed,
                        message: Some(final_message),
                        timestamp: Some(Utc::now().to_rfc3339()),
                    },
                    is_final: true,
                    metadata: metadata.clone(),
                };

                if let Err(e) = tx_clone.send(a2a::streaming::SseEvent::with_type(
                    SseEventType::TaskStatusUpdate,
                    serde_json::to_string(&status_event).unwrap_or_else(|_| "{}".to_string()),
                )) {
                    tracing::warn!("Failed to send task.completed SSE event: {}", e);
                }
            }
            Err(e) => {
                // Persist failure with error message
                let mut error_message =
                    Message::new_agent(vec![Part::Text(TextPart::new(e.to_string()))]);
                error_message.task_id = Some(task_id_clone.clone());
                error_message.context_id = Some(context_id_clone.clone());
                if let Some(meta) = metadata.clone() {
                    let mut merged = error_message.metadata.unwrap_or_default();
                    merged.extend(meta.clone());
                    error_message.metadata = Some(merged);
                }

                let failed_task = Task {
                    id: task_id_clone.clone(),
                    context_id: context_id_clone.clone(),
                    status: TaskStatus {
                        state: TaskState::Failed,
                        message: Some(error_message.clone()),
                        timestamp: Some(Utc::now().to_rfc3339()),
                    },
                    history: Some(vec![error_message.clone()]),
                    artifacts: None,
                    metadata: metadata.clone(),
                    kind: a2a::types::TaskKind::Task,
                };

                if let Err(err) = storage_clone.update_task(failed_task).await {
                    tracing::error!("Failed to persist failed streaming task: {}", err);
                }

                // Send failure status-update event (final=true) per A2A spec
                let mut status_message =
                    Message::new_agent(vec![Part::Text(TextPart::new(e.to_string()))]);
                status_message.task_id = Some(task_id_clone.clone());
                status_message.context_id = Some(context_id_clone.clone());

                let status_event = TaskStatusUpdateEvent {
                    task_id: task_id_clone,
                    context_id: context_id_clone,
                    kind: StatusUpdateKind::StatusUpdate,
                    status: TaskStatus {
                        state: TaskState::Failed,
                        message: Some(status_message),
                        timestamp: Some(Utc::now().to_rfc3339()),
                    },
                    is_final: true,
                    metadata: metadata.clone(),
                };

                if let Err(e) = tx_clone.send(a2a::streaming::SseEvent::with_type(
                    SseEventType::TaskStatusUpdate,
                    serde_json::to_string(&status_event).unwrap_or_else(|_| "{}".to_string()),
                )) {
                    tracing::warn!("Failed to send task.failed SSE event: {}", e);
                }
            }
        }
    });

    // Convert channel receiver to stream and wrap in Result
    let stream = tokio_stream::wrappers::UnboundedReceiverStream::new(rx).map(Ok::<_, A2aError>);

    Ok(Box::pin(stream))
}

struct A2aSseEventHandler {
    tx: tokio::sync::mpsc::UnboundedSender<a2a::streaming::SseEvent>,
    task_id: String,
    context_id: String,
    state: Arc<Mutex<StreamingState>>,
    metadata: Option<HashMap<String, Value>>,
}

#[async_trait::async_trait]
impl EventHandler for A2aSseEventHandler {
    async fn handle_event(&mut self, event: Event) -> Result<(), OxyError> {
        tracing::debug!("Handling event: {:?}", event.kind);
        match event.kind {
            EventKind::ArtifactStarted {
                kind: _,
                title,
                is_verified: _,
            } => {
                tracing::debug!(
                    "Processing ArtifactStarted: id={}, title={}",
                    event.source.id,
                    title
                );

                // Create new A2A Artifact with ID from event.source.id
                let artifact_id = event.source.id.clone();

                // Check if artifact already exists, if so skip
                {
                    let state = self.state.lock().await;
                    if state.artifacts.iter().any(|a| a.artifact_id == artifact_id) {
                        tracing::debug!(
                            "Artifact with id={} already exists, skipping",
                            artifact_id
                        );
                        return Ok(());
                    }
                }

                let mut artifact = Artifact::new(vec![])
                    .with_name(&title)
                    .with_description(&title);
                artifact.artifact_id = artifact_id.clone();
                artifact.metadata = self.metadata.clone();

                // Store artifact in state
                {
                    let mut state = self.state.lock().await;
                    state.artifacts.push(artifact.clone());
                }

                // Send TaskArtifactUpdateEvent SSE event with initial empty parts
                let event_data = TaskArtifactUpdateEvent {
                    task_id: self.task_id.clone(),
                    context_id: self.context_id.clone(),
                    kind: ArtifactUpdateKind::ArtifactUpdate,
                    artifact,
                    append: Some(false),
                    last_chunk: None,
                    metadata: self.metadata.clone(),
                };

                if let Err(e) = self.tx.send(a2a::streaming::SseEvent::with_type(
                    SseEventType::ArtifactUpdate,
                    serde_json::to_string(&event_data).unwrap_or_default(),
                )) {
                    tracing::warn!("Failed to send artifact.started SSE event: {}", e);
                }
            }
            EventKind::Updated { chunk } => {
                tracing::debug!("Processing updated event chunk: {:?}", chunk.delta);
                // Convert the output to A2A parts for streaming text
                let new_parts =
                    super::super::mapper::oxy_output_to_parts(&chunk.delta).map_err(|e| {
                        OxyError::RuntimeError(format!("Failed to convert output to parts: {}", e))
                    })?;

                // Append to the corresponding A2A Artifact's parts (based on event.source.id)
                let artifact_id = event.source.id.clone();
                let mut new_artifact = false;

                let updated_artifact = {
                    let mut state = self.state.lock().await;

                    // Find artifact by source ID and append new parts
                    if let Some(existing) = state
                        .artifacts
                        .iter_mut()
                        .find(|a| a.artifact_id == artifact_id)
                    {
                        existing.parts.extend(new_parts.clone());
                        existing.clone()
                    } else {
                        // If no artifact found with this ID, create a fallback text artifact
                        let mut artifact = Artifact::new(new_parts.clone())
                            .with_description("Agent text response");
                        artifact.artifact_id = artifact_id.clone();
                        artifact.metadata = self.metadata.clone();
                        state.artifacts.push(artifact.clone());
                        new_artifact = true;
                        artifact
                    }
                };

                // Send updated TaskArtifactUpdateEvent SSE event with new parts
                let event_data = TaskArtifactUpdateEvent {
                    task_id: self.task_id.clone(),
                    context_id: self.context_id.clone(),
                    kind: ArtifactUpdateKind::ArtifactUpdate,
                    artifact: Artifact {
                        artifact_id,
                        name: updated_artifact.name,
                        description: updated_artifact.description,
                        parts: new_parts.clone(), // Only send new parts in the event
                        metadata: updated_artifact.metadata.clone(),
                        extensions: updated_artifact.extensions.clone(),
                    },
                    append: Some(!new_artifact),
                    last_chunk: None,
                    metadata: self.metadata.clone(),
                };

                if let Err(e) = self.tx.send(a2a::streaming::SseEvent::with_type(
                    SseEventType::ArtifactUpdate,
                    serde_json::to_string(&event_data).unwrap_or_default(),
                )) {
                    tracing::warn!("Failed to send artifact.update SSE event: {}", e);
                }
            }
            EventKind::ArtifactFinished { error } => {
                tracing::debug!(
                    "Processing ArtifactFinished: id={}, error={:?}",
                    event.source.id,
                    error
                );

                let artifact_id = event.source.id.clone();

                // Mark A2A Artifact as complete
                let final_artifact = {
                    let state = self.state.lock().await;
                    state
                        .artifacts
                        .iter()
                        .find(|a| a.artifact_id == artifact_id)
                        .cloned()
                };

                if let Some(mut artifact) = final_artifact {
                    // Add error to metadata if present
                    if let Some(err_msg) = error {
                        let mut meta = artifact.metadata.unwrap_or_default();
                        meta.insert("error".to_string(), serde_json::Value::String(err_msg));
                        artifact.metadata = Some(meta);
                    }

                    // Send final TaskArtifactUpdateEvent SSE event with all parts
                    let mut artifact_without_parts = artifact.clone();
                    artifact_without_parts.parts = vec![];
                    let event_data = TaskArtifactUpdateEvent {
                        task_id: self.task_id.clone(),
                        context_id: self.context_id.clone(),
                        kind: ArtifactUpdateKind::ArtifactUpdate,
                        artifact: artifact_without_parts,
                        append: Some(false),
                        last_chunk: Some(true),
                        metadata: self.metadata.clone(),
                    };

                    if let Err(e) = self.tx.send(a2a::streaming::SseEvent::with_type(
                        SseEventType::ArtifactUpdate,
                        serde_json::to_string(&event_data).unwrap_or_default(),
                    )) {
                        tracing::warn!("Failed to send artifact.finished SSE event: {}", e);
                    }
                }
            }
            EventKind::Error { message } => {
                // Don't send task.failed for errors since retries may still succeed
                tracing::debug!("Error event received: {}", message);
            }
            EventKind::SQLQueryGenerated {
                query,
                database: _,
                source: _,
                is_verified: _,
            } => {
                // Convert SQL query to A2A TextPart - create separate artifact
                let artifact_id = Uuid::new_v4().to_string();
                let parts = vec![Part::Text(TextPart::new(format!("```sql\n{}\n```", query)))];

                let mut artifact = Artifact::new(parts.clone()).with_description("SQL query");
                artifact.artifact_id = artifact_id.clone();
                artifact.metadata = self.metadata.clone();

                {
                    let mut state = self.state.lock().await;
                    state.artifacts.push(artifact.clone());
                }

                let event_data = TaskArtifactUpdateEvent {
                    task_id: self.task_id.clone(),
                    context_id: self.context_id.clone(),
                    kind: ArtifactUpdateKind::ArtifactUpdate,
                    artifact,
                    append: Some(false),
                    last_chunk: None,
                    metadata: self.metadata.clone(),
                };

                if let Err(e) = self.tx.send(a2a::streaming::SseEvent::with_type(
                    SseEventType::ArtifactUpdate,
                    serde_json::to_string(&event_data).unwrap_or_default(),
                )) {
                    tracing::warn!("Failed to send SQL query SSE event: {}", e);
                }
            }
            EventKind::OmniQueryGenerated {
                query,
                is_verified: _,
            } => {
                // Convert OmniQuery to A2A DataPart - create separate artifact
                let json_value = serde_json::to_value(&query).map_err(|e| {
                    OxyError::RuntimeError(format!("Failed to serialize OmniQuery: {}", e))
                })?;

                let artifact_id = Uuid::new_v4().to_string();
                let parts = vec![Part::Data(DataPart::new(json_value))];

                let mut artifact = Artifact::new(parts.clone()).with_description("Omni query");
                artifact.artifact_id = artifact_id.clone();
                artifact.metadata = self.metadata.clone();

                {
                    let mut state = self.state.lock().await;
                    state.artifacts.push(artifact.clone());
                }

                let event_data = TaskArtifactUpdateEvent {
                    task_id: self.task_id.clone(),
                    context_id: self.context_id.clone(),
                    kind: ArtifactUpdateKind::ArtifactUpdate,
                    artifact,
                    append: Some(false),
                    last_chunk: None,
                    metadata: self.metadata.clone(),
                };

                if let Err(e) = self.tx.send(a2a::streaming::SseEvent::with_type(
                    SseEventType::ArtifactUpdate,
                    serde_json::to_string(&event_data).unwrap_or_default(),
                )) {
                    tracing::warn!("Failed to send omni query SSE event: {}", e);
                }
            }
            _ => {}
        }
        Ok(())
    }
}
