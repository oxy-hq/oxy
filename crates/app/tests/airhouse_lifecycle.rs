//! End-to-end integration test for the Airhouse onboarding lifecycle.
//!
//! Exercises:
//!   1. provision tenant (TenantProvisioner)
//!   2. create user (UserProvisioner)
//!   3. fetch connection info via the HTTP `/airhouse/me/connection` endpoint
//!   4. fetch credentials via the HTTP `/airhouse/me/credentials` endpoint
//!      (verifies the one-time-show flow flips state)
//!   5. deprovision user (UserProvisioner)
//!   6. deprovision tenant (TenantProvisioner)
//!
//! Plus two error-mapping tests:
//!   - 409 on create-tenant → provisioner adopts the existing remote tenant
//!   - 404 on delete-user   → deprovision still succeeds (idempotent)
//!
//! Postgres is provided by testcontainers; the Airhouse admin API is stubbed
//! with wiremock and matchers verify the auth header, path, and body shape.
//!
//! Run with:
//!   cargo nextest run -p oxy-app --test airhouse_lifecycle

use airhouse::api::handlers as airhouse_me;
use airhouse::entity::tenants as airhouse_tenants;
use airhouse::entity::users::{self as airhouse_users, AirhouseUserStatus};
use airhouse::entity::{Tenants as AirhouseTenants, Users as AirhouseUsers};
use airhouse::{AirhouseAdminClient, AirhouseError, TenantProvisioner, UserProvisioner};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{get, post};
use base64::Engine as _;
use base64::engine::general_purpose;
use chrono::Utc;
use entity::org_members::{self, OrgRole};
use entity::organizations;
use entity::users::{self, UserStatus};
use entity::workspaces::{self, WorkspaceStatus};
use migration::{Migrator, MigratorTrait};
use oxy_auth::types::AuthenticatedUser;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, ConnectionTrait, Database, DatabaseConnection,
    EntityTrait, QueryFilter,
};
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::matchers::{body_partial_json, header, method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ADMIN_TOKEN: &str = "test-admin-token";

// ── shared bootstrap ────────────────────────────────────────────────────────

static TEST_DB_URL: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();
/// Keeps the Postgres container handle alive for the process lifetime without
/// leaking. `ReuseDirective::Always` means tests across nextest processes share
/// one Postgres container instead of each starting their own.
static TEST_CONTAINER: tokio::sync::OnceCell<
    std::sync::Arc<testcontainers::ContainerAsync<testcontainers_modules::postgres::Postgres>>,
> = tokio::sync::OnceCell::const_new();

/// Resolve a Postgres "admin" URL we can `CREATE DATABASE` against, then
/// create a fresh per-test database, run the migrator, and point the global
/// `establish_connection()` pool at it.
///
/// CI runs the test process inside a container, so spawning Docker-in-Docker
/// via testcontainers fails. When `OXY_DATABASE_URL` is set (CI provisions a
/// postgres service container), use that directly. Locally, fall back to
/// testcontainers.
async fn test_db() -> DatabaseConnection {
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

    let db_name = format!("airhouse_lc_{}", Uuid::new_v4().simple());
    admin
        .execute_unprepared(&format!("CREATE DATABASE \"{db_name}\""))
        .await
        .expect("create per-test database");

    let test_url = match admin_url.rfind('/') {
        Some(pos) => format!("{}/{db_name}", &admin_url[..pos]),
        None => panic!("admin_url missing path"),
    };

    // Point oxy::database::client::establish_connection() at our per-test DB.
    unsafe { std::env::set_var("OXY_DATABASE_URL", &test_url) };

    let db = Database::connect(&test_url)
        .await
        .expect("connect to per-test database");
    Migrator::up(&db, None).await.expect("run migrations");
    airhouse::migration::up(&db)
        .await
        .expect("run airhouse migrations");
    db
}

fn set_test_encryption_key() {
    unsafe {
        std::env::set_var(
            "OXY_ENCRYPTION_KEY",
            general_purpose::STANDARD.encode([3u8; 32]),
        );
    }
}

/// Set the Airhouse env vars so `AirhouseConfig::cached()` (used by the HTTP
/// handlers) resolves to `Enabled`. The cached config is a process-wide
/// `OnceLock`, so this must be called before the first handler invocation —
/// nextest runs each test in its own process so the cache starts empty.
fn set_airhouse_env(server: &MockServer) {
    unsafe {
        std::env::set_var("AIRHOUSE_BASE_URL", server.uri());
        std::env::set_var("AIRHOUSE_ADMIN_TOKEN", ADMIN_TOKEN);
        std::env::set_var("AIRHOUSE_WIRE_HOST", "airhouse.test");
        std::env::set_var("AIRHOUSE_WIRE_PORT", "5445");
    }
}

fn admin_client(server: &MockServer) -> AirhouseAdminClient {
    AirhouseAdminClient::new(server.uri(), ADMIN_TOKEN)
}

/// Seed a user, org, workspace, and org membership row.
/// Returns `(workspace_id, user_id, AuthenticatedUser)`.
async fn seed_user_and_workspace(
    db: &DatabaseConnection,
    email_prefix: &str,
) -> (Uuid, Uuid, AuthenticatedUser) {
    seed_user_workspace_with_role(db, email_prefix, Some(OrgRole::Owner)).await
}

async fn seed_user_workspace_with_role(
    db: &DatabaseConnection,
    email_prefix: &str,
    role: Option<OrgRole>,
) -> (Uuid, Uuid, AuthenticatedUser) {
    let user_id = Uuid::new_v4();
    let now = Utc::now().fixed_offset();
    let email = format!("{email_prefix}-{user_id}@example.com");

    let user = users::ActiveModel {
        id: ActiveValue::Set(user_id),
        email: ActiveValue::Set(email.clone()),
        name: ActiveValue::Set("Test User".into()),
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
        name: ActiveValue::Set("Acme".into()),
        slug: ActiveValue::Set(format!("acme-{}", &org_id.simple().to_string()[..8])),
        created_at: ActiveValue::Set(now),
        updated_at: ActiveValue::Set(now),
    }
    .insert(db)
    .await
    .expect("insert org");

    let workspace_id = Uuid::new_v4();
    workspaces::ActiveModel {
        id: ActiveValue::Set(workspace_id),
        name: ActiveValue::Set("Acme Workspace".into()),
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

    if let Some(r) = role {
        org_members::ActiveModel {
            id: ActiveValue::Set(Uuid::new_v4()),
            org_id: ActiveValue::Set(org_id),
            user_id: ActiveValue::Set(user_id),
            role: ActiveValue::Set(r),
            created_at: ActiveValue::Set(now),
            updated_at: ActiveValue::Set(now),
        }
        .insert(db)
        .await
        .expect("insert membership");
    }

    let auth = AuthenticatedUser {
        id: user.id,
        email: user.email,
        name: user.name,
        picture: user.picture,
        status: user.status,
    };
    (workspace_id, user_id, auth)
}

// ── HTTP test harness ───────────────────────────────────────────────────────

fn auth_inject_layer(
    user: AuthenticatedUser,
) -> impl Clone + Fn(Request<Body>, Next) -> futures::future::BoxFuture<'static, Response> {
    move |mut req: Request<Body>, next: Next| {
        let user = user.clone();
        Box::pin(async move {
            req.extensions_mut().insert(user);
            next.run(req).await
        })
    }
}

fn build_router(user: AuthenticatedUser) -> Router {
    Router::new()
        .route("/airhouse/me/connection", get(airhouse_me::get_connection))
        .route(
            "/airhouse/me/credentials",
            get(airhouse_me::get_credentials),
        )
        .route("/airhouse/me/provision", post(airhouse_me::provision))
        .layer(middleware::from_fn(auth_inject_layer(user)))
}

async fn send_json(
    router: &Router,
    method_name: &str,
    path_str: &str,
    body: &str,
) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(method_name)
        .uri(path_str)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = router.clone().oneshot(req).await.expect("oneshot");
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .expect("read body");
    let json: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("parse json")
    };
    (status, json)
}

async fn get_json(router: &Router, path_str: &str) -> (StatusCode, Value) {
    send_json(router, "GET", path_str, "").await
}

async fn post_json(router: &Router, path_str: &str, tenant_name: &str) -> (StatusCode, Value) {
    let body = json!({ "tenant_name": tenant_name }).to_string();
    send_json(router, "POST", path_str, &body).await
}

// ── happy-path lifecycle ────────────────────────────────────────────────────

#[tokio::test]
async fn full_lifecycle_provision_to_deprovision() {
    set_test_encryption_key();
    let db = test_db().await;
    let server = MockServer::start().await;
    set_airhouse_env(&server);
    let (workspace_id, user_id, auth) = seed_user_and_workspace(&db, "alice").await;

    // POST /admin/v1/tenants — must carry the bearer token and a body with
    // ONLY {id}. The new Airhouse API rejects any extra fields like bucket
    // or prefix; the response still surfaces them, but they come from the
    // server's `[storage]` config now, not from us.
    Mock::given(method("POST"))
        .and(path("/admin/v1/tenants"))
        .and(header("authorization", &*format!("Bearer {ADMIN_TOKEN}")))
        .respond_with(move |req: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&req.body).unwrap();
            assert!(
                body.get("bucket").is_none() && body.get("prefix").is_none(),
                "create-tenant request must not include bucket/prefix (got {body})"
            );
            let id = body["id"].as_str().unwrap().to_string();
            let prefix = format!("tenants/{id}");
            ResponseTemplate::new(201).set_body_json(json!({
                "id": id,
                "pg_url": "postgres://internal",
                "bucket": "test-bucket",
                "prefix": prefix,
                "role": format!("airhouse_tenant_{id}"),
                "status": "active",
                "created_at": "2026-04-29T10:00:00Z",
            }))
        })
        .expect(1)
        .mount(&server)
        .await;

    // POST /admin/v1/tenants/{tenant}/users
    Mock::given(method("POST"))
        .and(path_regex(r"^/admin/v1/tenants/[^/]+/users$"))
        .and(header("authorization", &*format!("Bearer {ADMIN_TOKEN}")))
        .and(body_partial_json(json!({ "role": "admin" })))
        .respond_with(move |req: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&req.body).unwrap();
            assert!(
                body["password"].as_str().is_some_and(|p| !p.is_empty()),
                "password must be a non-empty plaintext string in the request"
            );
            let tenant_id = req
                .url
                .path()
                .trim_start_matches("/admin/v1/tenants/")
                .trim_end_matches("/users")
                .to_string();
            ResponseTemplate::new(201).set_body_json(json!({
                "id": Uuid::new_v4().to_string(),
                "tenant_id": tenant_id,
                "username": body["username"],
                "role": body["role"],
                "created_at": "2026-04-29T10:01:00Z",
            }))
        })
        .expect(1)
        .mount(&server)
        .await;

    // GET /admin/v1/tenants/{tenant} — reconcile on re-provision.
    Mock::given(method("GET"))
        .and(path_regex(r"^/admin/v1/tenants/[^/]+$"))
        .and(header("authorization", &*format!("Bearer {ADMIN_TOKEN}")))
        .respond_with(move |req: &wiremock::Request| {
            let id = req.url.path().rsplit('/').next().unwrap().to_string();
            ResponseTemplate::new(200).set_body_json(json!({
                "id": id,
                "pg_url": "postgres://internal",
                "bucket": "test-bucket",
                "prefix": format!("tenants/{id}"),
                "role": format!("airhouse_tenant_{id}"),
                "status": "active",
                "created_at": "2026-04-29T10:00:00Z",
            }))
        })
        .mount(&server)
        .await;

    // DELETE /admin/v1/tenants/{tenant}/users/{username}
    Mock::given(method("DELETE"))
        .and(path_regex(r"^/admin/v1/tenants/[^/]+/users/[^/]+$"))
        .and(header("authorization", &*format!("Bearer {ADMIN_TOKEN}")))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    // DELETE /admin/v1/tenants/{tenant}
    Mock::given(method("DELETE"))
        .and(path_regex(r"^/admin/v1/tenants/[^/]+$"))
        .and(header("authorization", &*format!("Bearer {ADMIN_TOKEN}")))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let router = build_router(auth);

    // Pre-provision: GET /connection must 404.
    let (status, _) = get_json(
        &router,
        &format!("/airhouse/me/connection?workspace_id={workspace_id}"),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "must 404 before provisioning"
    );

    // ── 1+2. provision tenant + user via the HTTP endpoint ─────────────
    let (status, body) = post_json(
        &router,
        &format!("/airhouse/me/provision?workspace_id={workspace_id}"),
        "acme",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["host"], "airhouse.test");
    assert_eq!(body["port"], 5445);
    assert_eq!(body["password_not_yet_shown"], true);
    let tenant_id = body["dbname"].as_str().expect("dbname").to_string();
    let username = body["username"].as_str().expect("username").to_string();

    let tenant_row = AirhouseTenants::find()
        .filter(airhouse_tenants::Column::WorkspaceId.eq(workspace_id))
        .one(&db)
        .await
        .unwrap()
        .expect("local tenant row written by provision");
    assert_eq!(tenant_row.airhouse_tenant_id, tenant_id);

    // Idempotency: a second POST must NOT call Airhouse again.
    let (status, body) = post_json(
        &router,
        &format!("/airhouse/me/provision?workspace_id={workspace_id}"),
        "acme",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["username"], username);

    // ── 3. fetch connection info via API ─────────────────────────────────
    let (status, body) = get_json(
        &router,
        &format!("/airhouse/me/connection?workspace_id={workspace_id}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["host"], "airhouse.test");
    assert_eq!(body["dbname"], tenant_id);
    assert_eq!(body["username"].as_str().unwrap(), username);
    assert_eq!(body["password_not_yet_shown"], true);

    // ── 4. fetch credentials — password persists across calls ────────────
    let (status, creds1) = get_json(
        &router,
        &format!("/airhouse/me/credentials?workspace_id={workspace_id}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let first_pw = creds1["password"]
        .as_str()
        .filter(|p| !p.is_empty())
        .expect("first credentials call must include the password")
        .to_string();
    assert_eq!(creds1["password_already_revealed"], false);

    let (status, creds2) = get_json(
        &router,
        &format!("/airhouse/me/credentials?workspace_id={workspace_id}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(creds2["password"].as_str().unwrap(), first_pw);
    assert_eq!(creds2["password_already_revealed"], true);

    let (status, body) = get_json(
        &router,
        &format!("/airhouse/me/connection?workspace_id={workspace_id}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["password_not_yet_shown"], false);

    // ── 5. delete user ────────────────────────────────────────────────────
    let user_prov = UserProvisioner::new(db.clone(), admin_client(&server));
    user_prov
        .deprovision(user_id, workspace_id)
        .await
        .expect("deprovision user");
    let local_users = AirhouseUsers::find()
        .filter(airhouse_users::Column::WorkspaceId.eq(workspace_id))
        .all(&db)
        .await
        .unwrap();
    assert!(local_users.is_empty(), "local user row must be cleared");

    // ── 6. delete tenant ──────────────────────────────────────────────────
    let tenant_prov = TenantProvisioner::new(db.clone(), admin_client(&server));
    tenant_prov
        .deprovision(workspace_id)
        .await
        .expect("deprovision tenant");
    let local_tenants = AirhouseTenants::find()
        .filter(airhouse_tenants::Column::WorkspaceId.eq(workspace_id))
        .all(&db)
        .await
        .unwrap();
    assert!(local_tenants.is_empty(), "local tenant row must be cleared");
}

// ── error-mapping: 409 on create-tenant ─────────────────────────────────────

#[tokio::test]
async fn create_tenant_409_is_adopted() {
    set_test_encryption_key();
    let db = test_db().await;
    let server = MockServer::start().await;
    let (workspace_id, _user_id, _auth) = seed_user_and_workspace(&db, "preexisting").await;

    Mock::given(method("POST"))
        .and(path("/admin/v1/tenants"))
        .and(header("authorization", &*format!("Bearer {ADMIN_TOKEN}")))
        .respond_with(ResponseTemplate::new(409).set_body_string("tenant already exists"))
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path_regex(r"^/admin/v1/tenants/[^/]+$"))
        .and(header("authorization", &*format!("Bearer {ADMIN_TOKEN}")))
        .respond_with(move |req: &wiremock::Request| {
            let id = req.url.path().rsplit('/').next().unwrap().to_string();
            ResponseTemplate::new(200).set_body_json(json!({
                "id": id,
                "pg_url": "postgres://internal",
                "bucket": "test-bucket",
                "prefix": format!("tenants/{id}"),
                "role": format!("airhouse_tenant_{id}"),
                "status": "active",
                "created_at": "2026-04-29T10:00:00Z",
            }))
        })
        .expect(1)
        .mount(&server)
        .await;

    let prov = TenantProvisioner::new(db.clone(), admin_client(&server));
    let rec = prov
        .provision(workspace_id, "preexisting".to_string())
        .await
        .expect("409 must be mapped to adoption, not propagated as an error");
    assert_eq!(rec.bucket, "test-bucket");

    let local = AirhouseTenants::find()
        .filter(airhouse_tenants::Column::WorkspaceId.eq(workspace_id))
        .one(&db)
        .await
        .unwrap()
        .expect("local row must be written even when the remote tenant pre-existed");
    assert_eq!(local.airhouse_tenant_id, rec.id);
}

// ── error-mapping: 404 on delete-user ───────────────────────────────────────

#[tokio::test]
async fn delete_user_404_is_idempotent() {
    set_test_encryption_key();
    let db = test_db().await;
    let server = MockServer::start().await;
    let (workspace_id, user_id, _auth) = seed_user_and_workspace(&db, "deleted").await;

    // Pre-seed an active local user row pointing at a tenant that exists locally.
    let now = Utc::now().fixed_offset();
    let tenant_row_id = Uuid::new_v4();
    airhouse_tenants::ActiveModel {
        id: ActiveValue::Set(tenant_row_id),
        workspace_id: ActiveValue::Set(workspace_id),
        airhouse_tenant_id: ActiveValue::Set("acme-zzz".into()),
        bucket: ActiveValue::Set("test-bucket".into()),
        prefix: ActiveValue::Set(Some("tenants/acme-zzz".into())),
        status: ActiveValue::Set(airhouse::entity::tenants::TenantStatus::Active),
        created_at: ActiveValue::Set(now),
    }
    .insert(&db)
    .await
    .unwrap();

    let local_user_id = Uuid::new_v4();
    airhouse_users::ActiveModel {
        id: ActiveValue::Set(local_user_id),
        tenant_row_id: ActiveValue::Set(tenant_row_id),
        workspace_id: ActiveValue::Set(workspace_id),
        oxy_user_id: ActiveValue::Set(user_id),
        username: ActiveValue::Set("ghost".into()),
        role: ActiveValue::Set(airhouse::entity::users::AirhouseUserRole::Reader),
        password_secret_id: ActiveValue::Set(None),
        password_revealed_at: ActiveValue::Set(None),
        status: ActiveValue::Set(AirhouseUserStatus::Active),
        created_at: ActiveValue::Set(now),
    }
    .insert(&db)
    .await
    .unwrap();

    Mock::given(method("DELETE"))
        .and(path("/admin/v1/tenants/acme-zzz/users/ghost"))
        .and(header("authorization", &*format!("Bearer {ADMIN_TOKEN}")))
        .respond_with(ResponseTemplate::new(404))
        .expect(2)
        .mount(&server)
        .await;

    let raw_result = admin_client(&server).delete_user("acme-zzz", "ghost").await;
    match raw_result {
        Ok(deleted) => assert!(!deleted, "404 must surface as Ok(false)"),
        Err(e) => panic!("404 on delete_user should not propagate as an error: {e:?}"),
    }

    let prov = UserProvisioner::new(db.clone(), admin_client(&server));
    prov.deprovision(user_id, workspace_id)
        .await
        .expect("404 from delete_user must not break deprovision");
    let remaining = AirhouseUsers::find()
        .filter(airhouse_users::Column::WorkspaceId.eq(workspace_id))
        .all(&db)
        .await
        .unwrap();
    assert!(remaining.is_empty(), "local user row must be cleared");
}

// ── provision endpoint: 503 when Airhouse is not configured ─────────────────

#[tokio::test]
async fn provision_returns_503_when_airhouse_disabled() {
    set_test_encryption_key();
    let db = test_db().await;
    let (workspace_id, _user_id, auth) = seed_user_and_workspace(&db, "no-airhouse").await;

    let router = build_router(auth);
    let (status, _) = post_json(
        &router,
        &format!("/airhouse/me/provision?workspace_id={workspace_id}"),
        "testname",
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);

    let (status, _) = get_json(
        &router,
        &format!("/airhouse/me/connection?workspace_id={workspace_id}"),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

// ── provision endpoint: 403 for callers who are not org members ─────────────

#[tokio::test]
async fn provision_returns_403_for_non_members() {
    set_test_encryption_key();
    let db = test_db().await;
    let server = MockServer::start().await;
    set_airhouse_env(&server);
    // Pass `None` for the role so no `org_members` row is written.
    let (workspace_id, _user_id, auth) = seed_user_workspace_with_role(&db, "stranger", None).await;

    let router = build_router(auth);
    let (status, _) = post_json(
        &router,
        &format!("/airhouse/me/provision?workspace_id={workspace_id}"),
        "stranger",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

// ── provision endpoint: 422 for invalid tenant names ────────────────────────

#[tokio::test]
async fn provision_returns_422_for_invalid_tenant_name() {
    set_test_encryption_key();
    let db = test_db().await;
    let server = MockServer::start().await;
    set_airhouse_env(&server);
    let (workspace_id, _user_id, auth) = seed_user_and_workspace(&db, "badname").await;

    let router = build_router(auth);
    let (status, _) = post_json(
        &router,
        &format!("/airhouse/me/provision?workspace_id={workspace_id}"),
        "1-starts-with-digit",
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

// ── unused-import guard ──────────────────────────────────────────────────────

#[allow(dead_code)]
fn _ensure_error_in_scope() -> Option<AirhouseError> {
    None
}
