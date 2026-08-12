use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "git_namespaces")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub installation_id: i64,
    pub name: String,
    #[serde(default)]
    pub oauth_token: String,
    pub owner_type: String,
    pub provider: String,
    pub slug: String,
    pub created_by: Uuid,
    pub org_id: Option<Uuid>,
    #[sea_orm(
        belongs_to,
        from = "created_by",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    #[serde(skip)]
    pub users: BelongsTo<super::users::Entity>,
    #[sea_orm(
        belongs_to,
        from = "org_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "SetNull"
    )]
    #[serde(skip)]
    pub organizations: BelongsTo<Option<super::organizations::Entity>>,
}

impl ActiveModelBehavior for ActiveModel {}
