use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// The call trace of one Oxy Function invocation: every `ctx.*` host call it
/// made, as timed records for the console's waterfall. One row per invocation
/// that reached tenant code, written once after the run and pruned after a
/// few days. See `oxy_custom_apps::call_trace`.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "app_function_invocation_calls")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub invocation_id: Uuid,
    pub app_id: Uuid,
    pub created_at: DateTimeWithTimeZone,
    /// Platform setup before tenant code ran (thread, isolate, compile).
    pub init_ms: Option<i32>,
    /// Every host call the invocation made, kept or not.
    pub total_calls: i32,
    /// Calls past the per-invocation cap, not kept.
    pub dropped: i32,
    /// Kept calls still in flight when the invocation ended.
    pub unfinished: i32,
    /// `{"v":1,"calls":[…]}`, sorted by start offset.
    #[sea_orm(column_type = "JsonBinary")]
    pub calls: Json,
}

impl ActiveModelBehavior for ActiveModel {}
