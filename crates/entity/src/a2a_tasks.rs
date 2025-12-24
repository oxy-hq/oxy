//! `SeaORM` Entity for A2A Tasks

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "a2a_tasks")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub agent_name: String,
    pub thread_id: Option<Uuid>,
    pub run_id: Option<Uuid>,
    pub context_id: Option<String>,
    pub state: String,
    pub metadata: Json,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::threads::Entity",
        from = "Column::ThreadId",
        to = "super::threads::Column::Id",
        on_update = "NoAction",
        on_delete = "SetNull"
    )]
    Threads,
    #[sea_orm(
        belongs_to = "super::runs::Entity",
        from = "Column::RunId",
        to = "super::runs::Column::Id",
        on_update = "NoAction",
        on_delete = "SetNull"
    )]
    Runs,
    #[sea_orm(has_many = "super::a2a_messages::Entity")]
    Messages,
    #[sea_orm(has_many = "super::a2a_task_status::Entity")]
    TaskStatus,
    #[sea_orm(has_many = "super::a2a_artifacts::Entity")]
    Artifacts,
}

impl Related<super::threads::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Threads.def()
    }
}

impl Related<super::runs::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Runs.def()
    }
}

impl Related<super::a2a_messages::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Messages.def()
    }
}

impl Related<super::a2a_task_status::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::TaskStatus.def()
    }
}

impl Related<super::a2a_artifacts::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Artifacts.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
