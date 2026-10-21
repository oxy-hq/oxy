//! What `resolve_app_role` remembers: that a user holds **no** role on an app,
//! and nothing else.
//!
//! The memo may over-refuse for up to a minute after a grant, and only until an
//! access-invalidation hook runs. It must never carry a role that was found,
//! because that would keep admitting a revoked admin.

use crate::common::test_db;
use entity::{
    app_members, apps, org_members, org_members::OrgRole, organizations, partner_grants, users,
    workspaces,
};
use oxy_api_custom_apps::custom_apps_auth::{invalidate_access_cache, resolve_app_role};
use oxy_api_custom_apps::custom_apps_cache::cached_no_app_role;
use oxy_server_authz::Caller;
use sea_orm::{ActiveModelTrait, ActiveValue, DatabaseConnection, EntityTrait};
use uuid::Uuid;

struct Fixture {
    db: DatabaseConnection,
    app: apps::Model,
}

async fn seed_user(db: &DatabaseConnection) -> (Uuid, String) {
    let id = Uuid::new_v4();
    let email = format!("memo-{id}@example.com");
    users::ActiveModel {
        id: ActiveValue::Set(id),
        email: ActiveValue::Set(Some(email.clone())),
        name: ActiveValue::Set("App Role Memo".into()),
        picture: ActiveValue::Set(None),
        email_verified: ActiveValue::Set(true),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("seed user");
    (id, email)
}

async fn join_org(db: &DatabaseConnection, org_id: Uuid, user_id: Uuid, role: OrgRole) {
    org_members::ActiveModel {
        id: ActiveValue::Set(Uuid::new_v4()),
        org_id: ActiveValue::Set(org_id),
        user_id: ActiveValue::Set(user_id),
        role: ActiveValue::Set(role),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("seed org member");
}

async fn fixture() -> Fixture {
    let db = test_db().await;
    let org_id = Uuid::new_v4();
    organizations::ActiveModel {
        id: ActiveValue::Set(org_id),
        name: ActiveValue::Set("Memo Org".into()),
        slug: ActiveValue::Set(format!("memo-org-{org_id}")),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("seed org");
    let workspace_id = Uuid::new_v4();
    workspaces::ActiveModel {
        id: ActiveValue::Set(workspace_id),
        name: ActiveValue::Set("Memo Workspace".into()),
        org_id: ActiveValue::Set(Some(org_id)),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("seed workspace");
    let id = Uuid::new_v4();
    let app = apps::ActiveModel {
        id: ActiveValue::Set(id),
        slug: ActiveValue::Set(format!("memo-app-{id}")),
        name: ActiveValue::Set("Memo App".into()),
        org_id: ActiveValue::Set(org_id),
        project_id: ActiveValue::Set(workspace_id),
        branch: ActiveValue::Set("main".into()),
        source_repo: ActiveValue::Set("memo/test".into()),
        status: ActiveValue::Set("active".into()),
        source_type: ActiveValue::Set("s3".into()),
        source_config: ActiveValue::Set(serde_json::json!({})),
        published_at: ActiveValue::Set(Some(chrono::Utc::now().into())),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("seed app");
    Fixture { db, app }
}

/// A grant written straight to the table, as a writer that does not call the
/// invalidation hook would.
async fn grant(db: &DatabaseConnection, app_id: Uuid, user_id: Uuid, role: &str) -> Uuid {
    let id = Uuid::new_v4();
    app_members::ActiveModel {
        id: ActiveValue::Set(id),
        app_id: ActiveValue::Set(app_id),
        user_id: ActiveValue::Set(user_id),
        role: ActiveValue::Set(role.to_string()),
        created_at: ActiveValue::Set(chrono::Utc::now().into()),
        created_by: ActiveValue::Set(None),
    }
    .insert(db)
    .await
    .expect("seed grant");
    id
}

fn session(user_id: Uuid, email: &str) -> Caller {
    Caller::without_credential(user_id, email)
}

#[tokio::test]
async fn a_grant_after_a_no_role_memo_shows_only_after_the_hook_runs() {
    let f = fixture().await;
    let (user, email) = seed_user(&f.db).await;
    join_org(&f.db, f.app.org_id, user, OrgRole::Member).await;
    let caller = session(user, &email);

    assert_eq!(
        resolve_app_role(&f.db, &caller, &f.app).await.unwrap(),
        None
    );
    assert!(
        cached_no_app_role(user, f.app.id, f.app.org_id),
        "a resolution that found no role is remembered"
    );

    // The grant lands without the hook: the memo may over-refuse, never over-admit.
    grant(&f.db, f.app.id, user, app_members::ROLE_MEMBER).await;
    assert_eq!(
        resolve_app_role(&f.db, &caller, &f.app).await.unwrap(),
        None,
        "within the window the memo still answers no role"
    );

    invalidate_access_cache();
    assert!(!cached_no_app_role(user, f.app.id, f.app.org_id));
    assert_eq!(
        resolve_app_role(&f.db, &caller, &f.app).await.unwrap(),
        Some("member"),
        "the hook clears the memo, so the grant is honoured at once"
    );
}

#[tokio::test]
async fn a_role_that_was_found_is_never_written_to_the_no_role_memo() {
    let f = fixture().await;

    // An org owner is the app's admin.
    let (owner, owner_email) = seed_user(&f.db).await;
    join_org(&f.db, f.app.org_id, owner, OrgRole::Owner).await;
    assert_eq!(
        resolve_app_role(&f.db, &session(owner, &owner_email), &f.app)
            .await
            .unwrap(),
        Some("admin")
    );
    assert!(!cached_no_app_role(owner, f.app.id, f.app.org_id));

    // A member with a grant holds the member role, and when the grant is revoked
    // the very next call sees it: no positive answer was kept.
    let (member, member_email) = seed_user(&f.db).await;
    join_org(&f.db, f.app.org_id, member, OrgRole::Member).await;
    let grant_id = grant(&f.db, f.app.id, member, app_members::ROLE_MEMBER).await;
    let caller = session(member, &member_email);
    assert_eq!(
        resolve_app_role(&f.db, &caller, &f.app).await.unwrap(),
        Some("member")
    );
    assert!(!cached_no_app_role(member, f.app.id, f.app.org_id));

    app_members::Entity::delete_by_id(grant_id)
        .exec(&f.db)
        .await
        .expect("revoke the grant");
    assert_eq!(
        resolve_app_role(&f.db, &caller, &f.app).await.unwrap(),
        None,
        "a revoked role is read fresh, not served from memory"
    );
}

#[tokio::test]
async fn no_role_is_not_remembered_for_a_member_of_a_partner_org() {
    // The fact loader collapses a failed partner-standing read to "no standing",
    // so for anyone who could be a partner operator the "not an admin" verdict
    // may be a guess. Nothing is remembered for them.
    let f = fixture().await;
    partner_grants::ActiveModel {
        org_id: ActiveValue::Set(f.app.org_id),
        status: ActiveValue::Set("active".into()),
        created_by: ActiveValue::Set(None),
        created_at: ActiveValue::Set(chrono::Utc::now().into()),
    }
    .insert(&f.db)
    .await
    .expect("seed partner grant");
    let (user, email) = seed_user(&f.db).await;
    join_org(&f.db, f.app.org_id, user, OrgRole::Member).await;

    assert_eq!(
        resolve_app_role(&f.db, &session(user, &email), &f.app)
            .await
            .unwrap(),
        None
    );
    assert!(!cached_no_app_role(user, f.app.id, f.app.org_id));
}
