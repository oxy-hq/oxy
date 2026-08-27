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

/// The seven columns before they are a row.
///
/// The compile-time gate holds one of these (it has parsed YAML but nothing
/// written yet); the runtime reader holds a [`Model`]. Both need the same
/// answer to "what does `config.yml` look like when these are put back
/// together", so the shape and the merge live here — next to the columns —
/// rather than in whichever crate happened to need them first.
#[derive(Debug, Clone, Default)]
pub struct CompiledConfig {
    pub databases: Json,
    pub models: Option<Json>,
    pub integrations: Option<Json>,
    pub repositories: Option<Json>,
    pub builder_agent: Option<Json>,
    pub mcp: Option<Json>,
    pub other: Option<Json>,
}

/// Reassemble the columns into the single top-level object `Config`
/// deserialises from.
///
/// `other` is the base because it carries every key that has no column of its
/// own; the projected columns are written over it, so a key that gained a
/// column mid-flight resolves to the column rather than the stale catch-all.
pub fn merge_compiled_config(cfg: &CompiledConfig) -> Json {
    let mut merged = match &cfg.other {
        Some(Json::Object(map)) => map.clone(),
        _ => serde_json::Map::new(),
    };
    merged.insert("databases".into(), cfg.databases.clone());
    for (key, value) in [
        ("models", &cfg.models),
        ("integrations", &cfg.integrations),
        ("repositories", &cfg.repositories),
        ("builder_agent", &cfg.builder_agent),
        ("mcp", &cfg.mcp),
    ] {
        if let Some(v) = value {
            merged.insert(key.into(), v.clone());
        }
    }
    Json::Object(merged)
}

impl Model {
    /// This row as one `config.yml`-shaped object. Same merge the compile-time
    /// gate validated, so the shape it accepted and the shape a reader serves
    /// cannot drift.
    pub fn merged_config(&self) -> Json {
        merge_compiled_config(&CompiledConfig {
            databases: self.databases.clone(),
            models: self.models.clone(),
            integrations: self.integrations.clone(),
            repositories: self.repositories.clone(),
            builder_agent: self.builder_agent.clone(),
            mcp: self.mcp.clone(),
            other: self.other.clone(),
        })
    }
}
