//! The two promote paths, the refused ones, and what every act answers.

use axum::http::StatusCode;
use entity::app_environment_events;
use oxy_api_custom_apps::custom_apps_publish::{PROMOTING_PUBLISH_DEPRECATED, publish};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde_json::json;

use super::{audit_rows, draft, events, fixture_builds, pointers, promote, sandbox_build};
use crate::sandbox_agent_token::fixture::{agent, tarball};
use crate::sandbox_publish::{build_pk, input};

/// A dry run plans and moves nothing; the act moves production to staging's
/// build, records `production / promote` and one `app.promoted` row, and
/// answers the plan with `moved`. Promoting it again is `already_serving`.
#[tokio::test]
async fn staging_promotes_to_production_after_a_dry_run() {
    let agent = agent().await;
    let (prod, stg) = fixture_builds(&agent).await;

    let (status, plan) = promote(
        &agent,
        "production",
        json!({ "from": "staging", "dry_run": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{plan}");
    assert_eq!(plan["dry_run"], true);
    assert_eq!(plan["moved"], false);
    assert_eq!(plan["action"], "promote");
    assert_eq!(
        (&plan["from"], &plan["to"]),
        (&json!("staging"), &json!("production"))
    );
    assert_eq!(plan["build"]["build_id"], "agent-stg", "{plan}");
    assert_eq!(plan["build"]["published_to"], "staging", "{plan}");
    assert_eq!(plan["current"]["build_id"], "agent-prod", "{plan}");
    assert_eq!(plan["expect_current"], json!(prod), "{plan}");
    assert!(plan["served_since"].is_string(), "{plan}");
    assert_eq!(plan["migrations"]["when"], "before_move", "{plan}");
    assert_eq!(plan["blockers"], json!([]), "{plan}");
    // The two builds carry different bytes for both functions.
    assert_eq!(
        plan["functions"]["changed"],
        json!(["smoke", "whoami"]),
        "{plan}"
    );
    assert_eq!(
        pointers(&agent).await.0,
        Some(prod),
        "a dry run moves nothing"
    );

    let body = json!({ "from": "staging", "build": plan["build"]["id"], "expect_current": plan["expect_current"] });
    let (status, acted) = promote(&agent, "production", body).await;
    assert_eq!(status, StatusCode::OK, "{acted}");
    assert_eq!(
        (&acted["moved"], &acted["dry_run"]),
        (&json!(true), &json!(false))
    );
    assert!(acted["migrations_applied"].is_object(), "{acted}");
    assert_eq!(pointers(&agent).await.0, Some(stg));
    let last = events(&agent).await.pop().expect("an event");
    assert_eq!(last, ("production".into(), Some(stg), "promote".into()));

    let rows = audit_rows(&agent, "app.promoted").await;
    assert_eq!(rows.len(), 1, "one row per pointer move");
    assert_eq!(rows[0].environment, "production");
    assert_eq!(rows[0].metadata["from"], "staging");
    assert_eq!(rows[0].metadata["build"], json!(stg));
    assert_eq!(rows[0].metadata["previous"], json!(prod));

    let (status, again) = promote(&agent, "production", json!({ "from": "staging" })).await;
    assert_eq!(status, StatusCode::CONFLICT, "{again}");
    assert_eq!(again["error"], "already_serving", "{again}");
}

/// A sandbox's build reaches staging as a pointer move with its own
/// `staging / promote` event, and only through staging reaches production.
#[tokio::test]
async fn a_sandbox_build_reaches_production_only_through_staging() {
    let agent = agent().await;
    let slug = agent.app.slug.clone();
    let built = sandbox_build(&agent, "p1", "sbx-p1", tarball(&slug, "sandbox"), true).await;

    let (status, refused) = promote(&agent, "production", json!({ "from": "dev-p1" })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
    assert_eq!(refused["error"], "promote_path_refused", "{refused}");

    let (status, plan) = promote(&agent, "staging", json!({ "from": "dev-p1" })).await;
    assert_eq!(status, StatusCode::OK, "{plan}");
    assert_eq!(plan["migrations"]["when"], "queued_after", "{plan}");
    assert!(plan["schedules"].is_null(), "production only: {plan}");
    assert_eq!(pointers(&agent).await.1, Some(built));
    let last = events(&agent).await.pop().expect("an event");
    assert_eq!(last, ("staging".into(), Some(built), "promote".into()));

    let (status, plan) = promote(&agent, "production", json!({ "from": "staging" })).await;
    assert_eq!(status, StatusCode::OK, "{plan}");
    assert_eq!(pointers(&agent).await.0, Some(built));
}

/// Every pair but the two paths is refused before anything is read, and a
/// name that is no environment is `invalid_environment`.
#[tokio::test]
async fn every_other_pair_is_refused() {
    let agent = agent().await;
    let before = pointers(&agent).await;
    for (to, from) in [
        ("staging", "production"),
        ("staging", "staging"),
        ("production", "production"),
        ("production", "dev-p1"),
        ("dev-p1", "staging"),
    ] {
        let (status, body) = promote(&agent, to, json!({ "from": from })).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{from} → {to}: {body}");
        assert_eq!(body["error"], "promote_path_refused", "{from} → {to}");
    }
    let (status, body) = promote(&agent, "prod", json!({ "from": "staging" })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"], "invalid_environment", "{body}");
    assert_eq!(pointers(&agent).await, before, "nothing moved");
}

/// An act whose plan saw another production is refused `target_moved` and
/// moves nothing — `null` (nothing served) included.
#[tokio::test]
async fn an_act_against_a_moved_target_is_refused() {
    let agent = agent().await;
    let (prod, stg) = fixture_builds(&agent).await;
    for expected in [json!(stg), json!(null)] {
        let body = json!({ "from": "staging", "expect_current": expected });
        let (status, refused) = promote(&agent, "production", body).await;
        assert_eq!(status, StatusCode::CONFLICT, "{refused}");
        assert_eq!(refused["error"], "target_moved", "{refused}");
    }
    assert_eq!(pointers(&agent).await.0, Some(prod));
}

/// `build` names a build the source served — by publish id or UUID; one it
/// never served is `build_not_served`, one that does not exist
/// `build_not_found`.
#[tokio::test]
async fn build_must_be_one_the_source_served() {
    let agent = agent().await;
    let slug = agent.app.slug.clone();
    let older = draft(&agent, "stg-older", tarball(&slug, "older")).await;
    draft(&agent, "stg-newer", tarball(&slug, "newer")).await;
    let unstaged = sandbox_build(&agent, "p2", "sbx-p2", tarball(&slug, "sandbox"), true).await;

    let body = json!({ "from": "staging", "build": "sbx-p2", "dry_run": true });
    let (status, refused) = promote(&agent, "production", body).await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    assert_eq!(refused["error"], "build_not_served", "{refused}");
    let body = json!({ "from": "staging", "build": "no-such-build" });
    let (status, refused) = promote(&agent, "production", body).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{refused}");
    assert_eq!(refused["error"], "build_not_found", "{refused}");

    // Staging moved on to stg-newer; the older build it served still promotes.
    let (status, plan) = promote(
        &agent,
        "production",
        json!({ "from": "staging", "build": older }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{plan}");
    assert_eq!(pointers(&agent).await.0, Some(older));
    assert_ne!(pointers(&agent).await.0, Some(unstaged));
}

/// `oxyc publish --promote` still works, through the one production move: the
/// publish's own staging event is recorded first, production's second, and
/// the answer warns that the flag is deprecated.
#[tokio::test]
async fn a_promoting_publish_records_staging_first_and_warns() {
    let agent = agent().await;
    let slug = agent.app.slug.clone();
    let mut promoting = input(&agent.t, &slug, "promoted-1", tarball(&slug, "promoted"));
    promoting.promote = true;
    let result = publish(promoting).await.expect("a promoting publish");
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w == PROMOTING_PUBLISH_DEPRECATED),
        "{:?}",
        result.warnings
    );
    assert!(PROMOTING_PUBLISH_DEPRECATED.contains("oxyc promote"));
    let build = build_pk(&agent.t.db, agent.app.id, "promoted-1")
        .await
        .expect("the build");
    let moved: Vec<_> = app_environment_events::Entity::find()
        .filter(app_environment_events::Column::BuildId.eq(build))
        .all(&agent.t.db)
        .await
        .expect("read the build's events");
    let at = |environment: &str, action: &str| {
        moved
            .iter()
            .find(|e| e.environment == environment && e.action == action)
            .map(|e| e.at)
            .unwrap_or_else(|| panic!("no {environment} / {action} event: {moved:?}"))
    };
    assert_eq!(moved.len(), 2, "{moved:?}");
    assert!(at("staging", "publish") <= at("production", "promote"));
    assert_eq!(pointers(&agent).await, (Some(build), Some(build)));

    // A draft publish says nothing of the kind.
    let plain = publish(input(&agent.t, &slug, "draft-1", tarball(&slug, "draft")))
        .await
        .expect("a draft");
    assert!(
        !plain
            .warnings
            .iter()
            .any(|w| w == PROMOTING_PUBLISH_DEPRECATED)
    );
}
