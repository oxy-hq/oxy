use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// One row per (build, function): an Oxy Function shipped inside a
/// custom-app bundle's `functions/` dir. The bundled JS lives in the
/// build store under `artifact_key`; the function belongs to a specific
/// `app_builds` row, so it versions and rolls back with its build via the
/// existing channel pointers (no per-function channel state needed).
///
/// See `internal-docs/2026-06-12-customer-apps-functions-design.md`.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "app_functions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub app_id: Uuid,
    /// FK → `app_builds.id`. The build that carries this function.
    pub build_id: Uuid,
    /// Function name (matches `^[a-z][a-z0-9-]{0,63}$`). Unique per build.
    pub name: String,
    /// The per-function manifest entry from `oxy-app.json`
    /// (`route` / `schedule` / `airwayStep` / `timeoutSeconds`).
    pub manifest_json: Option<Json>,
    /// Build-store key of the bundled JS artifact:
    /// `customer-apps/<app_id>/builds/<build_id>/functions/<name>.js`.
    pub artifact_key: String,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::app_builds::Entity",
        from = "Column::BuildId",
        to = "super::app_builds::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    AppBuilds,
}

impl Related<super::app_builds::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::AppBuilds.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
