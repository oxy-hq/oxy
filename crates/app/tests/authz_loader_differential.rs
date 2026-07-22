//! Differential tests for the authz **fact loader**, against a real PostgreSQL.
//!
//! Skips automatically when `OXY_DATABASE_URL` is unset (i.e. pure unit-test builds).
//!
//! To run locally:
//!   OXY_DATABASE_URL=postgres://... cargo nextest run -p oxy-app --test authz_loader_differential
//!
//! ## Why this exists
//!
//! The unit differential tests (`server::authz::differential`) prove the MODEL
//! matches the shipped guards — but they hand-build the [`PrincipalFacts`], so they
//! test an *assumption* about what the loader returns, not the loader. A loader bug
//! (wrong column, missed partner condition, an over-broad set) would sail straight
//! through them and only surface as a live wrong answer.
//!
//! So: seed real rows, run the REAL `load_principal_facts` against them, and assert
//! the facts are what the policy tests assume. Together the two layers are an
//! end-to-end proof with no production traffic required — which matters, because a
//! shadow window can't validate anything while there are ~no users to generate the
//! traffic it samples.
//!
//! Each test seeds its own uniquely-keyed rows, so they're independent and re-runnable.

use entity::{
    app_admins, org_members, organizations, partner_capabilities, partner_grants, partner_orgs,
    partner_role_bindings, users, workspace_members, workspaces,
};
use oxy::database::client::establish_connection;
use oxy_app::server::authz::loader::load_principal_facts;
use sea_orm::{ActiveModelTrait, ActiveValue, DatabaseConnection};
use uuid::Uuid;

fn db_unavailable() -> bool {
    std::env::var("OXY_DATABASE_URL").is_err()
}

async fn seed_user(conn: &DatabaseConnection) -> (Uuid, String) {
    let id = Uuid::new_v4();
    let email = format!("authz-diff-{id}@example.com");
    users::ActiveModel {
        id: ActiveValue::Set(id),
        email: ActiveValue::Set(email.clone()),
        name: ActiveValue::Set("Authz Differential".into()),
        picture: ActiveValue::Set(None),
        email_verified: ActiveValue::Set(true),
        magic_link_token: ActiveValue::Set(None),
        magic_link_token_expires_at: ActiveValue::Set(None),
        status: ActiveValue::Set(users::UserStatus::Active),
        created_at: ActiveValue::NotSet,
        last_login_at: ActiveValue::NotSet,
    }
    .insert(conn)
    .await
    .expect("seed user");
    (id, email)
}

async fn seed_org(conn: &DatabaseConnection) -> Uuid {
    let id = Uuid::new_v4();
    organizations::ActiveModel {
        id: ActiveValue::Set(id),
        name: ActiveValue::Set(format!("Authz Diff Org {id}")),
        slug: ActiveValue::Set(format!("authz-diff-{id}")),
        logo: ActiveValue::NotSet,
        logo_content_type: ActiveValue::NotSet,
        created_at: ActiveValue::NotSet,
        updated_at: ActiveValue::NotSet,
    }
    .insert(conn)
    .await
    .expect("seed org");
    id
}

async fn seed_membership(
    conn: &DatabaseConnection,
    org_id: Uuid,
    user_id: Uuid,
    role: org_members::OrgRole,
) -> Uuid {
    let id = Uuid::new_v4();
    org_members::ActiveModel {
        id: ActiveValue::Set(id),
        org_id: ActiveValue::Set(org_id),
        user_id: ActiveValue::Set(user_id),
        role: ActiveValue::Set(role),
        created_at: ActiveValue::NotSet,
        updated_at: ActiveValue::NotSet,
    }
    .insert(conn)
    .await
    .expect("seed membership");
    id
}

async fn seed_app_admin(conn: &DatabaseConnection, email: &str) {
    app_admins::ActiveModel {
        id: ActiveValue::Set(Uuid::new_v4()),
        email: ActiveValue::Set(email.to_string()),
        granted_by: ActiveValue::Set(None),
        created_at: ActiveValue::NotSet,
    }
    .insert(conn)
    .await
    .expect("seed app_admin");
}

/// The **positive** Global-Admin grant — the direction `app_admins` membership actually
/// *elevates* a principal. `loader_derives_org_role_sets_from_real_rows` only asserts the
/// negative (a user not in `app_admins` is not global admin), which a loader that never
/// set the flag would also pass. Seed the row and prove the grant flows through the
/// `authz::globals::is_app_admin_email` read the loader depends on.
#[tokio::test]
async fn loader_grants_global_admin_to_an_app_admins_member() {
    if db_unavailable() {
        eprintln!("skipping: OXY_DATABASE_URL unset");
        return;
    }
    let conn = establish_connection().await.expect("db connect");

    let (user_id, email) = seed_user(&conn).await;
    seed_app_admin(&conn, &email).await;

    let facts = load_principal_facts(&conn, user_id, &email)
        .await
        .expect("loader must resolve facts against a live seeded database");
    assert!(
        facts.is_global_admin,
        "a user seeded into app_admins must load as Global Admin — the elevation path that \
         only the negative case was guarding"
    );
}

/// The org-role sets are the backbone of every tenant ring: `owned ⊆ admin ⊆ member`.
/// If this derivation is wrong, every ring is wrong.
#[tokio::test]
async fn loader_derives_org_role_sets_from_real_rows() {
    if db_unavailable() {
        eprintln!("skipping: OXY_DATABASE_URL unset");
        return;
    }
    let conn = establish_connection().await.expect("db connect");

    for (role, expect_owned, expect_admin) in [
        (org_members::OrgRole::Owner, true, true),
        (org_members::OrgRole::Admin, false, true),
        (org_members::OrgRole::Member, false, false),
    ] {
        let (user_id, email) = seed_user(&conn).await;
        let org_id = seed_org(&conn).await;
        seed_membership(&conn, org_id, user_id, role.clone()).await;

        let facts = load_principal_facts(&conn, user_id, &email).await.expect(
            "the loader must resolve facts against a live seeded database; None means a lookup \
         errored, which every caller reads as unknown-not-absent",
        );

        assert_eq!(
            facts.owned_orgs.contains(&org_id),
            expect_owned,
            "owned_orgs for {role:?}"
        );
        assert_eq!(
            facts.admin_orgs.contains(&org_id),
            expect_admin,
            "admin_orgs for {role:?}"
        );
        // Every role is a member — the containment the rings rely on.
        assert!(
            facts.member_orgs.contains(&org_id),
            "member_orgs for {role:?}"
        );
    }
}

/// A user must never pick up standing in an org they don't belong to. This is the
/// tenant-isolation property the whole model rests on.
#[tokio::test]
async fn loader_gives_no_standing_in_a_foreign_org() {
    if db_unavailable() {
        eprintln!("skipping: OXY_DATABASE_URL unset");
        return;
    }
    let conn = establish_connection().await.expect("db connect");

    let (user_id, email) = seed_user(&conn).await;
    let mine = seed_org(&conn).await;
    let theirs = seed_org(&conn).await;
    seed_membership(&conn, mine, user_id, org_members::OrgRole::Owner).await;

    let facts = load_principal_facts(&conn, user_id, &email).await.expect(
        "the loader must resolve facts against a live seeded database; None means a lookup \
         errored, which every caller reads as unknown-not-absent",
    );

    assert!(facts.member_orgs.contains(&mine));
    for set in [&facts.owned_orgs, &facts.admin_orgs, &facts.member_orgs] {
        assert!(!set.contains(&theirs), "leaked standing into a foreign org");
    }
    assert!(
        !facts
            .partners
            .iter()
            .any(|p| p.client_orgs.contains(&theirs)),
        "leaked partner standing into a foreign org"
    );
}

/// A plain member of a NON-partner org must get no partner sets — the short-circuit
/// the loader relies on for the common case.
#[tokio::test]
async fn loader_gives_no_partner_sets_to_an_ordinary_member() {
    if db_unavailable() {
        eprintln!("skipping: OXY_DATABASE_URL unset");
        return;
    }
    let conn = establish_connection().await.expect("db connect");

    let (user_id, email) = seed_user(&conn).await;
    let org_id = seed_org(&conn).await;
    seed_membership(&conn, org_id, user_id, org_members::OrgRole::Member).await;

    let facts = load_principal_facts(&conn, user_id, &email).await.expect(
        "the loader must resolve facts against a live seeded database; None means a lookup \
         errored, which every caller reads as unknown-not-absent",
    );
    assert!(facts.partners.is_empty());
    assert!(!facts.is_global_admin, "not seeded into app_admins");
}

/// The partner path end-to-end, and the distinction that a mis-model would have leaked:
/// `managed_orgs` is the coarse "operates this client"; `develop_apps_orgs` is the
/// narrower data-plane capability. A managing partner WITHOUT develop_apps must get the
/// former and NOT the latter — otherwise it reads another tenant's app data.
#[tokio::test]
async fn loader_separates_managed_orgs_from_the_develop_apps_data_plane() {
    if db_unavailable() {
        eprintln!("skipping: OXY_DATABASE_URL unset");
        return;
    }
    let conn = establish_connection().await.expect("db connect");

    for develop_apps in [false, true] {
        let (user_id, email) = seed_user(&conn).await;
        let partner_org = seed_org(&conn).await;
        let client_org = seed_org(&conn).await;

        // The operator is an ordinary member of the PARTNER org...
        let membership_id =
            seed_membership(&conn, partner_org, user_id, org_members::OrgRole::Member).await;

        // ...that org holds an ACTIVE partner grant...
        partner_grants::ActiveModel {
            org_id: ActiveValue::Set(partner_org),
            status: ActiveValue::Set("active".into()),
            created_by: ActiveValue::Set(None),
            created_at: ActiveValue::NotSet,
        }
        .insert(&conn)
        .await
        .expect("seed grant");

        // ...with a ceiling that may or may not include develop_apps...
        partner_capabilities::ActiveModel {
            org_id: ActiveValue::Set(partner_org),
            manage_members: ActiveValue::Set(false),
            manage_apps: ActiveValue::Set(true),
            develop_apps: ActiveValue::Set(develop_apps),
            view_audit: ActiveValue::Set(false),
            manage_billing: ActiveValue::Set(false),
            manage_secrets: ActiveValue::Set(false),
            create_orgs: ActiveValue::Set(false),
            manage_org_settings: ActiveValue::Set(false),
            updated_at: ActiveValue::NotSet,
        }
        .insert(&conn)
        .await
        .expect("seed capabilities");

        // ...the member holds partner access...
        partner_role_bindings::ActiveModel {
            id: ActiveValue::Set(Uuid::new_v4()),
            org_member_id: ActiveValue::Set(membership_id),
            created_at: ActiveValue::NotSet,
        }
        .insert(&conn)
        .await
        .expect("seed binding");

        // ...and the partner manages the client.
        partner_orgs::ActiveModel {
            id: ActiveValue::Set(Uuid::new_v4()),
            partner_org_id: ActiveValue::Set(partner_org),
            managed_org_id: ActiveValue::Set(client_org),
            created_by: ActiveValue::Set(None),
            created_at: ActiveValue::NotSet,
        }
        .insert(&conn)
        .await
        .expect("seed partner_orgs");

        let facts = load_principal_facts(&conn, user_id, &email).await.expect(
            "the loader must resolve facts against a live seeded database; None means a lookup \
         errored, which every caller reads as unknown-not-absent",
        );

        let standing = facts
            .partners
            .iter()
            .find(|p| p.partner_id == partner_org)
            .expect("an operator of an active grant has a standing in that partner");
        assert!(
            standing.client_orgs.contains(&client_org),
            "an operator of an active grant manages the client (develop_apps={develop_apps})"
        );
        assert!(
            standing.caps.contains(&oxy_authz::Cap::ManageApps),
            "the seeded ceiling grants manage_apps"
        );
        assert_eq!(
            standing.caps.contains(&oxy_authz::Cap::DevelopApps),
            develop_apps,
            "the data plane must follow the develop_apps ceiling ONLY (develop_apps={develop_apps})"
        );
        // The client's own org is not one the operator is a member of.
        assert!(!facts.member_orgs.contains(&client_org));
    }
}

/// A member of a partner org WITHOUT a partner-access binding is just an employee of
/// that org — they manage nothing.
#[tokio::test]
async fn loader_gives_no_managed_orgs_without_a_partner_access_binding() {
    if db_unavailable() {
        eprintln!("skipping: OXY_DATABASE_URL unset");
        return;
    }
    let conn = establish_connection().await.expect("db connect");

    let (user_id, email) = seed_user(&conn).await;
    let partner_org = seed_org(&conn).await;
    let client_org = seed_org(&conn).await;
    seed_membership(&conn, partner_org, user_id, org_members::OrgRole::Member).await;

    partner_grants::ActiveModel {
        org_id: ActiveValue::Set(partner_org),
        status: ActiveValue::Set("active".into()),
        created_by: ActiveValue::Set(None),
        created_at: ActiveValue::NotSet,
    }
    .insert(&conn)
    .await
    .expect("seed grant");
    partner_orgs::ActiveModel {
        id: ActiveValue::Set(Uuid::new_v4()),
        partner_org_id: ActiveValue::Set(partner_org),
        managed_org_id: ActiveValue::Set(client_org),
        created_by: ActiveValue::Set(None),
        created_at: ActiveValue::NotSet,
    }
    .insert(&conn)
    .await
    .expect("seed partner_orgs");
    // NOTE: no partner_role_bindings row.

    let facts = load_principal_facts(&conn, user_id, &email).await.expect(
        "the loader must resolve facts against a live seeded database; None means a lookup \
         errored, which every caller reads as unknown-not-absent",
    );
    assert!(
        facts.partners.is_empty(),
        "a member without partner access manages nothing"
    );
}

/// A SUSPENDED grant confers nothing, however complete the rest of the setup is.
#[tokio::test]
async fn loader_ignores_a_suspended_partner_grant() {
    if db_unavailable() {
        eprintln!("skipping: OXY_DATABASE_URL unset");
        return;
    }
    let conn = establish_connection().await.expect("db connect");

    let (user_id, email) = seed_user(&conn).await;
    let partner_org = seed_org(&conn).await;
    let client_org = seed_org(&conn).await;
    let membership_id =
        seed_membership(&conn, partner_org, user_id, org_members::OrgRole::Member).await;

    partner_grants::ActiveModel {
        org_id: ActiveValue::Set(partner_org),
        status: ActiveValue::Set("suspended".into()),
        created_by: ActiveValue::Set(None),
        created_at: ActiveValue::NotSet,
    }
    .insert(&conn)
    .await
    .expect("seed grant");
    partner_role_bindings::ActiveModel {
        id: ActiveValue::Set(Uuid::new_v4()),
        org_member_id: ActiveValue::Set(membership_id),
        created_at: ActiveValue::NotSet,
    }
    .insert(&conn)
    .await
    .expect("seed binding");
    partner_orgs::ActiveModel {
        id: ActiveValue::Set(Uuid::new_v4()),
        partner_org_id: ActiveValue::Set(partner_org),
        managed_org_id: ActiveValue::Set(client_org),
        created_by: ActiveValue::Set(None),
        created_at: ActiveValue::NotSet,
    }
    .insert(&conn)
    .await
    .expect("seed partner_orgs");

    let facts = load_principal_facts(&conn, user_id, &email).await.expect(
        "the loader must resolve facts against a live seeded database; None means a lookup \
         errored, which every caller reads as unknown-not-absent",
    );
    assert!(
        facts.partners.is_empty(),
        "a suspended grant must confer nothing"
    );
}

/// `ws_admin_override` must carry ONLY the exceptional elevations — a Viewer/Member
/// override must not appear, or a plain member would gain workspace-admin.
#[tokio::test]
async fn loader_loads_only_elevating_workspace_overrides() {
    if db_unavailable() {
        eprintln!("skipping: OXY_DATABASE_URL unset");
        return;
    }
    let conn = establish_connection().await.expect("db connect");

    for (role, expect) in [
        (workspace_members::WorkspaceRole::Owner, true),
        (workspace_members::WorkspaceRole::Admin, true),
        (workspace_members::WorkspaceRole::Member, false),
        (workspace_members::WorkspaceRole::Viewer, false),
    ] {
        let (user_id, email) = seed_user(&conn).await;
        let org_id = seed_org(&conn).await;
        seed_membership(&conn, org_id, user_id, org_members::OrgRole::Member).await;

        let workspace_id = Uuid::new_v4();
        workspaces::ActiveModel {
            id: ActiveValue::Set(workspace_id),
            name: ActiveValue::Set(format!("authz-diff-ws-{workspace_id}")),
            org_id: ActiveValue::Set(Some(org_id)),
            created_by: ActiveValue::Set(Some(user_id)),
            status: ActiveValue::Set(workspaces::WorkspaceStatus::Ready),
            ..Default::default()
        }
        .insert(&conn)
        .await
        .expect("seed workspace");

        workspace_members::ActiveModel {
            id: ActiveValue::Set(Uuid::new_v4()),
            workspace_id: ActiveValue::Set(workspace_id),
            user_id: ActiveValue::Set(user_id),
            role: ActiveValue::Set(role.clone()),
            created_at: ActiveValue::NotSet,
            updated_at: ActiveValue::NotSet,
        }
        .insert(&conn)
        .await
        .expect("seed workspace member");

        let facts = load_principal_facts(&conn, user_id, &email).await.expect(
            "the loader must resolve facts against a live seeded database; None means a lookup \
         errored, which every caller reads as unknown-not-absent",
        );
        assert_eq!(
            facts.ws_admin_override.contains(&workspace_id),
            expect,
            "ws_admin_override must carry only Admin/Owner elevations (role={role:?})"
        );
    }
}
