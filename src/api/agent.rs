use crate::ai::{self, agent::LLMAgent};
use axum::{extract, Json};
use serde::{Deserialize, Serialize};
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
            return agent;
        }
        None => {
            let (agent, _) = ai::setup_agent(None).await.unwrap();
            return agent;
        }
    }
}

#[axum::debug_handler]
pub async fn ask(extract::Json(payload): extract::Json<AskRequest>) -> Json<AskResponse> {
    let agent = get_agent(payload.agent).await;
    let result = agent.request(&payload.question.clone()).await.unwrap();
    return Json(AskResponse { answer: result });
}
