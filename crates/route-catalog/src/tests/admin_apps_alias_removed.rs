//! `/api/customer-apps` is the only mount of the admin custom-app handlers.
//!
//! `/api/admin/apps` was kept for one release as an alias of the same handlers
//! because `oxyc` still called it. `oxyc` calls `/api/customer-apps` now, and
//! the alias is gone. The catalog is the walk of every route the router mounts,
//! so a path missing from it reaches no handler and answers 404.

use super::app_grant_scope::shape_of;
use crate::catalog;

fn mounted(method: &str, path: &str) -> bool {
    catalog()
        .routes
        .iter()
        .any(|route| route.method == method && shape_of(route.path) == shape_of(path))
}

/// Nothing is mounted at or under `/api/admin/apps`, for any method.
#[test]
fn admin_apps_is_not_mounted() {
    let leftovers: Vec<String> = catalog()
        .routes
        .iter()
        .filter(|route| {
            route.path == "/api/admin/apps" || route.path.starts_with("/api/admin/apps/")
        })
        .map(|route| format!("{} {}", route.method, route.path))
        .collect();
    assert!(
        leftovers.is_empty(),
        "`/api/admin/apps` is mounted again; `/api/customer-apps` is the only mount of the \
         admin app handlers: {leftovers:?}"
    );
}

/// The routes the alias used to serve are all still answered at `/api/customer-apps`.
#[test]
fn customer_apps_still_serves_the_routes_the_alias_did() {
    for (method, path) in [
        ("GET", "/api/customer-apps"),
        ("POST", "/api/customer-apps"),
        ("GET", "/api/customer-apps/{id}"),
        ("PATCH", "/api/customer-apps/{id}"),
        ("DELETE", "/api/customer-apps/{id}"),
        ("POST", "/api/customer-apps/{id}/publish"),
        ("GET", "/api/customer-apps/{id}/functions"),
        ("POST", "/api/customer-apps/{id}/functions/{name}/runs"),
        ("GET", "/api/customer-apps/{id}/function-runs/{run_id}"),
        ("GET", "/api/customer-apps/{id}/invocations"),
        ("GET", "/api/customer-apps/fleet-health"),
    ] {
        assert!(
            mounted(method, path),
            "{method} {path} is no longer mounted"
        );
    }
}
