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

#[sea_orm::model]
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
    #[sea_orm(
        belongs_to,
        from = "app_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    #[serde(skip)]
    pub apps: BelongsTo<super::apps::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
