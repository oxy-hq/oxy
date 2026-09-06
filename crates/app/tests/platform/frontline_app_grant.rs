//! The custom-app gate's frontline term, against a real database.
//!
//! This is the test two rounds of review asked for and two commit messages
//! promised. Both frontline defects on this branch lived in exactly this
//! decision and neither was reachable from a unit test:
//!
//!   * the first version admitted any active worker and trusted `Ring::AppAccess`
//!     to narrow it — but the ring is handed a WORKSPACE id where it expects an
//!     app id, so it never narrowed anything, and the term was live only while
//!     the fact loader was erroring;
//!   * the second required a grant but was then zeroed by `enforce`'s
//!     conjunction, so a granted worker got 403 on every healthy request.
//!
//! What makes those catchable here and not above is the join. The decision is
//! "active standing AND a grant on an app in THIS workspace", and every way of
//! getting it wrong — dropping the standing check, dropping the grant check,
//! scoping the grant to the org instead of the workspace — still passes a happy
//! path with one worker, one app and one grant. So each case below removes
//! exactly one of those and asserts the answer flips.

use entity::{app_members, apps, org_frontline_members, org_members, organizations, users};
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use uuid::Uuid;

use crate::common::{Schema, fresh_db};
use oxy_app::server::api::custom_apps_functions::host::org_directory;
use oxy_app::server::api::custom_apps_gates::frontline_worker_with_app_grant;

/// One org, two workspaces, one app in each.
struct Fx {
    org: Uuid,
    workspace_a: Uuid,
    workspace_b: Uuid,
    app_a: Uuid,
    app_b: Uuid,
}

async fn user(db: &DatabaseConnection, name: &str, email: Option<String>) -> Uuid {
    let id = Uuid::new_v4();
    users::ActiveModel {
        id: ActiveValue::Set(id),
        // `None` for a worker: a frontline user is deliberately mailbox-less,
        // and that NULL is what keeps them out of every email-keyed path.
        email: ActiveValue::Set(email),
        name: ActiveValue::Set(name.into()),
        picture: ActiveValue::Set(None),
        email_verified: ActiveValue::Set(false),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("seed user");
    id
}

async fn seed(db: &DatabaseConnection) -> Fx {
    let org = Uuid::new_v4();
    organizations::ActiveModel {
        id: ActiveValue::Set(org),
        name: ActiveValue::Set("Poke".into()),
        slug: ActiveValue::Set(format!("poke-{}", &org.simple().to_string()[..8])),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("seed org");

    let mut ws = Vec::new();
    for label in ["a", "b"] {
        let id = Uuid::new_v4();
        entity::workspaces::ActiveModel {
            id: ActiveValue::Set(id),
            org_id: ActiveValue::Set(Some(org)),
            name: ActiveValue::Set(format!("workspace {label}")),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("seed workspace");
        ws.push(id);
    }

    let mut app = Vec::new();
    for (i, project) in ws.iter().enumerate() {
        let id = Uuid::new_v4();
        apps::ActiveModel {
            id: ActiveValue::Set(id),
            org_id: ActiveValue::Set(org),
            project_id: ActiveValue::Set(*project),
            slug: ActiveValue::Set(format!("app-{i}")),
            name: ActiveValue::Set(format!("App {i}")),
            // Every NOT NULL column the table actually has. Spelled out rather
            // than defaulted, because a fixture that cannot insert is a test
            // that proves nothing about the logic it was written for.
            branch: ActiveValue::Set("main".into()),
            source_repo: ActiveValue::Set("git@example.com:poke/app.git".into()),
            status: ActiveValue::Set("active".into()),
            source_type: ActiveValue::Set("git".into()),
            source_config: ActiveValue::Set(serde_json::json!({})),
            visibility: ActiveValue::Set("org".into()),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("seed app");
        app.push(id);
    }

    Fx {
        org,
        workspace_a: ws[0],
        workspace_b: ws[1],
        app_a: app[0],
        app_b: app[1],
    }
}

async fn enrol(db: &DatabaseConnection, org: Uuid, user_id: Uuid, status: &str) {
    org_frontline_members::ActiveModel {
        org_id: ActiveValue::Set(org),
        user_id: ActiveValue::Set(user_id),
        status: ActiveValue::Set(status.into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("enrol worker");
}

async fn grant(db: &DatabaseConnection, app_id: Uuid, user_id: Uuid) {
    app_members::ActiveModel {
        id: ActiveValue::Set(Uuid::new_v4()),
        app_id: ActiveValue::Set(app_id),
        user_id: ActiveValue::Set(user_id),
        role: ActiveValue::Set("member".into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("grant app membership");
}

/// The decision, one removed ingredient at a time.
#[tokio::test]
async fn a_worker_reaches_only_the_workspace_they_hold_a_grant_in() {
    let (db, _url) = fresh_db(Schema::Central).await;
    let fx = seed(&db).await;

    // ── The happy path: active standing + a grant on an app in workspace A.
    let granted = user(&db, "Maria S.", None).await;
    enrol(&db, fx.org, granted, "active").await;
    grant(&db, fx.app_a, granted).await;

    assert!(
        frontline_worker_with_app_grant(&db, fx.org, granted, fx.workspace_a).await,
        "an enrolled worker holding a grant on this workspace's app must reach it — \
         this is the case that returned 403 on every healthy request"
    );

    // ── Same worker, OTHER workspace. The join is the only thing that denies
    // this, and without it the term reads "has a grant anywhere in the org" —
    // exactly the org-wide reach a worker must not have.
    assert!(
        !frontline_worker_with_app_grant(&db, fx.org, granted, fx.workspace_b).await,
        "a grant on one workspace's app must not open another's"
    );

    // ── Standing, no grant. This is the fail-open the first version shipped:
    // it admitted every active worker and trusted a ring that could not see the
    // grant to narrow it.
    let ungranted = user(&db, "Sam T.", None).await;
    enrol(&db, fx.org, ungranted, "active").await;
    assert!(
        !frontline_worker_with_app_grant(&db, fx.org, ungranted, fx.workspace_a).await,
        "standing alone must not be access"
    );

    // ── Grant, no standing. A plain user with an app grant is not a frontline
    // worker, and this term must not be the thing that lets them in — their
    // access comes from org membership through the ring, not from here.
    let outsider = user(&db, "Nia O.", Some("nia@example.com".into())).await;
    grant(&db, fx.app_a, outsider).await;
    assert!(
        !frontline_worker_with_app_grant(&db, fx.org, outsider, fx.workspace_a).await,
        "a grant without frontline standing is not this term's business"
    );

    // ── Suspended standing. `is_active_frontline` reads `status == "active"`,
    // and a worker who has left must lose access without their rows being
    // deleted — the submissions they filed still have to attribute to somebody.
    let suspended = user(&db, "Gone A.", None).await;
    enrol(&db, fx.org, suspended, "suspended").await;
    grant(&db, fx.app_a, suspended).await;
    assert!(
        !frontline_worker_with_app_grant(&db, fx.org, suspended, fx.workspace_a).await,
        "suspended standing must deny even with a live grant"
    );

    // ── The fixture must not be proving this by accident: no worker here holds
    // an org_members row, which is what would make every assertion above pass
    // for the wrong reason on a term that checked membership instead.
    for who in [granted, ungranted, suspended] {
        assert!(
            org_members::Entity::find()
                // By USER id, not `find_by_id`. `org_members` has a surrogate
                // primary key, so `find_by_id(user)` searches the membership-row
                // id space with a `users.id` — two spaces that never collide, so
                // it returned None unconditionally. The assertion written to
                // stop this test passing for the wrong reason was itself passing
                // for the wrong reason, and would have held with all three
                // workers holding memberships.
                .filter(org_members::Column::UserId.eq(who))
                .one(&db)
                .await
                .expect("query")
                .is_none(),
            "a seeded worker has an org_members row, so this test proves nothing"
        );
    }
}

/// The directory names people who can reach THIS app, and nobody else.
///
/// One rule covering two kinds of principal, so the cases that matter are where
/// the halves disagree: a worker granted here is in, the SAME worker is out of
/// the app next door, and suspending them takes them out of both. A plain union
/// of the two tables passes the first and fails the other two, which is the
/// version this replaces.
#[tokio::test]
async fn the_directory_names_whoever_can_reach_this_app() {
    let (db, _url) = fresh_db(Schema::Central).await;
    let fx = seed(&db).await;

    let office = user(&db, "Nia O.", Some("nia@example.com".into())).await;
    org_members::ActiveModel {
        id: ActiveValue::Set(Uuid::new_v4()),
        org_id: ActiveValue::Set(fx.org),
        user_id: ActiveValue::Set(office),
        role: ActiveValue::Set(entity::org_members::OrgRole::Member),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("seed member");

    let crew = user(&db, "Maria S.", None).await;
    enrol(&db, fx.org, crew, "active").await;
    grant(&db, fx.app_a, crew).await;

    let ungranted = user(&db, "Sam T.", None).await;
    enrol(&db, fx.org, ungranted, "active").await;

    async fn names(db: &DatabaseConnection, org: Uuid, app: Uuid) -> Vec<String> {
        org_directory(db, org, app).await.expect("directory")["people"]
            .as_array()
            .expect("people array")
            .iter()
            .map(|p| p["name"].as_str().unwrap_or_default().to_string())
            .collect()
    }

    let here = names(&db, fx.org, fx.app_a).await;
    assert!(
        here.contains(&"Nia O.".to_string()),
        "an org member must be in it"
    );
    assert!(
        here.contains(&"Maria S.".to_string()),
        "a worker granted on THIS app must be nameable — an app that cannot name most \
         of its users grows its own people model, which is what this exists to stop"
    );
    assert!(
        !here.contains(&"Sam T.".to_string()),
        "a worker with no grant cannot reach this app, so it must not name them"
    );

    // The app next door: same org, same worker, no grant. This is the case a
    // plain union gets wrong.
    let next_door = names(&db, fx.org, fx.app_b).await;
    assert!(
        next_door.contains(&"Nia O.".to_string()),
        "org membership is not per-app"
    );
    assert!(
        !next_door.contains(&"Maria S.".to_string()),
        "a grant is per-app, so the directory's frontline half must be too"
    );

    // Suspension removes them here as well as from the login and the kiosk —
    // one column, and every surface that reads it agrees.
    oxy_auth::frontline::set_worker_standing(&db, fx.org, crew, false)
        .await
        .expect("suspend");
    assert!(
        !names(&db, fx.org, fx.app_a)
            .await
            .contains(&"Maria S.".to_string()),
        "a suspended worker must leave the directory too"
    );
}
