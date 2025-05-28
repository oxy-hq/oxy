use crate::{db::client::establish_connection, errors::OxyError};
use axum::{
    extract::{self, Path},
    http::StatusCode,
};
use entity::prelude::Messages;
use sea_orm::{ColumnTrait, Condition, EntityTrait, Order, QueryFilter, QueryOrder};
use sea_orm::{QuerySelect, prelude::DateTimeWithTimeZone};
use serde::Serialize;
use uuid::Uuid;

#[derive(Serialize)]
pub struct MessageItem {
    pub id: String,
    pub content: String,
    pub is_human: bool,
    pub thread_id: String,
    pub created_at: DateTimeWithTimeZone,
}

pub async fn get_messages_by_thread(
    Path(thread_id): Path<String>,
) -> Result<extract::Json<Vec<MessageItem>>, StatusCode> {
    let connection = establish_connection().await;
    let uuid = Uuid::parse_str(&thread_id).map_err(|_| StatusCode::BAD_REQUEST)?;

    let messages = Messages::find()
        .filter(
            Condition::all()
                .add(<entity::prelude::Messages as EntityTrait>::Column::ThreadId.eq(uuid)),
        )
        .order_by(
            <entity::prelude::Messages as EntityTrait>::Column::CreatedAt,
            Order::Asc,
        )
        .all(&connection)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let message_items = messages
        .into_iter()
        .map(|m| MessageItem {
            id: m.id.to_string(),
            content: m.content,
            is_human: m.is_human,
            thread_id: m.thread_id.to_string(),
            created_at: m.created_at,
        })
        .collect();

    Ok(extract::Json(message_items))
}
