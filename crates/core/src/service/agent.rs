use crate::agent::AgentLauncher;
use crate::agent::types::AgentInput;
use crate::config::ConfigBuilder;
use crate::config::constants::{CONCURRENCY_SOURCE, CONSISTENCY_SOURCE, WORKFLOW_SOURCE};
use crate::config::model::AgentConfig;
use crate::errors::OxyError;
use crate::execute::types::{Event, EventKind, Output, OutputContainer, ProgressType};
use crate::execute::writer::{EventHandler, NoopHandler};
use crate::theme::StyledText;
use crate::utils::print_colored_sql;
use sea_orm::prelude::DateTimeWithTimeZone;
use serde::Deserialize;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
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
    project_path: PathBuf,
    agent: String,
) -> Result<String, OxyError> {
    let agent_path = get_path_by_name(project_path.clone(), agent).await?;
    let result = match run_agent(&project_path, &agent_path, question, NoopHandler, vec![]).await {
        Ok(output) => output.to_string(),
        Err(e) => format!("Error running agent: {e}"),
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
                EventKind::Finished { message } => {
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
    project_path: P,
    agent_ref: P,
    prompt: String,
    event_handler: H,
    memory: Vec<Message>,
) -> Result<OutputContainer, OxyError> {
    AgentLauncher::new()
        .with_local_context(project_path)
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

#[derive(Debug, Clone)]
pub struct Message {
    pub content: String,
    pub is_human: bool,
    pub created_at: DateTimeWithTimeZone,
}
