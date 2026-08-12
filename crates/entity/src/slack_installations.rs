use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "slack_installations")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub org_id: Uuid,
    pub slack_team_id: String,
    pub slack_team_name: String,
    pub slack_enterprise_id: Option<String>,
    pub bot_user_id: String,
    pub bot_token_secret_id: Uuid,
    #[sea_orm(column_type = "Text")]
    pub bot_scopes: String,
    pub installed_by_user_id: Uuid,
    pub installed_by_slack_user_id: String,
    pub installed_at: DateTimeWithTimeZone,
    pub revoked_at: Option<DateTimeWithTimeZone>,
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
        from = "bot_token_secret_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Restrict"
    )]
    #[serde(skip)]
    pub org_secrets: BelongsTo<super::org_secrets::Entity>,
    #[sea_orm(
        belongs_to,
        from = "installed_by_user_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    #[serde(skip)]
    pub users: BelongsTo<super::users::Entity>,
    #[sea_orm(has_many)]
    #[serde(skip)]
    pub slack_user_links: HasMany<super::slack_user_links::Entity>,
    #[sea_orm(has_many)]
    #[serde(skip)]
    pub slack_threads: HasMany<super::slack_threads::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
