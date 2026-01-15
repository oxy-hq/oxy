use async_openai::types::chat::{
    ChatCompletionRequestAssistantMessage, ChatCompletionRequestAssistantMessageContent,
    ChatCompletionRequestMessage, ChatCompletionRequestUserMessage,
    ChatCompletionRequestUserMessageContent,
};
use oxy::types::event::EventKind as DispatchEventKind;
use oxy::{
    adapters::{project::manager::ProjectManager, runs::TopicRef, session_filters::SessionFilters},
    checkpoint::types::RetryStrategy,
    config::{
        ConfigManager,
        constants::{CONCURRENCY_SOURCE, CONSISTENCY_SOURCE, WORKFLOW_SOURCE},
        model::{AgentConfig, ConnectionOverrides},
    },
    dispatcher::run::Dispatch,
    execute::{
        types::{Event, EventKind, Output, OutputContainer, ProgressType},
        writer::{EventHandler, NoopHandler},
    },
    observability::events,
    theme::StyledText,
    utils::print_colored_sql,
};
use oxy_agent::{
    AgentLauncher,
    fsm::config::AgenticInput,
    types::{AgentInput, Message as AgentMessage},
};
use oxy_shared::errors::OxyError;
use sea_orm::prelude::DateTimeWithTimeZone;
use serde::Deserialize;
use std::{
    io::Write,
    path::{Path, PathBuf},
};
use utoipa::ToSchema;

use super::eval::PBarsHandler;

/// Represents the source/origin of an agent execution for observability tracing
#[derive(Debug, Clone, serde::Serialize)]
pub enum ExecutionSource {
    /// Executed from CLI (oxy command)
    Cli,
    /// Executed from Web API/Chat interface
    WebApi { thread_id: String, user_id: String },
    /// Executed from Slack integration
    Slack {
        thread_id: String,
        channel_id: Option<String>,
    },
    /// Executed from A2A (Agent-to-Agent) protocol
    A2a {
        task_id: String,
        context_id: String,
        thread_id: String,
    },
    /// Executed from MCP (Model Context Protocol)
    Mcp { session_id: Option<String> },
    /// Internal/programmatic execution (tests, etc)
    Internal,
}

#[derive(Deserialize, ToSchema)]
pub struct Memory {
    pub content: String,
    pub is_human: bool,
}

#[derive(Deserialize, ToSchema)]
pub struct AskRequest {
    pub question: String,
    pub agent: String,
    pub title: String,
    pub project_path: String,
    pub memory: Vec<Memory>,
}

pub async fn ask_adhoc(
    question: String,
    project: ProjectManager,
    agent: String,
) -> Result<String, OxyError> {
    let config_manager = project.config_manager.clone();

    let agent_path = get_path_by_name(config_manager, agent).await?;
    let result = match run_agent(
        project,
        &agent_path,
        question,
        NoopHandler,
        vec![],
        None,
        None,
        None,                            // No globals
        None,                            // No variables
        Some(ExecutionSource::Internal), // Internal/programmatic call
        None,                            // No sandbox_info
    )
    .await
    {
        Ok(output) => output.to_string(),
        Err(e) => format!("Error running agent: {e}"),
    };
    Ok(result)
}

pub async fn list_agents(config_manager: ConfigManager) -> Result<Vec<String>, OxyError> {
    let project_path = config_manager.project_path();
    let agents = config_manager.list_agents().await?;
    Ok(agents
        .iter()
        .map(|absolute_path| {
            absolute_path
                .strip_prefix(project_path)
                .unwrap()
                .to_string_lossy()
                .to_string()
        })
        .collect())
}

pub async fn get_agent_config(
    config_manager: ConfigManager,
    relative_path: String,
) -> Result<AgentConfig, OxyError> {
    let agent = config_manager.resolve_agent(relative_path).await?;
    Ok(agent)
}

pub async fn get_path_by_name(
    config_manager: ConfigManager,
    agent_name: String,
) -> Result<PathBuf, OxyError> {
    let agents = config_manager.list_agents().await?;
    for agent in agents {
        let agent_config = config_manager.resolve_agent(agent.clone()).await?;
        if agent_config.name == agent_name {
            let path = config_manager.resolve_file(agent).await?;
            return Ok(PathBuf::from(path));
        }
    }
    Err(OxyError::ArgumentError(format!(
        "Agent with name {agent_name} not found"
    )))
}

#[derive(Default)]
pub struct AgentCLIHandler {
    pbar_handler: PBarsHandler,
}

#[async_trait::async_trait]
impl EventHandler for AgentCLIHandler {
    async fn handle_event(&mut self, event: Event) -> Result<(), OxyError> {
        match event.source.kind.as_str() {
            WORKFLOW_SOURCE => match event.kind {
                EventKind::Started { name, .. } => {
                    println!("\n⏳Running workflow: {name}");
                }
                EventKind::Finished { message, .. } => {
                    println!("{message}");
                }
                _ => {}
            },
            CONSISTENCY_SOURCE => match event.kind {
                EventKind::Progress { progress } => match progress {
                    ProgressType::Started(total) => {
                        __self
                            .pbar_handler
                            .get_or_create_bar(&event.source.id, total);
                    }
                    ProgressType::Updated(progress) => {
                        __self.pbar_handler.update_bar(&event.source.id, progress)?;
                    }
                    ProgressType::Finished => {
                        __self.pbar_handler.remove_bar(&event.source.id);
                    }
                },
                EventKind::Message { message } => {
                    println!("{message}");
                }
                EventKind::Error { message } => {
                    println!("{}", message.error());
                }
                _ => {}
            },
            CONCURRENCY_SOURCE => {}
            _ => match event.kind {
                EventKind::Updated { chunk } => match chunk.delta.clone() {
                    Output::SQL(sql) => {
                        print_colored_sql(&sql.0);
                    }
                    Output::Table(table) => match table.to_term() {
                        Ok(table) => {
                            println!("{}", "\nResult:".primary());
                            println!("{table}");
                        }
                        Err(e) => {
                            println!("{}", format!("Error displaying results: {e}").error());
                        }
                    },
                    Output::Text(text) => {
                        if chunk.finished {
                            println!("{text}");
                        } else {
                            print!("{text}");
                            std::io::stdout().flush().unwrap();
                        }
                    }
                    _ => {}
                },
                EventKind::Message { message } => {
                    println!("{message}");
                }
                EventKind::Error { message } => {
                    println!("{}", message.error());
                }
                _ => {}
            },
        }
        Ok(())
    }
}

#[tracing::instrument(skip_all, err, fields(
        otel.name = events::agent::run_agent::NAME,
        oxy.span_type = events::agent::run_agent::TYPE,
        oxy.agent.ref = %agent_ref.as_ref().to_string_lossy().to_string(),
        oxy.execution.source = tracing::field::Empty,
        oxy.user.id = tracing::field::Empty,
        oxy.thread.id = tracing::field::Empty,
        oxy.task.id = tracing::field::Empty,
        oxy.context.id = tracing::field::Empty,
    ))]
pub async fn run_agent<P: AsRef<Path>, H: EventHandler + Send + 'static>(
    project: ProjectManager,
    agent_ref: P,
    prompt: String,
    event_handler: H,
    memory: Vec<Message>,
    filters: Option<SessionFilters>,
    connections: Option<ConnectionOverrides>,
    globals: Option<indexmap::IndexMap<String, serde_json::Value>>,
    variables: Option<std::collections::HashMap<String, serde_json::Value>>,
    source: Option<ExecutionSource>,
    sandbox_info: Option<oxy::execute::types::event::SandboxInfo>,
) -> Result<OutputContainer, OxyError> {
    let agent_ref_str = agent_ref.as_ref().to_string_lossy().to_string();
    let project_path_str = project.config_manager.project_path().display().to_string();

    // Record execution source context in the trace span
    let span = tracing::Span::current();
    if let Some(ref exec_source) = source {
        match exec_source {
            ExecutionSource::WebApi { thread_id, user_id } => {
                span.record("oxy.execution.source", "web_api");
                span.record("oxy.user.id", user_id.as_str());
                span.record("oxy.thread.id", thread_id.as_str());
            }
            ExecutionSource::Slack {
                thread_id,
                channel_id,
            } => {
                span.record("oxy.execution.source", "slack");
                span.record("oxy.thread.id", thread_id.as_str());
                if let Some(cid) = channel_id {
                    span.record("oxy.context.id", cid.as_str());
                }
            }
            ExecutionSource::A2a {
                task_id,
                context_id,
                thread_id,
            } => {
                span.record("oxy.execution.source", "a2a");
                span.record("oxy.task.id", task_id.as_str());
                span.record("oxy.context.id", context_id.as_str());
                span.record("oxy.thread.id", thread_id.as_str());
            }
            ExecutionSource::Mcp { session_id } => {
                span.record("oxy.execution.source", "mcp");
                if let Some(sid) = session_id {
                    span.record("oxy.context.id", sid.as_str());
                }
            }
            _ => {}
        }
    }

    events::agent::run_agent::input(
        &project,
        &agent_ref_str,
        &project_path_str,
        &prompt,
        &memory,
        &variables,
        &source,
    );

    let output = AgentLauncher::new()
        .with_filters(filters)
        .with_connections(connections)
        .with_globals(globals)
        .with_sandbox_info(sandbox_info.clone())
        .with_project(project)
        .await?
        .launch(
            AgentInput {
                agent_ref: agent_ref.as_ref().to_string_lossy().to_string(),
                prompt,
                memory: memory.into_iter().map(Into::into).collect(),
                variables,
                a2a_task_id: None,
                a2a_thread_id: None,
                a2a_context_id: None,
                sandbox_info,
            },
            event_handler,
        )
        .await?;

    events::agent::run_agent::output(&output);

    Ok(output)
}

pub async fn run_agentic_workflow<P: AsRef<Path>, H: EventHandler + Send + 'static>(
    project_manager: ProjectManager,
    agent_ref: P,
    prompt: String,
    event_handler: H,
    memory: Vec<Message>,
) -> Result<OutputContainer, OxyError> {
    AgentLauncher::new()
        .with_project(project_manager)
        .await?
        .launch_agentic_workflow(
            agent_ref.as_ref().to_string_lossy().as_ref(),
            AgenticInput {
                context_id: uuid::Uuid::new_v4().to_string(),
                prompt,
                trace: memory.into_iter().map(|m| m.into()).collect(),
            },
            event_handler,
            None,
        )
        .await
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Message {
    pub content: String,
    pub is_human: bool,
    pub created_at: DateTimeWithTimeZone,
}

impl From<Message> for AgentMessage {
    fn from(val: Message) -> Self {
        Self {
            content: val.content,
            is_human: val.is_human,
            created_at: val.created_at.with_timezone(&chrono::Utc),
        }
    }
}

impl From<Message> for ChatCompletionRequestMessage {
    fn from(val: Message) -> Self {
        if val.is_human {
            ChatCompletionRequestUserMessage {
                content: ChatCompletionRequestUserMessageContent::Text(val.content),
                ..Default::default()
            }
            .into()
        } else {
            ChatCompletionRequestAssistantMessage {
                content: Some(ChatCompletionRequestAssistantMessageContent::Text(
                    val.content,
                )),
                ..Default::default()
            }
            .into()
        }
    }
}

pub struct AgenticRunner {
    context_id: String,
    prompt: String,
    memory: Vec<Message>,
}

impl AgenticRunner {
    pub fn new(context_id: String, prompt: String, memory: Vec<Message>) -> Self {
        Self {
            prompt,
            memory,
            context_id,
        }
    }
}

#[async_trait::async_trait]
impl Dispatch for AgenticRunner {
    async fn run(
        &self,
        project_manager: ProjectManager,
        topic_ref: TopicRef<DispatchEventKind>,
        source_id: String,
        retry_strategy: RetryStrategy,
    ) -> Result<OutputContainer, OxyError> {
        AgentLauncher::new()
            .with_project(project_manager)
            .await?
            .launch_agentic_workflow(
                &source_id,
                AgenticInput {
                    context_id: self.context_id.clone(),
                    prompt: self.prompt.clone(),
                    trace: self.memory.iter().cloned().map(|m| m.into()).collect(),
                },
                topic_ref,
                retry_strategy.run_index().map(|i| i.to_string()),
            )
            .await
    }
}
