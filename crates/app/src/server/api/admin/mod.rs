//! `/api/admin/*` — Oxy-staff admin surface. The outer guard in
//! `router::global` is `oxy_owner_or_app_admin_guard_middleware` so both
//! OXY_OWNER staff and members of the `app_admins` table can reach most
//! admin features. Sensitive subsets (billing operations and the
//! `app_admins` table itself — "promotion / demotion of admin and billing
//! adjustment") escalate to a strict OXY_OWNER guard via `route_layer`
//! below; that inner layer runs after the outer permissive check and
//! denies app-admin callers with 403.

pub mod app_admins;
pub mod app_publish_tokens;
pub mod apps;
pub mod assume;
pub mod audit;
pub mod billing;
pub mod compiles;
pub mod delegation;
pub mod explorer;
pub mod internal_jobs;
pub mod metrics;
pub mod org_subdomains;
pub mod orgs_admin;
pub mod oxy_access;
pub mod partners;
pub mod routing;
pub mod scope;
pub mod users_admin;
pub(crate) mod workspace_health;
pub mod workspaces_admin;

use axum::Router;
use axum::middleware;

use crate::server::api::middlewares::{app_scope_guard, oxy_owner_guard, platform_cap_guard};
use crate::server::authz::Action;
use crate::server::feature_flags;
use crate::server::router::AppState;

/// Admin routes are flat under `/api/admin/*` after the 2026-04-28 redesign.
/// Endpoints:
///   - GET    /admin/orgs?status=...
///   - GET    /admin/billing/prices
///   - GET    /admin/orgs/{org_id}/billing/subscription
///   - POST   /admin/orgs/{org_id}/billing/provision-subscription
///   - POST   /admin/orgs/{org_id}/billing/provision-checkout
///   - GET    /admin/orgs/{org_id}/billing/checkout
///   - POST   /admin/orgs/{org_id}/billing/checkout/resend
///   - POST   /admin/orgs/{org_id}/billing/checkout/cancel
///   - POST   /admin/orgs/{org_id}/billing/resync
///   - GET    /admin/feature-flags
///   - PATCH  /admin/feature-flags/{key}
///   - POST   /admin/apps
///   - GET    /admin/apps
///   - GET    /admin/apps/{id}
///   - PATCH  /admin/apps/{id}
///   - DELETE /admin/apps/{id}
///   - GET    /admin/app-admins
///   - POST   /admin/app-admins
///   - DELETE /admin/app-admins/{id}
///   - GET    /admin/internal-jobs/queue-stats
///   - GET    /admin/internal-jobs/recent-failures
///   - GET    /admin/internal-jobs/dead-letter
///   - POST   /admin/internal-jobs/dead-letter/{task_id}/reenqueue
///   - DELETE /admin/internal-jobs/dead-letter/{task_id}
///   - GET    /admin/internal-jobs/workers
///   - GET    /admin/internal-jobs/scheduled
///   - POST   /admin/internal-jobs/run-reaper
///   - GET    /admin/orgs-meta
///   - GET    /admin/orgs/{org_id}/detail
///   - PATCH  /admin/orgs/{org_id}
///   - DELETE /admin/orgs/{org_id}
///   - POST   /admin/orgs/{org_id}/transfer-ownership
///   - GET    /admin/users
///   - GET    /admin/users/{user_id}
///   - PATCH  /admin/users/{user_id}/status
///   - GET    /admin/users/{user_id}/org-memberships
///   - POST   /admin/users/{user_id}/org-memberships
///   - PATCH  /admin/users/{user_id}/org-memberships/{org_id}
///   - DELETE /admin/users/{user_id}/org-memberships/{org_id}
///   - GET    /admin/workspaces-meta
///   - GET    /admin/workspaces/{workspace_id}/detail
///   - PATCH  /admin/workspaces/{workspace_id}
///   - DELETE /admin/workspaces/{workspace_id}
///   - POST   /admin/workspaces/{workspace_id}/transfer-org
///   - POST   /admin/workspace-health/{workspace_id}/eval
/// Admin routes. The outer nest layer in `router::global` is the **door**
/// (`oxy_owner_or_app_admin_guard`): it answers "are you Oxy staff at all". Each
/// sub-router below then escalates to the capability its surface is actually about,
/// via `route_layer` — the inner layer runs after the outer one, so a request that
/// passed the door still gets a 403 here without the capability.
///
/// That per-section escalation is not new machinery: `billing` and `app_admins` have
/// always escalated to strict OXY_OWNER this way. What changed is that the *rest* of
/// the console used to escalate to nothing, so any staff standing reached all of it —
/// which is why an app publisher had the same authority as someone entitled to delete
/// a tenant. `require(Action::Platform*)` generalises the pattern that was already here.
///
/// One gate remains owner-only rather than capability-gated, deliberately:
/// * `billing` — the Billing queue, `Ring::GlobalOwnerOnly`.
///
/// `app_admins` — the **grant table itself** — used to be the second, on the reasoning
/// that "a capability that could edit the grant table would let its holder widen their
/// own grant, and the ceiling would mean nothing". That objection is real and is
/// answered by bounding the write rather than withholding the capability: `may_delegate`
/// admits only a grant strictly weaker than the writer's own, so the one row a holder
/// can never touch is their own, and only the owner can mint a peer. See
/// `admin::delegation`. **The capability gate here is a door, not the control** — the
/// handlers carry the row-level half (`actor_facts` once, then `refuse(may_delegate(..))`
/// per row), exactly as scope works.
///
/// **Scope is not enforced here** — see `platform_cap_guard`. A scoped operator passes
/// these gates and the handler filters its rows.
///
/// `internal_jobs::router()` is mounted separately at `/admin/internal-jobs`
/// in `router::global` because its routes were flattened during the
/// app-admin opening.
pub(crate) fn router() -> Router<AppState> {
    // route_layer applied per sub-router so only billing + app-admins get
    // the strict guard; everything else runs only under the outer
    // permissive guard.
    let strict = middleware::from_fn(oxy_owner_guard::oxy_owner_guard_middleware);
    let cap = |action| middleware::from_fn(platform_cap_guard::require(action));

    // The staff surface. Everything here is refused while the caller is acting as
    // a tenant: you cannot wield staff powers and wear a customer's identity in
    // the same breath. Ending the session (below) restores all of it.
    let staff_surface = feature_flags::routes::router()
        .route_layer(cap(Action::PlatformOperate))
        // Two layers, two questions: the capability admits you to the section, then the
        // scope guard fences which apps you may touch inside it. Layered here rather
        // than in ~20 handlers — see `app_scope_guard`.
        .merge(
            apps::router()
                .route_layer(middleware::from_fn(app_scope_guard::enforce_app_scope))
                .route_layer(cap(Action::PlatformApps)),
        )
        .merge(audit::router().route_layer(cap(Action::PlatformAudit)))
        .merge(app_publish_tokens::router().route_layer(cap(Action::PlatformApps)))
        .merge(explorer::router().route_layer(cap(Action::PlatformExplorer)))
        .merge(metrics::router().route_layer(cap(Action::PlatformOperate)))
        // Org administration and creation are one router but two capabilities; the
        // router-level gate is the broader `PlatformOrgs`, and `create_org` asks for
        // `PlatformOrgCreate` inside the handler where the verb is known.
        .merge(orgs_admin::router().route_layer(cap(Action::PlatformOrgs)))
        .merge(org_subdomains::router().route_layer(cap(Action::PlatformOrgs)))
        .merge(users_admin::router().route_layer(cap(Action::PlatformUsers)))
        .merge(workspaces_admin::router().route_layer(cap(Action::PlatformOrgs)))
        .merge(routing::router().route_layer(cap(Action::PlatformOperate)))
        .merge(workspace_health::router().route_layer(cap(Action::PlatformOperate)))
        .merge(partners::router().route_layer(cap(Action::PlatformPartners)))
        .merge(billing::router().route_layer(strict))
        .merge(app_admins::router().route_layer(cap(Action::PlatformGrants)))
        .route_layer(middleware::from_fn(assume::block_admin_while_acting));

    // Assume-role itself lives at `/api/assume`, NOT here — see `assume::router`.
    // It has to be reachable while acting (that's where the exit is) and by
    // partners (who are not staff and would be 403'd by this surface's guard).
    staff_surface
}

#[cfg(test)]
mod tests {
    //! Regression tests for the route-layer escalation pattern used by `billing`.
    //! These pin the property that even when the outer permissive layer in
    //! `router::global` admits a Global Admin (a non-Owner staff member), a route
    //! nested under `route_layer(oxy_owner_guard_middleware)` still rejects with 403.
    //!
    //! `app_admins` was the second such surface and is no longer: it is capability-gated
    //! plus row-fenced (see `admin::delegation`). These tests build their own router, so
    //! they cannot notice that change — which is exactly why the assertion that the real
    //! mount matches lives in `crates/app/tests/app_scope_boundary.rs` as a source scan.
    //! A fixture that mounts its own probe under `strict` proves the *middleware* works
    //! and nothing whatsoever about what ships.
    //!
    //! The escalation does NOT touch the database — `oxy_owner_guard`
    //! consults only the `OXY_OWNER` env var — so we can pin the layering
    //! behavior with a plain Tower service test rather than a full
    //! integration harness.
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use entity::users::UserStatus;
    use oxy_auth::types::AuthenticatedUser;
    use tower::ServiceExt;
    use uuid::Uuid;

    /// Set / unset env vars for the duration of one test. Necessary
    /// because the guard reads `OXY_OWNER` at request time, not boot.
    struct EnvGuard {
        key: &'static str,
        prev: Option<String>,
    }
    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let prev = std::env::var(key).ok();
            unsafe { std::env::set_var(key, value) };
            Self { key, prev }
        }
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.prev {
                Some(v) => unsafe { std::env::set_var(self.key, v) },
                None => unsafe { std::env::remove_var(self.key) },
            }
        }
    }

    fn stub_user(email: &str) -> AuthenticatedUser {
        AuthenticatedUser {
            id: Uuid::new_v4(),
            email: email.to_string(),
            name: "stub".to_string(),
            picture: None,
            status: UserStatus::Active,
        }
    }

    /// Build the same router shape as `admin::router()` minus the leaf
    /// handlers (DB-heavy). One stub handler per surface so we can assert
    /// per-path response codes.
    fn test_router() -> Router {
        let strict = middleware::from_fn(oxy_owner_guard::oxy_owner_guard_middleware);
        let billing = Router::new()
            .route("/billing/probe", get(|| async { StatusCode::OK }))
            .route_layer(strict.clone());
        // `partners` and `app-admins` mount WITHOUT the strict layer — a Global Admin
        // must reach both. (Partners is the regression guard for the 403-on-partners
        // bug; app-admins is capability-gated now, with the delegation bound doing the
        // narrowing inside the handler rather than at the door.)
        let open = Router::new()
            .route("/feature-flags/probe", get(|| async { StatusCode::OK }))
            .route("/partners/probe", get(|| async { StatusCode::OK }))
            .route("/app-admins/probe", get(|| async { StatusCode::OK }));
        Router::new().merge(billing).merge(open)
    }

    async fn request_as(router: Router, path: &str, user: Option<AuthenticatedUser>) -> StatusCode {
        let mut req = Request::builder().uri(path).body(Body::empty()).unwrap();
        if let Some(u) = user {
            req.extensions_mut().insert(u);
        }
        router.oneshot(req).await.unwrap().status()
    }

    /// A non-owner caller gets 403 on billing, but 200 on the non-escalated paths.
    /// This pins the route_layer escalation itself.
    #[tokio::test]
    async fn strict_layer_rejects_non_owner_on_billing() {
        let _g = EnvGuard::set("OXY_OWNER", "owner@example.com");
        let app = test_router();

        let admin = stub_user("admin@example.com");

        assert_eq!(
            request_as(app.clone(), "/billing/probe", Some(admin.clone())).await,
            StatusCode::FORBIDDEN,
            "Global Admin must NOT reach billing through the strict route_layer"
        );
        assert_eq!(
            request_as(app.clone(), "/app-admins/probe", Some(admin.clone())).await,
            StatusCode::OK,
            "app_admins is capability-gated, not owner-strict: a Global Admin reaches \
             the console and `may_delegate` decides which rows they may write"
        );
        assert_eq!(
            request_as(app.clone(), "/partners/probe", Some(admin.clone())).await,
            StatusCode::OK,
            "Global Admin MUST reach partners — provisioning is not owner-strict"
        );
        assert_eq!(
            request_as(app, "/feature-flags/probe", Some(admin)).await,
            StatusCode::OK,
            "non-escalated paths stay reachable by any authenticated caller"
        );
    }

    /// Sanity: an Owner reaches every surface — confirms the strict
    /// layer doesn't accidentally reject owners.
    #[tokio::test]
    async fn strict_layer_allows_owner_everywhere() {
        let _g = EnvGuard::set("OXY_OWNER", "owner@example.com");
        let app = test_router();
        let owner = stub_user("owner@example.com");

        for path in [
            "/billing/probe",
            "/app-admins/probe",
            "/feature-flags/probe",
        ] {
            assert_eq!(
                request_as(app.clone(), path, Some(owner.clone())).await,
                StatusCode::OK,
                "Global Owner must reach {path}"
            );
        }
    }

    /// Missing auth extension → 401 from the strict guard. Protects
    /// against a deploy that accidentally drops the auth middleware.
    #[tokio::test]
    async fn strict_layer_rejects_unauthenticated() {
        let _g = EnvGuard::set("OXY_OWNER", "owner@example.com");
        let app = test_router();

        assert_eq!(
            request_as(app, "/billing/probe", None).await,
            StatusCode::UNAUTHORIZED
        );
    }
}
