use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "organizations")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub name: String,
    #[sea_orm(unique)]
    pub slug: String,
    /// Org-level uploaded logo bytes (white-labels the workspace HQ chrome:
    /// rail tile + HQ heading). `None` falls back to the code-first `logo.*`
    /// file at the workspace root, then to the name initial. `serde(skip)`
    /// keeps the raw bytes out of every org JSON response — the logo is
    /// served as an image by the dedicated `/{workspace_id}/logo` endpoint.
    #[serde(skip)]
    pub logo: Option<Vec<u8>>,
    /// Content type of `logo` (e.g. `image/png`), set together with it.
    #[serde(skip)]
    pub logo_content_type: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    #[sea_orm(has_many)]
    #[serde(skip)]
    pub org_members: HasMany<super::org_members::Entity>,
    #[sea_orm(has_many)]
    #[serde(skip)]
    pub org_invitations: HasMany<super::org_invitations::Entity>,
    #[sea_orm(has_many)]
    #[serde(skip)]
    pub org_secrets: HasMany<super::org_secrets::Entity>,
    #[sea_orm(has_many)]
    #[serde(skip)]
    pub workspaces: HasMany<super::workspaces::Entity>,
    #[sea_orm(has_many)]
    #[serde(skip)]
    pub git_namespaces: HasMany<super::git_namespaces::Entity>,
    #[sea_orm(has_many)]
    #[serde(skip)]
    pub slack_installations: HasMany<super::slack_installations::Entity>,
    #[sea_orm(has_many)]
    #[serde(skip)]
    pub slack_oauth_states: HasMany<super::slack_oauth_states::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
