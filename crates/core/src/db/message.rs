use crate::db::client::establish_connection;
use entity::messages;
use entity::prelude::Messages;
use sea_orm::ActiveValue;
use sea_orm::EntityTrait;
use sea_orm::QueryFilter;
use sea_orm::QueryOrder;
use sea_orm::entity::*;
use sea_orm::{ActiveModelTrait, DbErr};
use uuid::Uuid;

pub async fn get_messages_by_conversation(
    conversation_id: Uuid,
) -> Result<Vec<messages::Model>, DbErr> {
    let connection = establish_connection().await;

    Messages::find()
        .filter(messages::Column::ConversationId.eq(conversation_id))
        .order_by_asc(messages::Column::CreatedAt)
        .all(&connection)
        .await
}

pub async fn save_message(conversation_id: Uuid, content: &str, is_human: bool) -> messages::Model {
    let connection: sea_orm::DatabaseConnection = establish_connection().await;
    let new_message = entity::messages::ActiveModel {
        id: ActiveValue::Set(Uuid::new_v4()),
        is_human: ActiveValue::Set(is_human),
        conversation_id: ActiveValue::set(conversation_id),
        content: ActiveValue::Set(content.to_string()),
        created_at: ActiveValue::not_set(),
    };

    new_message
        .insert(&connection)
        .await
        .expect("Error saving new message")
}

pub async fn update_message(id: Uuid, content: &str) -> Result<messages::Model, DbErr> {
    let connection = establish_connection().await;
    let message = Messages::find_by_id(id)
        .one(&connection)
        .await?
        .ok_or(DbErr::RecordNotFound("".to_string()))?;
    let mut message: messages::ActiveModel = message.into();
    message.content = ActiveValue::Set(content.to_string());
    let message = message.update(&connection).await?;
    Ok(message)
}
