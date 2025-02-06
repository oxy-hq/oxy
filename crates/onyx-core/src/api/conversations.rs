use crate::db::client::establish_connection;
use crate::db::conversations::get_conversation_by_agent;
use crate::db::message::get_messages_by_conversation;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::Json;
use entity::prelude::*;
use sea_orm::{prelude::DateTimeWithTimeZone, EntityTrait};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize)]
pub struct ConversationItem {
    agent: String,
    title: String,
    id: Uuid,
}

#[derive(Serialize)]
pub struct ListConversationResponse {
    conversations: Vec<ConversationItem>,
}

#[derive(Deserialize)]
pub struct ListConversationRequest {}

pub async fn list() -> Json<ListConversationResponse> {
    let connection = establish_connection().await;
    let results = Conversations::find().all(&connection).await.unwrap();
    let mut items: Vec<ConversationItem> = Vec::new();
    for result in results {
        items.push(ConversationItem {
            agent: result.agent,
            title: result.title,
            id: result.id,
        });
    }
    let data = ListConversationResponse {
        conversations: items,
    };
    Json(data)
}

#[derive(Serialize)]
pub struct MessageItem {
    content: String,
    id: Uuid,
    is_human: bool,
    created_at: DateTimeWithTimeZone,
}

#[derive(Serialize)]
pub struct GetConversationResponse {
    title: String,
    id: Uuid,
    messages: Vec<MessageItem>,
    agent: String,
}

pub async fn get(Path(agent): Path<String>) -> Result<Json<GetConversationResponse>, StatusCode> {
    let conversation = get_conversation_by_agent(agent.as_str()).await;
    if conversation.is_none() {
        // check agent exists
        return Err(StatusCode::NOT_FOUND);
    }
    let c = conversation.unwrap();
    let msgs = get_messages_by_conversation(c.id)
        .await
        .expect("Failed to get messages");
    let res = GetConversationResponse {
        title: c.title,
        id: c.id,
        messages: msgs
            .iter()
            .map(|m| MessageItem {
                content: m.content.clone(),
                id: m.id,
                is_human: m.is_human,
                created_at: m.created_at,
            })
            .collect(),
        agent: c.agent,
    };
    Ok(Json(res))
}
