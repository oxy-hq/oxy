//! `workspace_compiled_configs` — the compiled view of `config.yml`,
//! one row per revision. Fields are unstructured JSONB rather than
//! normalised because the existing `Config` struct is huge and the
//! structure is the data model; we mirror it 1:1 so the runtime read
//! reconstructs a `Config` with `serde_json::from_value` and no
//! translation layer.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
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
