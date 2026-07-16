//! `SeaORM` Entity for App Publish Tokens.
//!
//! Long-lived bearer credentials for machine auth (primarily `oxy publish`
//! in CI), minted by global app-admins. Only `token_hash` (a SHA-256 of the
//! plaintext) is persisted — the plaintext is shown once at creation and
//! never stored. `token_prefix` is a short, non-secret display fragment
//! (e.g. `oxypublish_ab12cd34`) so the admin UI can identify a token without
//! revealing it. A live token (`revoked_at IS NULL`) authenticates as its
//! owner **only on the customer-apps admin surface**.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "app_publish_tokens")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub name: String,
    #[sea_orm(unique)]
    pub token_hash: String,
    pub token_prefix: String,
    /// App-admin user who minted this token. **NULL for an OIDC-minted machine
    /// token** (design §6) — it authorizes by `app_id` + consent, not by a user.
    pub created_by: Option<Uuid>,
    pub created_at: DateTimeWithTimeZone,
    pub last_used_at: Option<DateTimeWithTimeZone>,
    /// When set, the token is revoked and no longer authenticates.
    pub revoked_at: Option<DateTimeWithTimeZone>,
    /// The app this token may publish. **NULL = legacy staff-wide token** (the
    /// original identity-of-the-minter behaviour). Set = an app-scoped fallback
    /// token that can only publish this one app.
    pub app_id: Option<Uuid>,
    /// **NULL = legacy non-expiring.** Required (enforced at the app layer) for a
    /// partner-minted token — a long-lived secret in someone's CI must expire.
    pub expires_at: Option<DateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::CreatedBy",
        to = "super::users::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Users,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Users.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
