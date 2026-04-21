use sea_orm::entity::prelude::*;

/// Persisted outcome of a child task, written atomically before updating the
/// parent's in-memory state. This is the **single source of truth** for
/// child→parent result handoff. On crash recovery, the coordinator rebuilds
/// parent `WaitingOnChildren` state from this table rather than from
/// `task_metadata` JSONB, closing the crash-consistency window.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "agentic_task_outcomes")]
pub struct Model {
    /// The child task ID (same as the child's run_id / task_id).
    #[sea_orm(primary_key, auto_increment = false)]
    pub child_id: String,
    /// The parent task ID that is waiting on this child.
    pub parent_id: String,
    /// `done` | `failed` | `cancelled`
    pub status: String,
    /// The child's answer (for `done`) or error message (for `failed`).
    pub answer: Option<String>,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::run::Entity",
        from = "Column::ChildId",
        to = "super::run::Column::Id",
        on_delete = "Cascade"
    )]
    ChildRun,
}

impl Related<super::run::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ChildRun.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
