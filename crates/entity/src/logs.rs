//! `SeaORM` Entity for Logs

use sea_orm::entity::prelude::*;

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
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::UserId",
        to = "super::users::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Users,
    #[sea_orm(
        belongs_to = "super::threads::Entity",
        from = "Column::ThreadId",
        to = "super::threads::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Threads,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Users.def()
    }
}

impl Related<super::threads::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Threads.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
