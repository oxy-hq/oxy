//! Access control for customer apps.
//!
//! Two independent grants compose to "can this user reach this app":
//!
//! 1. **Org membership** — the historical check. A member of the owning
//!    org sees every app in that org.
//! 2. **Oxy access** — the workspace owner has flipped the "let Oxy
//!    build apps on our data" toggle (a row in `workspace_oxy_access`)
//!    AND the caller is a member of `app_admins` (an Oxy-staff role
//!    managed by `OXY_OWNER` users). Neither half is sufficient on its
//!    own — the customer must opt in per-workspace, and the user must
//!    be a recognised Oxy staff member.
//!
//! The combined check is fronted by a `(user_id, app_id) → bool` cache
//! so a Next.js page load's asset storm doesn't hit the DB three times
//! per chunk. Cache TTL matches the existing membership cache so a
//! revocation of any source propagates within a minute.
//!
//! All helpers are async because the underlying tables are queried. The
//! middleware/login-response paths use the email-keyed admin check
//! (`is_app_admin_email`) which has its own small cache.
//!
//! ## Bundle / manifest helpers
//!
//! Manifest schema, caching, and bundle-dir resolution live in
//! `customer_apps_manifest`. `authenticate_and_authorize` lives here so
//! any handler — not just the debug one — can reuse the full auth
//! pipeline without duplicating the logic.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};
use std::time::Instant;

use entity::prelude::{AppAdmins, Apps, OrgMembers, Organizations, WorkspaceOxyAccess};
use entity::{app_admins, apps, org_members, organizations, workspace_oxy_access};
use oxy::database::client::establish_connection;
use oxy_auth::authenticator::Authenticator;
use oxy_auth::built_in::BuiltInAuthenticator;
use oxy_auth::user::UserService;
use sea_orm::{ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter};
use tracing::error;
use uuid::Uuid;

use super::customer_apps_cache::{cached_user, get_fresh, insert_with_sweep, set_cached_user};

// ── Combined per-(user, app) access cache ───────────────────────────────────

fn access_cache() -> &'static RwLock<HashMap<(Uuid, Uuid), (bool, Instant)>> {
    static CACHE: OnceLock<RwLock<HashMap<(Uuid, Uuid), (bool, Instant)>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

fn cached_access(user_id: Uuid, app_id: Uuid) -> Option<bool> {
    get_fresh(access_cache(), &(user_id, app_id))
}

fn set_cached_access(user_id: Uuid, app_id: Uuid, allowed: bool) {
    insert_with_sweep(access_cache(), (user_id, app_id), allowed);
}

/// Called by the admin / oxy-access mutation handlers so a freshly
/// toggled state takes effect immediately instead of waiting out the
/// cache TTL. We don't know which user_id × app_id pairs are affected,
/// so we drop the whole cache.
pub fn invalidate_access_cache() {
    if let Ok(mut guard) = access_cache().write() {
        guard.clear();
    }
}

// ── Global app admin cache (email-keyed) ────────────────────────────────────

fn admin_cache() -> &'static RwLock<HashMap<String, (bool, Instant)>> {
    static CACHE: OnceLock<RwLock<HashMap<String, (bool, Instant)>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

fn cached_admin(email: &str) -> Option<bool> {
    get_fresh(admin_cache(), &email.to_string())
}

fn set_cached_admin(email: String, is_admin: bool) {
    insert_with_sweep(admin_cache(), email, is_admin);
}

/// Same broad-strokes invalidation as [`invalidate_access_cache`], for
/// admin-table mutations.
pub fn invalidate_admin_cache() {
    if let Ok(mut guard) = admin_cache().write() {
        guard.clear();
    }
}

// ── Primary access check ────────────────────────────────────────────────────

/// True if `user` may serve / read data products for `app`. Two
/// independent access paths:
///
/// - **Oxy staff path**: `app_admins` membership + the workspace has
///   opted in via `workspace_oxy_access`. Works on draft (unpublished)
///   apps too — that's how Oxy engineers iterate.
/// - **Customer path**: the app is **published** (`published_at IS NOT
///   NULL`) AND the user is a member of the owning org. Unpublished
///   apps are invisible to customers — they look like 404s.
///
/// Short-circuits on the staff path so an Oxy engineer's request
/// skips the org-membership query, and on the unpublished check so
/// customers don't fan out to membership for draft apps.
pub async fn user_can_access_app(
    db: &DatabaseConnection,
    user_id: Uuid,
    user_email: &str,
    app: &apps::Model,
) -> Result<bool, DbErr> {
    if let Some(v) = cached_access(user_id, app.id) {
        return Ok(v);
    }

    // Staff path first: works regardless of publish state.
    let allowed = if is_app_admin_email(db, user_email).await?
        && is_oxy_access_enabled(db, app.project_id).await?
    {
        true
    } else if app.published_at.is_some() {
        // Customer path: only if published.
        is_org_member(db, user_id, app.org_id).await?
    } else {
        false
    };

    set_cached_access(user_id, app.id, allowed);
    Ok(allowed)
}

/// Returns `true` when `user_id` is a member of `org_id`.
///
/// Does **not** cache — callers that need caching should go through
/// [`user_can_access_app`] instead.
pub(crate) async fn is_org_member(
    db: &DatabaseConnection,
    user_id: Uuid,
    org_id: Uuid,
) -> Result<bool, DbErr> {
    OrgMembers::find()
        .filter(org_members::Column::UserId.eq(user_id))
        .filter(org_members::Column::OrgId.eq(org_id))
        .one(db)
        .await
        .map(|opt| opt.is_some())
}

/// True when the customer has enabled "Oxy can build tailored apps on
/// our data" for this workspace. A row in `workspace_oxy_access` is
/// the toggle.
pub async fn is_oxy_access_enabled(
    db: &DatabaseConnection,
    workspace_id: Uuid,
) -> Result<bool, DbErr> {
    WorkspaceOxyAccess::find()
        .filter(workspace_oxy_access::Column::WorkspaceId.eq(workspace_id))
        .one(db)
        .await
        .map(|opt| opt.is_some())
}

/// Global app-admin check. Email is normalised (trim + lowercase) before
/// lookup so the table can be queried case-insensitively without a
/// functional index. Cached for [`CACHE_TTL`].
pub async fn is_app_admin_email(db: &DatabaseConnection, email: &str) -> Result<bool, DbErr> {
    let key = email.trim().to_ascii_lowercase();
    if key.is_empty() {
        return Ok(false);
    }
    if let Some(v) = cached_admin(&key) {
        return Ok(v);
    }
    let found = AppAdmins::find()
        .filter(app_admins::Column::Email.eq(key.clone()))
        .one(db)
        .await?
        .is_some();
    set_cached_admin(key, found);
    Ok(found)
}

/// Convenience: load an app by `(org_slug, app_slug)`. Used by the few
/// callers that need both the access check and the app row but don't
/// already have the app in hand.
pub async fn load_app_by_slugs(
    db: &DatabaseConnection,
    org_id: Uuid,
    app_slug: &str,
) -> Result<Option<apps::Model>, DbErr> {
    Apps::find()
        .filter(apps::Column::OrgId.eq(org_id))
        .filter(apps::Column::Slug.eq(app_slug))
        .one(db)
        .await
}

// ── Auth helper ──────────────────────────────────────────────────────────────

/// What the auth flow returns on success.
pub(super) struct AuthOutcome {
    pub app: apps::Model,
    pub is_staff: bool,
}

/// Authenticate the request and confirm the caller has access to
/// (org, app). Returns the app row + user info on success; an HTTP
/// status on any failure.
pub(super) async fn authenticate_and_authorize(
    headers: &axum::http::HeaderMap,
    org_slug: &str,
    app_slug: &str,
) -> Result<AuthOutcome, axum::http::StatusCode> {
    use axum::http::StatusCode;

    let identity = BuiltInAuthenticator::new()
        .authenticate(headers)
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let email_key = identity.email.to_ascii_lowercase();
    let user = if let Some(u) = cached_user(&email_key) {
        u
    } else {
        let u = UserService::find_user_by_identity(&identity)
            .await
            .map_err(|e| {
                error!("user lookup failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?
            .ok_or(StatusCode::UNAUTHORIZED)?;
        set_cached_user(email_key, u.clone());
        u
    };

    let db = establish_connection().await.map_err(|e| {
        error!("DB connection failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let org = Organizations::find()
        .filter(organizations::Column::Slug.eq(org_slug))
        .one(&db)
        .await
        .map_err(|e| {
            error!("org lookup failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let app = Apps::find()
        .filter(apps::Column::OrgId.eq(org.id))
        .filter(apps::Column::Slug.eq(app_slug))
        .one(&db)
        .await
        .map_err(|e| {
            error!("app lookup failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let allowed = user_can_access_app(&db, user.id, &user.email, &app)
        .await
        .map_err(|e| {
            error!("access check failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if !allowed {
        return Err(StatusCode::FORBIDDEN);
    }

    let is_staff = is_app_admin_email(&db, &user.email).await.unwrap_or(false);

    Ok(AuthOutcome { app, is_staff })
}

// ── Bootstrap: OXY_GLOBAL_ADMINS env → app_admins table ─────────────────────

/// Reads `OXY_GLOBAL_ADMINS` (preferred) or the legacy `OXY_APP_ADMINS`
/// (comma-separated emails) once at startup and inserts any missing rows
/// into `app_admins` with `granted_by = NULL`. Idempotent — re-running is
/// harmless. After the seed, OXY_OWNER users can add/remove admins through
/// the UI; the env var becomes a bootstrap-only convenience, never a
/// permanent allow-list.
///
/// If both env vars are set, the contents are unioned so a half-migrated
/// deployment doesn't lose admins. A deprecation warning is logged when
/// the legacy name is observed.
pub async fn bootstrap_app_admins_from_env(db: &DatabaseConnection) -> Result<(), DbErr> {
    let mut raw_inputs: Vec<String> = Vec::new();
    if let Ok(v) = std::env::var("OXY_GLOBAL_ADMINS") {
        raw_inputs.push(v);
    }
    if let Ok(v) = std::env::var("OXY_APP_ADMINS") {
        tracing::warn!(
            "OXY_APP_ADMINS is deprecated — rename to OXY_GLOBAL_ADMINS. \
             Both are accepted for now; the legacy name will be removed in \
             a future release."
        );
        raw_inputs.push(v);
    }
    if raw_inputs.is_empty() {
        return Ok(());
    }
    let emails: Vec<String> = raw_inputs
        .iter()
        .flat_map(|s| s.split(','))
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    if emails.is_empty() {
        return Ok(());
    }

    let existing: Vec<String> = AppAdmins::find()
        .filter(app_admins::Column::Email.is_in(emails.clone()))
        .all(db)
        .await?
        .into_iter()
        .map(|m| m.email)
        .collect();

    let to_insert: Vec<_> = emails
        .into_iter()
        .filter(|e| !existing.contains(e))
        .map(|email| app_admins::ActiveModel {
            id: sea_orm::ActiveValue::Set(Uuid::new_v4()),
            email: sea_orm::ActiveValue::Set(email),
            granted_by: sea_orm::ActiveValue::Set(None),
            created_at: sea_orm::ActiveValue::NotSet,
        })
        .collect();

    if to_insert.is_empty() {
        return Ok(());
    }

    let count = to_insert.len();
    AppAdmins::insert_many(to_insert).exec(db).await?;
    invalidate_admin_cache();
    tracing::info!(
        count,
        "bootstrap_app_admins: seeded {count} global admin(s) from env"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_cache_round_trip() {
        let user = Uuid::new_v4();
        let app = Uuid::new_v4();
        set_cached_access(user, app, true);
        assert_eq!(cached_access(user, app), Some(true));
        invalidate_access_cache();
        assert_eq!(cached_access(user, app), None);
    }
}
