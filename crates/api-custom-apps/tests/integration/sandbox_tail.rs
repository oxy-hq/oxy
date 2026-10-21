//! The live tail of an environment, `GET /api/customer-apps/{id}/environments/
//! {name}/tail` (`custom_apps_tail`), end to end in one process: the route as
//! `oxy-app` mounts it, the real `LISTEN` connection, the follower inside
//! `run_with_runtime`, and the watcher table between them. The process both
//! produces and listens, so a line goes out over `NOTIFY` and comes back in
//! through `LISTEN` exactly as it would between two replicas.
//!
//! **Needs** Postgres only.

pub(crate) mod fixture;

use axum::http::StatusCode;
use chrono::Utc;
use entity::org_members::OrgRole;
use entity::users::UserStatus;
use oxy_api_custom_apps::custom_apps_tail::admission::admit;
use oxy_api_custom_apps::custom_apps_tail::{event, producer, recheck};
use oxy_auth::types::AuthenticatedUser;
use oxy_auth::user::LOCAL_GUEST_EMAIL;
use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
use serde_json::Value;
use uuid::Uuid;

use self::fixture::{call, install_listener, invocation_of, open, raw_listen, tailed_app};
use crate::custom_app_functions_fixture::seeded_tenant;
use crate::custom_app_visibility::{seed_member, seed_user};
use crate::sandbox_publish::app_with_two_sandboxes;

fn tail_of(app: Uuid, environment: &str) -> String {
    format!("/customer-apps/{app}/environments/{environment}/tail")
}

/// `name`, then what tells events of its kind apart.
fn shape(event: &str, data: &Value) -> String {
    match event {
        "log" => format!("log:{}:{}:{}", data["seq"], data["level"], data["message"]),
        "finish" => format!("finish:{}", data["status"]),
        other => other.to_string(),
    }
}

/// A call in a tailed sandbox reaches the subscriber as it runs: `start`,
/// each line in order with its level, then `finish` — for that sandbox's
/// invocation, and nothing of the other sandbox's.
#[tokio::test]
async fn a_tailed_sandbox_call_delivers_start_lines_and_finish() {
    let t = seeded_tenant().await;
    let app = tailed_app(&t).await;
    install_listener();
    let mut tail = open(&tail_of(app.id, "dev-a1"), &[]).await.expect("opens");
    let opened = tail.until("open").await;
    let open = opened.last().expect("open");
    assert_eq!(
        (&open.data["environment"], &open.data["resumed"]),
        (&Value::from("dev-a1"), &Value::Bool(false))
    );

    call(&t, "dev-b2").await;
    let called = call(&t, "dev-a1").await;
    let events = tail.until("finish").await;
    let shapes: Vec<String> = events.iter().map(|e| shape(&e.event, &e.data)).collect();
    assert_eq!(
        shapes,
        [
            "start",
            r#"log:1:"info":"one""#,
            r#"log:2:"warn":"two""#,
            r#"finish:"success""#
        ]
    );
    let invocation = invocation_of(&called);
    for e in &events {
        assert_eq!(e.data["invocation_id"], invocation.as_str(), "{e:?}");
        assert_eq!(e.data["function"], "chatty");
    }
    let ids: Vec<i64> = events
        .iter()
        .map(|e| e.id.as_deref().expect("an id").parse().expect("ms"))
        .collect();
    assert!(ids.windows(2).all(|w| w[0] < w[1]), "ids increase: {ids:?}");
}

/// A producer `NOTIFY`s only for a pair someone is tailing: with `dev-a1`
/// tailed, a call in `dev-b2` sends nothing on the channel, and a call in
/// `dev-a1` is heard on the same connection.
#[tokio::test]
async fn an_unwatched_pair_sends_no_notify() {
    let t = seeded_tenant().await;
    let app = tailed_app(&t).await;
    install_listener();
    let (_client, mut heard) = raw_listen().await;
    let mut tail = open(&tail_of(app.id, "dev-a1"), &[]).await.expect("opens");
    tail.until("open").await;

    call(&t, "dev-b2").await;
    let quiet = producer::FOLLOW_EVERY * 4;
    assert!(
        tokio::time::timeout(quiet, heard.recv()).await.is_err(),
        "an unwatched sandbox's call was sent on the channel"
    );

    call(&t, "dev-a1").await;
    let payload = tokio::time::timeout(std::time::Duration::from_secs(10), heard.recv())
        .await
        .expect("the watched call is heard")
        .expect("a payload");
    let envelope = event::unpack(&payload).expect("an envelope");
    assert_eq!(
        (envelope.app_id, envelope.environment.as_str()),
        (app.id, "dev-a1")
    );
}

/// A stream opened with `Last-Event-ID` replays the outcomes of the
/// invocations that finished after it — marked `replayed`, oldest first —
/// and none of their lines.
#[tokio::test]
async fn a_resume_replays_outcomes_not_lines() {
    let t = seeded_tenant().await;
    let app = tailed_app(&t).await;
    install_listener();
    // A minute back: the rows' times are the writer's clock, this is the
    // test's, and nothing ran in `dev-a1` before these two calls.
    let before = Utc::now().timestamp_millis() - 60_000;
    let first = invocation_of(&call(&t, "dev-a1").await);
    let second = invocation_of(&call(&t, "dev-a1").await);
    call(&t, "dev-b2").await;

    let resume = before.to_string();
    let mut tail = open(&tail_of(app.id, "dev-a1"), &[("last-event-id", &resume)])
        .await
        .expect("opens");
    let opened = tail.until("open").await;
    assert_eq!(opened.last().expect("open").data["resumed"], true);
    let replayed = [tail.next().await, tail.next().await];
    for (e, invocation) in replayed.iter().zip([&first, &second]) {
        assert_eq!(e.event, "finish", "no lines are replayed: {e:?}");
        assert_eq!(e.data["invocation_id"], invocation.as_str());
        assert_eq!(
            (&e.data["replayed"], &e.data["status"]),
            (&Value::Bool(true), &Value::from("success"))
        );
        let id: i64 = e.id.as_deref().expect("an id").parse().expect("ms");
        assert!(
            id > before,
            "a resumed stream's ids continue after the one it was given"
        );
    }
}

fn user(id: Uuid, email: &str) -> AuthenticatedUser {
    AuthenticatedUser {
        id,
        email: Some(email.to_string()),
        name: "Tail".to_string(),
        picture: None,
        status: UserStatus::Active,
        credential: None,
    }
}

async fn refused(user: &AuthenticatedUser, publish_token: bool, app: Uuid, env: &str) -> String {
    match admit(user, publish_token, app, env).await {
        Ok(_) => "admitted".to_string(),
        Err(e) => e.code().to_string(),
    }
}

/// Who may tail what: staff with non-production reach tail every existing
/// environment; every environment needs the app's admin first, as `/logs`
/// does (an org officer is one, a plain member is not); staging and sandboxes
/// then need non-production reach, which no tenant has; a publish token is
/// refused whoever minted it.
#[tokio::test]
async fn every_environment_needs_an_admin_and_the_rest_non_production_reach() {
    let t = seeded_tenant().await;
    let app = app_with_two_sandboxes(&t, fixture::APP).await.id;
    let staff = user(t.guest_id, LOCAL_GUEST_EMAIL);
    for environment in ["production", "staging", "dev-a1"] {
        assert_eq!(
            refused(&staff, false, app, environment).await,
            "admitted",
            "{environment}"
        );
    }
    assert_eq!(
        refused(&staff, false, app, "dev-zz").await,
        "environment_not_found"
    );
    assert_eq!(
        refused(&staff, false, app, "prod").await,
        "invalid_environment"
    );
    assert_eq!(
        refused(&staff, true, app, "dev-a1").await,
        "publish_token_refused"
    );
    assert_eq!(
        refused(&staff, false, Uuid::new_v4(), "dev-a1").await,
        "app_not_found"
    );

    let (member_id, member_email) = seed_user(&t.db).await;
    seed_member(&t.db, t.org_id, member_id, OrgRole::Member).await;
    let member = user(member_id, &member_email);
    // A non-admin is turned away by the admin gate in every environment,
    // before non-production reach is asked: `app_admin_required`, never the
    // `non_production_refused` it used to get on staging and a sandbox.
    for environment in ["production", "staging", "dev-a1"] {
        assert_eq!(
            refused(&member, false, app, environment).await,
            "app_admin_required",
            "{environment}"
        );
    }

    let (officer_id, officer_email) = seed_user(&t.db).await;
    seed_member(&t.db, t.org_id, officer_id, OrgRole::Admin).await;
    let officer = user(officer_id, &officer_email);
    assert_eq!(
        refused(&officer, false, app, "production").await,
        "admitted"
    );
    for environment in ["staging", "dev-a1"] {
        assert_eq!(
            refused(&officer, false, app, environment).await,
            "non_production_refused",
            "{environment}"
        );
    }
}

/// What an open stream is asked every minute: yes while its caller may read
/// it, no once the sandbox is being deleted — and no for a caller who never
/// could. A `no` ends the stream with `done {"reason":"access_revoked"}`.
#[tokio::test]
async fn an_open_stream_is_asked_again_and_a_deleted_sandbox_ends_it() {
    let t = seeded_tenant().await;
    let app = app_with_two_sandboxes(&t, fixture::APP).await.id;
    let staff = recheck::of(user(t.guest_id, LOCAL_GUEST_EMAIL), app, "dev-a1".into());
    assert!(staff().await, "staff still may");

    let (member_id, member_email) = seed_user(&t.db).await;
    seed_member(&t.db, t.org_id, member_id, OrgRole::Member).await;
    let member = recheck::of(user(member_id, &member_email), app, "dev-a1".into());
    assert!(!member().await, "a tenant member never could");

    t.db.execute_raw(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "UPDATE app_environments SET deleting_at = now() WHERE app_id = $1 AND name = 'dev-a1'",
        [app.into()],
    ))
    .await
    .expect("mark the sandbox deleting");
    assert!(!staff().await, "a sandbox being deleted ends its streams");
}

/// Through the route as mounted: a refusal is the sandbox routes' coded
/// JSON with its status, and a stream given `?timeout=` ends with
/// `done {"reason":"timeout"}` and then closes.
#[tokio::test]
async fn the_route_codes_its_refusals_and_ends_on_its_timeout() {
    let t = seeded_tenant().await;
    let app = app_with_two_sandboxes(&t, fixture::APP).await;
    install_listener();
    let refusal = |uri: String| async move { open(&uri, &[]).await.err().expect("refused") };
    let (status, body) = refusal(format!("{}?timeout=0", tail_of(app.id, "dev-a1"))).await;
    assert_eq!(
        (status, &body["error"]),
        (StatusCode::BAD_REQUEST, &Value::from("invalid_timeout"))
    );
    let (status, body) = refusal(tail_of(app.id, "dev-zz")).await;
    assert_eq!(
        (status, &body["error"]),
        (StatusCode::NOT_FOUND, &Value::from("environment_not_found"))
    );

    let uri = format!("{}?timeout=1", tail_of(app.id, "dev-a1"));
    let mut tail = open(&uri, &[]).await.expect("opens");
    let events = tail.until("done").await;
    let names: Vec<&str> = events.iter().map(|e| e.event.as_str()).collect();
    assert_eq!(names, ["open", "done"]);
    assert_eq!(events[1].data["reason"], "timeout");
    assert!(tail.ended().await, "nothing follows done");
}
