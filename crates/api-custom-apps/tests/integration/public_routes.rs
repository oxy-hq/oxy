//! The routes `public_routes()` mounts are the ones `oxy-server` merges beside
//! the public tree, and each is reachable. No database: the Host-resolved health
//! read answers before it authenticates when the `Host` names no custom app, and
//! that is the one answer that needs no credential.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use oxy_api_custom_apps::public_routes;
use oxy_app::surface::bare_app_state;
use tower::ServiceExt;

fn app() -> axum::Router {
    public_routes().with_state(bare_app_state())
}

#[tokio::test]
async fn the_host_resolved_health_read_is_mounted_and_refuses_an_admin_host() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/customer-apps/health")
                .header(header::HOST, "app.oxygen-hq.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "a host that names no custom app is told to use the slug-explicit route, \
         not asked for a credential"
    );
}

#[tokio::test]
async fn a_route_this_crate_does_not_mount_is_a_404() {
    // `availability` and `debug` are the siblings that stay in `oxy-app`'s public
    // tree; they must not be reachable here, or the same path would be mounted
    // twice and the composition root's merge would panic.
    for path in [
        "/customer-apps/acme/dash/availability",
        "/customer-apps/acme/dash/debug",
    ] {
        let response = app()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
    }
}
