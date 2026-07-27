use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// One row per Oxy Functions invocation (route, schedule, or airway).
/// See `internal-docs/customer-apps-functions.md` §11.12.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "app_function_invocations")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub app_id: Uuid,
    pub build_id: Uuid,
    pub function_name: String,
    /// `"route"` | `"schedule"` | `"airway"`.
    pub mode: String,
    /// `None` for system (schedule/airway) invocations.
    pub user_id: Option<Uuid>,
    /// `"running"` | `"success"` | `"error"` | `"cancelled"` | `"timeout"`.
    pub status: String,
    pub duration_ms: Option<i64>,
    pub error: Option<String>,
    pub cancel_requested_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    /// Caller-supplied idempotency key (route mode); unique per
    /// (app, function, user). `None` when the caller sent none.
    pub idempotency_key: Option<String>,
    /// Stored response body of a successful invocation, kept only when an
    /// `idempotency_key` is present so a retry can replay it.
    pub result_body: Option<String>,
    /// Hash of the request body for a keyed invocation, so a key reused with a
    /// different body is rejected instead of silently replaying the first result.
    pub request_hash: Option<i64>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
