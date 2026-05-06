//! Integration tests for `TenantProvisioner`.
//!
//! Spins up a Postgres testcontainer, applies the central migrator, and
//! drives `provision` / `deprovision` against a wiremock-backed Airhouse
//! admin client.
//!
//! Run with: `cargo nextest run -p oxy-app --test airhouse_provisioner`

use airhouse::entity::Tenants as AirhouseTenants;
use airhouse::entity::tenants::{self as airhouse_tenants, TenantStatus};
use airhouse::{AirhouseAdminClient, TenantProvisioner};
use chrono::Utc;
use entity::organizations;
use entity::workspaces::{self, WorkspaceStatus};
use migration::{Migrator, MigratorTrait};
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, Database, DatabaseConnection, EntityTrait,
    QueryFilter,
};
use serde_json::{Value, json};
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

static TEST_DB_URL: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();
/// Keeps the Postgres container handle alive for the process lifetime without
/// leaking. `ReuseDirective::Always` means tests across nextest processes share
/// one Postgres container instead of each starting their own.
static TEST_CONTAINER: tokio::sync::OnceCell<
    std::sync::Arc<testcontainers::ContainerAsync<testcontainers_modules::postgres::Postgres>>,
> = tokio::sync::OnceCell::const_new();

async fn test_db() -> DatabaseConnection {
    // Resolve an admin Postgres URL we can `CREATE DATABASE` against. CI runs
    // tests inside a container so Docker-in-Docker is unavailable; when
    // `OXY_DATABASE_URL` is set we use it directly. Locally, spin up (or
    // reuse) a Postgres testcontainer.
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

    let db_name = format!("airhouse_prov_{}", Uuid::new_v4().simple());
    use sea_orm::ConnectionTrait;
    admin
        .execute_unprepared(&format!("CREATE DATABASE \"{db_name}\""))
        .await
        .expect("create per-test database");

    // Replace only the trailing /<dbname>, not occurrences inside the userinfo.
    let test_url = match admin_url.rfind('/') {
        Some(pos) => format!("{}/{db_name}", &admin_url[..pos]),
        None => panic!("admin_url missing path: {admin_url}"),
    };
    let db = Database::connect(&test_url)
        .await
        .expect("connect to per-test database");
    Migrator::up(&db, None).await.expect("run migrations");
    airhouse::migration::up(&db)
        .await
        .expect("run airhouse migrations");
    db
}

/// Create an org + workspace row and return the workspace id.
async fn seed_workspace(db: &DatabaseConnection, name: &str) -> Uuid {
    let now = Utc::now().fixed_offset();

    let org_id = Uuid::new_v4();
    organizations::ActiveModel {
        id: ActiveValue::Set(org_id),
        name: ActiveValue::Set(name.to_string()),
        slug: ActiveValue::Set(format!("{name}-{}", org_id.simple())),
        created_at: ActiveValue::Set(now),
        updated_at: ActiveValue::Set(now),
    }
    .insert(db)
    .await
    .expect("seed org");

    let workspace_id = Uuid::new_v4();
    workspaces::ActiveModel {
        id: ActiveValue::Set(workspace_id),
        name: ActiveValue::Set(name.to_string()),
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
    .expect("seed workspace");

    workspace_id
}

fn tenant_body(id: &str, prefix: &str) -> Value {
    json!({
        "id": id,
        "pg_url": "postgres://h/d",
        "bucket": "test-bucket",
        "prefix": prefix,
        "role": format!("airhouse_tenant_{id}"),
        "status": "active",
        "created_at": "2026-04-29T10:00:00Z",
    })
}

fn make_provisioner(db: DatabaseConnection, server: &MockServer) -> TenantProvisioner {
    let client = AirhouseAdminClient::new(server.uri(), "tok");
    TenantProvisioner::new(db, client)
}

#[tokio::test]
async fn provision_fresh_creates_remote_and_local_row() {
    let db = test_db().await;
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/admin/v1/tenants"))
        .respond_with(move |req: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&req.body).unwrap();
            let id = body["id"].as_str().unwrap().to_string();
            let prefix = format!("tenants/{id}");
            ResponseTemplate::new(201).set_body_json(tenant_body(&id, &prefix))
        })
        .mount(&server)
        .await;

    let workspace_id = seed_workspace(&db, "acme").await;
    let prov = make_provisioner(db.clone(), &server);

    let rec = prov
        .provision(workspace_id, "acme".to_string())
        .await
        .expect("provision");
    assert_eq!(rec.bucket, "test-bucket");

    let local = AirhouseTenants::find()
        .filter(airhouse_tenants::Column::WorkspaceId.eq(workspace_id))
        .one(&db)
        .await
        .unwrap()
        .expect("local row written");
    assert_eq!(local.status, TenantStatus::Active);
    assert_eq!(local.airhouse_tenant_id, rec.id);
}

#[tokio::test]
async fn provision_is_idempotent_when_local_and_remote_exist() {
    let db = test_db().await;
    let server = MockServer::start().await;

    let create_calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let create_calls_clone = create_calls.clone();
    Mock::given(method("POST"))
        .and(path("/admin/v1/tenants"))
        .respond_with(move |req: &wiremock::Request| {
            create_calls_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let body: Value = serde_json::from_slice(&req.body).unwrap();
            let id = body["id"].as_str().unwrap().to_string();
            ResponseTemplate::new(201).set_body_json(tenant_body(&id, "tenants/x"))
        })
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path_regex_admin_get())
        .respond_with(move |req: &wiremock::Request| {
            let id = req.url.path().rsplit('/').next().unwrap().to_string();
            ResponseTemplate::new(200).set_body_json(tenant_body(&id, "tenants/x"))
        })
        .mount(&server)
        .await;

    let workspace_id = seed_workspace(&db, "idem").await;
    let prov = make_provisioner(db.clone(), &server);

    prov.provision(workspace_id, "idem".to_string())
        .await
        .unwrap();
    prov.provision(workspace_id, "idem".to_string())
        .await
        .unwrap();

    assert_eq!(create_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    let count = AirhouseTenants::find()
        .filter(airhouse_tenants::Column::WorkspaceId.eq(workspace_id))
        .all(&db)
        .await
        .unwrap()
        .len();
    assert_eq!(count, 1, "exactly one local row per workspace");
}

#[tokio::test]
async fn provision_adopts_remote_on_409() {
    let db = test_db().await;
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/admin/v1/tenants"))
        .respond_with(ResponseTemplate::new(409).set_body_string("already exists"))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path_regex_admin_get())
        .respond_with(move |req: &wiremock::Request| {
            let id = req.url.path().rsplit('/').next().unwrap().to_string();
            ResponseTemplate::new(200).set_body_json(tenant_body(&id, "tenants/x"))
        })
        .mount(&server)
        .await;

    let workspace_id = seed_workspace(&db, "adopt").await;
    let prov = make_provisioner(db.clone(), &server);

    let rec = prov
        .provision(workspace_id, "adopt".to_string())
        .await
        .expect("provision adopts");
    assert!(!rec.id.is_empty());

    let local = AirhouseTenants::find()
        .filter(airhouse_tenants::Column::WorkspaceId.eq(workspace_id))
        .one(&db)
        .await
        .unwrap()
        .expect("local row written");
    assert_eq!(local.status, TenantStatus::Active);
}

#[tokio::test]
async fn provision_recreates_when_remote_missing() {
    let db = test_db().await;
    let server = MockServer::start().await;

    // Pre-seed a local row pointing at a tenant that "used to exist".
    let workspace_id = seed_workspace(&db, "drift").await;
    let local_id = Uuid::new_v4();
    let stale_tenant_id = "drift-stale".to_string();
    airhouse_tenants::ActiveModel {
        id: ActiveValue::Set(local_id),
        workspace_id: ActiveValue::Set(workspace_id),
        airhouse_tenant_id: ActiveValue::Set(stale_tenant_id.clone()),
        bucket: ActiveValue::Set("test-bucket".into()),
        prefix: ActiveValue::Set(Some("tenants/drift-stale".into())),
        status: ActiveValue::Set(TenantStatus::Failed),
        created_at: ActiveValue::Set(Utc::now().fixed_offset()),
    }
    .insert(&db)
    .await
    .unwrap();

    Mock::given(method("GET"))
        .and(path(format!("/admin/v1/tenants/{stale_tenant_id}")))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/admin/v1/tenants"))
        .respond_with(move |req: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&req.body).unwrap();
            let id = body["id"].as_str().unwrap().to_string();
            ResponseTemplate::new(201).set_body_json(tenant_body(&id, "tenants/drift-stale"))
        })
        .mount(&server)
        .await;

    let prov = make_provisioner(db.clone(), &server);
    // Re-provision: the tenant name is ignored since a local row already exists.
    prov.provision(workspace_id, "drift".to_string())
        .await
        .expect("provision recreates");

    let local = AirhouseTenants::find_by_id(local_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(local.status, TenantStatus::Active);
    assert_eq!(local.airhouse_tenant_id, stale_tenant_id);
}

#[tokio::test]
async fn deprovision_removes_local_and_calls_remote() {
    let db = test_db().await;
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/admin/v1/tenants"))
        .respond_with(move |req: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&req.body).unwrap();
            let id = body["id"].as_str().unwrap().to_string();
            ResponseTemplate::new(201).set_body_json(tenant_body(&id, "tenants/x"))
        })
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path_regex_admin_delete())
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let workspace_id = seed_workspace(&db, "del").await;
    let prov = make_provisioner(db.clone(), &server);
    prov.provision(workspace_id, "del".to_string())
        .await
        .unwrap();
    prov.deprovision(workspace_id).await.unwrap();

    let count = AirhouseTenants::find()
        .filter(airhouse_tenants::Column::WorkspaceId.eq(workspace_id))
        .all(&db)
        .await
        .unwrap()
        .len();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn deprovision_is_noop_when_local_row_absent() {
    let db = test_db().await;
    let server = MockServer::start().await;
    let workspace_id = seed_workspace(&db, "noop").await;

    let prov = make_provisioner(db.clone(), &server);
    prov.deprovision(workspace_id).await.expect("noop");
    // No mocks registered — wiremock would reject any request, so this also
    // verifies we never called Airhouse.
}

#[tokio::test]
async fn invalid_tenant_name_is_rejected_before_airhouse_call() {
    let db = test_db().await;
    let server = MockServer::start().await;
    let workspace_id = seed_workspace(&db, "badname").await;

    let prov = make_provisioner(db.clone(), &server);
    let err = prov
        .provision(workspace_id, "1-starts-with-digit".to_string())
        .await
        .expect_err("invalid name must be rejected");
    assert!(
        matches!(err, airhouse::ProvisionerError::InvalidTenantName(_)),
        "expected InvalidTenantName, got {err:?}"
    );
    // No mocks registered — verifies Airhouse was never called.
}

// `path("/admin/v1/tenants/{id}")` doesn't accept patterns; match by prefix-trimmed regex.
fn path_regex_admin_get() -> wiremock::matchers::PathRegexMatcher {
    wiremock::matchers::path_regex(r"^/admin/v1/tenants/[^/]+$")
}

fn path_regex_admin_delete() -> wiremock::matchers::PathRegexMatcher {
    wiremock::matchers::path_regex(r"^/admin/v1/tenants/[^/]+$")
}
