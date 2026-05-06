//! Integration tests for `UserProvisioner` and the `/airhouse/me/credentials`
//! one-time-reveal flow.
//!
//! Spins up a Postgres testcontainer, applies the central migrator, and drives
//! the provisioner / reveal logic against a wiremock-backed Airhouse client.
//!
//! Run with: `cargo nextest run -p oxy-app --test airhouse_user_provisioner`

use airhouse::entity::Users as AirhouseUsers;
use airhouse::entity::tenants::{self as airhouse_tenants, TenantStatus};
use airhouse::entity::users::{self as airhouse_users, AirhouseUserStatus};
use airhouse::user_provisioner::secret_name_for;
use airhouse::{AirhouseAdminClient, UserProvisioner};
use base64::Engine as _;
use base64::engine::general_purpose;
use chrono::Utc;
use entity::org_members::OrgRole;
use entity::organizations;
use entity::users::{self, UserStatus};
use entity::workspaces::{self, WorkspaceStatus};
use migration::{Migrator, MigratorTrait};
use oxy::adapters::secrets::OrgSecretsService;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, ConnectionTrait, Database, DatabaseConnection,
    EntityTrait, QueryFilter,
};
use serde_json::{Value, json};
use std::sync::Mutex;
use uuid::Uuid;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

static TEST_DB_URL: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();
/// Keeps the Postgres container handle alive for the process lifetime without
/// leaking. `ReuseDirective::Always` means tests across nextest processes share
/// one Postgres container instead of each starting their own.
static TEST_CONTAINER: tokio::sync::OnceCell<
    std::sync::Arc<testcontainers::ContainerAsync<testcontainers_modules::postgres::Postgres>>,
> = tokio::sync::OnceCell::const_new();
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Set a test-stable encryption key so OrgSecretsService can encrypt/decrypt.
fn set_test_encryption_key() {
    let _g = ENV_LOCK.lock().unwrap();
    unsafe {
        std::env::set_var(
            "OXY_ENCRYPTION_KEY",
            general_purpose::STANDARD.encode([7u8; 32]),
        );
    }
}

async fn test_db() -> DatabaseConnection {
    // CI runs tests inside a container, so Docker-in-Docker is unavailable.
    // When `OXY_DATABASE_URL` is set we use it as the admin URL directly;
    // locally fall back to a Postgres testcontainer.
    let admin_url = TEST_DB_URL
        .get_or_init(|| async {
            if let Ok(url) = std::env::var("OXY_DATABASE_URL") {
                return url;
            }

            use testcontainers::runners::AsyncRunner;
            use testcontainers::{ImageExt, ReuseDirective};
            use testcontainers_modules::postgres::Postgres;

            let container = TEST_CONTAINER
                .get_or_init(|| async {
                    std::sync::Arc::new(
                        Postgres::default()
                            .with_tag("18-alpine")
                            .with_reuse(ReuseDirective::Always)
                            .start()
                            .await
                            .expect("start postgres testcontainer (is Docker running?)"),
                    )
                })
                .await;
            let port = container
                .get_host_port_ipv4(5432_u16)
                .await
                .expect("get postgres port");
            format!("postgresql://postgres:postgres@127.0.0.1:{port}/postgres")
        })
        .await
        .clone();

    let mut admin = None;
    for attempt in 0..10 {
        match Database::connect(&admin_url).await {
            Ok(c) => {
                admin = Some(c);
                break;
            }
            Err(e) if attempt < 9 => {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                eprintln!("connect attempt {attempt} failed: {e}");
            }
            Err(e) => panic!("connect: {e}"),
        }
    }
    let admin = admin.unwrap();

    let db_name = format!("airhouse_user_{}", Uuid::new_v4().simple());
    admin
        .execute_unprepared(&format!("CREATE DATABASE \"{db_name}\""))
        .await
        .expect("create per-test database");

    let test_url = match admin_url.rfind('/') {
        Some(pos) => format!("{}/{db_name}", &admin_url[..pos]),
        None => panic!("admin_url missing path"),
    };
    let db = Database::connect(&test_url)
        .await
        .expect("connect to per-test database");
    Migrator::up(&db, None).await.expect("run migrations");
    airhouse::migration::up(&db)
        .await
        .expect("run airhouse migrations");

    // Point oxy::database::client::establish_connection() at our per-test DB so
    // OrgSecretsService can use it.
    unsafe { std::env::set_var("OXY_DATABASE_URL", &test_url) };
    db
}

/// Seed an org, workspace, user, and a pre-existing active tenant.
/// Returns `(workspace_id, user_id, tenant_model)`.
async fn seed_workspace_with_user(
    db: &DatabaseConnection,
    email: &str,
) -> (Uuid, Uuid, airhouse_tenants::Model) {
    let user_id = Uuid::new_v4();
    let now = Utc::now().fixed_offset();
    users::ActiveModel {
        id: ActiveValue::Set(user_id),
        email: ActiveValue::Set(format!("{email}-{user_id}@example.com")),
        name: ActiveValue::Set("Test".into()),
        picture: ActiveValue::Set(None),
        email_verified: ActiveValue::Set(true),
        magic_link_token: ActiveValue::Set(None),
        magic_link_token_expires_at: ActiveValue::Set(None),
        status: ActiveValue::Set(UserStatus::Active),
        created_at: ActiveValue::NotSet,
        last_login_at: ActiveValue::NotSet,
    }
    .insert(db)
    .await
    .expect("insert user");

    let org_id = Uuid::new_v4();
    organizations::ActiveModel {
        id: ActiveValue::Set(org_id),
        name: ActiveValue::Set("Test Org".into()),
        slug: ActiveValue::Set(format!("org-{}", org_id.simple())),
        created_at: ActiveValue::Set(now),
        updated_at: ActiveValue::Set(now),
    }
    .insert(db)
    .await
    .expect("insert org");

    let workspace_id = Uuid::new_v4();
    workspaces::ActiveModel {
        id: ActiveValue::Set(workspace_id),
        name: ActiveValue::Set("Test Workspace".into()),
        git_namespace_id: ActiveValue::Set(None),
        git_remote_url: ActiveValue::Set(None),
        created_at: ActiveValue::Set(now),
        updated_at: ActiveValue::Set(now),
        path: ActiveValue::Set(None),
        last_opened_at: ActiveValue::Set(None),
        created_by: ActiveValue::Set(None),
        org_id: ActiveValue::Set(Some(org_id)),
        status: ActiveValue::Set(WorkspaceStatus::Ready),
        error: ActiveValue::Set(None),
    }
    .insert(db)
    .await
    .expect("insert workspace");

    let tenant = airhouse_tenants::ActiveModel {
        id: ActiveValue::Set(Uuid::new_v4()),
        workspace_id: ActiveValue::Set(workspace_id),
        airhouse_tenant_id: ActiveValue::Set({
            let s = workspace_id.simple().to_string();
            format!("ten-{}", &s[..8])
        }),
        bucket: ActiveValue::Set("test-bucket".into()),
        prefix: ActiveValue::Set(Some("tenants/x".into())),
        status: ActiveValue::Set(TenantStatus::Active),
        created_at: ActiveValue::Set(now),
    }
    .insert(db)
    .await
    .expect("insert tenant");

    (workspace_id, user_id, tenant)
}

fn user_resp(id: &str, tenant_id: &str, role: &str) -> Value {
    json!({
        "id": Uuid::new_v4().to_string(),
        "tenant_id": tenant_id,
        "username": id,
        "role": role,
        "created_at": "2026-04-29T10:01:00Z",
    })
}

fn make_provisioner(db: DatabaseConnection, server: &MockServer) -> UserProvisioner {
    let client = AirhouseAdminClient::new(server.uri(), "tok");
    UserProvisioner::new(db, client)
}

#[tokio::test]
async fn provision_creates_user_and_persists_password_secret() {
    set_test_encryption_key();
    let db = test_db().await;
    let server = MockServer::start().await;
    let (workspace_id, user_id, tenant) = seed_workspace_with_user(&db, "alice").await;

    let tenant_id = tenant.airhouse_tenant_id.clone();
    Mock::given(method("POST"))
        .and(path(format!("/admin/v1/tenants/{tenant_id}/users")))
        .respond_with(move |req: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&req.body).unwrap();
            let username = body["username"].as_str().unwrap();
            let role = body["role"].as_str().unwrap();
            ResponseTemplate::new(201).set_body_json(user_resp(username, &tenant_id, role))
        })
        .mount(&server)
        .await;

    let prov = make_provisioner(db.clone(), &server);
    let provisioned = prov
        .provision(user_id, workspace_id, OrgRole::Member)
        .await
        .expect("provision");

    let local = AirhouseUsers::find_by_id(provisioned.airhouse_user_id)
        .one(&db)
        .await
        .unwrap()
        .expect("local row");
    assert_eq!(local.status, AirhouseUserStatus::Active);
    assert!(local.password_revealed_at.is_none());
    let secret_id = local.password_secret_id.expect("secret stored");

    // Decrypting the stored secret yields a non-empty password.
    let pw = OrgSecretsService::get_by_id(secret_id).await.unwrap();
    assert!(!pw.is_empty());
    assert_eq!(pw.len(), 43, "url-safe base64 of 32 bytes");
}

#[tokio::test]
async fn provision_is_idempotent() {
    set_test_encryption_key();
    let db = test_db().await;
    let server = MockServer::start().await;
    let (workspace_id, user_id, tenant) = seed_workspace_with_user(&db, "idem").await;

    let create_calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let calls = create_calls.clone();
    let tenant_id = tenant.airhouse_tenant_id.clone();
    Mock::given(method("POST"))
        .and(path(format!("/admin/v1/tenants/{tenant_id}/users")))
        .respond_with(move |req: &wiremock::Request| {
            calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let body: Value = serde_json::from_slice(&req.body).unwrap();
            let username = body["username"].as_str().unwrap();
            let role = body["role"].as_str().unwrap();
            ResponseTemplate::new(201).set_body_json(user_resp(username, &tenant_id, role))
        })
        .mount(&server)
        .await;

    let prov = make_provisioner(db.clone(), &server);
    prov.provision(user_id, workspace_id, OrgRole::Member)
        .await
        .unwrap();
    prov.provision(user_id, workspace_id, OrgRole::Member)
        .await
        .unwrap();

    assert_eq!(
        create_calls.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "second provision must not call airhouse"
    );
    let count = AirhouseUsers::find()
        .filter(airhouse_users::Column::WorkspaceId.eq(workspace_id))
        .filter(airhouse_users::Column::OxyUserId.eq(user_id))
        .all(&db)
        .await
        .unwrap()
        .len();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn provision_owner_maps_to_admin_role() {
    set_test_encryption_key();
    let db = test_db().await;
    let server = MockServer::start().await;
    let (workspace_id, user_id, tenant) = seed_workspace_with_user(&db, "owner").await;

    let captured_role = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let captured = captured_role.clone();
    let tenant_id = tenant.airhouse_tenant_id.clone();
    Mock::given(method("POST"))
        .and(path(format!("/admin/v1/tenants/{tenant_id}/users")))
        .respond_with(move |req: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&req.body).unwrap();
            let username = body["username"].as_str().unwrap();
            let role = body["role"].as_str().unwrap().to_string();
            *captured.lock().unwrap() = role.clone();
            ResponseTemplate::new(201).set_body_json(user_resp(username, &tenant_id, &role))
        })
        .mount(&server)
        .await;

    let prov = make_provisioner(db.clone(), &server);
    prov.provision(user_id, workspace_id, OrgRole::Owner)
        .await
        .unwrap();
    assert_eq!(*captured_role.lock().unwrap(), "admin");
}

#[tokio::test]
async fn deprovision_removes_local_and_secret() {
    set_test_encryption_key();
    let db = test_db().await;
    let server = MockServer::start().await;
    let (workspace_id, user_id, tenant) = seed_workspace_with_user(&db, "del").await;

    let tenant_id = tenant.airhouse_tenant_id.clone();
    Mock::given(method("POST"))
        .and(path(format!("/admin/v1/tenants/{tenant_id}/users")))
        .respond_with(move |req: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&req.body).unwrap();
            let username = body["username"].as_str().unwrap();
            let role = body["role"].as_str().unwrap();
            ResponseTemplate::new(201).set_body_json(user_resp(username, &tenant_id, role))
        })
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path_regex(r"^/admin/v1/tenants/[^/]+/users/[^/]+$"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let prov = make_provisioner(db.clone(), &server);
    let provisioned = prov
        .provision(user_id, workspace_id, OrgRole::Member)
        .await
        .unwrap();
    let secret_id = AirhouseUsers::find_by_id(provisioned.airhouse_user_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap()
        .password_secret_id
        .unwrap();

    prov.deprovision(user_id, workspace_id).await.unwrap();

    let local_count = AirhouseUsers::find()
        .filter(airhouse_users::Column::WorkspaceId.eq(workspace_id))
        .filter(airhouse_users::Column::OxyUserId.eq(user_id))
        .all(&db)
        .await
        .unwrap()
        .len();
    assert_eq!(local_count, 0);

    // Secret must be gone too.
    assert!(OrgSecretsService::get_by_id(secret_id).await.is_err());
}

#[tokio::test]
async fn deprovision_is_noop_when_local_row_absent() {
    set_test_encryption_key();
    let db = test_db().await;
    let server = MockServer::start().await;
    let (workspace_id, user_id, _) = seed_workspace_with_user(&db, "noop").await;

    let prov = make_provisioner(db.clone(), &server);
    prov.deprovision(user_id, workspace_id)
        .await
        .expect("noop deprovision");
    // No mocks registered — wiremock would reject any request.
}

#[tokio::test]
async fn reveal_returns_password_on_every_call() {
    // The reveal flow no longer deletes the secret on first read. Both calls
    // return the same password; only `password_revealed_at` advances.
    set_test_encryption_key();
    let db = test_db().await;
    let server = MockServer::start().await;
    let (workspace_id, user_id, tenant) = seed_workspace_with_user(&db, "reveal").await;

    let tenant_id = tenant.airhouse_tenant_id.clone();
    Mock::given(method("POST"))
        .and(path(format!("/admin/v1/tenants/{tenant_id}/users")))
        .respond_with(move |req: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&req.body).unwrap();
            let username = body["username"].as_str().unwrap();
            let role = body["role"].as_str().unwrap();
            ResponseTemplate::new(201).set_body_json(user_resp(username, &tenant_id, role))
        })
        .mount(&server)
        .await;

    let prov = make_provisioner(db.clone(), &server);
    prov.provision(user_id, workspace_id, OrgRole::Member)
        .await
        .unwrap();

    let pw1 = simulate_reveal(&db, workspace_id, user_id)
        .await
        .unwrap()
        .expect("first reveal returns Some");
    assert!(!pw1.is_empty());
    let pw2 = simulate_reveal(&db, workspace_id, user_id)
        .await
        .unwrap()
        .expect("second reveal returns Some");
    assert_eq!(pw1, pw2, "secret must persist across reveals");

    // DB state: revealed_at set, secret_id still present.
    let local = AirhouseUsers::find()
        .filter(airhouse_users::Column::WorkspaceId.eq(workspace_id))
        .filter(airhouse_users::Column::OxyUserId.eq(user_id))
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(local.password_revealed_at.is_some());
    assert!(local.password_secret_id.is_some());
}

/// Mimics `airhouse_me::get_credentials` for the test (without HTTP):
/// always decrypts and returns the secret, marking `password_revealed_at`.
async fn simulate_reveal(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<Option<String>, sea_orm::DbErr> {
    use sea_orm::ActiveValue;
    let local = AirhouseUsers::find()
        .filter(airhouse_users::Column::WorkspaceId.eq(workspace_id))
        .filter(airhouse_users::Column::OxyUserId.eq(user_id))
        .one(db)
        .await?
        .expect("local row exists");
    let Some(secret_id) = local.password_secret_id else {
        return Ok(None);
    };
    let pw = OrgSecretsService::get_by_id(secret_id).await.unwrap();
    let mut active: airhouse_users::ActiveModel = local.into();
    active.password_revealed_at = ActiveValue::Set(Some(Utc::now().fixed_offset()));
    active.update(db).await?;
    Ok(Some(pw))
}

#[tokio::test]
async fn rotate_password_replaces_secret_and_calls_airhouse_delete_create() {
    set_test_encryption_key();
    let db = test_db().await;
    let server = MockServer::start().await;
    let (workspace_id, user_id, tenant) = seed_workspace_with_user(&db, "rot").await;

    let create_calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let delete_calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let create_calls_clone = create_calls.clone();
    let tenant_id = tenant.airhouse_tenant_id.clone();
    Mock::given(method("POST"))
        .and(path(format!("/admin/v1/tenants/{tenant_id}/users")))
        .respond_with(move |req: &wiremock::Request| {
            create_calls_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let body: Value = serde_json::from_slice(&req.body).unwrap();
            let username = body["username"].as_str().unwrap();
            let role = body["role"].as_str().unwrap();
            ResponseTemplate::new(201).set_body_json(user_resp(username, &tenant_id, role))
        })
        .mount(&server)
        .await;
    let delete_calls_clone = delete_calls.clone();
    Mock::given(method("DELETE"))
        .and(path_regex(r"^/admin/v1/tenants/[^/]+/users/[^/]+$"))
        .respond_with(move |_req: &wiremock::Request| {
            delete_calls_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            ResponseTemplate::new(204)
        })
        .mount(&server)
        .await;

    let prov = make_provisioner(db.clone(), &server);
    prov.provision(user_id, workspace_id, OrgRole::Member)
        .await
        .unwrap();

    let local_before = AirhouseUsers::find()
        .filter(airhouse_users::Column::WorkspaceId.eq(workspace_id))
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let secret_before = OrgSecretsService::get_by_id(local_before.password_secret_id.unwrap())
        .await
        .unwrap();

    // Mark as revealed so we can assert rotation clears the timestamp.
    let _ = simulate_reveal(&db, workspace_id, user_id).await;

    prov.rotate_password(user_id, workspace_id)
        .await
        .expect("rotate password");

    assert_eq!(
        delete_calls.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "rotation must delete the existing Airhouse user"
    );
    assert_eq!(
        create_calls.load(std::sync::atomic::Ordering::SeqCst),
        2,
        "rotation must create a new Airhouse user (1 from provision + 1 from rotate)"
    );

    let local_after = AirhouseUsers::find_by_id(local_before.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(local_after.username, local_before.username);
    assert!(local_after.password_secret_id.is_some());
    assert!(
        local_after.password_revealed_at.is_none(),
        "rotation must reset the audit timestamp so the UI re-shows 'first reveal' cues"
    );
    let secret_after = OrgSecretsService::get_by_id(local_after.password_secret_id.unwrap())
        .await
        .unwrap();
    assert_ne!(secret_before, secret_after, "secret must change");
}

// Force the test binary to retain the helper.
#[allow(dead_code)]
fn _ensure_used(ws: Uuid, id: Uuid) -> String {
    secret_name_for(ws, id)
}
