//! A production move applies the target build's pending migrations before the
//! pointer moves — re-read from the stored bundle — and a failure leaves the
//! pointer where it was (environments design §5.5a, ruling 4). The org's OLTP
//! store is provisioned for real (`AppStore`), on an org of the test's own.

use axum::http::StatusCode;
use entity::workspaces;
use oxy_api_custom_apps::admin::apps::handlers::publish_one;
use oxy_api_custom_apps::custom_apps_migrations::{MigrationTarget, read_ledger};
use oxy_api_custom_apps::custom_apps_promote::AuditedBy;
use oxy_api_custom_apps::custom_apps_promote::resolve::PromoteRequest;
use oxy_api_custom_apps::custom_apps_promote::{PromoteError, act};
use oxy_api_custom_apps::custom_apps_publish::publish;
use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter};
use serde_json::json;
use uuid::Uuid;

use crate::custom_app_functions_fixture::{Tenant, seeded_tenant, throwaway_org};
use crate::custom_app_functions_shape_zoo_oltp::AppStore;
use crate::sandbox_publish::{build_pk, bundle, input, noop};

const ORDERS: &[u8] = b"create table orders (id int primary key);";
const BROKEN: &[u8] = b"create table broken (;";
const EXTRA: &[u8] = b"create table extra (id int);";

/// An app with a provisioned OLTP store in a workspace of its own org.
struct Migrating {
    t: Tenant,
    slug: String,
    workspace: Uuid,
    store: AppStore,
}

impl Migrating {
    async fn new() -> Self {
        let t = throwaway_org(&seeded_tenant().await).await;
        let slug = format!("prm-{}", &Uuid::new_v4().simple().to_string()[..8]);
        let workspace = Uuid::new_v4();
        workspaces::ActiveModel {
            id: ActiveValue::Set(workspace),
            name: ActiveValue::Set("Promote migrations".into()),
            org_id: ActiveValue::Set(Some(t.org_id)),
            ..Default::default()
        }
        .insert(&t.db)
        .await
        .expect("seed the workspace");
        let store = AppStore::new(&t, &slug).await;
        store.provision(workspace).await;
        Self {
            t,
            slug,
            workspace,
            store,
        }
    }

    /// A draft `build_id` declaring `files` under `migrations/`.
    async fn draft(&self, build_id: &str, files: &[(&str, &[u8])]) -> Uuid {
        let manifest = json!({ "migrations": { "dir": "migrations" } });
        let tarball = bundle(
            &self.slug,
            &[noop("noop", json!({ "route": true }))],
            manifest,
            files,
        );
        let mut draft = input(&self.t, &self.slug, build_id, tarball);
        draft.project_id = self.workspace;
        publish(draft).await.expect("a draft");
        self.build(build_id).await
    }

    async fn build(&self, build_id: &str) -> Uuid {
        let app = self.app_id().await;
        build_pk(&self.t.db, app, build_id)
            .await
            .expect("the build")
    }

    async fn app_id(&self) -> Uuid {
        entity::apps::Entity::find()
            .filter(entity::apps::Column::OrgId.eq(self.t.org_id))
            .filter(entity::apps::Column::Slug.eq(&self.slug))
            .one(&self.t.db)
            .await
            .expect("read the app")
            .expect("the app")
            .id
    }

    async fn production(&self) -> Option<Uuid> {
        crate::app_environments::serving(&self.t.db, self.app_id().await)
            .await
            .production
    }

    async fn ledger(&self) -> Vec<String> {
        let ledger = read_ledger(
            &self.t.db,
            self.app_id().await,
            "oltp",
            &MigrationTarget::Production,
        )
        .await
        .expect("read production's ledger");
        let mut files: Vec<String> = ledger.into_keys().collect();
        files.sort();
        files
    }

    async fn promote(&self) -> Result<serde_json::Value, PromoteError> {
        let req = PromoteRequest {
            from: "staging".into(),
            ..Default::default()
        };
        let plan = act::promote(
            &self.t.db,
            &self.t.guest(),
            self.app_id().await,
            "production",
            &req,
        )
        .await?;
        Ok(serde_json::to_value(plan).expect("a plan"))
    }
}

#[tokio::test]
async fn a_promote_applies_pending_files_before_the_pointer_and_a_failure_keeps_it() {
    let app = Migrating::new().await;
    let first = app
        .draft("m-1", &[("migrations/0001_orders.sql", ORDERS)])
        .await;
    assert!(
        app.ledger().await.is_empty(),
        "a draft applies nothing to production"
    );

    let plan = app.promote().await.expect("the promote");
    assert_eq!(
        plan["migrations"]["oltp"],
        json!(["0001_orders.sql"]),
        "{plan}"
    );
    assert_eq!(
        plan["migrations_applied"]["oltp"],
        json!(["0001_orders.sql"]),
        "{plan}"
    );
    assert_eq!(app.production().await, Some(first));
    assert_eq!(app.ledger().await, vec!["0001_orders.sql"]);

    app.draft(
        "m-2",
        &[
            ("migrations/0001_orders.sql", ORDERS),
            ("migrations/0002_broken.sql", BROKEN),
        ],
    )
    .await;
    let refused = app.promote().await.expect_err("a broken file");
    assert!(matches!(refused, PromoteError::Migration(_)), "{refused:?}");
    assert_eq!(refused.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(refused.code(), "migration_failed");
    assert_eq!(app.production().await, Some(first), "the pointer stayed");
    assert_eq!(app.ledger().await, vec!["0001_orders.sql"]);
    app.store.cleanup().await;
}

/// The console's Make Live is the same production move: a draft's new file is
/// applied before production moves to it — the hole a draft followed by Make
/// Live used to leave (new code in front of a table never created).
#[tokio::test]
async fn make_live_applies_the_drafts_pending_files() {
    let app = Migrating::new().await;
    app.draft("m-1", &[("migrations/0001_orders.sql", ORDERS)])
        .await;
    let second = app
        .draft(
            "m-2",
            &[
                ("migrations/0001_orders.sql", ORDERS),
                ("migrations/0003_extra.sql", EXTRA),
            ],
        )
        .await;
    let app_id = app.app_id().await;
    publish_one(&app.t.db, app_id, &app.t.guest(), AuditedBy::Caller)
        .await
        .map_err(|e| e.status)
        .expect("make live");
    assert_eq!(app.production().await, Some(second));
    assert_eq!(
        app.ledger().await,
        vec!["0001_orders.sql", "0003_extra.sql"]
    );
    app.store.cleanup().await;
}
