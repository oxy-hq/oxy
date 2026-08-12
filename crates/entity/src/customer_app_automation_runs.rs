use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Persistent state for automation runs triggered from custom-app
/// bundles via `useAutomationRun` (legacy: `useProcedureRun`). See
/// `migration::m20260526_000001_create_customer_app_procedure_runs`
/// for the original schema rationale; the table was renamed from
/// `customer_app_procedure_runs` to `customer_app_automation_runs` by
/// `m20260623_000001_rename_procedures_to_automations` (which leaves a
/// back-compat view under the old name). The module
/// `entity::customer_app_procedure_runs` is kept as an alias (see
/// `lib.rs`).
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "customer_app_automation_runs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub procedure_id: String,
    /// `running` | `done` | `failed` | `cancelled`.
    pub status: String,
    /// Caller-supplied params object passed through to the automation's
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

impl ActiveModelBehavior for ActiveModel {}
