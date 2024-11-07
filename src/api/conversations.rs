use crate::db::client::establish_connection;
use axum::{extract, Json};
use entity::prelude::*;
use sea_orm::ActiveModelTrait;
use sea_orm::ActiveValue;
use sea_orm::EntityTrait;
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

pub async fn list() -> Json<ListConversationResponse> {
    let connection = establish_connection().await;
    let results = Conversations::find().all(&connection).await.unwrap();
    let mut items = Vec::new();
    for result in results {
        items.push(ConversationItem {
            title: result.title,
            id: result.id,
        });
    }
    let data = ListConversationResponse {
        conversations: items,
    };
    Json(data)
}

#[derive(Deserialize)]
pub struct CreateConversationRequest {
    pub title: String,
}

#[derive(Serialize)]
pub struct CreateConversationResponse {
    pub title: String,
    pub id: Uuid,
}

pub async fn create(
    extract::Json(payload): extract::Json<CreateConversationRequest>,
) -> Json<CreateConversationResponse> {
    let connection = establish_connection().await;
    let new_conversation = entity::conversations::ActiveModel {
        id: ActiveValue::Set(Uuid::new_v4()),
        title: ActiveValue::Set(payload.title.clone()),
        created_at: ActiveValue::not_set(),
        updated_at: ActiveValue::not_set(),
        deleted_at: ActiveValue::not_set(),
    };
    let inserted = new_conversation
        .insert(&connection)
        .await
        .expect("Error saving new conversation");
    let data = CreateConversationResponse {
        title: inserted.title,
        id: inserted.id,
    };
    Json(data)
}
