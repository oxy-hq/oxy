//! `releases_of` over hand-seeded history.

use chrono::Duration;
use entity::app_builds;
use oxy_api_custom_apps::admin::apps::releases::{bounded, releases_of};
use sea_orm::{EntityTrait, FromQueryResult};
use serde_json::Value;

use super::{
    Calls, OK, applied, bare_app, build, calls, days_ago, moved, now, release, vitals_row,
};
use crate::custom_app_functions_fixture::seeded_tenant;

async fn read(
    t: &crate::custom_app_functions_fixture::Tenant,
    app: &entity::apps::Model,
    days: i64,
) -> Value {
    let timeline = releases_of(&t.db, app, days, now(), true)
        .await
        .expect("the timeline");
    serde_json::to_value(timeline).expect("serialize")
}

/// Z (before the window), A, C (pruned), B, then a rollback to A. B fails 9
/// of 101 calls — 8 fingerprinted, one success that answered 503 — after A's
/// 1 of 100, at three times A's route p95, and its `/orders` LCP doubles.
#[tokio::test]
async fn releases_are_stretches_judged_against_the_build_before() {
    let t = seeded_tenant().await;
    let app = bare_app(&t).await;
    let db = &t.db;
    let (z, a, b, c) = (
        build(db, app.id, "z").await,
        build(db, app.id, "a").await,
        build(db, app.id, "b").await,
        build(db, app.id, "c").await,
    );
    let never_live = build(db, app.id, "d").await;
    moved(db, app.id, "production", Some(z), "promote", days_ago(40)).await;
    moved(db, app.id, "production", Some(a), "promote", days_ago(20)).await;
    moved(db, app.id, "production", Some(c), "promote", days_ago(15)).await;
    moved(db, app.id, "production", Some(b), "promote", days_ago(10)).await;
    moved(db, app.id, "production", Some(a), "rollback", days_ago(2)).await;

    let shed = Calls {
        status: "shed",
        ..OK
    };
    let running = Calls {
        status: "running",
        ..OK
    };
    let error = Calls {
        status: "error",
        ..OK
    };
    calls(db, app.id, a, days_ago(20), 99, OK).await;
    calls(db, app.id, a, days_ago(19), 1, error).await;
    calls(db, app.id, a, days_ago(19), 1, shed).await;
    calls(db, app.id, a, days_ago(19), 1, running).await;
    // On A, but while B served: a call counts only inside its own build's stretch.
    calls(db, app.id, a, days_ago(9), 5, error).await;
    calls(db, app.id, c, days_ago(14), 30, OK).await;
    let slow = Calls {
        duration_ms: 300,
        ..OK
    };
    calls(db, app.id, b, days_ago(10), 92, slow).await;
    let fingerprinted = Calls {
        status: "error",
        fingerprint: Some("fp-b"),
        ..slow
    };
    calls(db, app.id, b, days_ago(9), 8, fingerprinted).await;
    let answered_503 = Calls {
        result_status: Some(503),
        ..slow
    };
    calls(db, app.id, b, days_ago(8), 1, answered_503).await;
    calls(db, app.id, a, days_ago(2), 60, OK).await;
    // An old failure, seen before the window and again inside it: not new.
    let old = Calls {
        status: "error",
        fingerprint: Some("fp-old"),
        ..OK
    };
    calls(db, app.id, z, days_ago(35), 1, old).await;
    calls(db, app.id, a, days_ago(1), 1, old).await;

    applied(
        db,
        app.id,
        "0002_b.sql",
        Some(b),
        days_ago(10) - Duration::minutes(1),
    )
    .await;
    applied(db, app.id, "0003_d.sql", Some(never_live), days_ago(5)).await;

    // The pruned build: its calls cascade away, its event keeps a NULL build.
    app_builds::Entity::delete_by_id(c)
        .exec(db)
        .await
        .expect("prune c");

    let body = read(&t, &app, 30).await;
    let releases = body["releases"].as_array().expect("releases");
    let order: Vec<_> = releases
        .iter()
        .map(|r| r["index"].as_u64().unwrap())
        .collect();
    assert_eq!(order, [4, 3, 2, 1, 0], "newest first: {body:#}");
    assert_eq!(body["parts"]["metrics"], "ok");

    let z_release = release(&body, 0);
    assert_eq!(z_release["clipped"], true);
    assert_eq!(z_release["build"]["build_id"], "z");
    assert_eq!(z_release["verdict"]["kind"], "not_comparable");

    let a1 = release(&body, 1);
    assert_eq!(
        a1["metrics"]["calls"], 100,
        "shed and running are not calls: {a1:#}"
    );
    assert_eq!(a1["metrics"]["failed"], 1);
    assert_eq!(a1["metrics"]["shed"], 1);
    assert_eq!(
        a1["verdict"]["why"], "too few calls: 0 of 50",
        "Z had no calls"
    );

    let pruned = release(&body, 2);
    assert_eq!(pruned["pruned"], true);
    assert!(pruned["build"].is_null());
    assert!(pruned["metrics"].is_null());

    let b_release = release(&body, 3);
    assert_eq!(b_release["previous"], 1, "B skips the pruned C to reach A");
    assert_eq!(b_release["metrics"]["calls"], 101);
    assert_eq!(
        b_release["metrics"]["failed"], 9,
        "the 503 counts: {b_release:#}"
    );
    assert_eq!(b_release["metrics"]["p95_ms"], 300);
    assert_eq!(b_release["verdict"]["kind"], "worse");
    let metrics: Vec<_> = b_release["verdict"]["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["metric"].as_str().unwrap())
        .collect();
    assert_eq!(metrics, ["failure_rate", "p95"]);
    assert_eq!(b_release["migrations"][0]["filename"], "0002_b.sql");
    assert_eq!(b_release["new_issues"][0]["fingerprint"], "fp-b");
    assert_eq!(b_release["new_issues"][0]["occurrences"], 8);
    // Red, but a later release replaced it: nothing to roll back from.
    assert_eq!(b_release["rollback"]["offered"], false);
    assert_eq!(b_release["rollback"]["why_not"], "not_current");

    let a2 = release(&body, 4);
    assert_eq!(a2["current"], true);
    assert!(a2["ended_at"].is_null());
    assert_eq!(a2["started_by"]["action"], "rollback");
    assert_eq!(a2["previous"], 3);
    assert_eq!(a2["metrics"]["calls"], 61);
    assert_eq!(a2["verdict"]["kind"], "not_worse");
    assert!(
        a2["new_issues"].as_array().unwrap().is_empty(),
        "fp-old is not new"
    );
    assert!(a2["rollback"].is_null(), "only a red release offers one");

    let unreleased = body["unreleased_migrations"].as_array().unwrap();
    assert_eq!(unreleased.len(), 1);
    assert_eq!(unreleased[0]["filename"], "0003_d.sql");
}

/// Vitals come only from rows naming their build, inside that build's
/// release, and only values in bounds are samples. A forged `9e9`, a `1e400`
/// (which a `float8` cast would fail the whole statement on) and a string are
/// counted as page loads and never reach the p75.
#[tokio::test]
async fn vitals_are_read_per_screen_and_build_and_bounded_in_sql() {
    let t = seeded_tenant().await;
    let app = bare_app(&t).await;
    let db = &t.db;
    let (a, b) = (build(db, app.id, "a").await, build(db, app.id, "b").await);
    moved(db, app.id, "production", Some(a), "promote", days_ago(6)).await;
    moved(db, app.id, "production", Some(b), "promote", days_ago(3)).await;
    let row = |build: uuid::Uuid, lcp: &str, path: &str| {
        format!(r#"{{"lcp": {lcp}, "cls": 10, "inp": 80, "path": "{path}", "build": "{build}"}}"#)
    };
    for k in 0..25 {
        let at = days_ago(5) + Duration::minutes(k);
        vitals_row(
            db,
            app.id,
            t.guest_id,
            at,
            &row(a, &(1000 + k).to_string(), "/orders"),
        )
        .await;
        let at = days_ago(2) + Duration::minutes(k);
        vitals_row(
            db,
            app.id,
            t.guest_id,
            at,
            &row(b, &(2000 + k).to_string(), "/orders"),
        )
        .await;
    }
    let at = days_ago(2);
    for forged in ["9e9", "1e400", r#""slow""#, "-5"] {
        vitals_row(db, app.id, t.guest_id, at, &row(b, forged, "/orders")).await;
    }
    // No build: not read. A's build during B's release: not read.
    let unattributed = r#"{"lcp": 99999, "path": "/orders"}"#;
    vitals_row(db, app.id, t.guest_id, at, unattributed).await;
    vitals_row(db, app.id, t.guest_id, at, &row(a, "50000", "/orders")).await;
    vitals_row(db, app.id, t.guest_id, at, &row(b, "700", "/settings")).await;

    let body = read(&t, &app, 7).await;
    assert_eq!(body["parts"]["vitals"], "ok", "{body:#}");
    let a_screens = &release(&body, 0)["vitals"];
    assert_eq!(a_screens[0]["path"], "/orders");
    assert_eq!(a_screens[0]["samples"], 25);
    assert_eq!(a_screens[0]["lcp_p75"], 1018);
    let b_release = release(&body, 1);
    let b_screens = b_release["vitals"].as_array().unwrap();
    assert_eq!(b_screens.len(), 2, "{b_screens:#?}");
    assert_eq!(b_screens[0]["path"], "/orders", "most page loads first");
    assert_eq!(b_screens[0]["samples"], 29);
    assert_eq!(
        b_screens[0]["lcp_p75"], 2018,
        "the forged values are not samples"
    );
    assert_eq!(b_screens[0]["cls_p75"], 10);
    assert_eq!(b_screens[1]["path"], "/settings");
    // LCP 1018 → 2018 on /orders, both sides over 20 loads: amber, never red.
    assert_eq!(b_release["verdict"]["kind"], "slower");
    assert_eq!(b_release["verdict"]["reasons"][0]["metric"], "lcp");
    assert_eq!(b_release["verdict"]["reasons"][0]["screen"], "/orders");
}

/// A red release serving now names the build a rollback goes back to: the
/// release it was compared with.
#[tokio::test]
async fn a_red_release_serving_now_offers_a_rollback_to_its_previous_build() {
    let t = seeded_tenant().await;
    let app = bare_app(&t).await;
    let db = &t.db;
    let (a, b) = (build(db, app.id, "a").await, build(db, app.id, "b").await);
    moved(db, app.id, "production", Some(a), "promote", days_ago(5)).await;
    moved(db, app.id, "production", Some(b), "promote", days_ago(1)).await;
    calls(db, app.id, a, days_ago(5), 100, OK).await;
    calls(db, app.id, b, days_ago(1), 55, OK).await;
    let error = Calls {
        status: "error",
        ..OK
    };
    calls(db, app.id, b, days_ago(1), 5, error).await;

    let body = read(&t, &app, 7).await;
    let current = release(&body, 1);
    assert_eq!(current["verdict"]["kind"], "worse", "{current:#}");
    assert_eq!(current["metrics"]["failure_rate_bp"], 833);
    assert_eq!(current["rollback"]["offered"], true);
    assert_eq!(current["rollback"]["to"]["id"], a.to_string());
    assert_eq!(current["rollback"]["to"]["build_id"], "a");
    assert!(current["rollback"]["why_not"].is_null());

    // One day: A's release is the anchor, counted from the window's start,
    // and holds no calls there — B is no longer comparable.
    let body = read(&t, &app, 1).await;
    let current = release(&body, 1);
    assert_eq!(release(&body, 0)["clipped"], true);
    assert_eq!(current["verdict"]["kind"], "not_comparable");
    assert!(current["rollback"].is_null());
}

#[derive(Debug, FromQueryResult)]
struct One {
    #[allow(dead_code)]
    x: String,
}

/// A part that runs out of time answers `None` alone, and the pool's next
/// read is not left under its timeout.
#[tokio::test]
async fn a_slow_part_is_unavailable_alone() {
    let db = crate::common::test_db().await;
    let slow: Option<Vec<One>> =
        bounded(&db, "probe", "SELECT pg_sleep(1)::text AS x", vec![], 50).await;
    assert!(slow.is_none(), "cancelled by its statement timeout");
    let next: Option<Vec<One>> = bounded(
        &db,
        "probe",
        "SELECT current_setting('statement_timeout') AS x, pg_sleep(0.2)::text",
        vec![],
        4_000,
    )
    .await;
    assert_eq!(next.expect("the next read runs").len(), 1);
}
