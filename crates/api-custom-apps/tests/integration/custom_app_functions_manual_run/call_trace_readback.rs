//! The call-trace read-back: `GET /customer-apps/{id}/invocations/{inv}/calls`.
//!
//! - a real `/fn` call leaves one record per `ctx.*` call, holding the fetch's
//!   host and never its path;
//! - a staging call's held fetch reads `held`, a call that made no `ctx.*`
//!   call reads `recorded` with no calls;
//! - another app's invocation id is the same `404` as an unknown one, and a
//!   caller without non-production reach is refused a staging invocation;
//! - deleting the invocation row deletes its trace.
//!
//! The trace is written off the request path, so each read polls until it
//! lands.

use std::time::{Duration, Instant};

use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use entity::users::UserStatus;
use oxy_api_custom_apps::admin::apps::invocation_calls;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use oxy_auth::types::AuthenticatedUser;
use sea_orm::EntityTrait;
use serde_json::Value;
use uuid::Uuid;

use super::readback::{APP, OTHER_APP, PRODUCTION_BUILD, STAGING_BUILD, ran, two_builds};
use super::{answer, get_admin};
use crate::custom_app_functions_fixture::seeded_tenant;
use crate::staging_functions::{make_guest_staff, production_host, staging_host};

const WRITE_DEADLINE: Duration = Duration::from_secs(20);

async fn calls_of(app: Uuid, invocation: Uuid) -> (StatusCode, Value) {
    get_admin(&format!(
        "/customer-apps/{app}/invocations/{invocation}/calls"
    ))
    .await
}

/// The trace once it has landed: polled, because the write is spawned.
async fn recorded(app: Uuid, invocation: Uuid) -> Value {
    let started = Instant::now();
    loop {
        let (status, body) = calls_of(app, invocation).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        if body["recorded"] == true {
            return body;
        }
        assert!(
            started.elapsed() < WRITE_DEADLINE,
            "the trace never landed: {body}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn the_calls_route_returns_one_invocations_calls_and_no_one_elses() {
    let t = seeded_tenant().await;
    let app_id = two_builds(&t, APP, PRODUCTION_BUILD, STAGING_BUILD).await;
    let other_app = two_builds(&t, OTHER_APP, "rb-other-prod", "rb-other-stg").await;
    make_guest_staff();

    let live = ran(&t, APP, "writes", &production_host(&t, APP)).await;
    let staged = ran(&t, APP, "writes", &staging_host(&t, APP)).await;
    let quiet = ran(&t, APP, "whoami", &production_host(&t, APP)).await;
    let theirs = ran(&t, OTHER_APP, "writes", &production_host(&t, OTHER_APP)).await;

    // Production sent the fetch, to a host that does not resolve.
    let body = recorded(app_id, live).await;
    assert_eq!(body["total_calls"], 1, "{body}");
    assert_eq!(
        (&body["dropped"], &body["unfinished"]),
        (&0.into(), &0.into())
    );
    assert!(body["init_ms"].as_u64().is_some(), "{body}");
    let fetch = &body["calls"][0];
    assert_eq!(fetch["op"], "fetch", "{body}");
    assert_eq!(fetch["family"], "fetch");
    assert_eq!(fetch["verb"], "POST");
    assert_eq!(fetch["target"], "readback-probe.invalid");
    assert_eq!(fetch["status"], "error", "{body}");
    assert!(fetch["error_type"].as_str().is_some(), "{body}");
    assert!(
        !body.to_string().contains("/orders"),
        "the fetch's path is never recorded: {body}"
    );
    assert!(
        fetch["start_ms"].as_u64() >= body["init_ms"].as_u64(),
        "a call starts after the isolate: {body}"
    );

    // Staging held the same fetch; it did not fail.
    let body = recorded(app_id, staged).await;
    assert_eq!(body["environment"], "staging");
    assert_eq!(body["calls"][0]["status"], "held", "{body}");
    assert_eq!(body["calls"][0]["error_type"], "held");

    // A function that made no ctx call is recorded, with nothing in it.
    let body = recorded(app_id, quiet).await;
    assert_eq!(body["calls"], serde_json::json!([]), "{body}");
    assert_eq!(body["total_calls"], 0);
    assert!(body["reason"].is_null(), "{body}");

    // Another app's invocation through this app's path, and the other way
    // round, is the same 404 as an id that does not exist.
    for (app, invocation) in [
        (app_id, theirs),
        (other_app, live),
        (app_id, Uuid::new_v4()),
    ] {
        let (status, body) = calls_of(app, invocation).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{app}/{invocation}: {body}");
        assert_eq!(body["error"], "invocation_not_found");
        assert!(body.get("calls").is_none(), "{body}");
    }

    // Without non-production reach, a staging invocation's calls are refused.
    let outsider = AuthenticatedUserExtractor(AuthenticatedUser {
        id: Uuid::new_v4(),
        email: Some("tenant-admin@customer.example".to_string()),
        name: "Outsider".to_string(),
        picture: None,
        status: UserStatus::Active,
        credential: None,
    });
    let refused = invocation_calls::get_invocation_calls(outsider, None, Path((app_id, staged)))
        .await
        .map(|_| ())
        .expect_err("a staging invocation's calls are staff's");
    let (status, body) = answer(refused.into_response()).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"], "non_production_refused");

    // The trace goes with its invocation row.
    entity::prelude::AppFunctionInvocations::delete_by_id(live)
        .exec(&t.db)
        .await
        .expect("delete the invocation");
    let trace = entity::prelude::AppFunctionInvocationCalls::find_by_id(live)
        .one(&t.db)
        .await
        .expect("read the trace");
    assert!(trace.is_none(), "the trace cascades with its invocation");
}
