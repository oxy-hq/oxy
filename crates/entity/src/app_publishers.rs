//! `SeaORM` Entity for app publishers (trusted publishing config).
//!
//! A row authorizes a specific GitHub Actions workflow to publish a specific app
//! via OIDC — **app-scoped**, matching the package registries (PyPI/npm/crates.io
//! are all per-project). A presented OIDC token whose claims match a row's
//! `(repo_owner_id, repo_name, workflow_ref, environment)` is exchanged for a
//! short-lived credential caveated to `app_id`.
//!
//! `repo_owner_id` is the GitHub NUMERIC account id, not the name — the
//! account-resurrection defence. `environment` is required so the client can gate
//! the publish job behind required-reviewers.
//!
//! See `internal-docs/partner-platform.md`.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "app_publishers")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub app_id: Uuid,
    pub repo_owner: String,
    pub repo_owner_id: i64,
    pub repo_name: String,
    pub workflow_ref: String,
    pub environment: String,
    pub created_by: Option<Uuid>,
    pub created_at: DateTimeWithTimeZone,
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
