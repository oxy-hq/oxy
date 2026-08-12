//! `SeaORM` Entity for API Keys

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "api_keys")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub user_id: Uuid,
    pub key_hash: String,
    pub name: String,
    pub expires_at: Option<DateTimeWithTimeZone>,
    pub last_used_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    pub is_active: bool,
    pub project_id: Uuid,
    /// When set, this key authenticates as the *custom app* (e.g. a
    /// Vercel-hosted Next.js bundle's server-side calls back into oxy).
    /// Nullable so CLI-style user-scoped keys stay unaffected.
    pub app_id: Option<Uuid>,
    #[sea_orm(
        belongs_to,
        from = "project_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    pub projects: BelongsTo<super::workspaces::Entity>,
    #[sea_orm(
        belongs_to,
        from = "user_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    pub users: BelongsTo<super::users::Entity>,
    #[sea_orm(
        belongs_to,
        from = "app_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    pub apps: BelongsTo<Option<super::apps::Entity>>,
}

impl ActiveModelBehavior for ActiveModel {}

impl Model {
    pub fn is_expired(&self) -> bool {
        match self.expires_at {
            Some(expires_at) => {
                let now = chrono::Utc::now();
                let expires_at_chrono = chrono::DateTime::<chrono::Utc>::from(expires_at);
                expires_at_chrono < now
            }
            None => false, // No expiration date means it never expires
        }
    }

    pub fn is_valid(&self) -> bool {
        self.is_active && !self.is_expired()
    }
}

impl ActiveModel {}
