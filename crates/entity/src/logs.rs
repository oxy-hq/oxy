//! `SeaORM` Entity for Logs

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "logs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub user_id: Uuid,
    #[sea_orm(column_type = "Text")]
    pub prompts: String,
    pub thread_id: Uuid,
    pub log: Json,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    #[sea_orm(
        belongs_to,
        from = "user_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    pub users: BelongsTo<super::users::Entity>,
    #[sea_orm(
        belongs_to,
        from = "thread_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    pub threads: BelongsTo<super::threads::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
