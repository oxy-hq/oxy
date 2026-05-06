use std::collections::HashMap;
use std::str::FromStr;

use axum::extract::{Json, Path};
use axum::http::{HeaderMap, StatusCode};
use chrono::Utc;
use email_address::EmailAddress;
use entity::org_invitations;
use entity::org_invitations::InviteStatus;
use entity::org_members;
use entity::org_members::OrgRole;
use entity::organizations;
use entity::prelude::*;
use entity::workspaces;
use handlebars::Handlebars;
use once_cell::sync::Lazy;
use oxy::database::client::establish_connection;
use oxy::database::filters::UserQueryFilterExt;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use oxy_shared::errors::OxyError;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, FromQueryResult, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::middlewares::org_context::OrgContextExtractor;
use super::middlewares::role_guards::{OrgAdmin, OrgOwner};

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateOrgRequest {
    pub name: String,
    pub slug: String,
}

#[derive(Deserialize)]
pub struct UpdateOrgRequest {
    pub name: Option<String>,
    pub slug: Option<String>,
}

#[derive(Serialize)]
pub struct OrgResponse {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub role: String,
    pub created_at: String,
    /// Populated by list endpoints only. Single-org endpoints leave this None
    /// to avoid an extra query on the hot path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_count: Option<i64>,
}

#[derive(Deserialize)]
pub struct UpdateRoleRequest {
    pub role: String,
}

#[derive(Deserialize)]
pub struct InviteRequest {
    pub email: String,
    pub role: String,
}

#[derive(Deserialize)]
pub struct BulkInviteRequest {
    pub invitations: Vec<InviteRequest>,
}

#[derive(Serialize)]
pub struct BulkInviteResponse {
    pub invitations: Vec<InvitationResponse>,
}

#[derive(Serialize)]
pub struct MemberResponse {
    pub id: Uuid,
    pub user_id: Uuid,
    pub email: String,
    pub name: String,
    pub role: String,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct InvitationResponse {
    pub id: Uuid,
    pub email: String,
    pub role: String,
    pub token: String,
    pub status: String,
    pub expires_at: String,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct InvitationSummary {
    pub id: Uuid,
    pub email: String,
    pub role: String,
    pub token: String,
    pub status: String,
    pub expires_at: String,
    pub created_at: String,
}

/// Pending invitation addressed to the authenticated user, enriched with the
/// org it's for so the UI can render a meaningful accept screen.
#[derive(Serialize)]
pub struct MyInvitationResponse {
    pub id: Uuid,
    pub token: String,
    pub role: String,
    pub expires_at: String,
    pub created_at: String,
    pub org_id: Uuid,
    pub org_name: String,
    pub org_slug: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invited_by_name: Option<String>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub(crate) fn org_response(org: &organizations::Model, role: &OrgRole) -> OrgResponse {
    OrgResponse {
        id: org.id,
        name: org.name.clone(),
        slug: org.slug.clone(),
        role: role.as_str().to_string(),
        created_at: org.created_at.to_rfc3339(),
        workspace_count: None,
        member_count: None,
    }
}

/// Canonical slug generation. The frontend has a preview slugify for UX, but
/// this function is the source of truth — the stored slug always comes from here.
pub(crate) fn slugify_name(name: &str) -> String {
    slugify::slugify(name, "", "-", None)
}

/// Slugs that collide with top-level frontend routes. An org with one of these
/// slugs would be unreachable at `/{slug}` because React Router resolves the
/// static path first. Keep in sync with the routes declared in
/// `web-app/src/App.tsx` and any future top-level additions.
const RESERVED_ORG_SLUGS: &[&str] = &[
    "admin",
    "api",
    "app",
    "apps",
    "auth",
    "github",
    "invite",
    "invitations",
    "login",
    "logout",
    "onboarding",
    "orgs",
    "settings",
    "signin",
    "signup",
    "static",
    "workspace",
    "workspaces",
];

pub(crate) fn is_reserved_slug(slug: &str) -> bool {
    RESERVED_ORG_SLUGS.contains(&slug)
}

/// Trims, lowercases, and validates an invitee email. Returns the normalized
/// form on success or `BAD_REQUEST` for empty / malformed input. Centralizing
/// here keeps the single-invite and bulk-invite paths in sync.
pub(crate) fn normalize_invite_email(raw: &str) -> Result<String, StatusCode> {
    let normalized = raw.trim().to_lowercase();
    if normalized.is_empty() || !EmailAddress::is_valid(&normalized) {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(normalized)
}

// ---------------------------------------------------------------------------
// Organization CRUD
// ---------------------------------------------------------------------------

/// POST /orgs
pub async fn create_org(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Json(req): Json<CreateOrgRequest>,
) -> Result<Json<OrgResponse>, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("DB connection error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let slug = slugify_name(&req.slug);
    if slug.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    if is_reserved_slug(&slug) {
        // Not a collision — the resource doesn't exist, the name is forbidden
        // because it would shadow a top-level frontend route. 422 lets the
        // client distinguish this from a real slug-taken case.
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }

    let txn = db.begin().await.map_err(|e| {
        tracing::error!("Failed to begin transaction: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Best-effort slug uniqueness check. The DB UNIQUE constraint is the real
    // guard against races; this SELECT is an early-exit optimisation only.
    let existing = Organizations::find()
        .filter(organizations::Column::Slug.eq(&slug))
        .one(&txn)
        .await
        .map_err(|e| {
            tracing::error!("Failed to check slug uniqueness: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if existing.is_some() {
        return Err(StatusCode::CONFLICT);
    }

    let now = Utc::now().fixed_offset();
    let org_id = Uuid::new_v4();

    let org = organizations::ActiveModel {
        id: ActiveValue::Set(org_id),
        name: ActiveValue::Set(req.name),
        slug: ActiveValue::Set(slug),
        created_at: ActiveValue::Set(now),
        updated_at: ActiveValue::Set(now),
    };
    let org = org.insert(&txn).await.map_err(|e| {
        let msg = e.to_string();
        if msg.contains("unique") || msg.contains("duplicate") {
            tracing::warn!("Slug uniqueness conflict on insert (caught at DB level): {e}");
            return StatusCode::CONFLICT;
        }
        tracing::error!("Failed to insert organization: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Add the creator as owner.
    let member = org_members::ActiveModel {
        id: ActiveValue::Set(Uuid::new_v4()),
        org_id: ActiveValue::Set(org_id),
        user_id: ActiveValue::Set(user.id),
        role: ActiveValue::Set(OrgRole::Owner),
        created_at: ActiveValue::Set(now),
        updated_at: ActiveValue::Set(now),
    };
    member.insert(&txn).await.map_err(|e| {
        tracing::error!("Failed to insert org member: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Eager-insert the org_billing row so the SubscriptionGuard always finds
    // it. Status starts as `incomplete` — admin runs `provision_subscription`
    // after the sales call to flip it Active.
    let billing = entity::org_billing::ActiveModel {
        id: ActiveValue::Set(Uuid::new_v4()),
        org_id: ActiveValue::Set(org_id),
        status: ActiveValue::Set(entity::org_billing::BillingStatus::Incomplete),
        seats_paid: ActiveValue::Set(0),
        created_at: ActiveValue::Set(now),
        updated_at: ActiveValue::Set(now),
        ..Default::default()
    };
    billing.insert(&txn).await.map_err(|e| {
        tracing::error!("Failed to insert org_billing row: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    txn.commit().await.map_err(|e| {
        tracing::error!("Failed to commit transaction: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Airhouse provisioning is explicit only — the user triggers it from the
    // Settings → Airhouse page via POST /api/airhouse/me/provision.

    Ok(Json(org_response(&org, &OrgRole::Owner)))
}

/// GET /orgs
pub async fn list_orgs(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
) -> Result<Json<Vec<OrgResponse>>, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("DB connection error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let memberships = OrgMembers::find()
        .filter(org_members::Column::UserId.eq(user.id))
        .all(&db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query memberships: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let org_ids: Vec<Uuid> = memberships.iter().map(|m| m.org_id).collect();
    if org_ids.is_empty() {
        return Ok(Json(vec![]));
    }

    let orgs = Organizations::find()
        .filter(organizations::Column::Id.is_in(org_ids.clone()))
        .all(&db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query organizations: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let member_counts = count_members_per_org(&db, &org_ids).await?;
    let workspace_counts = count_workspaces_per_org(&db, &org_ids).await?;

    let responses: Vec<OrgResponse> = orgs
        .iter()
        .filter_map(|org| {
            let membership = memberships.iter().find(|m| m.org_id == org.id)?;
            Some(OrgResponse {
                id: org.id,
                name: org.name.clone(),
                slug: org.slug.clone(),
                role: membership.role.as_str().to_string(),
                created_at: org.created_at.to_rfc3339(),
                workspace_count: Some(workspace_counts.get(&org.id).copied().unwrap_or(0)),
                member_count: Some(member_counts.get(&org.id).copied().unwrap_or(0)),
            })
        })
        .collect();

    Ok(Json(responses))
}

#[derive(FromQueryResult)]
struct OrgCountRow {
    org_id: Uuid,
    count: i64,
}

async fn count_members_per_org(
    db: &sea_orm::DatabaseConnection,
    org_ids: &[Uuid],
) -> Result<HashMap<Uuid, i64>, StatusCode> {
    let rows: Vec<OrgCountRow> = OrgMembers::find()
        .filter(org_members::Column::OrgId.is_in(org_ids.to_vec()))
        .select_only()
        .column(org_members::Column::OrgId)
        .column_as(Expr::col(org_members::Column::Id).count(), "count")
        .group_by(org_members::Column::OrgId)
        .into_model::<OrgCountRow>()
        .all(db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to count members per org: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(rows.into_iter().map(|r| (r.org_id, r.count)).collect())
}

async fn count_workspaces_per_org(
    db: &sea_orm::DatabaseConnection,
    org_ids: &[Uuid],
) -> Result<HashMap<Uuid, i64>, StatusCode> {
    let rows: Vec<OrgCountRow> = Workspaces::find()
        .filter(workspaces::Column::OrgId.is_in(org_ids.to_vec()))
        .select_only()
        .column(workspaces::Column::OrgId)
        .column_as(Expr::col(workspaces::Column::Id).count(), "count")
        .group_by(workspaces::Column::OrgId)
        .into_model::<OrgCountRow>()
        .all(db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to count workspaces per org: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(rows.into_iter().map(|r| (r.org_id, r.count)).collect())
}

/// GET /orgs/:org_id
pub async fn get_org(
    OrgContextExtractor(ctx): OrgContextExtractor,
) -> Result<Json<OrgResponse>, StatusCode> {
    Ok(Json(org_response(&ctx.org, &ctx.membership.role)))
}

/// PATCH /orgs/:org_id
pub async fn update_org(
    OrgAdmin(ctx): OrgAdmin,
    Json(req): Json<UpdateOrgRequest>,
) -> Result<Json<OrgResponse>, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("DB connection error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut active: organizations::ActiveModel = ctx.org.clone().into();
    if let Some(name) = req.name {
        active.name = ActiveValue::Set(name);
    }
    if let Some(slug) = req.slug {
        let normalized = slugify_name(&slug);
        if normalized.is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
        if is_reserved_slug(&normalized) {
            // See create_org — 422 distinguishes "forbidden name" from a real
            // slug-already-taken collision.
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
        active.slug = ActiveValue::Set(normalized);
    }
    active.updated_at = ActiveValue::Set(Utc::now().fixed_offset());

    let updated = active.update(&db).await.map_err(|e| {
        let msg = e.to_string();
        if msg.contains("unique") || msg.contains("duplicate") {
            tracing::warn!("Slug uniqueness conflict on update (caught at DB level): {e}");
            return StatusCode::CONFLICT;
        }
        tracing::error!("Failed to update organization: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(org_response(&updated, &ctx.membership.role)))
}

/// DELETE /orgs/:org_id
pub async fn delete_org(OrgOwner(ctx): OrgOwner) -> Result<StatusCode, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("DB connection error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Deprovision before delete so we hit Airhouse with the local row still
    // present. The FK is ON DELETE CASCADE, so even if this fails the local
    // row is cleaned up by the org delete below.
    if let Some(provisioner) = airhouse::provisioner_for(db.clone())
        && let Err(e) = provisioner.deprovision(ctx.org.id).await
    {
        tracing::warn!(org_id = %ctx.org.id, "airhouse tenant deprovisioning failed: {e}");
    }

    Organizations::delete_by_id(ctx.org.id)
        .exec(&db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete organization: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Members management
// ---------------------------------------------------------------------------

/// GET /orgs/:org_id/members
pub async fn list_members(
    OrgContextExtractor(ctx): OrgContextExtractor,
) -> Result<Json<Vec<MemberResponse>>, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("DB connection error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let members = OrgMembers::find()
        .filter(org_members::Column::OrgId.eq(ctx.org.id))
        .find_also_related(Users)
        .all(&db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query members: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let responses: Vec<MemberResponse> = members
        .into_iter()
        .filter_map(|(member, user)| {
            let user = user?;
            Some(MemberResponse {
                id: member.id,
                user_id: member.user_id,
                email: user.email,
                name: user.name,
                role: member.role.as_str().to_string(),
                created_at: member.created_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(responses))
}

/// PATCH /orgs/:org_id/members/:user_id
pub async fn update_member_role(
    OrgAdmin(ctx): OrgAdmin,
    Path((_org_id, target_user_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<UpdateRoleRequest>,
) -> Result<Json<MemberResponse>, StatusCode> {
    // Owners cannot demote themselves — use "leave org" or transfer ownership first.
    if target_user_id == ctx.membership.user_id && ctx.membership.role == OrgRole::Owner {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Validate the new role.
    let new_role = OrgRole::from_str(&req.role).map_err(|_| StatusCode::BAD_REQUEST)?;

    // Only Owners can grant the Owner role.
    if new_role == OrgRole::Owner && ctx.membership.role != OrgRole::Owner {
        return Err(StatusCode::FORBIDDEN);
    }

    let db = establish_connection().await.map_err(|e| {
        tracing::error!("DB connection error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Transaction with row-level locks on the org's owner rows prevents a
    // TOCTOU where two concurrent demotions both see owner_count > 1 and
    // drop the last owner.
    let txn = db.begin().await.map_err(|e| {
        tracing::error!("Failed to start transaction: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let target = OrgMembers::find()
        .filter(org_members::Column::OrgId.eq(ctx.org.id))
        .filter(org_members::Column::UserId.eq(target_user_id))
        .lock_exclusive()
        .one(&txn)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query target member: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Only Owners can demote other Admins or Owners.
    if matches!(target.role, OrgRole::Owner | OrgRole::Admin)
        && ctx.membership.role != OrgRole::Owner
    {
        return Err(StatusCode::FORBIDDEN);
    }

    // Cannot change the last owner's role. The lock_exclusive() above holds
    // an exclusive lock on the target row through commit; locking all other
    // owners here prevents concurrent owner-removing transactions from racing.
    if target.role == OrgRole::Owner && new_role != OrgRole::Owner {
        let owner_count = OrgMembers::find()
            .filter(org_members::Column::OrgId.eq(ctx.org.id))
            .filter(org_members::Column::Role.eq(OrgRole::Owner))
            .lock_exclusive()
            .count(&txn)
            .await
            .map_err(|e| {
                tracing::error!("Failed to count owners: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        if owner_count <= 1 {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    let mut active: org_members::ActiveModel = target.into();
    active.role = ActiveValue::Set(new_role);
    active.updated_at = ActiveValue::Set(Utc::now().fixed_offset());
    let updated = active.update(&txn).await.map_err(|e| {
        tracing::error!("Failed to update member role: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let user = Users::find_by_id(target_user_id)
        .one(&txn)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query user: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    txn.commit().await.map_err(|e| {
        tracing::error!("Failed to commit transaction: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(MemberResponse {
        id: updated.id,
        user_id: updated.user_id,
        email: user.email,
        name: user.name,
        role: updated.role.as_str().to_string(),
        created_at: updated.created_at.to_rfc3339(),
    }))
}

/// DELETE /orgs/:org_id/members/:user_id
pub async fn remove_member(
    OrgAdmin(ctx): OrgAdmin,
    Path((_org_id, target_user_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    // Owners cannot remove themselves — use a dedicated "leave org" flow.
    if target_user_id == ctx.membership.user_id && ctx.membership.role == OrgRole::Owner {
        return Err(StatusCode::BAD_REQUEST);
    }

    let db = establish_connection().await.map_err(|e| {
        tracing::error!("DB connection error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Transaction with row-level locks on the org's owner rows prevents a
    // TOCTOU where two concurrent removals both see owner_count > 1 and
    // delete the last owner.
    let txn = db.begin().await.map_err(|e| {
        tracing::error!("Failed to start transaction: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let target = OrgMembers::find()
        .filter(org_members::Column::OrgId.eq(ctx.org.id))
        .filter(org_members::Column::UserId.eq(target_user_id))
        .lock_exclusive()
        .one(&txn)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query target member: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Only Owners can remove other Owners or Admins.
    if matches!(target.role, OrgRole::Owner | OrgRole::Admin)
        && ctx.membership.role != OrgRole::Owner
    {
        return Err(StatusCode::FORBIDDEN);
    }

    // Cannot remove the last owner.
    if target.role == OrgRole::Owner {
        let owner_count = OrgMembers::find()
            .filter(org_members::Column::OrgId.eq(ctx.org.id))
            .filter(org_members::Column::Role.eq(OrgRole::Owner))
            .lock_exclusive()
            .count(&txn)
            .await
            .map_err(|e| {
                tracing::error!("Failed to count owners: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        if owner_count <= 1 {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    // Remove workspace-level role overrides for this user scoped to workspaces
    // in this org, so a re-invite doesn't silently reactivate stale overrides.
    {
        use entity::prelude::{WorkspaceMembers, Workspaces};
        use entity::workspace_members::Column as WsMemberCol;
        use entity::workspaces::Column as WorkspaceCol;

        let workspace_ids: Vec<Uuid> = Workspaces::find()
            .filter(WorkspaceCol::OrgId.eq(ctx.org.id))
            .select_only()
            .column(WorkspaceCol::Id)
            .into_tuple()
            .all(&txn)
            .await
            .map_err(|e| {
                tracing::error!("Failed to list org workspaces: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        if !workspace_ids.is_empty() {
            WorkspaceMembers::delete_many()
                .filter(WsMemberCol::UserId.eq(target_user_id))
                .filter(WsMemberCol::WorkspaceId.is_in(workspace_ids))
                .exec(&txn)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to delete workspace overrides: {e}");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
        }
    }

    let active: org_members::ActiveModel = target.into();
    active.delete(&txn).await.map_err(|e| {
        tracing::error!("Failed to remove member: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    txn.commit().await.map_err(|e| {
        tracing::error!("Failed to commit transaction: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Sync Stripe seat quantity (decrement). Same best-effort pattern as
    // accept_invitation — reconciliation catches any failure.
    if let Ok(svc) = crate::api::billing::billing_service().await {
        let org_id_bg = ctx.org.id;
        tokio::spawn(async move {
            if let Err(e) = svc.sync_seats(org_id_bg).await {
                tracing::warn!(?e, ?org_id_bg, "sync_seats failed after remove_member");
            }
        });
    }

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Invitations
// ---------------------------------------------------------------------------

/// POST /orgs/:org_id/invitations
pub async fn create_invitation(
    OrgAdmin(ctx): OrgAdmin,
    headers: HeaderMap,
    Json(req): Json<InviteRequest>,
) -> Result<Json<InvitationResponse>, StatusCode> {
    // Validate role.
    let role = OrgRole::from_str(&req.role).map_err(|_| StatusCode::BAD_REQUEST)?;

    // Only Owners can invite with the Owner role.
    if role == OrgRole::Owner && ctx.membership.role != OrgRole::Owner {
        return Err(StatusCode::FORBIDDEN);
    }

    // Normalize + format-check the invited email. Lowercased for
    // case-insensitive comparisons against existing rows.
    let invited_email = normalize_invite_email(&req.email)?;

    let db = establish_connection().await.map_err(|e| {
        tracing::error!("DB connection error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Reject if the email already belongs to a member of this org.
    if let Some(existing_user) = Users::find()
        .filter_by_email(&invited_email)
        .one(&db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to lookup invited user: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    {
        let already_member = OrgMembers::find()
            .filter(org_members::Column::OrgId.eq(ctx.org.id))
            .filter(org_members::Column::UserId.eq(existing_user.id))
            .one(&db)
            .await
            .map_err(|e| {
                tracing::error!("Failed to check existing membership: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        if already_member.is_some() {
            return Err(StatusCode::CONFLICT);
        }
    }

    // Reject duplicate pending invitations for the same email.
    let existing = OrgInvitations::find()
        .filter(org_invitations::Column::OrgId.eq(ctx.org.id))
        .filter(org_invitations::Column::Email.eq(&invited_email))
        .filter(org_invitations::Column::Status.eq(InviteStatus::Pending))
        .one(&db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to check existing invitation: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if existing.is_some() {
        return Err(StatusCode::CONFLICT);
    }

    let now = Utc::now().fixed_offset();
    let token = Uuid::new_v4().to_string();
    let expires_at = (Utc::now() + chrono::Duration::days(7)).fixed_offset();

    let invitation = org_invitations::ActiveModel {
        id: ActiveValue::Set(Uuid::new_v4()),
        org_id: ActiveValue::Set(ctx.org.id),
        email: ActiveValue::Set(invited_email.clone()),
        role: ActiveValue::Set(role),
        invited_by: ActiveValue::Set(ctx.membership.user_id),
        token: ActiveValue::Set(token),
        status: ActiveValue::Set(InviteStatus::Pending),
        expires_at: ActiveValue::Set(expires_at),
        created_at: ActiveValue::Set(now),
    };
    let invitation = invitation.insert(&db).await.map_err(|e| {
        tracing::error!("Failed to insert invitation: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Fire off the invitation email in the background. Failure to send does not
    // block the response — the DB row + returned token remain the source of truth.
    let inviter = Users::find_by_id(ctx.membership.user_id)
        .one(&db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to lookup inviter: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let base_url = super::auth::extract_base_url_from_headers(&headers);
    let inviter_name = inviter
        .as_ref()
        .map(|u| u.name.clone())
        .unwrap_or_else(|| "A teammate".to_string());
    let inviter_email = inviter
        .as_ref()
        .map(|u| u.email.clone())
        .unwrap_or_default();
    let to_email = invitation.email.clone();
    let token_clone = invitation.token.clone();
    let org_name = ctx.org.name.clone();
    tokio::spawn(async move {
        if let Err(e) = send_invitation_email(
            &to_email,
            &token_clone,
            &base_url,
            &inviter_name,
            &inviter_email,
            &org_name,
        )
        .await
        {
            tracing::error!("Failed to send invitation email: {e}");
        }
    });

    Ok(Json(InvitationResponse {
        id: invitation.id,
        email: invitation.email,
        role: invitation.role.as_str().to_string(),
        token: invitation.token,
        status: invitation.status.as_str().to_string(),
        expires_at: invitation.expires_at.to_rfc3339(),
        created_at: invitation.created_at.to_rfc3339(),
    }))
}

/// POST /orgs/:org_id/invitations/bulk
///
/// All-or-nothing: every invitation is validated and inserted in a single
/// transaction. If any one fails (duplicate email, already a member, role
/// violation), the whole batch is rolled back so the caller never has to
/// reason about partial state. Emails are spawned only after a successful
/// commit.
pub async fn create_bulk_invitations(
    OrgAdmin(ctx): OrgAdmin,
    headers: HeaderMap,
    Json(req): Json<BulkInviteRequest>,
) -> Result<Json<BulkInviteResponse>, StatusCode> {
    if req.invitations.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    // Guard against pathological payloads (e.g. a misconfigured CSV import)
    // landing a single request big enough to block the connection while we
    // validate + write thousands of rows in one transaction.
    const MAX_BULK_INVITES: usize = 50;
    if req.invitations.len() > MAX_BULK_INVITES {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }

    // Pre-validate every entry (role parse, owner-grant restriction, non-empty
    // email) and normalize emails. Reject the whole request on any input
    // error before touching the DB. Also catches dupes inside the same batch.
    let mut prepared: Vec<(String, OrgRole)> = Vec::with_capacity(req.invitations.len());
    let mut seen_emails: std::collections::HashSet<String> = std::collections::HashSet::new();
    for inv in &req.invitations {
        let role = OrgRole::from_str(&inv.role).map_err(|_| StatusCode::BAD_REQUEST)?;
        if role == OrgRole::Owner && ctx.membership.role != OrgRole::Owner {
            return Err(StatusCode::FORBIDDEN);
        }
        let email = normalize_invite_email(&inv.email)?;
        if !seen_emails.insert(email.clone()) {
            return Err(StatusCode::CONFLICT);
        }
        prepared.push((email, role));
    }

    let db = establish_connection().await.map_err(|e| {
        tracing::error!("DB connection error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let txn = db.begin().await.map_err(|e| {
        tracing::error!("Failed to begin transaction: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let now = Utc::now().fixed_offset();
    let expires_at = (Utc::now() + chrono::Duration::days(7)).fixed_offset();
    let mut inserted: Vec<org_invitations::Model> = Vec::with_capacity(prepared.len());

    for (email, role) in prepared {
        // Reject if the email already belongs to a member of this org.
        if let Some(existing_user) = Users::find()
            .filter_by_email(&email)
            .one(&txn)
            .await
            .map_err(|e| {
                tracing::error!("Failed to lookup invited user: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?
        {
            let already_member = OrgMembers::find()
                .filter(org_members::Column::OrgId.eq(ctx.org.id))
                .filter(org_members::Column::UserId.eq(existing_user.id))
                .one(&txn)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to check existing membership: {e}");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
            if already_member.is_some() {
                return Err(StatusCode::CONFLICT);
            }
        }

        // Reject duplicate pending invitations for the same email.
        let existing = OrgInvitations::find()
            .filter(org_invitations::Column::OrgId.eq(ctx.org.id))
            .filter(org_invitations::Column::Email.eq(&email))
            .filter(org_invitations::Column::Status.eq(InviteStatus::Pending))
            .one(&txn)
            .await
            .map_err(|e| {
                tracing::error!("Failed to check existing invitation: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        if existing.is_some() {
            return Err(StatusCode::CONFLICT);
        }

        let invitation = org_invitations::ActiveModel {
            id: ActiveValue::Set(Uuid::new_v4()),
            org_id: ActiveValue::Set(ctx.org.id),
            email: ActiveValue::Set(email),
            role: ActiveValue::Set(role),
            invited_by: ActiveValue::Set(ctx.membership.user_id),
            token: ActiveValue::Set(Uuid::new_v4().to_string()),
            status: ActiveValue::Set(InviteStatus::Pending),
            expires_at: ActiveValue::Set(expires_at),
            created_at: ActiveValue::Set(now),
        };
        let invitation = invitation.insert(&txn).await.map_err(|e| {
            tracing::error!("Failed to insert invitation: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        inserted.push(invitation);
    }

    // Look up inviter on the same transaction so we have it for email dispatch.
    let inviter = Users::find_by_id(ctx.membership.user_id)
        .one(&txn)
        .await
        .map_err(|e| {
            tracing::error!("Failed to lookup inviter: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    txn.commit().await.map_err(|e| {
        tracing::error!("Failed to commit transaction: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let base_url = super::auth::extract_base_url_from_headers(&headers);
    let inviter_name = inviter
        .as_ref()
        .map(|u| u.name.clone())
        .unwrap_or_else(|| "A teammate".to_string());
    let inviter_email = inviter
        .as_ref()
        .map(|u| u.email.clone())
        .unwrap_or_default();
    let org_name = ctx.org.name.clone();
    for invitation in &inserted {
        let to_email = invitation.email.clone();
        let token = invitation.token.clone();
        let base_url = base_url.clone();
        let inviter_name = inviter_name.clone();
        let inviter_email = inviter_email.clone();
        let org_name = org_name.clone();
        tokio::spawn(async move {
            if let Err(e) = send_invitation_email(
                &to_email,
                &token,
                &base_url,
                &inviter_name,
                &inviter_email,
                &org_name,
            )
            .await
            {
                tracing::error!("Failed to send invitation email: {e}");
            }
        });
    }

    let invitations: Vec<InvitationResponse> = inserted
        .into_iter()
        .map(|inv| InvitationResponse {
            id: inv.id,
            email: inv.email,
            role: inv.role.as_str().to_string(),
            token: inv.token,
            status: inv.status.as_str().to_string(),
            expires_at: inv.expires_at.to_rfc3339(),
            created_at: inv.created_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(BulkInviteResponse { invitations }))
}

/// GET /orgs/:org_id/invitations
pub async fn list_invitations(
    OrgAdmin(ctx): OrgAdmin,
) -> Result<Json<Vec<InvitationSummary>>, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("DB connection error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let invitations = OrgInvitations::find()
        .filter(org_invitations::Column::OrgId.eq(ctx.org.id))
        .filter(org_invitations::Column::Status.eq(InviteStatus::Pending))
        .filter(org_invitations::Column::ExpiresAt.gt(Utc::now().fixed_offset()))
        .all(&db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query invitations: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let responses: Vec<InvitationSummary> = invitations
        .into_iter()
        .map(|inv| InvitationSummary {
            id: inv.id,
            email: inv.email,
            role: inv.role.as_str().to_string(),
            token: inv.token,
            status: inv.status.as_str().to_string(),
            expires_at: inv.expires_at.to_rfc3339(),
            created_at: inv.created_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(responses))
}

/// DELETE /orgs/:org_id/invitations/:invitation_id
pub async fn revoke_invitation(
    OrgAdmin(ctx): OrgAdmin,
    Path((_org_id, invitation_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("DB connection error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let invitation = OrgInvitations::find_by_id(invitation_id)
        .filter(org_invitations::Column::OrgId.eq(ctx.org.id))
        .one(&db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query invitation: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let active: org_invitations::ActiveModel = invitation.into();
    active.delete(&db).await.map_err(|e| {
        tracing::error!("Failed to delete invitation: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /invitations/mine — list non-expired, pending invitations addressed to
/// the authenticated user's email. Powers the post-login "you've been invited"
/// screen shown before the org-creation onboarding step.
pub async fn list_my_invitations(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
) -> Result<Json<Vec<MyInvitationResponse>>, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("DB connection error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Stored invitation emails are normalized to lowercase at insert time
    // (see create_invitation), so a plain equality match on the lowercased
    // user email covers RFC 5321 §2.4 case-insensitivity without needing
    // a LOWER(...) wrap in SQL.
    let email_lower = user.email.to_lowercase();
    let invitations = OrgInvitations::find()
        .filter(org_invitations::Column::Email.eq(email_lower))
        .filter(org_invitations::Column::Status.eq(InviteStatus::Pending))
        .filter(org_invitations::Column::ExpiresAt.gt(Utc::now().fixed_offset()))
        // Most-recently-sent invites appear first; stable order across refetches
        // avoids the list jittering between renders when users have several.
        .order_by_desc(org_invitations::Column::CreatedAt)
        .all(&db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query pending invitations: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if invitations.is_empty() {
        return Ok(Json(Vec::new()));
    }

    // Batch-load referenced orgs + inviters in one query each — avoids N+1.
    let org_ids: Vec<Uuid> = invitations.iter().map(|i| i.org_id).collect();
    let inviter_ids: Vec<Uuid> = invitations.iter().map(|i| i.invited_by).collect();

    let orgs = Organizations::find()
        .filter(organizations::Column::Id.is_in(org_ids))
        .all(&db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load orgs for invitations: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let org_map: HashMap<Uuid, organizations::Model> =
        orgs.into_iter().map(|o| (o.id, o)).collect();

    let inviters = Users::find()
        .filter(entity::users::Column::Id.is_in(inviter_ids))
        .all(&db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load inviters: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let inviter_name_map: HashMap<Uuid, String> = inviters
        .into_iter()
        .map(|u| {
            let display = if u.name.is_empty() { u.email } else { u.name };
            (u.id, display)
        })
        .collect();

    let responses: Vec<MyInvitationResponse> = invitations
        .into_iter()
        .filter_map(|inv| {
            let org = org_map.get(&inv.org_id)?;
            Some(MyInvitationResponse {
                id: inv.id,
                token: inv.token,
                role: inv.role.as_str().to_string(),
                expires_at: inv.expires_at.to_rfc3339(),
                created_at: inv.created_at.to_rfc3339(),
                org_id: org.id,
                org_name: org.name.clone(),
                org_slug: org.slug.clone(),
                invited_by_name: inviter_name_map.get(&inv.invited_by).cloned(),
            })
        })
        .collect();

    Ok(Json(responses))
}

/// POST /invitations/:token/accept
pub async fn accept_invitation(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Path(token): Path<String>,
) -> Result<Json<OrgResponse>, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("DB connection error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let invitation = OrgInvitations::find()
        .filter(org_invitations::Column::Token.eq(&token))
        .one(&db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query invitation: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Verify the invitation was sent to this user's email (case-insensitive per RFC 5321).
    if invitation.email.to_lowercase() != user.email.to_lowercase() {
        return Err(StatusCode::FORBIDDEN);
    }

    // Check status and expiration.
    if invitation.status != InviteStatus::Pending {
        return Err(StatusCode::BAD_REQUEST);
    }
    if invitation.expires_at < Utc::now() {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Check if user is already a member.
    let existing_member = OrgMembers::find()
        .filter(org_members::Column::OrgId.eq(invitation.org_id))
        .filter(org_members::Column::UserId.eq(user.id))
        .one(&db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to check existing membership: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if existing_member.is_some() {
        return Err(StatusCode::CONFLICT);
    }

    // Create membership and mark invitation as accepted atomically.
    let txn = db.begin().await.map_err(|e| {
        tracing::error!("Failed to begin transaction: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let now = Utc::now().fixed_offset();
    let member = org_members::ActiveModel {
        id: ActiveValue::Set(Uuid::new_v4()),
        org_id: ActiveValue::Set(invitation.org_id),
        user_id: ActiveValue::Set(user.id),
        role: ActiveValue::Set(invitation.role.clone()),
        created_at: ActiveValue::Set(now),
        updated_at: ActiveValue::Set(now),
    };
    member.insert(&txn).await.map_err(|e| {
        tracing::error!("Failed to create membership: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut inv_active: org_invitations::ActiveModel = invitation.clone().into();
    inv_active.status = ActiveValue::Set(InviteStatus::Accepted);
    inv_active.update(&txn).await.map_err(|e| {
        tracing::error!("Failed to update invitation status: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    txn.commit().await.map_err(|e| {
        tracing::error!("Failed to commit transaction: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Sync the Stripe seat quantity to reflect the newly added member. Spawned
    // so the user-facing request doesn't block on Stripe; the reconciliation
    // loop is the safety net if this fails.
    if let Ok(svc) = crate::api::billing::billing_service().await {
        let org_id_bg = invitation.org_id;
        tokio::spawn(async move {
            if let Err(e) = svc.sync_seats(org_id_bg).await {
                tracing::warn!(?e, ?org_id_bg, "sync_seats failed after accept_invitation");
            }
        });
    }

    // Return org details.
    let org = Organizations::find_by_id(invitation.org_id)
        .one(&db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query organization: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(org_response(&org, &invitation.role)))
}

// ---------------------------------------------------------------------------
// Invitation email
// ---------------------------------------------------------------------------

static INVITATION_TEMPLATE: Lazy<Handlebars<'static>> = Lazy::new(|| {
    let mut hbs = Handlebars::new();
    hbs.register_template_string("invitation", include_str!("../../emails/invitation.hbs"))
        .expect("invitation.hbs is valid");
    hbs
});

/// Sends an invitation email. Piggybacks on the magic-link SES config for the
/// sender identity; if magic-link auth is not configured, this is a no-op so
/// the admin can still share the copy-able invite token manually.
async fn send_invitation_email(
    to_email: &str,
    token: &str,
    base_url: &str,
    inviter_name: &str,
    inviter_email: &str,
    org_name: &str,
) -> Result<(), OxyError> {
    use crate::emails::{
        EmailMessage, EmailProvider, local_test::LocalTestEmailProvider, ses::SesEmailProvider,
    };

    let magic_link_config = oxy::config::oxy::get_oxy_config()
        .ok()
        .and_then(|c| c.authentication)
        .and_then(|a| a.magic_link);

    let Some(config) = magic_link_config else {
        tracing::warn!(
            "Invitation email not sent — magic-link email config missing. Token can still be shared via the Pending Invitations UI."
        );
        return Ok(());
    };

    let invite_url = format!("{base_url}/invite/{token}");
    let subject = format!("You've been invited to {org_name} on Oxygen");
    let text_body = format!(
        "{inviter_name} ({inviter_email}) has invited you to join {org_name} on Oxygen.\n\nAccept the invitation:\n{invite_url}\n\nThis invitation expires in 7 days. If you weren't expecting this, you can safely ignore this email."
    );
    let message = EmailMessage {
        subject,
        html_body: build_invitation_email_html(
            &invite_url,
            to_email,
            inviter_name,
            inviter_email,
            org_name,
        )?,
        text_body,
    };

    if std::env::var("MAGIC_LINK_LOCAL_TEST").is_ok() {
        LocalTestEmailProvider
            .send(&config.from_email, to_email, message)
            .await
    } else {
        SesEmailProvider::new(config.aws_region.as_deref())
            .await
            .send(&config.from_email, to_email, message)
            .await
    }
}

fn build_invitation_email_html(
    invite_url: &str,
    to_email: &str,
    inviter_name: &str,
    inviter_email: &str,
    org_name: &str,
) -> Result<String, OxyError> {
    let data = serde_json::json!({
        "invite_url": invite_url,
        "to_email": to_email,
        "invited_by_name": inviter_name,
        "invited_by_email": inviter_email,
        "org_name": org_name,
        "year": Utc::now().format("%Y").to_string(),
    });

    INVITATION_TEMPLATE
        .render("invitation", &data)
        .map_err(|e| OxyError::RuntimeError(format!("Failed to render invitation template: {e}")))
}

#[cfg(test)]
#[path = "organizations_tests.rs"]
mod organizations_tests;
