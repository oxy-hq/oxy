use crate::db::client::establish_connection;
use entity::conversations::{self};
use entity::prelude::*;
use sea_orm::entity::*;
use sea_orm::ActiveModelTrait;
use sea_orm::ActiveValue;
use sea_orm::EntityTrait;
use sea_orm::QueryFilter;
use uuid::Uuid;

pub async fn get_conversation_by_agent(agent: &str) -> Option<conversations::Model> {
    let connection = establish_connection().await;
    let conversations = Conversations::find()
        .filter(conversations::Column::Agent.eq(agent))
        .one(&connection)
        .await;

    return conversations.unwrap();
}

pub async fn create_conversation(agent_name: &str) -> conversations::Model {
    let connection = establish_connection().await;
    let new_conversation = entity::conversations::ActiveModel {
        id: ActiveValue::Set(Uuid::new_v4()),
        created_at: ActiveValue::not_set(),
        updated_at: ActiveValue::not_set(),
        deleted_at: ActiveValue::not_set(),
        agent: ActiveValue::Set(agent_name.to_string()),
        title: ActiveValue::Set(agent_name.to_string()),
    };
    let inserted = new_conversation
        .insert(&connection)
        .await
        .expect("Error saving new conversation");
    inserted
}
