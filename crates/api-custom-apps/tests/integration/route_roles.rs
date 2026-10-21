//! Every route this crate serves answers from any replica.
//!
//! Moved here from `oxy-app`'s `tests/routing/global_route_roles.rs` with the
//! routes. `oxy-app` cannot depend on this crate, so the declaration set its
//! harness installs no longer contains them, and `classify` there would answer
//! with the FleetOk default — a pass that says nothing about the product. Here
//! the declarations are the ones `oxy-server` hands in, asked through the same
//! `classify` the request path uses.
//!
//! `/customer-apps/{*path}` is the case that motivated the original assertion.
//! `serve_dispatch` answers everything under it: bundle bytes from S3, and
//! `POST …/fn/<name>`, which executes an Oxy Function. The function half was
//! IdeOnly while it read the working copy; it reads the promoted revision now,
//! so a custom app's backend runs on whichever replica the request lands on.
//! Both halves are asserted, because a declaration that covered only one would
//! look right.

use oxy_api_custom_apps::public_route_roles;
use oxy_api_custom_apps::serve::serve_dispatch_roles;
use oxy_app::surface::roles::{RouteRole, classify, install_route_declarations_for_tests_with};

const ORG: &str = "acme";
const APP: &str = "dash";

/// The public seam merges this crate at the `/api` root with no prefix, so its
/// declared paths are absolute and the installer only adds `/api`.
fn install() {
    let extra = public_route_roles()
        .iter()
        .map(|d| (d.method, d.path.to_string(), d.role))
        .collect();
    install_route_declarations_for_tests_with(extra);
}

#[test]
fn health_logs_and_errors_stay_on_the_fleet() {
    install();
    for (method, path) in [
        ("GET", format!("/api/customer-apps/{ORG}/{APP}/health")),
        ("GET", "/api/customer-apps/health".to_string()),
        ("GET", format!("/api/customer-apps/{ORG}/{APP}/logs")),
        ("GET", format!("/api/customer-apps/{ORG}/{APP}/errors")),
    ] {
        assert_eq!(
            classify(method, &path),
            RouteRole::FleetOk,
            "{method} {path}: an availability read pinned to the singleton would go dark \
             exactly when the singleton is the thing in trouble",
        );
    }
}

#[test]
fn the_serve_route_and_its_function_entry_both_run_on_any_replica() {
    let roles = serve_dispatch_roles();
    let role_of = |path: &str| {
        roles
            .iter()
            .find(|d| d.path == path)
            .unwrap_or_else(|| panic!("`serve_dispatch_roles` declares no `{path}`"))
            .role
    };
    assert_eq!(
        role_of("/customer-apps/{org}/{app}/fn/{name}"),
        RouteRole::FleetOk,
        "an Oxy Function reads its workspace through the compile boundary",
    );
    assert_eq!(
        role_of("/customer-apps/{*path}"),
        RouteRole::FleetOk,
        "bundle bytes come from S3",
    );
    assert!(
        roles.iter().all(|d| d.method == "*"),
        "the serve route takes every method: POST form submits, Server Actions, the beacon"
    );
}
