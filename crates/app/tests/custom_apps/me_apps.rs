//! `GET /api/me/apps` (`me_apps::openable_apps`) against seeded rows: one list
//! across every org the caller belongs to, decided by the same ring as the
//! workspace launcher, through the real facts loader.

use super::custom_app_visibility::{
    seed_app, seed_app_member, seed_member, seed_org, seed_user, seed_workspace,
};
use crate::common::test_db;
use entity::{org_members::OrgRole, organizations};
use oxy_app::server::api::me_apps::openable_apps;
use oxy_app::server::authz::{Caller, loader};
use sea_orm::{ActiveModelTrait, ActiveValue, DatabaseConnection};
use uuid::Uuid;

async fn name_org(conn: &DatabaseConnection, id: Uuid, name: &str) {
    organizations::ActiveModel {
        id: ActiveValue::Set(id),
        name: ActiveValue::Set(name.to_string()),
        ..Default::default()
    }
    .update(conn)
    .await
    .expect("rename org");
}

/// `(org name, app name)` in the order the list returns them.
async fn listed_for(conn: &DatabaseConnection, who: (Uuid, &str)) -> Vec<(String, String)> {
    let caller = Caller::without_credential(who.0, who.1);
    let facts = loader::load_principal_facts_scoped(conn, &caller, false)
        .await
        .expect("facts");
    openable_apps(conn, &facts, "https://app.example.com")
        .await
        .expect("apps")
        .into_iter()
        .map(|a| (a.org.name, a.name))
        .collect()
}

#[tokio::test]
async fn every_org_the_caller_belongs_to_in_one_sorted_list() {
    let conn = test_db().await;
    let (user, email) = seed_user(&conn).await;
    let (zed, acme, stranger) = (
        seed_org(&conn).await,
        seed_org(&conn).await,
        seed_org(&conn).await,
    );
    name_org(&conn, zed, "Zed Foods").await;
    name_org(&conn, acme, "acme").await;
    seed_member(&conn, zed, user, OrgRole::Member).await;
    seed_member(&conn, acme, user, OrgRole::Member).await;

    // Two workspaces in one org: both are listed, no per-workspace query.
    let (acme_ws1, acme_ws2) = (
        seed_workspace(&conn, acme).await,
        seed_workspace(&conn, acme).await,
    );
    let zed_ws = seed_workspace(&conn, zed).await;
    seed_app(&conn, zed, zed_ws, "Rota", false).await;
    seed_app(&conn, acme, acme_ws2, "Stock", false).await;
    seed_app(&conn, acme, acme_ws1, "Checklists", false).await;
    let granted = seed_app(&conn, acme, acme_ws1, "Payroll", true).await;
    seed_app_member(&conn, granted, user, "member").await;
    seed_app(&conn, acme, acme_ws1, "Board pack", true).await;
    let stranger_ws = seed_workspace(&conn, stranger).await;
    seed_app(&conn, stranger, stranger_ws, "Not yours", false).await;

    let listed = listed_for(&conn, (user, &email)).await;
    let expected: Vec<(String, String)> = [
        ("acme", "Checklists"),
        ("acme", "Payroll"),
        ("acme", "Stock"),
        ("Zed Foods", "Rota"),
    ]
    .iter()
    .map(|(o, a)| (o.to_string(), a.to_string()))
    .collect();
    assert_eq!(
        listed, expected,
        "the ungranted restricted app and the other tenant's app stay out"
    );
}

#[tokio::test]
async fn a_card_carries_its_org_workspace_and_an_absolute_url() {
    let conn = test_db().await;
    let (user, email) = seed_user(&conn).await;
    let org = seed_org(&conn).await;
    seed_member(&conn, org, user, OrgRole::Member).await;
    let ws = seed_workspace(&conn, org).await;
    let id = seed_app(&conn, org, ws, "Ops", false).await;

    let caller = Caller::without_credential(user, &email);
    let facts = loader::load_principal_facts_scoped(&conn, &caller, false)
        .await
        .expect("facts");
    // No custom-apps zone, so the app opens on the path route of the base.
    // SAFETY: nextest runs each test in its own process (`test_db` asserts it).
    unsafe {
        for var in [
            "OXY_CUSTOM_APPS_ZONE",
            "OXY_API_URL",
            "OXY_ORG_SUBDOMAIN_ZONE",
        ] {
            std::env::remove_var(var);
        }
    }
    let apps = openable_apps(&conn, &facts, "https://app.example.com/")
        .await
        .expect("apps");
    let [card] = apps.as_slice() else {
        panic!("one app expected, got {}", apps.len());
    };
    assert_eq!((card.id, card.workspace_id, card.org.id), (id, ws, org));
    assert_eq!(
        card.url,
        format!(
            "https://app.example.com/customer-apps/{}/{}/",
            card.org.slug, card.slug
        )
    );
}

#[tokio::test]
async fn someone_in_no_org_has_no_apps() {
    let conn = test_db().await;
    let (user, email) = seed_user(&conn).await;
    let org = seed_org(&conn).await;
    let ws = seed_workspace(&conn, org).await;
    seed_app(&conn, org, ws, "Ops", false).await;
    assert!(listed_for(&conn, (user, &email)).await.is_empty());
}
