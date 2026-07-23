//! The seeded example app actually renders — the launcher lists it, and its
//! bundle serves.
//!
//! `seed_example_app.rs` proves the seed writes the right rows and bytes. That's
//! not the same as the app working: the rows are only correct in terms of what
//! the *reading* code does with them. These tests drive the real handlers —
//! `list_custom_apps` (the home grid) and `serve_dispatch` (the bundle) —
//! against real seeded data, so a change to either reader that orphans the seed
//! fails here rather than on a developer's first `oxy seed`.
//!
//! Auth: `BuiltInAuthenticator` falls back to a guest identity when no auth
//! method is configured, which is the default in a test process. The guest is a
//! plain user with no special standing, so the org-membership gate is exercised
//! for real — the harness strips `OXY_OWNER`/`OXY_GLOBAL_ADMINS` precisely so
//! the staff path can't short-circuit it.

mod common;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use chrono::Utc;
use common::{APP_SLUG, demo_workspace_id, examples_path, test_db};
use entity::prelude::{Apps, Organizations};
use entity::{apps, org_members, org_members::OrgRole, organizations};
use oxy_app::cli::commands::seed;
use oxy_app::server::api::{custom_apps_serve, workspace_custom_apps};
use oxy_auth::types::Identity;
use oxy_auth::user::UserService;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseConnection, EntityTrait, IntoActiveModel,
    QueryFilter,
};
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

/// Just the two readers under test. Mounting these rather than the whole
/// `api_router` keeps the test to the handlers in question — the full router
/// drags in background workers and agentic state that have nothing to do with
/// whether an app renders.
fn router() -> Router {
    Router::new()
        .route(
            "/{workspace_id}/custom-apps",
            get(workspace_custom_apps::list_custom_apps),
        )
        .route(
            "/customer-apps/{*path}",
            get(custom_apps_serve::serve_dispatch),
        )
}

async fn get_json(uri: &str) -> (StatusCode, Value) {
    let resp = router()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .expect("oneshot");
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

async fn get_text(uri: &str) -> (StatusCode, String) {
    let resp = router()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .expect("oneshot");
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 4 * 1024 * 1024)
        .await
        .expect("read body");
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

/// Make the guest a member of `org_id`. The serve path authenticates as the
/// guest, and the customer path in `user_can_access_app` requires membership —
/// the seed doesn't grant it, so a test that wants a *successful* render has to.
async fn add_guest_to_org(db: &DatabaseConnection, org_id: Uuid) -> Uuid {
    let guest = UserService::get_or_create_user(&Identity {
        email: oxy_auth::user::LOCAL_GUEST_EMAIL.to_string(),
        name: Some("Local User".to_string()),
        picture: None,
    })
    .await
    .expect("guest user");
    let now = Utc::now().fixed_offset();
    org_members::ActiveModel {
        id: ActiveValue::Set(Uuid::new_v4()),
        org_id: ActiveValue::Set(org_id),
        user_id: ActiveValue::Set(guest.id),
        role: ActiveValue::Set(OrgRole::Member),
        created_at: ActiveValue::Set(now),
        updated_at: ActiveValue::Set(now),
    }
    .insert(db)
    .await
    .expect("insert guest membership");
    guest.id
}

async fn org_id(db: &DatabaseConnection, slug: &str) -> Uuid {
    Organizations::find()
        .filter(organizations::Column::Slug.eq(slug))
        .one(db)
        .await
        .expect("query org")
        .unwrap_or_else(|| panic!("no {slug} org"))
        .id
}

// ── The home grid ────────────────────────────────────────────────────────────

#[tokio::test]
async fn home_grid_lists_the_seeded_app_with_its_card_metadata() {
    // Bound but unused: it's the call that points this process at a fresh
    // migrated DB. The handler reaches it through `establish_connection()`.
    let _db = test_db().await;
    seed::seed_demo(Some(examples_path())).await.expect("seed");

    let (status, body) = get_json(&format!("/{}/custom-apps", demo_workspace_id())).await;
    assert_eq!(status, StatusCode::OK);

    let apps = body.as_array().expect("array of apps");
    let card = apps
        .iter()
        .find(|a| a["slug"] == APP_SLUG)
        .unwrap_or_else(|| panic!("{APP_SLUG} not on the home grid; got {body}"));

    assert_eq!(card["name"], "Oxy Starter");
    assert_eq!(card["org_slug"], "local");
    assert_eq!(card["url"], format!("/customer-apps/local/{APP_SLUG}/"));
    // Everything below comes from the bundle's oxy-app.json via
    // app_builds.manifest_json. If the seed stopped storing the manifest, the
    // card would still list — just blank — so assert the fields, not the row.
    assert!(
        card["description"].is_string(),
        "card has no description: {card}"
    );
    assert_eq!(
        card["icon_url"],
        format!("/customer-apps/local/{APP_SLUG}/icon.svg")
    );
    assert_eq!(
        card["art_url"],
        format!("/customer-apps/local/{APP_SLUG}/card.svg")
    );
    assert!(
        card["suggested_questions"]
            .as_array()
            .is_some_and(|q| !q.is_empty()),
        "the manifest's ask block should reach the card: {card}"
    );
}

#[tokio::test]
async fn home_grid_is_scoped_to_its_own_workspace() {
    let _db = test_db().await;
    seed::seed_demo(Some(examples_path())).await.expect("seed");

    // Acme's app exists, but it belongs to Acme's workspace. Asking the demo
    // workspace's grid must not return it — `list_custom_apps` filters on
    // project_id, and cross-tenant leakage here would put another org's app on
    // your home page.
    let (status, body) = get_json(&format!("/{}/custom-apps", demo_workspace_id())).await;
    assert_eq!(status, StatusCode::OK);
    let orgs: Vec<&str> = body
        .as_array()
        .expect("array")
        .iter()
        .filter_map(|a| a["org_slug"].as_str())
        .collect();
    assert!(
        orgs.iter().all(|o| *o == "local"),
        "the demo workspace's grid leaked another org's app: {orgs:?}"
    );

    // And an unrelated workspace shows nothing.
    let (status, body) = get_json(&format!("/{}/custom-apps", Uuid::new_v4())).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().expect("array").len(), 0);
}

#[tokio::test]
async fn unpublishing_removes_the_app_from_the_home_grid() {
    let db = test_db().await;
    seed::seed_demo(Some(examples_path())).await.expect("seed");

    let local = org_id(&db, "local").await;
    let app = Apps::find()
        .filter(apps::Column::OrgId.eq(local))
        .filter(apps::Column::Slug.eq(APP_SLUG))
        .one(&db)
        .await
        .expect("query app")
        .expect("seeded app");

    let mut active = app.into_active_model();
    active.published_at = ActiveValue::Set(None);
    active.update(&db).await.expect("unpublish");

    // `published_at` is the ONLY thing separating "on the grid" from "reachable
    // by direct URL", which is why the seed always sets it. Pin the rule.
    let (status, body) = get_json(&format!("/{}/custom-apps", demo_workspace_id())).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !body
            .as_array()
            .expect("array")
            .iter()
            .any(|a| a["slug"] == APP_SLUG),
        "an unpublished app must not appear on the home grid: {body}"
    );
}

// ── The bundle ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn opening_the_app_serves_the_bundle_with_its_runtime_identity() {
    let db = test_db().await;
    seed::seed_demo(Some(examples_path())).await.expect("seed");
    add_guest_to_org(&db, org_id(&db, "local").await).await;

    let (status, html) = get_text(&format!("/customer-apps/local/{APP_SLUG}/")).await;
    assert_eq!(status, StatusCode::OK, "body: {html}");

    // The app is dead without this: it reads projectId from __OXY_APP__ to
    // address the data plane, and injection only happens if the bundle has a
    // </head> — which it silently skips with a warn! when absent.
    assert!(
        html.contains("window.__OXY_APP__"),
        "runtime identity was not injected into the bundle"
    );
    assert!(
        html.contains(&demo_workspace_id().to_string()),
        "the injected projectId should be the demo workspace"
    );
    assert!(html.contains("Oxy Starter"), "served the wrong document");
}

#[tokio::test]
async fn app_assets_serve_from_the_build_store() {
    let db = test_db().await;
    seed::seed_demo(Some(examples_path())).await.expect("seed");
    add_guest_to_org(&db, org_id(&db, "local").await).await;

    // The launcher card points at this exact URL, so a card with a broken image
    // is exactly this request failing.
    let (status, svg) = get_text(&format!("/customer-apps/local/{APP_SLUG}/icon.svg")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(svg.contains("<svg"), "icon.svg did not serve: {svg}");
}

#[tokio::test]
async fn a_non_member_cannot_open_another_orgs_app() {
    let db = test_db().await;
    seed::seed_demo(Some(examples_path())).await.expect("seed");

    // The guest joins `local` — and only `local`. Acme's app is published, so
    // the only thing standing between this request and another tenant's app is
    // the org-membership check.
    add_guest_to_org(&db, org_id(&db, "local").await).await;

    let (status, _) = get_text(&format!("/customer-apps/acme/{APP_SLUG}/")).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a non-member reached another org's custom app"
    );
}
