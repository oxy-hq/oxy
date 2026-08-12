use sea_orm::Condition;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use super::org_members::OrgRole;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum InviteStatus {
    #[sea_orm(string_value = "pending")]
    Pending,
    #[sea_orm(string_value = "accepted")]
    Accepted,
    #[sea_orm(string_value = "expired")]
    Expired,
}

impl InviteStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            InviteStatus::Pending => "pending",
            InviteStatus::Accepted => "accepted",
            InviteStatus::Expired => "expired",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "pending" => Ok(InviteStatus::Pending),
            "accepted" => Ok(InviteStatus::Accepted),
            "expired" => Ok(InviteStatus::Expired),
            _ => Err(format!("Invalid invite status: {s}")),
        }
    }
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "org_invitations")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub org_id: Uuid,
    pub email: String,
    pub role: OrgRole,
    pub invited_by: Uuid,
    #[sea_orm(unique)]
    pub token: String,
    pub status: InviteStatus,
    pub expires_at: DateTimeWithTimeZone,
    pub created_at: DateTimeWithTimeZone,
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
        from = "invited_by",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    #[serde(skip)]
    pub users: BelongsTo<super::users::Entity>,
}

impl Model {
    /// Past its `expires_at`, regardless of what `status` says.
    pub fn is_expired(&self, now: DateTimeWithTimeZone) -> bool {
        self.expires_at <= now
    }

    /// Usable: can still be accepted. See [`live_pending`] for the query-side
    /// form of the same rule.
    pub fn is_live(&self, now: DateTimeWithTimeZone) -> bool {
        self.status == InviteStatus::Pending && !self.is_expired(now)
    }
}

/// **The** definition of "this invitation can still be accepted": `pending`
/// *and* not past `expires_at`. Every read path must use this.
///
/// Expiry is derived from `expires_at`, never from `status` — nothing
/// transitions a row to [`InviteStatus::Expired`], so a lapsed invite stays
/// `pending` forever. When call sites each wrote their own filter, that gap
/// bit hard: the create path checked `status='pending'` alone (so a lapsed
/// invite blocked its own replacement with a 409, permanently) while the list
/// path also required `expires_at > now()` (so the offending row was invisible
/// to the admin who could have revoked it). Same row, opposite conclusions,
/// no way out of it in the product. Keep the two facts welded together here.
pub fn live_pending(now: DateTimeWithTimeZone) -> Condition {
    Condition::all()
        .add(Column::Status.eq(InviteStatus::Pending))
        .add(Column::ExpiresAt.gt(now))
}

/// The complement of [`live_pending`] within `pending`: rows that are still
/// marked pending but can no longer be accepted. These are inert to every
/// path except the one that supersedes them on the next invite.
pub fn expired_pending(now: DateTimeWithTimeZone) -> Condition {
    Condition::all()
        .add(Column::Status.eq(InviteStatus::Pending))
        .add(Column::ExpiresAt.lte(now))
}

impl ActiveModelBehavior for ActiveModel {}

#[cfg(test)]
#[path = "org_invitations_tests.rs"]
mod org_invitations_tests;
