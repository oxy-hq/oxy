use crate::agent::AgentLauncher;
use crate::agent::types::AgentInput;
use crate::config::ConfigBuilder;
use crate::config::constants::{CONCURRENCY_SOURCE, CONSISTENCY_SOURCE, WORKFLOW_SOURCE};
use crate::config::model::AgentConfig;
use crate::db::message::update_message;
use crate::db::{
    conversations::{create_conversation, get_conversation_by_agent},
    message::save_message,
};
use crate::errors::OxyError;
use crate::execute::types::{Event, EventKind, Output, OutputContainer, ProgressType};
use crate::execute::writer::{EventHandler, NoopHandler};
use crate::theme::StyledText;
use crate::utils::print_colored_sql;
use futures::Stream;
use sea_orm::prelude::DateTimeWithTimeZone;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use tokio::sync::mpsc::Sender;
use tokio_stream::wrappers::ReceiverStream;
use utoipa::ToSchema;
use uuid::Uuid;

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

#[derive(Serialize, Clone, ToSchema)]
pub struct Message {
    content: String,
    id: Uuid,
    is_human: bool,
    created_at: DateTimeWithTimeZone,
}

struct MessageStream {
    id: Uuid,
    is_human: bool,
    created_at: DateTimeWithTimeZone,
    tx: Sender<Message>,
}

impl MessageStream {
    fn new(
        id: Uuid,
        is_human: bool,
        created_at: DateTimeWithTimeZone,
        tx: Sender<Message>,
    ) -> Self {
        MessageStream {
            id,
            is_human,
            created_at,
            tx,
        }
    }
}

#[async_trait::async_trait]
impl EventHandler for MessageStream {
    async fn handle_event(&mut self, event: Event) -> Result<(), OxyError> {
        if let EventKind::Updated { chunk } = event.kind {
            let message = Message {
                content: chunk.delta.to_string(),
                id: self.id,
                is_human: self.is_human,
                created_at: self.created_at,
            };
            self.tx.send(message).await?;
        }
        Ok(())
    }
}

pub async fn ask(payload: AskRequest) -> Result<impl Stream<Item = Message>, OxyError> {
    let conversation = get_conversation_by_agent(payload.agent.as_str()).await;
    let conversation_id: Uuid;
    match conversation {
        Some(c) => {
            conversation_id = c.id;
        }
        None => {
            let new_conversation = create_conversation(&payload.agent, &payload.title).await;
            conversation_id = new_conversation.id;
        }
    }
    let question = save_message(conversation_id, &payload.question, true).await;
    let answer = save_message(conversation_id, "", false).await;
    let (tx, rx) = tokio::sync::mpsc::channel(100);
    tx.send(Message {
        content: payload.question.to_string(),
        id: question.id,
        is_human: question.is_human,
        created_at: question.created_at,
    })
    .await?;
    let message_stream = MessageStream::new(answer.id, answer.is_human, answer.created_at, tx);
    let prompt = payload.question.to_string();
    let project_path = payload.project_path.to_string();
    let agent_path = payload.agent.to_string();

    let _ = tokio::spawn(async move {
        let output_container =
            run_agent(&project_path, &agent_path, prompt, message_stream).await?;
        update_message(answer.id, &output_container.to_string())
            .await
            .map_err(|err| OxyError::DBError(format!("Failed to update message:\n{}", err)))
    });

    Ok(ReceiverStream::new(rx))
}

pub async fn ask_adhoc(
    question: String,
    project_path: PathBuf,
    agent: String,
) -> Result<String, OxyError> {
    let agent_path = get_path_by_name(project_path.clone(), agent).await?;
    let result = match run_agent(&project_path, &agent_path, question, NoopHandler).await {
        Ok(output) => output.to_string(),
        Err(e) => format!("Error running agent: {}", e),
    };
    Ok(result)
}

pub async fn list_agents(project_path: PathBuf) -> Result<Vec<String>, OxyError> {
    let config_builder = ConfigBuilder::new().with_project_path(&project_path)?;
    let config = config_builder.build().await?;

    let agents = config.list_agents().await?;
    Ok(agents
        .iter()
        .map(|absolute_path| {
            absolute_path
                .strip_prefix(&project_path)
                .unwrap()
                .to_string_lossy()
                .to_string()
        })
        .collect())
}

pub async fn get_agent_config(
    project_path: PathBuf,
    relative_path: String,
) -> Result<AgentConfig, OxyError> {
    let config_builder = ConfigBuilder::new().with_project_path(&project_path)?;
    let config = config_builder.build().await?;

    let agent = config.resolve_agent(relative_path).await?;
    Ok(agent)
}

pub async fn get_path_by_name(
    project_path: PathBuf,
    agent_name: String,
) -> Result<PathBuf, OxyError> {
    let config_builder = ConfigBuilder::new().with_project_path(&project_path)?;
    let config = config_builder.build().await?;

    let agents = config.list_agents().await?;
    for agent in agents {
        let agent_config = config.resolve_agent(agent.clone()).await?;
        if agent_config.name == agent_name {
            let path = project_path.join(agent);
            return Ok(path);
        }
    }
    Err(OxyError::ArgumentError(format!(
        "Agent with name {} not found",
        agent_name
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
                EventKind::Started { name } => {
                    println!("\n⏳Running workflow: {}", name);
                }
                EventKind::Finished { message } => {
                    println!("{}", message);
                }
                _ => {}
            },
            CONSISTENCY_SOURCE => match event.kind {
                EventKind::Progress { progress } => match progress {
                    ProgressType::Started(total) => {
                        self.pbar_handler.get_or_create_bar(&event.source.id, total);
                    }
                    ProgressType::Updated(progress) => {
                        self.pbar_handler.update_bar(&event.source.id, progress)?;
                    }
                    ProgressType::Finished => {
                        self.pbar_handler.remove_bar(&event.source.id);
                    }
                },
                EventKind::Message { message } => {
                    println!("{}", message);
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
                            println!("{}", table);
                        }
                        Err(e) => {
                            println!("{}", format!("Error displaying results: {}", e).error());
                        }
                    },
                    Output::Text(text) => {
                        if chunk.finished {
                            println!("{}", text);
                        } else {
                            print!("{}", text);
                            std::io::stdout().flush().unwrap();
                        }
                    }
                    _ => {}
                },
                EventKind::Message { message } => {
                    println!("{}", message);
                }
                _ => {}
            },
        }
        Ok(())
    }
}

pub async fn run_agent<P: AsRef<Path>, H: EventHandler + Send + 'static>(
    project_path: P,
    agent_ref: P,
    prompt: String,
    event_handler: H,
) -> Result<OutputContainer, OxyError> {
    AgentLauncher::new()
        .with_local_context(project_path)
        .await?
        .launch(
            AgentInput {
                agent_ref: agent_ref.as_ref().to_string_lossy().to_string(),
                prompt,
            },
            event_handler,
        )
        .await
}
