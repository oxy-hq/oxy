use sea_orm::entity::prelude::*;

/// Persisted task queue entry. Each row represents a task assignment that
/// survives process crashes. Workers poll this table for work; the coordinator
/// inserts rows when delegating.
///
/// Lifecycle: `queued` -> `claimed` -> `completed` | `failed` | `cancelled` | `dead`
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "agentic_task_queue")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub task_id: String,
    pub run_id: String,
    pub parent_task_id: Option<String>,
    /// `queued` | `claimed` | `completed` | `failed` | `cancelled` | `dead`
    pub queue_status: String,
    /// Serialized `TaskSpec` as JSONB.
    pub spec: Json,
    /// Serialized `TaskPolicy` as JSONB (optional).
    pub policy: Option<Json>,
    /// Which worker claimed this task (NULL while queued).
    pub worker_id: Option<String>,
    /// Last heartbeat from the worker executing this task.
    pub last_heartbeat: Option<DateTimeWithTimeZone>,
    /// When the worker claimed this task.
    pub claimed_at: Option<DateTimeWithTimeZone>,
    /// Per-task visibility timeout in seconds. If a claimed task's heartbeat is
    /// older than this, the reaper resets it to `queued`.
    pub visibility_timeout_secs: i32,
    /// How many times this task has been claimed (incremented on each claim).
    pub claim_count: i32,
    /// Maximum number of claims before the task is dead-lettered.
    pub max_claims: i32,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::run::Entity",
        from = "Column::RunId",
        to = "super::run::Column::Id",
        on_delete = "Cascade"
    )]
    Run,
}

impl Related<super::run::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Run.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
