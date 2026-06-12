//! `semantic_views` — one row per `.view.yml` parsed in a revision.
//! `compiled_sql_blob_key` is reserved for Phase 1.6b when we move
//! large multi-dialect compiled SQL bodies into S3 (semantic views
//! across 5 dialects routinely top tens of KB each).

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

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
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::revisions::Entity",
        from = "Column::RevisionId",
        to = "super::revisions::Column::RevisionId",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Revisions,
}

impl Related<super::revisions::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Revisions.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
