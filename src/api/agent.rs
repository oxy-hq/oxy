use crate::ai::{self, agent::LLMAgent};
use axum::{body::Body, extract, response::IntoResponse};
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
