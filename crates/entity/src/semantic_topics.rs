//! `semantic_topics` — one row per `.topic.yml` parsed in a revision.
//! `compiled_sql_blob_key`, when set, points at an S3 object holding
//! the canonical YAML for the topic; readers prefer that blob over
//! `definition` to keep Postgres tablespace bounded for workspaces
//! with large semantic models.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "semantic_topics")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub revision_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub name: String,
    pub file_path: String,
    pub definition: Json,
    /// S3 key for the compiled blob. NULL when no S3 backend is
    /// configured at compile time — readers fall back to `definition`.
    pub compiled_sql_blob_key: Option<String>,
    #[sea_orm(
        belongs_to,
        from = "revision_id",
        to = "revision_id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    #[serde(skip)]
    pub revisions: BelongsTo<super::revisions::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
