//! A **production** app-secret write is audited like every other environment's:
//! set and delete on the staff mount (`/api/customer-apps/{id}/secrets`) and a
//! set on the tenant mount (`/api/workspaces/{ws}/custom-apps/{app_id}/secrets`)
//! each leave a `custom_app.secret.*` row naming `production`, beside the
//! reveal that always did. Before, only a non-production write was recorded,
//! so the console — which wrote production alone — left no row at all.
//!
//! The tenant project-secrets routes reach the same production row by id —
//! Settings → Secrets rotates (`PUT`) and deletes (`DELETE`) an app secret
//! there — and leave the same rows; a plain project secret and a
//! description-only edit leave none.
//!
//! The handlers are called directly with the extractors the router would
//! build, as in `staging_secrets_admin`.

use axum::extract::{Path, Query};
use entity::audit_events;
use entity::workspace_members::WorkspaceRole;
use oxy::service::secret_manager::{CreateSecretParams, SecretManagerService};
use oxy_api_custom_apps::custom_apps_secrets::environment::EnvironmentQuery;
use oxy_api_custom_apps::custom_apps_secrets::{admin_delete, admin_set, workspace_set};
use oxy_app::surface::testing::secrets::{self, UpdateSecretRequest};
use oxy_auth::user::LOCAL_GUEST_EMAIL;
use oxy_server_authz::role_guards::WorkspaceAdmin;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use crate::common::demo_workspace_id;
use crate::custom_app_functions_fixture::seeded_tenant;
use crate::staging_secrets_admin::{app_with_both_values, request, user};

/// `(action, environment, target_id)` of every secret row about `app`, sorted:
/// rows written in the same instant have no order to assert.
async fn secret_rows(
    db: &sea_orm::DatabaseConnection,
    org: Uuid,
    app: Uuid,
) -> Vec<(String, String, String)> {
    let mut rows: Vec<_> = audit_events::Entity::find()
        .filter(audit_events::Column::OrgId.eq(org))
        .filter(audit_events::Column::Action.starts_with("custom_app.secret."))
        .filter(audit_events::Column::TargetId.contains(app.to_string()))
        .all(db)
        .await
        .expect("audit rows")
        .into_iter()
        .map(|r| (r.action, r.environment, r.target_id.unwrap_or_default()))
        .collect();
    rows.sort();
    rows
}

#[tokio::test]
async fn production_secret_writes_are_audited_on_both_mounts() {
    let t = seeded_tenant().await;
    // Staging's value, then production's — both set by staff.
    let app = app_with_both_values(&t, "prod-audit").await;
    let staff = || user(t.guest_id, LOCAL_GUEST_EMAIL);

    admin_delete(
        Path((app, "QB_TOKEN".to_string())),
        Query(EnvironmentQuery::default()),
        staff(),
        None,
    )
    .await
    .expect("staff delete production's");
    workspace_set(
        WorkspaceAdmin(WorkspaceRole::Admin),
        Path((demo_workspace_id(), app)),
        user(t.guest_id, LOCAL_GUEST_EMAIL),
        None,
        request("WEBHOOK_SECRET", "whsec", None),
    )
    .await
    .expect("tenant set production's");
    admin_set(Path(app), staff(), None, request("QB_TOKEN", "again", None))
        .await
        .expect("staff set production's again");

    let production = |action: &str, key: &str| {
        (
            action.to_string(),
            "production".to_string(),
            format!("apps/{app}/{key}"),
        )
    };
    let rows = secret_rows(&t.db, t.org_id, app).await;
    let production_rows: Vec<_> = rows
        .iter()
        .filter(|r| r.1 == "production")
        .cloned()
        .collect();
    assert_eq!(
        production_rows,
        vec![
            production("custom_app.secret.deleted", "QB_TOKEN"),
            production("custom_app.secret.set", "QB_TOKEN"),
            production("custom_app.secret.set", "QB_TOKEN"),
            production("custom_app.secret.set", "WEBHOOK_SECRET"),
        ],
        "every production write, on either mount, leaves a row: {rows:?}"
    );
}

async fn production_rows(
    db: &sea_orm::DatabaseConnection,
    org: Uuid,
    app: Uuid,
) -> Vec<(String, String, String)> {
    let mut rows = secret_rows(db, org, app).await;
    rows.retain(|r| r.1 == "production");
    rows
}

/// The id of the stored secret `name` in the demo workspace.
async fn row_id(db: &sea_orm::DatabaseConnection, name: &str) -> String {
    SecretManagerService::new(demo_workspace_id())
        .list_secrets(db)
        .await
        .expect("rows")
        .into_iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no row {name}"))
        .id
        .to_string()
}

fn edit(body: serde_json::Value) -> axum::Json<UpdateSecretRequest> {
    axum::Json(serde_json::from_value(body).expect("request"))
}

#[tokio::test]
async fn tenant_rotate_and_delete_by_id_are_audited() {
    use axum::response::IntoResponse;
    let t = seeded_tenant().await;
    let app = app_with_both_values(&t, "prod-audit-by-id").await;
    let ws = demo_workspace_id();
    let admin = || WorkspaceAdmin(WorkspaceRole::Admin);
    let tenant = || user(Uuid::new_v4(), "tenant-admin@customer.example");
    // A plain project secret beside the app's, which no app row is about.
    let plain_name = format!("PLAIN_{}", Uuid::new_v4().simple());
    SecretManagerService::new(ws)
        .create_secret(
            &t.db,
            CreateSecretParams {
                name: plain_name.clone(),
                value: "v".to_string(),
                description: None,
                created_by: t.guest_id,
            },
        )
        .await
        .expect("plain secret");

    let production = row_id(&t.db, &format!("apps/{app}/QB_TOKEN")).await;
    let plain = row_id(&t.db, &plain_name).await;
    // The fixture's own staff writes, which are not this test's.
    let before = production_rows(&t.db, t.org_id, app).await;
    for (id, body) in [
        (
            &production,
            serde_json::json!({ "description": "notes only" }),
        ),
        (&production, serde_json::json!({ "value": "rotated" })),
        (&plain, serde_json::json!({ "value": "rotated" })),
    ] {
        let answer = secrets::update_secret(admin(), tenant(), Path((ws, id.clone())), edit(body))
            .await
            .expect("update");
        assert_eq!(answer.into_response().status(), axum::http::StatusCode::OK);
    }
    for id in [&production, &plain] {
        let answer = secrets::delete_secret(admin(), tenant(), Path((ws, id.clone())))
            .await
            .expect("delete");
        let status = answer.into_response().status();
        assert_eq!(status, axum::http::StatusCode::NO_CONTENT);
    }

    let mut added = production_rows(&t.db, t.org_id, app).await;
    for row in &before {
        let at = added
            .iter()
            .position(|r| r == row)
            .expect("earlier row kept");
        added.remove(at);
    }
    let target = format!("apps/{app}/QB_TOKEN");
    assert_eq!(
        added,
        vec![
            (
                "custom_app.secret.deleted".to_string(),
                "production".to_string(),
                target.clone()
            ),
            (
                "custom_app.secret.set".to_string(),
                "production".to_string(),
                target
            ),
        ],
        "one row for the rotation and one for the delete; none for the \
         description-only edit (before: {before:?})"
    );
    let plain_rows = audit_events::Entity::find()
        .filter(audit_events::Column::Action.starts_with("custom_app.secret."))
        .filter(audit_events::Column::TargetId.eq(plain_name.as_str()))
        .all(&t.db)
        .await
        .expect("audit rows");
    assert!(
        plain_rows.is_empty(),
        "a plain project secret is no app's: {plain_rows:?}"
    );
}
