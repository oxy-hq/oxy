//! `verified_queries` — one row per `.sql` file in the workspace.
//! `content_sha256` lets the runtime answer "is this verified query
//! equivalent to the one the analytics agent matched on its last
//! run?" without re-hashing.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "verified_queries")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub revision_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub file_path: String,
    pub content_sha256: String,
    pub content: String,
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
