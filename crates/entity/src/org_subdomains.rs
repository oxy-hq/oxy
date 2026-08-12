use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Opt-in org subdomain routing. One row per org granted a bare subdomain.
/// The subdomain label IS the org slug (`organizations.slug`) —
/// `<slug>.oxygen-hq.com` — so there's no label column; presence of an
/// `enabled` row is the opt-in flag. `default_workspace_id` is the project
/// the subdomain root scopes to.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "org_subdomains")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique)]
    pub org_id: Uuid,
    pub default_workspace_id: Option<Uuid>,
    pub enabled: bool,
    pub created_by: Option<Uuid>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    #[sea_orm(
        belongs_to,
        from = "org_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    #[serde(skip)]
    pub organizations: BelongsTo<super::organizations::Entity>,
    #[sea_orm(
        belongs_to,
        from = "default_workspace_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "SetNull"
    )]
    #[serde(skip)]
    pub workspaces: BelongsTo<Option<super::workspaces::Entity>>,
    #[sea_orm(
        belongs_to,
        from = "created_by",
        to = "id",
        on_update = "NoAction",
        on_delete = "SetNull"
    )]
    #[serde(skip)]
    pub users: BelongsTo<Option<super::users::Entity>>,
}

impl ActiveModelBehavior for ActiveModel {}
