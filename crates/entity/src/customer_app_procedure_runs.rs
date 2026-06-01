use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Persistent state for procedure runs triggered from customer-app
/// bundles via `useProcedureRun`. See
/// `migration::m20260526_000001_create_customer_app_procedure_runs`
/// for the schema rationale.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "customer_app_procedure_runs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub procedure_id: String,
    /// `running` | `done` | `failed` | `cancelled`.
    pub status: String,
    /// Caller-supplied params object passed through to the procedure's
    /// render context. Stored verbatim so a re-poll can return them
    /// for diagnostics.
    pub params: Option<Json>,
    pub progress_step: Option<String>,
    pub progress_percent: Option<i16>,
    pub result_summary: Option<String>,
    pub result_outputs: Option<Json>,
    pub error_message: Option<String>,
    pub error_code: Option<String>,
    /// Non-NULL when a cancel was requested; the spawned task reads
    /// this on the next progress checkpoint and aborts.
    pub cancel_requested_at: Option<DateTimeWithTimeZone>,
    pub started_at: DateTimeWithTimeZone,
    pub completed_at: Option<DateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
