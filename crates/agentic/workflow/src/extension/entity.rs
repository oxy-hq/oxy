use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "agentic_workflow_state")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub run_id: String,
    pub workflow_yaml_hash: String,
    pub workflow_config: Json,
    pub workflow_context: Json,
    pub variables: Option<Json>,
    pub trace_id: String,
    pub current_step: i32,
    pub results: Json,
    /// Vestigial: always persisted as `{}` by `apply_result_delta_in_txn` and
    /// reconstructed from `results` at load time (see `rebuild_render_context`
    /// in `extension/mod.rs`). Kept on the schema for backward compatibility
    /// with existing rows; do not write to it.
    pub render_context: Json,
    pub pending_children: Json,
    pub decision_version: i64,
    pub updated_at: ChronoDateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
