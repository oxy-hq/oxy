//! `semantic_views` — one row per `.view.yml` parsed in a revision.
//! `compiled_sql_blob_key` is reserved for Phase 1.6b when we move
//! large multi-dialect compiled SQL bodies into S3 (semantic views
//! across 5 dialects routinely top tens of KB each).

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "semantic_views")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub revision_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub name: String,
    pub file_path: String,
    pub definition: Json,
    /// S3 key for the dialect-compiled SQL bodies. NULL until Phase
    /// 1.6b populates it.
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
