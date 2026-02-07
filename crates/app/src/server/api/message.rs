use std::collections::HashSet;

use crate::server::service::{
    statics::BROADCASTER,
    types::run::{RootReference, RunStatus},
};
use axum::{
    extract::{self, Path},
    http::StatusCode,
};
use entity::prelude::Messages;
use itertools::Itertools;
use oxy::{database::client::establish_connection, execute::types::Usage};
use oxy_shared::errors::OxyError;
use sea_orm::{ColumnTrait, Condition, EntityTrait, Order, QueryFilter, QueryOrder};
use sea_orm::{FromQueryResult, QuerySelect, prelude::DateTimeWithTimeZone};
use serde::Serialize;
use uuid::Uuid;

#[derive(Serialize, FromQueryResult, Debug)]
pub struct MessageItem {
    pub id: Uuid,
    pub content: String,
    pub is_human: bool,
    pub thread_id: Uuid,
    pub created_at: DateTimeWithTimeZone,
    #[sea_orm(nested)]
    pub usage: Usage,
    #[sea_orm(nested)]
    pub run_info: Option<MappedRunDetails>,
}

#[derive(Serialize, FromQueryResult, Debug)]
pub struct MappedRunDetails {
    pub children: Option<serde_json::Value>,
    pub blocks: Option<serde_json::Value>,
    pub error: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub source_id: Option<String>,
    pub run_index: Option<i32>,
    pub lookup_id: Option<Uuid>,
    #[sea_orm(nested)]
    pub root_ref: Option<RootReference>,
    #[sea_orm(skip)]
    pub status: Option<RunStatus>,
}

impl MappedRunDetails {
    pub fn set_status(&mut self, status: RunStatus) {
        self.status = Some(status);
    }

    pub fn task_id(&self) -> Result<String, OxyError> {
        let source_id = self.source_id.as_ref().ok_or(OxyError::RuntimeError(
            "Source ID is required to generate task ID".to_string(),
        ))?;
        self.run_index
            .map(|index| format!("{}::{}", source_id, index))
            .ok_or(OxyError::RuntimeError(
                "Run index is required to generate task ID".to_string(),
            ))
    }
}

pub async fn get_messages_by_thread(
    Path((_project_id, thread_id)): Path<(Uuid, String)>,
) -> Result<extract::Json<Vec<MessageItem>>, StatusCode> {
    let connection = establish_connection().await.map_err(|e| {
        tracing::error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let uuid = Uuid::parse_str(&thread_id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let query = Messages::find()
        .select_only()
        .columns([
            entity::messages::Column::Id,
            entity::messages::Column::Content,
            entity::messages::Column::IsHuman,
            entity::messages::Column::ThreadId,
            entity::messages::Column::CreatedAt,
            entity::messages::Column::InputTokens,
            entity::messages::Column::OutputTokens,
        ])
        .columns([
            entity::runs::Column::Children,
            entity::runs::Column::Blocks,
            entity::runs::Column::Error,
            entity::runs::Column::Metadata,
            entity::runs::Column::SourceId,
            entity::runs::Column::RunIndex,
            entity::runs::Column::LookupId,
            entity::runs::Column::RootSourceId,
            entity::runs::Column::RootRunIndex,
            entity::runs::Column::RootReplayRef,
        ])
        .filter(
            Condition::all()
                .add(<entity::prelude::Messages as EntityTrait>::Column::ThreadId.eq(uuid)),
        )
        .order_by(
            <entity::prelude::Messages as EntityTrait>::Column::CreatedAt,
            Order::Asc,
        )
        .order_by_with_nulls(
            <entity::prelude::Runs as EntityTrait>::Column::RunIndex,
            Order::Desc,
            sea_orm::sea_query::NullOrdering::First,
        )
        .left_join(entity::runs::Entity);
    let mut message_items: Vec<MessageItem> = query
        .into_model::<MessageItem>()
        .all(&connection)
        .await
        .map_err(|err| {
            tracing::error!("Database error when fetching messages: {}", err);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    tracing::info!("Debug messages: {:?}", message_items);

    let topics = BROADCASTER.list_topics::<HashSet<String>>().await;
    for message in message_items.iter_mut() {
        if let Some(item) = &mut message.run_info {
            let task_id = match &item.root_ref {
                Some(root_ref) => root_ref.task_id().ok(),
                None => item.task_id().ok(),
            };
            if let Some(task_id) = task_id {
                let status = match topics.contains(&task_id) {
                    true => RunStatus::Running,
                    false => match (&item.blocks, &item.error) {
                        (Some(_), None) => RunStatus::Completed,
                        (_, Some(_)) => RunStatus::Failed,
                        _ => RunStatus::Canceled,
                    },
                };
                item.set_status(status);
            }
        }
    }

    Ok(extract::Json(
        message_items
            .into_iter()
            .unique_by(|i| i.id.to_string())
            .collect(),
    ))
}
