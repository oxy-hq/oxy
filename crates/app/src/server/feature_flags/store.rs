//! Sea-ORM helpers for the `feature_flags` table.

use chrono::Utc;
use entity::feature_flag::{ActiveModel, Column, Entity, Model};
use sea_orm::ActiveValue::Set;
use sea_orm::sea_query::OnConflict;
use sea_orm::{DatabaseConnection, DbErr, EntityTrait};

pub async fn fetch_all(db: &DatabaseConnection) -> Result<Vec<Model>, DbErr> {
    Entity::find().all(db).await
}

pub async fn upsert(db: &DatabaseConnection, key: &str, enabled: bool) -> Result<Model, DbErr> {
    let now = Utc::now().into();
    let active = ActiveModel {
        key: Set(key.to_string()),
        enabled: Set(enabled),
        updated_at: Set(now),
    };
    Entity::insert(active)
        .on_conflict(
            OnConflict::column(Column::Key)
                .update_columns([Column::Enabled, Column::UpdatedAt])
                .to_owned(),
        )
        .exec_with_returning(db)
        .await
}
