//! `/api/admin/*` — Oxy-staff admin surface. The outer guard in
//! `router::global` is `oxy_owner_or_app_admin_guard_middleware` so both
//! OXY_OWNER staff and members of the `app_admins` table can reach most
//! admin features. Sensitive subsets (billing operations and the
//! `app_admins` table itself — "promotion / demotion of admin and billing
//! adjustment") escalate to a strict OXY_OWNER guard via `route_layer`
//! below; that inner layer runs after the outer permissive check and
//! denies app-admin callers with 403.

pub mod app_admins;
pub mod apps;
pub mod billing;
pub mod internal_jobs;
pub mod orgs_admin;
pub mod oxy_access;
pub mod users_admin;
pub mod workspaces_admin;

use axum::Router;
use axum::middleware;

use crate::server::api::middlewares::oxy_owner_guard;
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
/// Admin routes. The outer nest layer in `router::global` is permissive
/// (OXY_OWNER **or** app_admins). `billing` and `app_admins` sub-routers
/// escalate to strict OXY_OWNER via `route_layer` — the inner layer runs
/// after the outer one, so a request that passed the permissive check
/// still gets a 403 here if the caller isn't an owner.
///
/// `internal_jobs::router()` is mounted separately at `/admin/internal-jobs`
/// in `router::global` because its routes were flattened during the
/// app-admin opening.
pub(crate) fn router() -> Router<AppState> {
    // route_layer applied per sub-router so only billing + app-admins get
    // the strict guard; everything else runs only under the outer
    // permissive guard.
    let strict = middleware::from_fn(oxy_owner_guard::oxy_owner_guard_middleware);
    feature_flags::routes::router()
        .merge(apps::router())
        .merge(orgs_admin::router())
        .merge(users_admin::router())
        .merge(workspaces_admin::router())
        .merge(billing::router().route_layer(strict.clone()))
        .merge(app_admins::router().route_layer(strict))
}

#[cfg(test)]
mod tests {
    //! Regression tests for the route-layer escalation pattern used by
    //! `billing` and `app_admins`. These pin the property that even when
    //! the outer permissive layer in `router::global` admits a Global
    //! Admin (a non-Owner staff member), a route nested under
    //! `route_layer(oxy_owner_guard_middleware)` still rejects with 403.
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
        let app_admins = Router::new()
            .route("/app-admins/probe", get(|| async { StatusCode::OK }))
            .route_layer(strict);
        let open = Router::new().route("/feature-flags/probe", get(|| async { StatusCode::OK }));
        Router::new().merge(billing).merge(app_admins).merge(open)
    }

    async fn request_as(router: Router, path: &str, user: Option<AuthenticatedUser>) -> StatusCode {
        let mut req = Request::builder().uri(path).body(Body::empty()).unwrap();
        if let Some(u) = user {
            req.extensions_mut().insert(u);
        }
        router.oneshot(req).await.unwrap().status()
    }

    /// A non-owner caller (the "Global Admin" case for these tests — the
    /// app_admins DB check is bypassed because we mount only the inner
    /// strict layer) gets 403 on billing and app-admins paths, but 200
    /// on the non-escalated path. This pins the route_layer escalation.
    #[tokio::test]
    async fn strict_layer_rejects_non_owner_on_billing_and_app_admins() {
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
            StatusCode::FORBIDDEN,
            "Global Admin must NOT reach app_admins through the strict route_layer"
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
