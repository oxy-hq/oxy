//! `workspace_compiled_configs` — the compiled view of `config.yml`,
//! one row per revision. Fields are unstructured JSONB rather than
//! normalised because the existing `Config` struct is huge and the
//! structure is the data model; we mirror it 1:1 so the runtime read
//! reconstructs a `Config` with `serde_json::from_value` and no
//! translation layer.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "workspace_compiled_configs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub revision_id: Uuid,
    pub databases: Json,
    pub models: Option<Json>,
    pub integrations: Option<Json>,
    pub repositories: Option<Json>,
    pub builder_agent: Option<Json>,
    pub mcp: Option<Json>,
    /// Catch-all for top-level config.yml fields not surfaced above —
    /// lets new keys land without a schema migration on day one.
    pub other: Option<Json>,
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
