//! Access control for custom apps.
//!
//! Two independent grants compose to "can this user reach this app":
//!
//! 1. **Org membership** — the historical check. A member of the owning
//!    org sees every app in that org.
//! 2. **Oxy staff** — the caller is a member of `app_admins` (an Oxy-staff
//!    role managed by `OXY_OWNER` users) AND the workspace has NOT locked
//!    Oxy out (no row in `workspace_oxy_lockdown`). Staff access is the
//!    DEFAULT (inverted 2026-07-14 — the old opt-in consent row was
//!    self-grantable by staff, so it protected nobody); an org officer can
//!    revoke it at any time with the lockdown switch.
//!
//! The combined check is fronted by a `(user_id, app_id) → bool` cache
//! so a Next.js page load's asset storm doesn't hit the DB three times
//! per chunk. Cache TTL matches the existing membership cache so a
//! revocation of any source propagates within a minute.
//!
//! All helpers are async because the underlying tables are queried. The
//! email-keyed Global-Admin check (`app_admins`) now lives in
//! `server::authz::globals::is_app_admin_email` — authz owns that read.
//!
//! ## Bundle / manifest helpers
//!
//! Manifest schema, caching, and bundle-dir resolution live in
//! `custom_apps_manifest`. `authenticate_and_authorize` lives here so
//! any handler — not just the debug one — can reuse the full auth
//! pipeline without duplicating the logic.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};
use std::time::Instant;

use entity::prelude::{
    AppAdmins, AppMembers, AppTeamGrants, Apps, OrgMembers, OrgTeamMembers, Organizations,
    WorkspaceOxyLockdown,
};
use entity::{
    app_admins, app_members, app_team_grants, apps, org_members, org_team_members, organizations,
    workspace_oxy_lockdown,
};
use oxy::database::client::establish_connection;
use oxy_auth::authenticator::Authenticator;
use oxy_auth::built_in::BuiltInAuthenticator;
use oxy_auth::user::UserService;
use sea_orm::{ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter};
use tracing::error;
use uuid::Uuid;

use super::custom_apps_cache::{cached_user, get_fresh, insert_with_sweep, set_cached_user};

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

// ── Primary access check ────────────────────────────────────────────────────

/// True if `user` may serve / read data products for `app`. Two
/// independent access paths:
///
/// - **Oxy staff path**: platform standing is staff — Global Owner **or** Global
///   Admin, read through `oxy_server_authz::globals::platform_standing`, not from
///   `app_admins` directly — plus the workspace has not locked Oxy out
///   (`workspace_oxy_lockdown`). Works on draft (unpublished) apps too, which is
///   how Oxy engineers iterate.
/// - **Customer path**: the app is **published** (`published_at IS NOT NULL`) and
///   then, by visibility:
///   - `org` (the default) — any member of the owning org.
///   - `members` (restricted) — an org member who also holds a grant on the app,
///     direct or through a team ([`has_app_grant`]; a grant narrows the org, it is
///     not a way into one), **or** an org officer, who keeps a break-glass path so
///     an org cannot lock its own owners out of its app.
///
///   Unpublished apps are invisible to customers — they look like 404s.
///
/// The customer path mirrors `Ring::AppAccess` in `oxy-authz`; this is the shipped
/// gate that ring is differenced against, so the two must be changed together.
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

    // Staff path first: works regardless of publish state. Oxy staff may access
    // BY DEFAULT — unless the org has locked them out (inverted 2026-07-14).
    //
    // "Staff" is admin OR owner, read through the one place that knows what the
    // platform sources are. It used to be app-admin ONLY, so a Global Owner who wasn't
    // also in `app_admins` could PUBLISH a custom app (publish resolves staff via
    // platform_standing) but not VIEW one — the same question answered two ways in one
    // subsystem. Both operator tiers reach everything; they separate only at
    // owner-exclusive destructive operations.
    let allowed = if oxy_server_authz::globals::platform_standing(db, user_email)
        .await
        .is_staff()
        && !is_oxy_locked_down(db, app.project_id).await?
    {
        true
    } else if app.published_at.is_some() {
        // Customer path: only if published — and, for a RESTRICTED app
        // (`visibility = 'members'`), org membership alone is no longer enough.
        // An org officer (owner/admin) keeps a break-glass path so an org can't
        // lock its own staff out of its app. Mirrors `Ring::AppAccess` in
        // `oxy-authz`, which states the same rule; this is the shipped gate that
        // ring is differenced against.
        if app.is_restricted() {
            // A grant NARROWS the org — it is not a way into one. Without the
            // org-membership conjunction, a grant row let a non-member load the
            // app's shell while `check_custom_app_gates` (which requires org
            // membership) 403'd every query behind it. Mirrors the same term in
            // `Ring::AppAccess`.
            (is_org_member(db, user_id, app.org_id).await?
                && has_app_grant(db, user_id, app.id).await?)
                || is_org_officer(db, user_id, app.org_id).await?
        } else {
            is_org_member(db, user_id, app.org_id).await?
        }
    } else {
        false
    };

    set_cached_access(user_id, app.id, allowed);
    Ok(allowed)
}

/// Whether `user_id` holds any grant on `app_id`, **direct or through a team**.
///
/// The ONE place the two grant kinds are unioned on the shipped-gate side; the fact
/// loader does the same union for `oxy-authz`. Anything asking "does this user have a
/// grant" must come through here, or the two sources drift.
///
/// Deliberately an existence check and not a role resolution. Both callers only need
/// "is there a grant": [`user_can_access_app`] gates access on it, and
/// [`resolve_app_role`] reports `member` for anyone who isn't already `admin` — and
/// `admin` comes from `Ring::AppAdmin`, which reads the loader's unioned
/// `app_admin_memberships`. A second strongest-grant-wins resolution here would be a
/// parallel copy of a rule the model already owns, which is exactly the drift
/// `oxy-authz` exists to end. The strongest-wins property is pinned where it lives,
/// in the loader (`authz_loader_differential`).
///
/// Short-circuits on the direct row so the common case costs one query.
pub async fn has_app_grant(
    db: &DatabaseConnection,
    user_id: Uuid,
    app_id: Uuid,
) -> Result<bool, DbErr> {
    let direct = AppMembers::find()
        .filter(app_members::Column::AppId.eq(app_id))
        .filter(app_members::Column::UserId.eq(user_id))
        .one(db)
        .await?;
    if direct.is_some() {
        return Ok(true);
    }

    let team_ids: Vec<Uuid> = OrgTeamMembers::find()
        .filter(org_team_members::Column::UserId.eq(user_id))
        .all(db)
        .await?
        .into_iter()
        .map(|row| row.team_id)
        .collect();
    if team_ids.is_empty() {
        return Ok(false);
    }
    AppTeamGrants::find()
        .filter(app_team_grants::Column::AppId.eq(app_id))
        .filter(app_team_grants::Column::TeamId.is_in(team_ids))
        .one(db)
        .await
        .map(|row| row.is_some())
}

/// Returns `true` when `user_id` is an **officer** (owner or admin) of `org_id`.
/// The break-glass term for restricted-app access: an org's own officers are never
/// locked out of its apps. (`.is_in` rather than the `Owner | Admin` match literal
/// the authz-boundary guard bans in handlers.)
pub(crate) async fn is_org_officer(
    db: &DatabaseConnection,
    user_id: Uuid,
    org_id: Uuid,
) -> Result<bool, DbErr> {
    OrgMembers::find()
        .filter(org_members::Column::UserId.eq(user_id))
        .filter(org_members::Column::OrgId.eq(org_id))
        .filter(
            org_members::Column::Role
                .is_in([org_members::OrgRole::Owner, org_members::OrgRole::Admin]),
        )
        .one(db)
        .await
        .map(|opt| opt.is_some())
}

/// The invoking user's role **within this app**, as surfaced to a function through
/// `ctx.user.appRole`.
///
/// `Some("admin")` when they administer it — any org **officer** (owner or admin),
/// an `app_members` admin row, or Oxy staff. `Some("member")` for a plain
/// `app_members` row, `None` otherwise. A plain org member is NOT an admin unless
/// granted the row — that is the line the model draws (officer, not member), and the
/// `app_members` admin role is how a non-officer becomes one.
///
/// This mirrors `Ring::AppAdmin` in `oxy-authz`; a function that gates a privileged
/// surface on it is server-enforcing, not merely hiding a tab.
pub async fn resolve_app_role(
    db: &DatabaseConnection,
    user_id: Uuid,
    user_email: &str,
    app: &apps::Model,
) -> Result<Option<&'static str>, DbErr> {
    // The admin verdict comes from the ONE model — not a second copy of the rule
    // written out here. Restating "staff OR org owner OR app-admin row" in SQL is
    // exactly the drift `oxy-authz` exists to end, and it would silently diverge
    // the moment `Ring::AppAdmin` changed.
    //
    // Workspace facts are skipped: no app ring reads them.
    let is_admin =
        match oxy_server_authz::loader::load_principal_facts_scoped(db, user_id, user_email, false)
            .await
        {
            Some(facts) => oxy_authz::allows(
                &facts,
                oxy_authz::Action::AppAdmin,
                &oxy_authz::Resource::app_with_visibility(app.id, app.org_id, app.is_restricted()),
            ),
            // Facts unknown (a DB blip) → not admin. Fail closed.
            None => false,
        };
    if is_admin {
        return Ok(Some(app_members::ROLE_ADMIN));
    }
    // Not an admin — a plain grant still reports as "member" so an app can
    // distinguish "belongs to this app" from "just any org member". Goes through
    // `has_app_grant` so a team-granted user isn't reported as `None`, which would
    // make team grants invisible to `ctx.user.appRole`.
    Ok(has_app_grant(db, user_id, app.id)
        .await?
        .then_some(app_members::ROLE_MEMBER))
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
/// Whether this workspace has LOCKED Oxy staff out. A row in
/// `workspace_oxy_lockdown` is
/// the toggle.
pub async fn is_oxy_locked_down(
    db: &DatabaseConnection,
    workspace_id: Uuid,
) -> Result<bool, DbErr> {
    WorkspaceOxyLockdown::find()
        .filter(workspace_oxy_lockdown::Column::WorkspaceId.eq(workspace_id))
        .one(db)
        .await
        .map(|opt| opt.is_some())
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
pub(crate) struct AuthOutcome {
    pub app: apps::Model,
    pub user_id: Uuid,
    pub user_email: String,
    pub is_staff: bool,
}

/// Authenticate the request and confirm the caller has access to
/// (org, app). Returns the app row + user info on success; an HTTP
/// status on any failure.
pub(crate) async fn authenticate_and_authorize(
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

    // Same definition as the access check above — otherwise an owner would pass access
    // but be flagged a customer, and silently lose draft previews.
    let is_staff = oxy_server_authz::globals::platform_standing(&db, &user.email)
        .await
        .is_staff();

    Ok(AuthOutcome {
        app,
        user_id: user.id,
        user_email: user.email,
        is_staff,
    })
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
    oxy_server_authz::globals::invalidate_admin_cache();
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
