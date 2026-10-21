//! Row T1: the token tails a sandbox it created, asked what `/logs` asks of
//! it (`custom_apps_tail::admission`), and nothing else.

use axum::http::StatusCode;
use serde_json::Value;
use uuid::Uuid;

use super::fixture::agent;
use crate::app_environments::seed_sandbox;
use crate::sandbox_tail::fixture::install_listener;

/// Its own sandbox streams, `open` to `done`. Production, staging (it was
/// minted without the grant) and a sandbox a person created are each the
/// `404 environment_not_found` of the other environment routes, in the
/// token's one body; another app's id is the allow-list's `404`.
#[tokio::test]
async fn a_token_tails_its_own_sandbox_and_nothing_else() {
    let a = agent().await;
    let (status, body) = a.create("dev-own").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    seed_sandbox(&a.t.db, a.app.id, "dev-col", a.t.guest_id).await;
    install_listener();
    let tail = |app: Uuid, environment: &str| {
        format!("/customer-apps/{app}/environments/{environment}/tail?timeout=1")
    };

    let (status, body) = a.get(&tail(a.app.id, "dev-own")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let stream = body.as_str().expect("an event stream, not JSON");
    for event in ["event: open", "event: done"] {
        assert!(stream.contains(event), "{event} in {stream}");
    }

    for environment in ["production", "staging", "dev-col"] {
        let (status, body) = a.get(&tail(a.app.id, environment)).await;
        assert_eq!(
            (status, &body["code"]),
            (StatusCode::NOT_FOUND, &Value::from("environment_not_found")),
            "{environment}: {body}"
        );
    }
    let (status, body) = a.get(&tail(Uuid::new_v4(), "dev-own")).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}
