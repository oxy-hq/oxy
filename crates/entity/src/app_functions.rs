use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// One row per (build, function): an Oxy Function shipped inside a
/// custom-app bundle's `functions/` dir. The bundled JS lives in the
/// build store under `artifact_key`; the function belongs to a specific
/// `app_builds` row, so it versions and rolls back with its build via the
/// existing channel pointers (no per-function channel state needed).
///
/// See `internal-docs/customer-apps-functions.md`.
#[sea_orm::model]
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
    #[sea_orm(
        belongs_to,
        from = "build_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    #[serde(skip)]
    pub app_builds: BelongsTo<super::app_builds::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
