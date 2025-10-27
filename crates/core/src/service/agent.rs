use crate::{
    adapters::{project::manager::ProjectManager, session_filters::SessionFilters},
    agent::{AgentLauncher, builders::fsm::config::AgenticInput, types::AgentInput},
    config::{
        ConfigManager,
        constants::{CONCURRENCY_SOURCE, CONSISTENCY_SOURCE, WORKFLOW_SOURCE},
        model::{AgentConfig, ConnectionOverrides},
    },
    errors::OxyError,
    execute::{
        types::{Event, EventKind, Output, OutputContainer, ProgressType},
        writer::{EventHandler, NoopHandler},
    },
    theme::StyledText,
    utils::print_colored_sql,
};
use async_openai::types::{
    ChatCompletionRequestAssistantMessage, ChatCompletionRequestAssistantMessageContent,
    ChatCompletionRequestMessage, ChatCompletionRequestUserMessage,
    ChatCompletionRequestUserMessageContent,
};
use sea_orm::prelude::DateTimeWithTimeZone;
use serde::Deserialize;
use std::{
    io::Write,
    path::{Path, PathBuf},
};
use utoipa::ToSchema;

use super::eval::PBarsHandler;

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

pub async fn run_agent<P: AsRef<Path>, H: EventHandler + Send + 'static>(
    project: ProjectManager,
    agent_ref: P,
    prompt: String,
    event_handler: H,
    memory: Vec<Message>,
    filters: Option<SessionFilters>,
    connections: Option<ConnectionOverrides>,
) -> Result<OutputContainer, OxyError> {
    AgentLauncher::new()
        .with_filters(filters)
        .with_connections(connections)
        .with_project(project)
        .await?
        .launch(
            AgentInput {
                agent_ref: agent_ref.as_ref().to_string_lossy().to_string(),
                prompt,
                memory,
            },
            event_handler,
        )
        .await
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
                prompt,
                trace: memory.into_iter().map(|m| m.into()).collect(),
            },
            event_handler,
        )
        .await
}

#[derive(Debug, Clone)]
pub struct Message {
    pub content: String,
    pub is_human: bool,
    pub created_at: DateTimeWithTimeZone,
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
