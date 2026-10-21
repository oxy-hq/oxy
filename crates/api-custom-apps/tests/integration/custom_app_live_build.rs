//! `GET <base>__oxy/build` end to end against the seeded app: the probe a
//! long-open tab polls to notice a publish.
//!
//! The unit tests pin the classification and the response shape. These pin the
//! seams: the probe sits behind the app's own gate, answers the same build the
//! shell injects as `window.__OXY_APP__.buildId`, and is never counted as a
//! page view — a tab polling every five minutes all day must not read as
//! hundreds of opens.

use super::custom_app_platform_runtime::{base, get, header_of, seeded_local_member, send};
use crate::common::APP_SLUG;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use entity::prelude::Apps;
use oxy_api_custom_apps::custom_apps_usage_queue as usage_queue;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

#[tokio::test]
async fn the_probe_answers_the_build_the_shell_injects_and_is_not_a_view() {
    let db = seeded_local_member().await;
    let app = Apps::find()
        .filter(entity::apps::Column::Slug.eq(APP_SLUG))
        .one(&db)
        .await
        .expect("apps")
        .expect("the seeded app exists");
    // The build production serves is its `app_environments` row's pointer.
    let published = oxy_api_custom_apps::custom_apps_environments::fixed_pointers(&db, app.id)
        .await
        .expect("pointers")
        .production
        .expect("the seeded app is published");

    // Views are written off the request path (`custom_apps_usage_queue`); flush
    // anything an earlier test left queued so the baseline is settled.
    usage_queue::flush().await;
    let views_before = entity::prelude::CustomAppViewEvent::find()
        .all(&db)
        .await
        .expect("count views")
        .len();

    let sent = get(&format!("{}/__oxy/build", base())).await;
    assert_eq!(sent.status, StatusCode::OK, "body: {}", sent.body);
    assert_eq!(
        header_of(&sent, "cache-control").as_deref(),
        Some("private, no-store"),
        "a cached probe answer would hide the next publish"
    );
    let json: serde_json::Value = serde_json::from_str(&sent.body).expect("json body");
    assert_eq!(json["buildId"], published.to_string());

    // The shell names the same id the probe answers, so the client compares
    // like with like.
    let shell = get(&format!("{}/", base())).await;
    assert!(
        shell.body.contains(&format!("\"buildId\":\"{published}\"")),
        "the shell injects a different build id than the probe answers"
    );

    // The shell above IS a view; write what the two requests queued, then
    // check that the probe added nothing beyond it.
    usage_queue::flush().await;
    let views_after = entity::prelude::CustomAppViewEvent::find()
        .all(&db)
        .await
        .expect("count views")
        .len();
    assert_eq!(
        views_after,
        views_before + 1,
        "the probe was recorded as a page view (only the shell should count)"
    );

    // A wrong verb is an unknown reserved path, not a fall-through.
    let post = send(
        Request::builder()
            .method("POST")
            .uri(format!("{}/__oxy/build", base()))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(post.status, StatusCode::NOT_FOUND);
}

/// The probe inherits the app's gate: a visitor who cannot open the bundle
/// cannot learn when it was published either.
#[tokio::test]
async fn the_probe_is_behind_the_apps_own_gate() {
    let _db = seeded_local_member().await;
    // The guest joined `local` only; Acme's copy of the app is not theirs.
    let sent = get(&format!("/customer-apps/acme/{APP_SLUG}/__oxy/build")).await;
    assert_eq!(sent.status, StatusCode::FORBIDDEN);
}
