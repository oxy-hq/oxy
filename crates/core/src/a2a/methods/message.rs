//! Message handling logic for A2A message/send method.
//!
//! This module contains the implementation of synchronous message handling,
//! which converts A2A messages to Oxy format, executes the agent using
//! ChatService, and returns the completed task with artifacts.

use a2a::{
    error::A2aError,
    types::{Message, Task},
};
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::{Uuid, uuid};

use crate::{
    adapters::project::manager::ProjectManager,
    errors::OxyError,
    execute::{
        types::{Event, EventKind, Output, Usage},
        writer::EventHandler,
    },
    service::agent::{run_agent, run_agentic_workflow},
};

use super::super::chat_integration;

/// Handle a synchronous message/send request using ChatService.
///
/// This function:
/// 1. Converts the A2A message to ChatService request format
/// 2. Creates or retrieves a thread for conversation continuity
/// 3. Executes the agent via ChatService (which handles tool execution, artifacts, etc.)
/// 4. Converts the ChatService response to A2A Task with artifacts
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
/// A completed Task with artifacts from the agent execution.
pub async fn handle_send_message(
    agent_name: &str,
    agent_ref: String,
    message: Message,
    project_manager: &ProjectManager,
) -> Result<Task, A2aError> {
    tracing::info!(
        "Handling message for agent '{}' via ChatService",
        agent_name
    );

    // Generate task ID for tracking
    let task_id = Uuid::new_v4().to_string();

    // Use provided context_id or defer generation until needed
    let context_id = message
        .context_id
        .clone()
        .unwrap_or(Uuid::new_v4().to_string());

    // Convert A2A message to ChatService request format
    let chat_request = chat_integration::A2aMessageRequest::from_a2a_message(&message)?;

    // Execute via ChatService (synchronous execution)
    let response = execute_via_chat_service(chat_request, agent_ref, project_manager).await?;

    // Convert ChatService response to A2A Task
    let task = chat_integration::chat_response_to_task(response, &task_id, &context_id, &message)?;

    Ok(task)
}

/// Execute the agent via ChatService with proper thread and message management.
///
/// This creates a ChatService instance, creates/retrieves a thread, and executes
/// the agent synchronously, collecting all artifacts and tool results.
async fn execute_via_chat_service(
    chat_request: chat_integration::A2aMessageRequest,
    agent_ref: String,
    project_manager: &ProjectManager,
) -> Result<crate::api::agent::AskAgentResponse, A2aError> {
    // Build execution state for collecting streamed output
    let state = Arc::new(Mutex::new(AggregationState::default()));
    let handler = AggregatingEventHandler {
        state: state.clone(),
    };

    // Determine whether this is an agentic workflow
    let is_agentic = project_manager
        .config_manager
        .resolve_agentic_workflow(&agent_ref)
        .await
        .is_ok();

    // Execute the agent synchronously, letting the handler collect output
    let execution_result = if is_agentic {
        run_agentic_workflow(
            project_manager.clone(),
            agent_ref,
            chat_request.question,
            handler,
            vec![],
        )
        .await
    } else {
        run_agent(
            project_manager.clone(),
            agent_ref,
            chat_request.question,
            handler,
            vec![],
            chat_request.filters,
            chat_request.connections,
            chat_request.globals,
            None,
        )
        .await
    };

    // Extract collected output and usage
    let (mut content, usage, mut errored, mut error_message) = {
        let state = state.lock().await;
        (
            state.accumulated_text.clone(),
            state.usage.clone(),
            state.errored,
            state.error_message.clone(),
        )
    };

    if let Err(err) = execution_result {
        errored = true;
        if content.is_empty() {
            content = err.to_string();
        }
        error_message.get_or_insert_with(|| err.to_string());
    }

    Ok(crate::api::agent::AskAgentResponse {
        content,
        references: vec![],
        usage,
        artifacts: vec![],
        success: !errored,
        error_message,
    })
}

#[derive(Default)]
struct AggregationState {
    accumulated_text: String,
    usage: Option<Usage>,
    errored: bool,
    error_message: Option<String>,
}

struct AggregatingEventHandler {
    state: Arc<Mutex<AggregationState>>,
}

#[async_trait::async_trait]
impl EventHandler for AggregatingEventHandler {
    async fn handle_event(&mut self, event: Event) -> Result<(), OxyError> {
        match event.kind {
            EventKind::Updated { chunk } => {
                if let Output::Text(text) = chunk.delta {
                    let mut state = self.state.lock().await;
                    state.accumulated_text.push_str(&text);
                }
            }
            EventKind::Usage { usage } => {
                let mut state = self.state.lock().await;
                state.usage = Some(usage);
            }
            EventKind::Finished { message, error, .. } => {
                let mut state = self.state.lock().await;
                if !message.is_empty() {
                    state.accumulated_text.push_str(&message);
                }
                if let Some(err) = error {
                    state.errored = true;
                    state.error_message.get_or_insert(err);
                }
            }
            EventKind::Error { message } => {
                let mut state = self.state.lock().await;
                state.errored = true;
                state.error_message.get_or_insert(message);
            }
            _ => {}
        }

        Ok(())
    }
}
