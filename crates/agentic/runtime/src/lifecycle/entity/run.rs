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
    /// Soft FK → `agentic_schedules.id`. Stamped when a scheduler tick (or
    /// `run_now`) seeds this run; null for runs that came from any other
    /// path. Lets per-job history queries filter on a single column and
    /// lets the dashboard timeline match actual runs back to the schedule
    /// that produced them.
    pub schedule_id: Option<String>,
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
    /// Driver lease owner: the id of the coordinator process/loop currently
    /// driving this run. CAS-acquired; NULL means no live driver. Used to gate
    /// recovery selection so a periodic loop cannot double-drive a run.
    pub driver_id: Option<String>,
    /// Last heartbeat from the lease holder in `driver_id`. A lease is
    /// considered stale (re-acquirable) once this is older than the lease TTL.
    pub driver_heartbeat_at: Option<DateTimeWithTimeZone>,
    /// Set by the HTTP cancel endpoint. A DB-observable cancel signal so a
    /// recovered / Global run (driven out-of-process by the periodic loop
    /// or a standalone worker) can be cancelled — the in-memory watch
    /// channel only reaches a same-process coordinator.
    pub cancel_requested_at: Option<DateTimeWithTimeZone>,
    /// Workspace that owns this run. Plain UUID, no FK to `workspaces.id`
    /// (cross-domain reference per agentic boundary rules). Stamped at row
    /// insert by the `start_*_run` paths; the nil UUID means "local /
    /// pre-migration", which the local serve mode treats as its single
    /// workspace.
    ///
    /// Lets out-of-process drivers (recovery loop, latency worker)
    /// resolve which workspace's `PlatformContext` to use for a row they
    /// pick up — without this column they have no way to route a run
    /// back to its workspace.
    pub workspace_id: Uuid,
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
