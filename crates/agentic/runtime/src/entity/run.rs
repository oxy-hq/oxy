use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "agentic_runs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub question: String,
    pub answer: Option<String>,
    pub error_message: Option<String>,
    /// FK → threads(id); set when the run is initiated from a thread.
    pub thread_id: Option<Uuid>,
    /// Identifies the domain that created this run: `"analytics"`, `"builder"`, etc.
    pub source_type: Option<String>,
    /// Extensible JSONB blob for domain-specific data.
    pub metadata: Option<serde_json::Value>,
    /// Self-referential FK for task tree: the parent run that delegated to this one.
    pub parent_run_id: Option<String>,
    /// Single source of truth for run lifecycle:
    /// `running`, `awaiting_input`, `delegating`, `done`, `failed`, `cancelled`, `timed_out`
    pub task_status: Option<String>,
    /// Coordinator-specific JSONB state (child_task_ids, etc.).
    pub task_metadata: Option<serde_json::Value>,
    /// Recovery attempt number. 0 = original run, incremented on each recovery.
    pub attempt: i32,
    /// Non-null means "resume this run on next server startup". Replaces the
    /// old `needs_resume`/`shutdown` task_status values.
    pub recovery_requested_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::run_event::Entity")]
    RunEvents,
    #[sea_orm(has_one = "super::run_suspension::Entity")]
    Suspension,
}

impl Related<super::run_event::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::RunEvents.def()
    }
}

impl Related<super::run_suspension::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Suspension.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
