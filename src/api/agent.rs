use chrono::prelude::{DateTime, Utc};
use std::fs;

use crate::{
    ai::{self, agent::LLMAgent},
    config::model::{FileFormat, ProjectPath},
    db::{
        conversations::{create_conversation, get_conversation_by_agent},
        message::save_message,
    },
};
use async_stream::stream;
use axum::response::IntoResponse;
use axum::{extract, Json};
use axum_streams::StreamBodyAs;
use sea_orm::prelude::DateTimeWithTimeZone;
use serde::{Deserialize, Serialize};
use std::time::Duration;
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
    pub agent: String,
}

async fn get_agent(agent_path: &str) -> Box<dyn LLMAgent + Send> {
    let file_path = ProjectPath::get_path(agent_path);

    let agent = ai::setup_agent(Some(&file_path), &FileFormat::Markdown)
        .await
        .unwrap();
    agent
}

pub async fn ask(extract::Json(payload): extract::Json<AskRequest>) -> impl IntoResponse {
    let conversation = get_conversation_by_agent(payload.agent.as_str()).await;
    let conversation_id: Uuid;
    match conversation {
        Some(c) => {
            conversation_id = c.id;
        }
        None => {
            let new_conversation = create_conversation(&payload.agent).await;
            conversation_id = new_conversation.id;
        }
    }
    let question = save_message(conversation_id, &payload.question, true).await;
    let s = stream! {
        yield Message {
            content: payload.question.clone(),
            id: question.id,
            is_human: question.is_human,
            created_at: question.created_at,
        };

    let agent = get_agent(&payload.agent).await;
    let result: String =agent.request(&payload.question).await.unwrap().output;
    let answer = save_message(
        conversation_id,
        &result,
        false,
    ).await;

    let chunks = vec![answer.content];
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
    return StreamBodyAs::json_nl(s);
}

#[derive(Serialize)]
struct Message {
    content: String,
    id: Uuid,
    is_human: bool,
    created_at: DateTimeWithTimeZone,
}

#[derive(Serialize)]
pub struct AgentItem {
    name: String,
    updated_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct ListAgentResponse {
    agents: Vec<AgentItem>,
}

pub async fn list() -> Json<ListAgentResponse> {
    let agent_dir = ProjectPath::get_path("agents");
    let paths = fs::read_dir(agent_dir).unwrap();
    let mut agents = Vec::<AgentItem>::new();

    for path in paths {
        match path {
            Ok(e) => {
                let p = e.path();
                let file_name: &std::ffi::OsStr = p.file_name().unwrap();
                if file_name.to_string_lossy().ends_with(".yml") {
                    let agent_name = p.file_stem().unwrap().to_string_lossy().to_string();
                    agents.push(AgentItem {
                        name: agent_name,
                        updated_at: p.metadata().unwrap().modified().unwrap().into(),
                    });
                }
            }
            Err(e) => {
                eprintln!("Error reading agent directory: {}", e);
                continue;
            }
        }
    }

    agents.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Json(ListAgentResponse { agents })
}
