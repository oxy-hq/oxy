use std::{fs, path::PathBuf};

use crate::{
    ai::{self, agent::LLMAgent},
    yaml_parsers::config_parser::{get_config_path, parse_config},
};
use axum::{body::Body, response::IntoResponse};
use axum::{extract, Json};
use futures::StreamExt;
use reqwest::header;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio_stream::Stream;
use tokio_stream::{self as stream};
use uuid::Uuid;
#[derive(Serialize)]
pub struct ConversationItem {
    title: String,
    id: Uuid,
}

#[derive(Serialize)]
pub struct ListConversationResponse {
    conversations: Vec<ConversationItem>,
}

#[derive(Deserialize)]
pub struct ListConversationRequest {}

#[derive(Serialize)]
pub struct AskResponse {
    pub answer: String,
}

#[derive(Deserialize)]
pub struct AskRequest {
    pub question: String,
    pub agent: Option<String>,
}

async fn get_agent(agent_name: Option<String>) -> Box<dyn LLMAgent + Send> {
    match agent_name {
        Some(name) => {
            let (agent, _) = ai::setup_agent(Some(name.as_str())).await.unwrap();
            agent
        }
        None => {
            let (agent, _) = ai::setup_agent(None).await.unwrap();
            agent
        }
    }
}

pub async fn ask(extract::Json(payload): extract::Json<AskRequest>) -> impl IntoResponse {
    let agent = get_agent(payload.agent).await;
    let result: String = agent.request(&payload.question.clone()).await.unwrap();
    let s = stream_answer(result);
    let header = [(header::CONTENT_TYPE, "text/plain; charset=UTF-8")];
    (header, Body::from_stream(s))
}

fn stream_answer(answer: String) -> impl Stream<Item = Result<String, axum::Error>> {
    let chars = answer
        .chars()
        .map(|word| word.to_string())
        .collect::<Vec<_>>();
    return stream::iter(chars.into_iter().map(|word| async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        Ok::<_, axum::Error>(word)
    }))
    .buffered(4);
}

#[derive(Serialize)]
pub struct AgentItem {
    name: String,
}

#[derive(Serialize)]
pub struct ListAgentResponse {
    agents: Vec<AgentItem>,
}

pub async fn list() -> Json<ListAgentResponse> {
    let config_path = get_config_path();
    let config = parse_config(&config_path).unwrap();
    let agent_dir = PathBuf::from(&config.defaults.project_path).join("agents");
    let paths = fs::read_dir(agent_dir).unwrap();
    let mut agents = Vec::<AgentItem>::new();

    for path in paths {
        match path {
            Ok(e) => {
                let p = e.path();
                let file_name: &std::ffi::OsStr = p.file_name().unwrap();
                if file_name.to_string_lossy().ends_with(".yml") {
                    let agent_name = p.file_stem().unwrap().to_string_lossy().to_string();
                    agents.push(AgentItem { name: agent_name });
                }
            }
            Err(e) => {
                eprintln!("Error reading agent directory: {}", e);
                continue;
            }
        }
    }

    Json(ListAgentResponse { agents: agents })
}
