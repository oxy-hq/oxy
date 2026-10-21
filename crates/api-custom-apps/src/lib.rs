//! Custom Apps HTTP surface, the serving half: the `/customer-apps/{*path}`
//! route that serves a published bundle (and dispatches `POST …/fn/<name>` to
//! the function runtime), the per-app health, log and client-error reads, and
//! the browser beacon the serve route answers for.
//!
//! - [`serve`]: bundle bytes from the build store, the SPA fallback, the
//!   reserved `__oxy/` namespace and the headers every answer carries.
//! - [`client`]: the service worker, the client runtime and the web manifest the
//!   serve route injects and answers for.
//! - [`beacon`]: the batch the client runtime posts back (`__oxy/beacon`).
//! - [`html_cache`]: the rendered document, memoised per build.
//! - [`sourcemap`]: a browser stack resolved against the build's source maps.
//! - [`health`]: `/customer-apps/{org}/{app}/health`, a deployment-integrity ladder.
//! - [`logs`]: `/customer-apps/{org}/{app}/logs` and `/errors`.
//!
//! What this crate takes from lower crates is in `oxy-custom-apps` (the serve
//! resolver, the build store and its byte cache, the manifest, storage, the usage
//! queue) and `oxy-app-core` (the agent-token body, the function seam traits).
//! Functions, publish, sandboxes, secrets, migrations and the admin app console
//! are still in `oxy-app`; see `internal-docs/custom-apps-architecture.md` §5.
//!
//! Mounted by the `oxy-server` composition root through two seams:
//!
//! - [`public_routes`] rides `SurfaceSeams::public`, merged beside oxy-app's
//!   `build_public_routes` with **no** auth layer. Each handler authenticates
//!   inline (a session cookie or a bearer token), and does so before it looks the
//!   app up, so an anonymous caller cannot tell whether an app exists.
//! - [`serve::serve_dispatch`] rides `SurfaceSeams::outer`, the top-level
//!   `/customer-apps/{*path}` route (its layers are built by the composition
//!   root), with [`serve::serve_dispatch_roles`] as `SurfaceSeams::outer_decls`.

// The serve route's chained cache, store and function futures exceed the default
// type-layout query depth of 128, as the same code did in `oxy-app`.
#![recursion_limit = "256"]

pub mod beacon;
pub mod client;
pub mod health;
pub mod html_cache;
pub mod logs;
pub mod serve;
pub mod sourcemap;

use axum::Router;
use axum::routing::get;
use oxy_app_core::AppState;
use oxy_shared::fleet_role::{RouteRole, RouteRoleDecl};

/// The routes that authenticate inline and so sit outside the auth stack.
///
/// External liveness for ONE published custom app is in the public router
/// because it authenticates inline (session cookie or bearer token) like its
/// neighbours — and the token is mandatory: auth runs before the app lookup so
/// an unauthenticated caller gets 401 whether or not the app exists, which is
/// what keeps this from becoming an app-enumeration oracle. Two forms:
/// slug-explicit (pollable from the admin host, one monitor for every app) and
/// Host-resolved (`/customer-apps/health` on an `<org>--<slug>.customer-apps.<zone>`
/// subdomain, where `/api/*` is the only prefix the subdomain rewrite passes
/// through). NOT a k8s probe — see the [`health`] module docs.
///
/// Per-app debuggability — persisted Oxy Function output, and browser errors
/// with source maps applied server-side — has the same inline AUTHENTICATION as
/// its neighbours, but a narrower AUTHORIZATION: both call `require_app_admin`,
/// because function log output is what the author printed while debugging, not
/// the app's own data that every org member may already see.
pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/customer-apps/{org_slug}/{app_slug}/health",
            get(health::get_health),
        )
        .route("/customer-apps/health", get(health::get_health_for_host))
        .route(
            "/customer-apps/{org_slug}/{app_slug}/logs",
            get(logs::get_logs),
        )
        .route(
            "/customer-apps/{org_slug}/{app_slug}/errors",
            get(logs::get_errors),
        )
}

const FLEET: RouteRole = RouteRole::FleetOk;

/// Every route [`public_routes`] mounts, and which pod may serve it. All FleetOk:
/// health reads the build store and Postgres, and logs are ClickHouse reads plus
/// build-store maps, which every replica has — declared explicitly because the
/// type gate `RoleRouter::route_fleet` used to provide does not reach across the
/// crate line. Paths are absolute: the public seam merges this crate at the
/// `/api` root.
pub fn public_route_roles() -> &'static [RouteRoleDecl] {
    &[
        RouteRoleDecl {
            method: "*",
            path: "/customer-apps/{org_slug}/{app_slug}/health",
            role: FLEET,
        },
        RouteRoleDecl {
            method: "*",
            path: "/customer-apps/health",
            role: FLEET,
        },
        RouteRoleDecl {
            method: "*",
            path: "/customer-apps/{org_slug}/{app_slug}/logs",
            role: FLEET,
        },
        RouteRoleDecl {
            method: "*",
            path: "/customer-apps/{org_slug}/{app_slug}/errors",
            role: FLEET,
        },
    ]
}
