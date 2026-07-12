//! `/api/admin/users/*` — OXY_OWNER-only directory across all tenants.
//!
//! Surfaces every user in the database, regardless of org, so operators
//! can search by email, inspect their org memberships, and run targeted
//! actions (deactivate, change role, remove from org). Sits behind
//! `oxy_owner_guard_middleware`.

use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use chrono::Utc;
use entity::org_members::OrgRole;
use entity::users::UserStatus;
use entity::{app_admins, org_members, organizations, users, workspace_members};
use oxy::database::client::establish_connection;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseBackend, EntityTrait, FromQueryResult,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, Statement, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::server::router::AppState;

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/users", get(list_users))
        .route("/users/{user_id}", get(get_user_detail))
        .route("/users/{user_id}/status", patch(set_user_status))
        .route(
            "/users/{user_id}/org-memberships",
            post(add_to_org).get(list_org_memberships),
        )
        .route(
            "/users/{user_id}/org-memberships/{org_id}",
            patch(update_role).delete(remove_from_org),
        )
}

#[derive(Serialize)]
pub struct AdminUserRow {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub status: String,
    pub created_at: String,
    pub last_login_at: String,
    pub is_app_admin: bool,
    pub org_count: i64,
}

#[derive(Serialize)]
pub struct AdminUserDetail {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub status: String,
    pub created_at: String,
    pub last_login_at: String,
    pub picture: Option<String>,
    pub email_verified: bool,
    pub is_app_admin: bool,
    pub org_memberships: Vec<UserOrgMembership>,
    pub workspace_memberships: Vec<UserWorkspaceMembership>,
}

#[derive(Serialize)]
pub struct UserOrgMembership {
    pub org_id: Uuid,
    pub org_slug: String,
    pub org_name: String,
    pub role: String,
    pub joined_at: String,
}

#[derive(Serialize)]
pub struct UserWorkspaceMembership {
    pub workspace_id: Uuid,
    pub workspace_name: String,
    pub role: String,
    pub joined_at: String,
}

#[derive(Deserialize)]
pub struct ListUsersQuery {
    pub search: Option<String>,
    pub status: Option<String>,
    pub page: Option<u64>,
    pub page_size: Option<u64>,
}

#[derive(Deserialize)]
pub struct SetStatusBody {
    pub status: String,
}

#[derive(Deserialize)]
pub struct AddToOrgBody {
    pub org_id: Uuid,
    pub role: String,
}

#[derive(Deserialize)]
pub struct UpdateRoleBody {
    pub role: String,
}

pub async fn list_users(
    Query(q): Query<ListUsersQuery>,
) -> Result<Json<Vec<AdminUserRow>>, StatusCode> {
    let db = establish_connection().await.map_err(internal)?;
    let page = q.page.unwrap_or(0);
    let page_size = q.page_size.unwrap_or(50).min(200);

    let mut query = users::Entity::find().order_by_desc(users::Column::LastLoginAt);

    if let Some(needle) = q.search.as_ref().filter(|s| !s.trim().is_empty()) {
        let like = format!("%{}%", needle.trim());
        query = query.filter(
            sea_orm::Condition::any()
                .add(users::Column::Email.contains(like.as_str()))
                .add(users::Column::Name.contains(like.as_str())),
        );
    }
    if let Some(status_str) = q.status.as_ref().filter(|s| !s.is_empty()) {
        let status =
            UserStatus::from_str(status_str.as_str()).map_err(|_| StatusCode::BAD_REQUEST)?;
        query = query.filter(users::Column::Status.eq(status));
    }

    let rows = query
        .offset(page * page_size)
        .limit(page_size)
        .all(&db)
        .await
        .map_err(internal)?;

    // Pre-aggregate the per-row lookups: one IN (...) GROUP BY query for
    // org_count, one IN (...) query for the is_app_admin set. Avoids the
    // O(N) round-trips the previous loop hit on every page render.
    let user_ids: Vec<Uuid> = rows.iter().map(|u| u.id).collect();
    let emails_lower: Vec<String> = rows.iter().map(|u| u.email.to_ascii_lowercase()).collect();
    let org_counts = count_user_org_memberships_in(&db, &user_ids)
        .await
        .map_err(internal)?;
    let app_admin_set = lookup_app_admin_emails_in(&db, &emails_lower)
        .await
        .map_err(internal)?;

    let mut out = Vec::with_capacity(rows.len());
    for u in rows {
        let lc_email = u.email.to_ascii_lowercase();
        out.push(AdminUserRow {
            id: u.id,
            email: u.email,
            name: u.name,
            status: u.status.as_str().to_string(),
            created_at: u.created_at.to_rfc3339(),
            last_login_at: u.last_login_at.to_rfc3339(),
            is_app_admin: app_admin_set.contains(&lc_email),
            org_count: org_counts.get(&u.id).copied().unwrap_or(0),
        });
    }

    Ok(Json(out))
}

#[derive(FromQueryResult)]
struct UserIdCountRow {
    user_id: Uuid,
    cnt: i64,
}

#[derive(FromQueryResult)]
struct EmailRow {
    email: String,
}

async fn count_user_org_memberships_in(
    db: &sea_orm::DatabaseConnection,
    user_ids: &[Uuid],
) -> Result<HashMap<Uuid, i64>, sea_orm::DbErr> {
    if user_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let placeholders = sql_placeholders(user_ids.len());
    let sql = format!(
        "SELECT user_id, COUNT(*)::bigint AS cnt FROM org_members \
         WHERE user_id IN ({placeholders}) GROUP BY user_id"
    );
    let values: Vec<sea_orm::Value> = user_ids.iter().map(|id| (*id).into()).collect();
    let rows = UserIdCountRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        values,
    ))
    .all(db)
    .await?;
    Ok(rows.into_iter().map(|r| (r.user_id, r.cnt)).collect())
}

async fn lookup_app_admin_emails_in(
    db: &sea_orm::DatabaseConnection,
    emails: &[String],
) -> Result<HashSet<String>, sea_orm::DbErr> {
    if emails.is_empty() {
        return Ok(HashSet::new());
    }
    let placeholders = sql_placeholders(emails.len());
    let sql = format!("SELECT DISTINCT email FROM app_admins WHERE email IN ({placeholders})");
    let values: Vec<sea_orm::Value> = emails.iter().map(|e| e.clone().into()).collect();
    let rows = EmailRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        values,
    ))
    .all(db)
    .await?;
    Ok(rows.into_iter().map(|r| r.email).collect())
}

fn sql_placeholders(n: usize) -> String {
    (1..=n)
        .map(|i| format!("${i}"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub async fn get_user_detail(
    Path(user_id): Path<Uuid>,
) -> Result<Json<AdminUserDetail>, StatusCode> {
    let db = establish_connection().await.map_err(internal)?;
    let user = users::Entity::find_by_id(user_id)
        .one(&db)
        .await
        .map_err(internal)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let is_app_admin = app_admins::Entity::find()
        .filter(app_admins::Column::Email.eq(user.email.to_ascii_lowercase()))
        .count(&db)
        .await
        .map_err(internal)?
        > 0;

    let org_memberships = load_user_org_memberships(&db, user.id)
        .await
        .map_err(internal)?;
    let workspace_memberships = load_user_workspace_memberships(&db, user.id)
        .await
        .map_err(internal)?;

    Ok(Json(AdminUserDetail {
        id: user.id,
        email: user.email,
        name: user.name,
        status: user.status.as_str().to_string(),
        created_at: user.created_at.to_rfc3339(),
        last_login_at: user.last_login_at.to_rfc3339(),
        picture: user.picture,
        email_verified: user.email_verified,
        is_app_admin,
        org_memberships,
        workspace_memberships,
    }))
}

pub async fn list_org_memberships(
    Path(user_id): Path<Uuid>,
) -> Result<Json<Vec<UserOrgMembership>>, StatusCode> {
    let db = establish_connection().await.map_err(internal)?;
    let memberships = load_user_org_memberships(&db, user_id)
        .await
        .map_err(internal)?;
    Ok(Json(memberships))
}

pub async fn set_user_status(
    AuthenticatedUserExtractor(actor): AuthenticatedUserExtractor,
    Path(user_id): Path<Uuid>,
    Json(body): Json<SetStatusBody>,
) -> Result<StatusCode, StatusCode> {
    let new_status =
        UserStatus::from_str(body.status.as_str()).map_err(|_| StatusCode::BAD_REQUEST)?;
    let db = establish_connection().await.map_err(internal)?;
    let user = users::Entity::find_by_id(user_id)
        .one(&db)
        .await
        .map_err(internal)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let mut active: users::ActiveModel = user.into();
    active.status = Set(new_status);
    active.update(&db).await.map_err(internal)?;
    tracing::info!(
        admin_email = %actor.email,
        target_id = %user_id,
        new_status = %body.status,
        action = "set_user_status",
        "admin tenant action"
    );
    Ok(StatusCode::NO_CONTENT)
}

pub async fn add_to_org(
    Path(user_id): Path<Uuid>,
    Json(body): Json<AddToOrgBody>,
) -> Result<StatusCode, StatusCode> {
    let role = OrgRole::from_str(body.role.as_str()).map_err(|_| StatusCode::BAD_REQUEST)?;
    let db = establish_connection().await.map_err(internal)?;

    users::Entity::find_by_id(user_id)
        .one(&db)
        .await
        .map_err(internal)?
        .ok_or(StatusCode::NOT_FOUND)?;
    organizations::Entity::find_by_id(body.org_id)
        .one(&db)
        .await
        .map_err(internal)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let existing = org_members::Entity::find()
        .filter(org_members::Column::OrgId.eq(body.org_id))
        .filter(org_members::Column::UserId.eq(user_id))
        .one(&db)
        .await
        .map_err(internal)?;
    if existing.is_some() {
        return Err(StatusCode::CONFLICT);
    }

    let now = Utc::now().fixed_offset();
    let model = org_members::ActiveModel {
        id: ActiveValue::Set(Uuid::new_v4()),
        org_id: Set(body.org_id),
        user_id: Set(user_id),
        role: Set(role),
        created_at: Set(now),
        updated_at: Set(now),
    };
    model.insert(&db).await.map_err(internal)?;
    Ok(StatusCode::CREATED)
}

pub async fn update_role(
    AuthenticatedUserExtractor(actor): AuthenticatedUserExtractor,
    Path((user_id, org_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<UpdateRoleBody>,
) -> Result<StatusCode, Response> {
    let role = OrgRole::from_str(body.role.as_str())
        .map_err(|_| StatusCode::BAD_REQUEST.into_response())?;
    let db = establish_connection().await.map_err(internal_resp)?;
    let tx = db.begin().await.map_err(internal_resp)?;

    let membership = org_members::Entity::find()
        .filter(org_members::Column::OrgId.eq(org_id))
        .filter(org_members::Column::UserId.eq(user_id))
        .one(&tx)
        .await
        .map_err(internal_resp)?
        .ok_or_else(|| StatusCode::NOT_FOUND.into_response())?;

    // Last-owner guard: refuse to demote the only Owner of the org. The
    // owner_count read sits inside the same tx as the role update, so we
    // never race against a concurrent demote/remove.
    let target_was_owner = matches!(membership.role, OrgRole::Owner);
    let demoting = target_was_owner && !matches!(role, OrgRole::Owner);
    if demoting {
        let owner_count = org_members::Entity::find()
            .filter(org_members::Column::OrgId.eq(org_id))
            .filter(org_members::Column::Role.eq(OrgRole::Owner))
            .count(&tx)
            .await
            .map_err(internal_resp)?;
        if owner_count <= 1 {
            let slug = lookup_org_slug(&tx, org_id).await.unwrap_or_default();
            return Err(error_body(
                StatusCode::CONFLICT,
                "last_owner",
                Some(format!(
                    "Cannot demote the last remaining owner of org '{slug}'. \
                     Promote another member to Owner first."
                )),
            ));
        }
    }

    let mut active: org_members::ActiveModel = membership.into();
    let new_role_str = role.as_str().to_string();
    active.role = Set(role);
    active.updated_at = Set(Utc::now().fixed_offset());
    active.update(&tx).await.map_err(internal_resp)?;
    tx.commit().await.map_err(internal_resp)?;
    tracing::info!(
        admin_email = %actor.email,
        target_id = %user_id,
        org_id = %org_id,
        new_role = %new_role_str,
        action = "update_role",
        "admin tenant action"
    );
    Ok(StatusCode::NO_CONTENT)
}

pub async fn remove_from_org(
    AuthenticatedUserExtractor(actor): AuthenticatedUserExtractor,
    Path((user_id, org_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, Response> {
    let db = establish_connection().await.map_err(internal_resp)?;
    let tx = db.begin().await.map_err(internal_resp)?;

    // Look the target up first so we know if removing them strips the last
    // owner. owner_count + delete inside the same tx keeps the guard
    // consistent with the write.
    let membership = org_members::Entity::find()
        .filter(org_members::Column::OrgId.eq(org_id))
        .filter(org_members::Column::UserId.eq(user_id))
        .one(&tx)
        .await
        .map_err(internal_resp)?
        .ok_or_else(|| StatusCode::NOT_FOUND.into_response())?;

    if matches!(membership.role, OrgRole::Owner) {
        let owner_count = org_members::Entity::find()
            .filter(org_members::Column::OrgId.eq(org_id))
            .filter(org_members::Column::Role.eq(OrgRole::Owner))
            .count(&tx)
            .await
            .map_err(internal_resp)?;
        if owner_count <= 1 {
            let slug = lookup_org_slug(&tx, org_id).await.unwrap_or_default();
            return Err(error_body(
                StatusCode::CONFLICT,
                "last_owner",
                Some(format!(
                    "Cannot remove the last remaining owner of org '{slug}'. \
                     Promote another member to Owner first."
                )),
            ));
        }
    }

    let res = org_members::Entity::delete_many()
        .filter(org_members::Column::OrgId.eq(org_id))
        .filter(org_members::Column::UserId.eq(user_id))
        .exec(&tx)
        .await
        .map_err(internal_resp)?;
    if res.rows_affected == 0 {
        // Race: someone else already removed the row between our SELECT and
        // DELETE. Treat as 404 for the caller.
        return Err(StatusCode::NOT_FOUND.into_response());
    }
    tx.commit().await.map_err(internal_resp)?;
    tracing::info!(
        admin_email = %actor.email,
        target_id = %user_id,
        org_id = %org_id,
        action = "remove_from_org",
        "admin tenant action"
    );
    Ok(StatusCode::NO_CONTENT)
}

async fn lookup_org_slug<C>(db: &C, org_id: Uuid) -> Option<String>
where
    C: sea_orm::ConnectionTrait,
{
    organizations::Entity::find_by_id(org_id)
        .one(db)
        .await
        .ok()
        .flatten()
        .map(|o| o.slug)
}

async fn load_user_org_memberships(
    db: &sea_orm::DatabaseConnection,
    user_id: Uuid,
) -> Result<Vec<UserOrgMembership>, sea_orm::DbErr> {
    let rows = org_members::Entity::find()
        .filter(org_members::Column::UserId.eq(user_id))
        .order_by_asc(org_members::Column::CreatedAt)
        .all(db)
        .await?;

    // Batch the org lookups into a single query instead of one per row.
    let org_ids: Vec<Uuid> = rows.iter().map(|m| m.org_id).collect();
    let orgs_by_id: HashMap<Uuid, organizations::Model> = organizations::Entity::find()
        .filter(organizations::Column::Id.is_in(org_ids))
        .all(db)
        .await?
        .into_iter()
        .map(|o| (o.id, o))
        .collect();

    let mut out = Vec::with_capacity(rows.len());
    for m in rows {
        if let Some(org) = orgs_by_id.get(&m.org_id) {
            out.push(UserOrgMembership {
                org_id: org.id,
                org_slug: org.slug.clone(),
                org_name: org.name.clone(),
                role: m.role.as_str().to_string(),
                joined_at: m.created_at.to_rfc3339(),
            });
        }
    }
    Ok(out)
}

async fn load_user_workspace_memberships(
    db: &sea_orm::DatabaseConnection,
    user_id: Uuid,
) -> Result<Vec<UserWorkspaceMembership>, sea_orm::DbErr> {
    let rows = workspace_members::Entity::find()
        .filter(workspace_members::Column::UserId.eq(user_id))
        .order_by_asc(workspace_members::Column::CreatedAt)
        .all(db)
        .await?;

    // Batch the workspace lookups into a single query instead of one per row.
    let workspace_ids: Vec<Uuid> = rows.iter().map(|m| m.workspace_id).collect();
    let workspaces_by_id: HashMap<Uuid, entity::workspaces::Model> =
        entity::workspaces::Entity::find()
            .filter(entity::workspaces::Column::Id.is_in(workspace_ids))
            .all(db)
            .await?
            .into_iter()
            .map(|ws| (ws.id, ws))
            .collect();

    let mut out = Vec::with_capacity(rows.len());
    for m in rows {
        if let Some(ws) = workspaces_by_id.get(&m.workspace_id) {
            out.push(UserWorkspaceMembership {
                workspace_id: ws.id,
                workspace_name: ws.name.clone(),
                role: m.role.as_str().to_string(),
                joined_at: m.created_at.to_rfc3339(),
            });
        }
    }
    Ok(out)
}

fn internal<E: std::fmt::Display>(e: E) -> StatusCode {
    tracing::error!("users_admin internal error: {e}");
    StatusCode::INTERNAL_SERVER_ERROR
}

fn internal_resp<E: std::fmt::Display>(e: E) -> Response {
    tracing::error!("users_admin internal error: {e}");
    StatusCode::INTERNAL_SERVER_ERROR.into_response()
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

fn error_body(status: StatusCode, code: &'static str, message: Option<String>) -> Response {
    (status, Json(ErrorBody { code, message })).into_response()
}

/// Pure helper that captures the last-owner guard policy used by both
/// `update_role` and `remove_from_org`. Returns `true` when the operation
/// would strip the org of its last Owner and must be rejected.
///
/// - `current_owners` — distinct Owner user ids on the org right now.
/// - `target_user_id` — the user being demoted or removed.
/// - `new_role` — `Some(role)` for an update, `None` for a removal.
fn would_strand_owner(
    current_owners: &[Uuid],
    target_user_id: Uuid,
    new_role: Option<&OrgRole>,
) -> bool {
    let target_is_owner = current_owners.contains(&target_user_id);
    if !target_is_owner {
        return false;
    }
    let demoting = match new_role {
        Some(OrgRole::Owner) => false,
        Some(_) | None => true,
    };
    demoting && current_owners.len() <= 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_status_body_parses_active() {
        let parsed = UserStatus::from_str("active");
        assert!(parsed.is_ok());
        assert!(matches!(parsed.unwrap(), UserStatus::Active));
    }

    #[test]
    fn set_status_body_parses_deleted() {
        let parsed = UserStatus::from_str("deleted");
        assert!(parsed.is_ok());
        assert!(matches!(parsed.unwrap(), UserStatus::Deleted));
    }

    #[test]
    fn set_status_body_rejects_unknown() {
        assert!(UserStatus::from_str("suspended").is_err());
    }

    #[test]
    fn add_to_org_rejects_unknown_role() {
        assert!(OrgRole::from_str("not-a-role").is_err());
    }

    #[test]
    fn add_to_org_accepts_owner_admin_member() {
        assert!(matches!(
            OrgRole::from_str("owner").unwrap(),
            OrgRole::Owner
        ));
        assert!(matches!(
            OrgRole::from_str("admin").unwrap(),
            OrgRole::Admin
        ));
        assert!(matches!(
            OrgRole::from_str("member").unwrap(),
            OrgRole::Member
        ));
    }

    // ---------------- last-owner guard ----------------

    #[test]
    fn last_owner_guard_blocks_demote_when_only_owner() {
        let only_owner = Uuid::new_v4();
        let owners = vec![only_owner];
        assert!(
            would_strand_owner(&owners, only_owner, Some(&OrgRole::Admin)),
            "must block demote to Admin when only one owner exists"
        );
        assert!(
            would_strand_owner(&owners, only_owner, Some(&OrgRole::Member)),
            "must block demote to Member when only one owner exists"
        );
    }

    #[test]
    fn last_owner_guard_blocks_remove_when_only_owner() {
        let only_owner = Uuid::new_v4();
        let owners = vec![only_owner];
        assert!(
            would_strand_owner(&owners, only_owner, None),
            "must block remove when only one owner exists"
        );
    }

    #[test]
    fn last_owner_guard_allows_demote_when_fallback_owner_exists() {
        let target = Uuid::new_v4();
        let other = Uuid::new_v4();
        let owners = vec![target, other];
        assert!(
            !would_strand_owner(&owners, target, Some(&OrgRole::Admin)),
            "must allow demote when another Owner remains"
        );
        assert!(
            !would_strand_owner(&owners, target, None),
            "must allow remove when another Owner remains"
        );
    }

    #[test]
    fn last_owner_guard_ignores_non_owner_target() {
        let owner = Uuid::new_v4();
        let target = Uuid::new_v4();
        let owners = vec![owner];
        assert!(
            !would_strand_owner(&owners, target, Some(&OrgRole::Admin)),
            "non-owner demote must never be blocked"
        );
        assert!(
            !would_strand_owner(&owners, target, None),
            "non-owner remove must never be blocked"
        );
    }

    #[test]
    fn last_owner_guard_allows_no_op_promote_to_owner() {
        let only_owner = Uuid::new_v4();
        let owners = vec![only_owner];
        assert!(
            !would_strand_owner(&owners, only_owner, Some(&OrgRole::Owner)),
            "must allow updating the last owner to Owner (no-op)"
        );
    }
}
