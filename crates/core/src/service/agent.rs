use crate::config::ConfigBuilder;
use crate::config::model::AgentConfig;
use crate::errors::OxyError;
use crate::execute::workflow::NoopLogger;
use crate::{
    config::model::FileFormat,
    db::{
        conversations::{create_conversation, get_conversation_by_agent},
        message::save_message,
    },
    execute::agent::run_agent,
};
use async_stream::stream;
use futures::Stream;
use sea_orm::prelude::DateTimeWithTimeZone;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
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

pub async fn ask(payload: AskRequest) -> impl Stream<Item = Message> {
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
    let stream = stream! {
        yield Message {
            content: payload.question.clone(),
            id: question.id,
            is_human: question.is_human,
            created_at: question.created_at,
        };

    let project_path = PathBuf::from(payload.project_path.clone());
    let agent_path= project_path.join(&payload.agent);
    let config = ConfigBuilder::new()
        .with_project_path(project_path)
        .unwrap()
        .build()
        .await
        .unwrap();

    let result = match run_agent(
        &agent_path,
        &FileFormat::Markdown,
        Some(payload.question),
        Arc::new(config),
        None,
    ).await {
        Ok(output) => output.output.to_string(),
        Err(e) => format!("Error running agent: {}", e),
    };

    let answer = save_message(
        conversation_id,
        &result,
        false,
    ).await;

    let chunks = answer.content.chars()
    .collect::<Vec<_>>()
    .chunks(3)
    .map(|chunk| chunk.iter().collect::<String>())
    .collect::<Vec<_>>();

    let msgs = chunks.into_iter().map(|chunk| {
        Message {
            content: chunk.to_string(),
            id: answer.id,
            is_human: answer.is_human,
            created_at: answer.created_at,
        }
    }).collect::<Vec<_>>();

    for msg in msgs {
        tokio::time::sleep(Duration::from_millis(10)).await;
        yield msg;
    }
    };

    stream
}

pub async fn ask_preview(payload: AskRequest) -> impl Stream<Item = Message> {
    let stream = stream! {
        yield Message {
            content: payload.question.clone(),
            id: Uuid::new_v4(),
            is_human: true,
            created_at: chrono::offset::Utc::now().into(),
        };

    let project_path = PathBuf::from(payload.project_path.clone());
    let agent_path= project_path.join(&payload.agent);
    let config = ConfigBuilder::new()
        .with_project_path(project_path)
        .unwrap()
        .build()
        .await
        .unwrap();

    let result = match run_agent(
        &agent_path,
        &FileFormat::Markdown,
        Some(payload.question),
        Arc::new(config),
        None,
    ).await {
        Ok(output) => output.output.to_string(),
        Err(e) => format!("Error running agent: {}", e),
    };

    let answer_id = Uuid::new_v4();
    let answer_created_at = chrono::offset::Utc::now().into();

    let chunks = result.chars()
    .collect::<Vec<_>>()
    .chunks(3)
    .map(|chunk| chunk.iter().collect::<String>())
    .collect::<Vec<_>>();

    let msgs = chunks.into_iter().map(|chunk| {
        Message {
            content: chunk.to_string(),
            id: answer_id,
            is_human: false,
            created_at: answer_created_at,
        }
    }).collect::<Vec<_>>();

    for msg in msgs {
        tokio::time::sleep(Duration::from_millis(10)).await;
        yield msg;
    }
    };

    stream
}

pub async fn ask_adhoc(
    question: String,
    project_path: PathBuf,
    agent: String,
) -> Result<String, OxyError> {
    let config = ConfigBuilder::new()
        .with_project_path(project_path.clone())
        .unwrap()
        .build()
        .await
        .unwrap();

    let agent_path = get_path_by_name(project_path.clone(), agent).await?;

    let result = match run_agent(
        &agent_path,
        &FileFormat::Markdown,
        Some(question),
        Arc::new(config),
        Some(Box::new(NoopLogger {})),
    )
    .await
    {
        Ok(output) => output.output.to_string(),
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
