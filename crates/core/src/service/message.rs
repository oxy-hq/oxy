use crate::db::conversations::get_conversation_by_agent;
use crate::db::message::get_messages_by_conversation;
use crate::errors::OxyError;
use sea_orm::prelude::DateTimeWithTimeZone;
use serde::Serialize;
use uuid::Uuid;

#[derive(Serialize)]
pub struct MessageItem {
    pub content: String,
    pub id: Uuid,
    pub is_human: bool,
    pub created_at: DateTimeWithTimeZone,
}

pub async fn get_messages(agent: String) -> Result<Vec<MessageItem>, OxyError> {
    let conversation = get_conversation_by_agent(agent.as_str()).await;
    if conversation.is_none() {
        return Ok(vec![]);
    }
    let c = conversation.unwrap();
    let msgs = get_messages_by_conversation(c.id).await;

    let msgs = match msgs {
        Ok(messages) => messages,
        Err(e) => return Err(OxyError::RuntimeError(e.to_string())),
    };

    let res = msgs
        .iter()
        .map(|m| MessageItem {
            content: m.content.clone(),
            id: m.id,
            is_human: m.is_human,
            created_at: m.created_at,
        })
        .collect();
    Ok(res)
}
