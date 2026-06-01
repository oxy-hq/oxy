use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Global "Oxy app admin" role. Members of this table can access the
/// customer-apps admin surface and any registered customer app
/// regardless of org membership. Managed by `OXY_OWNER` users only.
///
/// Email is stored (not user_id) so grants can be created before the
/// user has signed in for the first time, matching the magic-link
/// onboarding model.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "app_admins")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique)]
    pub email: String,
    /// User who added this admin. NULL when the row was seeded from the
    /// legacy `OXY_APP_ADMINS` env var on startup.
    pub granted_by: Option<Uuid>,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::GrantedBy",
        to = "super::users::Column::Id",
        on_update = "NoAction",
        on_delete = "SetNull"
    )]
    Users,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Users.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
