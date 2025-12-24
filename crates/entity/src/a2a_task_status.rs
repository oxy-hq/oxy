//! `SeaORM` Entity for A2A Task Status History

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "a2a_task_status")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub task_id: Uuid,
    pub agent_name: String,
    pub state: String,
    pub message_id: Option<Uuid>,
    pub metadata: Option<Json>,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::a2a_tasks::Entity",
        from = "Column::TaskId",
        to = "super::a2a_tasks::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Tasks,
    #[sea_orm(
        belongs_to = "super::a2a_messages::Entity",
        from = "Column::MessageId",
        to = "super::a2a_messages::Column::Id",
        on_update = "NoAction",
        on_delete = "SetNull"
    )]
    Messages,
}

impl Related<super::a2a_tasks::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Tasks.def()
    }
}

impl Related<super::a2a_messages::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Messages.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
