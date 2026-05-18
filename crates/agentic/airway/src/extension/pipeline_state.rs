//! `airway_pipeline_state` — aggregate root for incremental ingest state.
//!
//! One row per pipeline_name. Holds the serialized `PipelineState`,
//! `Schema`, and a monotonic `version` for optimistic concurrency on
//! save.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "airway_pipeline_state")]
pub struct Model {
    /// Pipeline name (`AirwayPipelineSpec.name`).
    #[sea_orm(primary_key, auto_increment = false)]
    pub pipeline_name: String,
    /// Serialized `airway::PipelineState`.
    pub state: Json,
    /// Serialized `airway::Schema`.
    pub schema_json: Json,
    /// Monotonic version used for optimistic concurrency on save.
    pub version: i64,
    pub updated_at: ChronoDateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
