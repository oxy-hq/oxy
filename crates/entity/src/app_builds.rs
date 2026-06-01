use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// One row per successful publish of a customer app. The bundle's files
/// live in S3 under `s3_prefix`; `apps.draft_build_id` /
/// `apps.published_build_id` point at the build currently serving each
/// channel. Keeping every build (bounded by a keep-last-N GC) is what
/// makes one-click rollback cheap.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "app_builds")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub app_id: Uuid,
    /// Engineer-facing version string (git sha or CI run id). Unique per
    /// app via `(app_id, build_id)`.
    pub build_id: String,
    /// S3 prefix holding this build's files:
    /// `customer-apps/<app_id>/builds/<build_id>/`.
    pub s3_prefix: String,
    /// Optional build/runtime manifest captured at publish time
    /// (`oxy-app.json`). Drives the future `artifact_type` serve branch.
    pub manifest_json: Option<Json>,
    pub created_at: DateTimeWithTimeZone,
    /// User (app-admin) who ran the publish. NULL for builds created before
    /// this column existed. Powers the "who deployed" audit in the admin UI.
    pub published_by: Option<Uuid>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::apps::Entity",
        from = "Column::AppId",
        to = "super::apps::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Apps,
}

impl Related<super::apps::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Apps.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
