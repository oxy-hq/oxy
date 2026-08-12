use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Partner access — bound to a person's **org membership** in the partner org.
///
/// A row means this member is a partner OPERATOR: they act on the partner's
/// clients, bounded by the partner's ceiling. There is no role and no per-client
/// scope — one partnership, and everyone on it reaches every client. A member of
/// the partner org WITHOUT a row is just an ordinary employee.
///
/// This is what kills the old duplicate membership system: a partner's people are
/// simply `org_members` of the partner org. No email keying, no orphaned `user_id`,
/// no parallel invitation flow. Org invitations already cover "grant before first
/// login". (The table keeps its historical name for migration continuity.)
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "partner_role_bindings")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// One access row per member of the partner org.
    #[sea_orm(unique)]
    pub org_member_id: Uuid,
    pub created_at: DateTimeWithTimeZone,
    #[sea_orm(
        belongs_to,
        from = "org_member_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    #[serde(skip)]
    pub org_members: BelongsTo<super::org_members::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
