use super::*;
use axum::Router;
use axum::body::to_bytes;
use axum::http::Request as HttpRequest;
use axum::middleware;
use axum::routing::{get, post};
use tower::ServiceExt;

fn install_roles() {
    crate::server::role_manifest::install_route_declarations_for_tests();
}

fn nested_router() -> Router {
    let workspace_routes = Router::new()
        .route("/compile", post(|| async { "should not reach" }))
        .route("/threads", get(|| async { "threads ok" }));
    let api_routes = Router::new().nest("/{workspace_id}", workspace_routes);
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .nest("/api", api_routes)
        .layer(middleware::from_fn(enforce_role))
}

#[tokio::test]
async fn ide_only_route_on_serve_replica_returns_421_through_nest() {
    install_roles();
    unsafe { std::env::set_var("OXY_ROLE", "serve") };
    crate::server::role_manifest::init_process_role_from_env();

    let resp = nested_router()
        .oneshot(
            HttpRequest::post("/api/some-uuid/compile")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::MISDIRECTED_REQUEST);
    assert_eq!(resp.headers().get(HEADER_REQUIRED_ROLE).unwrap(), "ide");
    assert!(
        resp.headers()
            .get(HEADER_SERVED_BY)
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("serve@")
    );

    unsafe { std::env::remove_var("OXY_ROLE") };
}

#[tokio::test]
async fn worker_with_upstream_does_not_forward_ide_route() {
    install_roles();
    unsafe {
        std::env::set_var("OXY_ROLE", "worker");
        std::env::set_var("OXY_IDE_UPSTREAM", "http://ide.invalid:80");
    }
    crate::server::role_manifest::init_process_role_from_env();

    let resp = nested_router()
        .oneshot(
            HttpRequest::post("/api/some-uuid/compile")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::MISDIRECTED_REQUEST);
    unsafe {
        std::env::remove_var("OXY_ROLE");
        std::env::remove_var("OXY_IDE_UPSTREAM");
    }
}

#[tokio::test]
async fn already_forwarded_ide_route_on_serve_breaks_loop() {
    install_roles();
    unsafe {
        std::env::set_var("OXY_ROLE", "serve");
        std::env::set_var("OXY_IDE_UPSTREAM", "http://ide.invalid:80");
    }
    crate::server::role_manifest::init_process_role_from_env();

    let resp = nested_router()
        .oneshot(
            HttpRequest::post("/api/some-uuid/compile")
                .header("x-oxy-forwarded-by", "serve")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::MISDIRECTED_REQUEST);
    unsafe {
        std::env::remove_var("OXY_ROLE");
        std::env::remove_var("OXY_IDE_UPSTREAM");
    }
}

#[tokio::test]
async fn fleet_ok_route_on_serve_replica_passes_through_nest() {
    install_roles();
    unsafe { std::env::set_var("OXY_ROLE", "serve") };
    crate::server::role_manifest::init_process_role_from_env();

    let resp = nested_router()
        .oneshot(
            HttpRequest::get("/api/some-uuid/threads")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), 1024).await.unwrap();
    assert_eq!(&body[..], b"threads ok");

    unsafe { std::env::remove_var("OXY_ROLE") };
}

#[tokio::test]
async fn health_probe_passes_on_every_role_including_worker() {
    install_roles();
    for role in ["ide", "serve", "worker"] {
        unsafe { std::env::set_var("OXY_ROLE", role) };
        crate::server::role_manifest::init_process_role_from_env();
        let resp = nested_router()
            .oneshot(HttpRequest::get("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "health probe failed under OXY_ROLE={role}"
        );
    }
    unsafe { std::env::remove_var("OXY_ROLE") };
}

#[tokio::test]
async fn all_role_accepts_everything() {
    install_roles();
    unsafe { std::env::remove_var("OXY_ROLE") };
    crate::server::role_manifest::init_process_role_from_env();

    let resp = nested_router()
        .oneshot(
            HttpRequest::post("/api/some-uuid/compile")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        resp.headers()
            .get(HEADER_SERVED_BY)
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("all@")
    );
}

mod branch_escalation {
    use super::*;

    #[test]
    fn a_branch_escalates_a_fleet_route() {
        assert_eq!(
            escalate_for_branch(RouteRole::FleetOk, Some("branch=feature-x")),
            RouteRole::IdeOnly
        );
        assert_eq!(
            escalate_for_branch(RouteRole::FleetOk, Some("limit=10&branch=feature-x&x=1")),
            RouteRole::IdeOnly
        );
    }

    #[test]
    fn nothing_de_escalates_an_ide_route() {
        for query in [None, Some(""), Some("branch="), Some("branch=main")] {
            assert_eq!(
                escalate_for_branch(RouteRole::IdeOnly, query),
                RouteRole::IdeOnly,
                "query {query:?} must not relax an IdeOnly route"
            );
        }
    }

    #[test]
    fn an_empty_or_absent_branch_does_not_escalate() {
        for query in [None, Some(""), Some("branch="), Some("limit=10")] {
            assert_eq!(
                escalate_for_branch(RouteRole::FleetOk, query),
                RouteRole::FleetOk,
                "query {query:?} must not pin a fleet route to the ide"
            );
        }
    }

    #[test]
    fn a_lookalike_parameter_does_not_escalate() {
        for query in ["default_branch=x", "branchy=x", "base_branch=main"] {
            assert_eq!(
                escalate_for_branch(RouteRole::FleetOk, Some(query)),
                RouteRole::FleetOk,
                "{query} is not a branch hint"
            );
        }
    }
}
