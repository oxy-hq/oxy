//! `GET /customer-apps/{id}/releases` through the real routers.

use axum::Extension;
use axum::extract::{Path, Query};
use axum::http::StatusCode;
use entity::users::UserStatus;
use oxy_api_custom_apps::admin::apps::releases::{ReleaseQuery, list_app_releases};
use oxy_auth::extractor::AuthenticatedUserExtractor;
use oxy_auth::types::{AppPublishTokenAuth, AuthenticatedUser};
use oxy_auth::user::LOCAL_GUEST_EMAIL;
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Calls, OK, calls};
use crate::promote_rollback::{fixture_builds, person, promote, rollback, sandbox_build};
use crate::sandbox_agent_token::fixture::{Agent, agent, tarball};

/// Re-date production's events of `build` to `days` ago, as the demo fixture
/// does: real moves land seconds apart.
async fn redate(agent: &Agent, build: Uuid, days: i64) {
    agent
        .t
        .db
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "UPDATE app_environment_events SET at = now() - make_interval(days => $3) \
             WHERE app_id = $1 AND environment = 'production' AND build_id = $2",
            [agent.app.id.into(), build.into(), (days as i32).into()],
        ))
        .await
        .expect("re-date production's event");
}

/// agent-prod served production for two days and failed nothing; agent-stg
/// has served it for a day and fails 6 of 60.
async fn red_release(agent: &Agent) -> (Uuid, Uuid) {
    let (prod, stg) = fixture_builds(agent).await;
    let (status, moved) = promote(agent, "production", json!({ "from": "staging" })).await;
    assert_eq!(status, StatusCode::OK, "{moved}");
    redate(agent, prod, 3).await;
    redate(agent, stg, 1).await;
    let db = &agent.t.db;
    let since =
        |days| chrono::Utc::now() - chrono::Duration::days(days) + chrono::Duration::minutes(5);
    calls(db, agent.app.id, prod, since(3), 100, OK).await;
    calls(db, agent.app.id, stg, since(1), 54, OK).await;
    let error = Calls {
        status: "error",
        ..OK
    };
    calls(db, agent.app.id, stg, since(1), 6, error).await;
    (prod, stg)
}

async fn timeline(agent: &Agent, query: &str) -> (StatusCode, Value) {
    let uri = format!("/customer-apps/{}/releases{query}", agent.app.id);
    person(agent, "GET", &uri, None).await
}

fn lane<'a>(body: &'a Value, name: &str) -> &'a Vec<Value> {
    body["lanes"]
        .as_array()
        .expect("lanes")
        .iter()
        .find(|l| l["lane"] == name)
        .and_then(|l| l["events"].as_array())
        .unwrap_or_else(|| panic!("no {name} lane: {body:#}"))
}

/// The staff console's read: three lanes, the sandbox's move among them, and
/// a red release serving now whose `rollback.to` is the build the dry-run
/// plan of a rollback then names.
#[tokio::test]
async fn the_timeline_offers_a_rollback_the_plan_names() {
    let agent = agent().await;
    let slug = agent.app.slug.clone();
    red_release(&agent).await;
    sandbox_build(&agent, "ann", "sbx-ann", tarball(&slug, "sandbox"), true).await;

    let (status, body) = timeline(&agent, "").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["window"]["days"], 7, "the default window");
    assert_eq!(body["non_production"], "included");
    let lanes: Vec<_> = body["lanes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["lane"].clone())
        .collect();
    assert_eq!(lanes, ["production", "staging", "sandboxes"]);
    assert!(
        lane(&body, "staging")
            .iter()
            .any(|e| e["action"] == "publish"),
        "{body:#}"
    );
    assert!(
        lane(&body, "sandboxes")
            .iter()
            .any(|e| e["environment"] == "dev-ann")
    );

    let current = &body["releases"][0];
    assert_eq!(current["current"], true);
    assert_eq!(current["build"]["build_id"], "agent-stg");
    assert_eq!(current["verdict"]["kind"], "worse", "{current:#}");
    assert_eq!(current["rollback"]["offered"], true);
    assert_eq!(current["rollback"]["to"]["build_id"], "agent-prod");

    let to = current["rollback"]["to"]["id"].as_str().expect("an id");
    let (status, plan) = rollback(&agent, "production", json!({ "to": to, "dry_run": true })).await;
    assert_eq!(status, StatusCode::OK, "{plan}");
    assert_eq!(plan["build"]["build_id"], "agent-prod");
    assert_eq!(plan["blockers"], json!([]), "{plan}");
}

#[tokio::test]
async fn a_window_out_of_range_is_refused() {
    let agent = agent().await;
    for query in ["?days=0", "?days=31", "?days=week"] {
        let (status, body) = timeline(&agent, query).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}: {body}");
        assert_eq!(body["error"], "invalid_days", "{query}");
    }
    let (status, _) = timeline(&agent, "?days=30").await;
    assert_eq!(status, StatusCode::OK);
}

/// Not on the sandbox loop's allow-list: not found, like every other route.
#[tokio::test]
async fn a_sandbox_agent_token_is_not_found() {
    let agent = agent().await;
    let (status, body) = agent.app_get("releases").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}

fn caller(id: Uuid, email: &str) -> AuthenticatedUserExtractor {
    AuthenticatedUserExtractor(AuthenticatedUser {
        id,
        email: Some(email.to_string()),
        name: "Timeline".to_string(),
        picture: None,
        status: UserStatus::Active,
        credential: None,
    })
}

async fn read_as(
    agent: &Agent,
    who: AuthenticatedUserExtractor,
    marker: Option<Extension<AppPublishTokenAuth>>,
) -> Value {
    let query = Ok(Query(ReleaseQuery { days: Some(7) }));
    let axum::Json(body) = list_app_releases(who, marker, Path(agent.app.id), query)
        .await
        .expect("production is readable by whoever the mount admits");
    serde_json::to_value(body).expect("serialize")
}

/// A publish token — whoever minted it — and a caller oxy-authz does not let
/// open the app's non-production environments are answered production's
/// lane and releases, and told the rest is not permitted.
#[tokio::test]
async fn without_reach_only_production_is_answered() {
    let agent = agent().await;
    red_release(&agent).await;
    let token = Extension(AppPublishTokenAuth {
        token_id: Uuid::new_v4(),
        app_id: None,
        machine_identity: None,
    });
    let staff = caller(agent.t.guest_id, LOCAL_GUEST_EMAIL);
    let tenant = caller(Uuid::new_v4(), "tenant-admin@customer.example");
    for body in [
        read_as(&agent, staff, Some(token)).await,
        read_as(&agent, tenant, None).await,
    ] {
        assert_eq!(body["non_production"], "not_permitted", "{body:#}");
        assert!(lane(&body, "staging").is_empty());
        assert!(lane(&body, "sandboxes").is_empty());
        assert!(!lane(&body, "production").is_empty());
        assert_eq!(body["releases"][0]["verdict"]["kind"], "worse");
    }
    let staff = caller(agent.t.guest_id, LOCAL_GUEST_EMAIL);
    let body = read_as(&agent, staff, None).await;
    assert_eq!(
        body["non_production"], "included",
        "the same person, no token"
    );
}
