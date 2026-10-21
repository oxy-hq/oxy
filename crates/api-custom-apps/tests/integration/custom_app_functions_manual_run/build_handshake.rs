//! The `/fn` build handshake, end to end through the serve route: every answer
//! names the build it ran against (`x-oxy-live-build`), and a call from a page
//! on a DIFFERENT build (`x-oxy-client-build`) asking for a function the live
//! build lacks is told `409 ClientBuildStale` rather than a bare 404 — while a
//! call that names no build, or the live one, keeps the 404 it always had.

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

use super::readback::{APP, PRODUCTION_BUILD, STAGING_BUILD, build_pk, two_builds};
use crate::custom_app_functions_fixture::{Tenant, seeded_tenant, serve_router};
use crate::staging_functions::{make_guest_staff, production_host};

async fn call(
    t: &Tenant,
    name: &str,
    client_build: Option<&str>,
) -> (StatusCode, HeaderMap, String) {
    let mut request = Request::builder()
        .method("POST")
        .uri(format!("/customer-apps/{}/{APP}/fn/{name}", t.org_slug))
        .header("content-type", "application/json")
        .header("host", production_host(t, APP));
    if let Some(build) = client_build {
        request = request.header("x-oxy-client-build", build);
    }
    let response = serve_router()
        .oneshot(request.body(Body::from("{}")).expect("request"))
        .await
        .expect("oneshot");
    let (status, headers) = (response.status(), response.headers().clone());
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    (
        status,
        headers,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

fn live_build(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-oxy-live-build")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

#[tokio::test]
async fn fn_names_the_live_build_and_labels_a_stale_callers_miss() {
    let t = seeded_tenant().await;
    let app_id = two_builds(&t, APP, PRODUCTION_BUILD, STAGING_BUILD).await;
    make_guest_staff();
    let live = build_pk(&t, app_id, PRODUCTION_BUILD).await.to_string();

    // A call that ran names the build it ran in.
    let (status, headers, body) = call(&t, "whoami", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(live_build(&headers).as_deref(), Some(live.as_str()));

    // An older page asking for a function the live build lacks: stale.
    let older = Uuid::new_v4().to_string();
    let (status, headers, body) = call(&t, "renamed-away", Some(&older)).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let json: Value = serde_json::from_str(&body).expect("json");
    assert_eq!(json["error"], "ClientBuildStale");
    assert_eq!(json["live"], live);
    assert_eq!(live_build(&headers).as_deref(), Some(live.as_str()));

    // No header is unknown, never stale; the live build's own page gets the
    // plain 404 too.
    for client in [None, Some(live.as_str()), Some("not-a-uuid")] {
        let (status, headers, body) = call(&t, "renamed-away", client).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{client:?}: {body}");
        let json: Value = serde_json::from_str(&body).expect("json");
        assert_eq!(json["error"], "FunctionNotFound", "{client:?}");
        assert_eq!(live_build(&headers).as_deref(), Some(live.as_str()));
    }

    // An older page calling a function that still exists is served, never
    // refused: a publish must not become an outage for open tabs.
    let (status, _, body) = call(&t, "whoami", Some(&older)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
}
