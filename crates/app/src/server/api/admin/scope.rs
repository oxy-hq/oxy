//! The one place a staff handler asks "does my grant reach this org?".
//!
//! `platform_cap_guard` cannot answer it. It decides on `Resource::platform()`, which has
//! a nil org — so a **bounded** `global_admin` passes every capability gate on this
//! console, and narrowing is left to the handler. That split is deliberate (capabilities
//! gate verbs, scope filters rows) and it has now been got wrong three times, each time
//! in a file that had no reason to know the rule existed:
//!
//! * the custom-app registry — fixed with `app_scope_guard` plus three handler checks;
//! * org membership — `add_to_org` could grant Owner in any tenant;
//! * org and workspace administration — `DELETE /admin/orgs/{any}` ran unfenced, which
//!   is the reach this whole change opens by describing ("every Global Admin could delete
//!   any org") and outranks the other two: a wrongly-added member can be removed, a
//!   deleted org cannot be un-deleted.
//!
//! Two identical copies of this fence already existed, in `users_admin` and
//! `apps::handlers`, differing only in imports. A third copy was the wrong answer, so
//! this is the shared one.

use axum::http::StatusCode;
use oxy_auth::types::AuthenticatedUser;
use oxy_authz::Scope;
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::server::authz::globals;

/// Refuse when the caller's platform grant does not reach `org_id`.
///
/// **404, not 403** — an operator with no reach into an org must not learn it exists by
/// being told "forbidden". Consistent with every other out-of-scope answer on this
/// branch.
///
/// **500 on an unreadable grant.** These are write paths; treating "unknown" as
/// "unbounded" is how one transient `DbErr` hands out tenant Owner or drops a tenant.
/// The lenient read-path behaviour lives in `apps::handlers::scope_org_filter` and is
/// deliberately not shared with this.
///
/// Callers pass their own connection: this module opens none, so a handler that already
/// has a handle does not pay for a second. `platform_cap_guard` opens its own because it
/// runs before any handler exists.
pub async fn deny_out_of_scope(
    db: &DatabaseConnection,
    actor: &AuthenticatedUser,
    org_id: Uuid,
) -> Result<(), StatusCode> {
    // The Global Owner is root and holds no grant row — their standing is the env
    // allow-list, so reading the grant table for them would find nothing and refuse.
    if globals::is_global_owner(&actor.email) {
        return Ok(());
    }
    match globals::platform_grant_checked(db, &actor.email).await {
        Ok(Some(grant)) => match &grant.scope {
            Scope::All => Ok(()),
            Scope::Orgs(orgs) if orgs.contains(&org_id) => Ok(()),
            Scope::Orgs(_) => Err(StatusCode::NOT_FOUND),
        },
        // Unreachable in practice rather than merely permitted: `platform_cap_guard`'s
        // oracle is table-only, and `allows` needs a standing, so a caller with no grant
        // row cannot be here at all unless they are the owner who returned above. Kept
        // as the defensive default; if this arm ever executes, the guard above it changed
        // and that is the thing to look at.
        Ok(None) => Ok(()),
        Err(e) => {
            tracing::error!(
                target: "authz",
                error = %e,
                "platform grant unreadable on a scoped admin WRITE — refusing"
            );
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// The same fence for a resource whose org is **nullable** — a workspace.
///
/// `if let Some(org) = ws.org_id { deny(..) }` was the obvious spelling and the wrong
/// one: the `None` arm is not a check that passes, it is no check at all, so an
/// org-less workspace was deletable by any bounded grant. It also defeated the boundary
/// test, which asserts the fence is *called* — and it was, conditionally.
///
/// `None` refuses for a bounded grant, because a null org is by definition not in
/// `Scope::Orgs(..)`. Unbounded grants and the owner still pass, which is the same
/// answer they get everywhere else. This is the direction the whole branch settled on:
/// an org that cannot be established means refuse, exactly as an unreadable grant does.
pub async fn deny_out_of_scope_opt(
    db: &DatabaseConnection,
    actor: &AuthenticatedUser,
    org_id: Option<Uuid>,
) -> Result<(), StatusCode> {
    match org_id {
        Some(org_id) => deny_out_of_scope(db, actor, org_id).await,
        None => {
            if globals::is_global_owner(&actor.email) {
                return Ok(());
            }
            match globals::platform_grant_checked(db, &actor.email).await {
                Ok(Some(grant)) if matches!(grant.scope, Scope::Orgs(_)) => {
                    Err(StatusCode::NOT_FOUND)
                }
                Ok(_) => Ok(()),
                Err(e) => {
                    tracing::error!(
                        target: "authz",
                        error = %e,
                        "platform grant unreadable fencing an org-less resource — refusing"
                    );
                    Err(StatusCode::INTERNAL_SERVER_ERROR)
                }
            }
        }
    }
}
