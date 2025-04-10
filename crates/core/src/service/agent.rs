use crate::agent::AgentLauncher;
use crate::agent::types::AgentInput;
use crate::config::ConfigBuilder;
use crate::config::constants::AGENT_SOURCE;
use crate::config::model::AgentConfig;
use crate::db::message::update_message;
use crate::errors::OxyError;
use crate::execute::agent::AgentReference;
use crate::execute::types::{Chunk, Event, EventKind, Output, Source};
use crate::execute::writer::{BufWriter, EventHandler, NoopHandler};
use crate::theme::StyledText;
use crate::utils::print_colored_sql;
use crate::{
    config::model::FileFormat,
    db::{
        conversations::{create_conversation, get_conversation_by_agent},
        message::save_message,
    },
    execute::agent::run_agent as run_agent_legacy,
};
use futures::Stream;
use sea_orm::prelude::DateTimeWithTimeZone;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::Sender;
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct Memory {
    pub content: String,
    pub is_human: bool,
}

#[derive(Deserialize)]
pub struct AskRequest {
    pub question: String,
    pub agent: String,
    pub title: String,
    pub project_path: String,
    pub memory: Vec<Memory>,
}

#[derive(Serialize, Clone)]
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
        let (output, _) = run_agent(&project_path, &agent_path, prompt, message_stream).await?;
        update_message(answer.id, &output)
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
        Ok(output) => output.0,
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

pub async fn run_agent<H: EventHandler + Send + 'static, P: AsRef<Path>>(
    project_path: P,
    agent_ref: P,
    prompt: String,
    event_handler: H,
) -> Result<(String, Vec<AgentReference>), OxyError> {
    #[cfg(not(feature = "builders"))]
    return {
        let mut event_handler = event_handler;
        let source = Source {
            parent_id: None,
            id: AGENT_SOURCE.to_string(),
            kind: AGENT_SOURCE.to_string(),
        };
        let config = ConfigBuilder::new()
            .with_project_path(&project_path)?
            .build()
            .await?;
        let result =
            run_agent_legacy(agent_ref, &FileFormat::Markdown, Some(prompt), config, None).await?;
        for c in result.output.to_string().chars() {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            event_handler
                .handle_event(Event {
                    source: source.clone(),
                    kind: EventKind::Updated {
                        chunk: Chunk {
                            key: None,
                            delta: Output::Text(c.to_string()),
                            finished: false,
                        },
                    },
                })
                .await?;
        }
        event_handler
            .handle_event(Event {
                source: source.clone(),
                kind: EventKind::Updated {
                    chunk: Chunk {
                        key: None,
                        delta: Output::Text("".to_string()),
                        finished: true,
                    },
                },
            })
            .await?;
        Ok((result.output.to_string(), result.references))
    };
    #[cfg(feature = "builders")]
    return {
        let (output, references) =
            run_agent_with_builders(project_path, agent_ref, prompt, event_handler).await?;
        Ok((output.to_string(), references))
    };
}

pub struct AgentCLIHandler;

#[async_trait::async_trait]
impl EventHandler for AgentCLIHandler {
    async fn handle_event(&mut self, event: Event) -> Result<(), OxyError> {
        match event.kind {
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
        }
        Ok(())
    }
}

pub struct AgentReferencesHandler<H> {
    handler: H,
    references: Arc<Mutex<Vec<AgentReference>>>,
}

#[async_trait::async_trait]
impl<H> EventHandler for AgentReferencesHandler<H>
where
    H: EventHandler + Send + 'static,
{
    async fn handle_event(&mut self, event: Event) -> Result<(), OxyError> {
        if let EventKind::Updated { chunk } = &event.kind {
            if let Output::Table(table) = &chunk.delta {
                if let Some(reference) = table.clone().into_reference() {
                    let mut references = self.references.lock().unwrap();
                    references.push(reference);
                }
            }
        }
        self.handler.handle_event(event).await?;
        Ok(())
    }
}

pub async fn run_agent_with_builders<P: AsRef<Path>, H: EventHandler + Send + 'static>(
    project_path: P,
    agent_ref: P,
    prompt: String,
    event_handler: H,
) -> Result<(Output, Vec<AgentReference>), OxyError> {
    let mut buf_writer = BufWriter::new();
    let tx = buf_writer.create_writer(None)?;
    let references = Arc::new(Mutex::new(vec![]));
    let event_handler = AgentReferencesHandler {
        handler: event_handler,
        references: references.clone(),
    };
    let event_handle =
        tokio::spawn(async move { buf_writer.write_to_handler(event_handler).await });

    let result = AgentLauncher::new()
        .with_local_context(project_path)
        .await?
        .launch(
            AgentInput {
                agent_ref: agent_ref.as_ref().to_string_lossy().to_string(),
                prompt,
            },
            tx,
        )
        .await;
    event_handle.await??;
    let output = result?;
    let references = Arc::try_unwrap(references)
        .map_err(|_| OxyError::RuntimeError("Failed to eject value from loop".to_string()))?
        .into_inner()?;
    Ok((output, references))
}
