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

pub mod admin;
mod app_function_agent;
pub mod app_function_executor;
pub mod beacon;
pub mod client;
pub mod custom_app_provenance;
pub mod custom_app_template;
pub mod custom_apps_activity;
pub mod custom_apps_agent_built;
pub mod custom_apps_agent_refusal;
pub mod custom_apps_availability;
pub mod custom_apps_build_history;
pub mod custom_apps_debug;
pub mod custom_apps_env_schedules;
pub mod custom_apps_functions;
pub mod custom_apps_migrations;
pub mod custom_apps_nonproduction;
pub mod custom_apps_open_builds;
/// `oxyc promote` / `oxyc rollback`, and the one function that moves
/// production's pointer for every writer.
pub mod custom_apps_promote;
pub mod custom_apps_publish;
/// The audit row of a bundle publish, and what one says of a publish token.
pub(crate) mod custom_apps_publish_audit;
pub mod custom_apps_publish_form;
pub mod custom_apps_publish_oidc;
pub mod custom_apps_publish_refusal;
pub mod custom_apps_sandboxes;
pub mod custom_apps_scaffold;
pub mod custom_apps_secrets;
pub mod custom_apps_shell_context;
pub mod custom_apps_staging_held;
pub mod custom_apps_tail;
pub mod custom_apps_threads;
pub mod custom_apps_validate;
pub mod health;
pub mod html_cache;
mod invocation_placement;
pub mod logs;
pub mod me_apps;
pub mod seed;
pub mod seed_apps;
pub mod seed_storage;
pub mod serve;
pub mod sourcemap;
pub mod webhooks;
pub mod workspace_custom_apps;

// ── the names the moved modules reach their neighbours by ─────────────────────
//
// The modules above came out of `oxy-app`'s `server::api`, where every
// `custom_apps_*` module and the lowered ones' aliases sat side by side in
// `api/mod.rs`, and a module reached a sibling as `super::custom_apps_x`. They
// sit at this crate's root now, so the same paths resolve here, to the same
// code: the aliases below are `api/mod.rs`'s, pointing at the crates the
// modules were lowered into (`oxy-custom-apps`, `oxy-app-core`).
pub use oxy_app_core::agent_body as custom_apps_agent_body;
#[cfg(test)]
pub(crate) use oxy_app_core::agent_fixture as custom_apps_agent_fixture;
pub use oxy_custom_apps::asset_manifest as custom_apps_asset_manifest;
pub use oxy_custom_apps::build_state as custom_apps_build_state;
pub use oxy_custom_apps::build_store as custom_apps_build_store;
pub use oxy_custom_apps::bundle_cache as custom_apps_bundle_cache;
pub use oxy_custom_apps::cache as custom_apps_cache;
pub use oxy_custom_apps::env_resolve as custom_apps_env_resolve;
pub use oxy_custom_apps::environments as custom_apps_environments;
pub use oxy_custom_apps::manifest as custom_apps_manifest;
pub use oxy_custom_apps::non_production as custom_apps_non_production;
pub use oxy_custom_apps::precompress as custom_apps_precompress;
pub use oxy_custom_apps::publish_authz as custom_apps_publish_authz;
pub use oxy_custom_apps::sandbox_instance as custom_apps_sandbox_instance;
pub use oxy_custom_apps::source as custom_apps_source;
pub use oxy_custom_apps::storage as custom_apps_storage;
pub use oxy_custom_apps::sync as custom_apps_sync;
pub use oxy_custom_apps::telemetry as custom_apps_telemetry;
pub use oxy_custom_apps::tracking as custom_apps_tracking;
pub use oxy_custom_apps::updates as custom_apps_updates;
pub use oxy_custom_apps::usage_airhouse as custom_apps_usage_airhouse;
pub use oxy_custom_apps::usage_queue as custom_apps_usage_queue;

/// A sandbox agent token on the custom-app serving paths: the admission rules
/// (`oxy_custom_apps::agent`) and the usage count that wraps `/fn` and `/logs`.
pub mod custom_apps_agent {
    pub use oxy_app_core::usage_probe::{UsageProbe, seen};
    pub use oxy_custom_apps::agent::*;
}

/// Who may open an app, and the request-facing flow every handler calls.
pub mod custom_apps_auth {
    pub use oxy_custom_apps::auth::*;
    pub use oxy_custom_apps::authenticate::*;
}

/// The data-plane gate chain, answered as a response: `oxy_custom_apps::gate`
/// holds the steps and the refusals; this maps a refusal onto the JSON envelope
/// the SDK reads (`oxy_app_core::gate_response`).
pub mod custom_apps_gates {
    use axum::http::HeaderMap;
    use axum::response::Response;
    use uuid::Uuid;

    pub use oxy_custom_apps::gate::{CustomAppContext, frontline_worker_with_app_grant};

    /// `oxy_custom_apps::gate::check_custom_app_gates`, with a refusal turned
    /// into its response.
    pub async fn check_custom_app_gates(
        headers: &HeaderMap,
        project_id: Uuid,
    ) -> Result<CustomAppContext, Response> {
        oxy_custom_apps::gate::check_custom_app_gates(headers, project_id)
            .await
            .map_err(|e| oxy_app_core::gate_response::refusal(e.status(), e.message(), e.code()))
    }
}

/// A staging build's semantic pin and the request that names one.
pub mod custom_apps_staging_pin {
    pub use oxy_app_core::staging_pin_scope::{
        cache_partition, current_staging_pin, with_staging_pin,
    };
    pub use oxy_custom_apps::staging_pin::request;
    pub use oxy_custom_apps::staging_pin::{
        PinRefusal, SemanticPinNotice, check_pinnable, pin_drift, pinned_revision_for,
        validate_pin_for_publish,
    };
}

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
