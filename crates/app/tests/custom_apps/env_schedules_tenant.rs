//! The tenant's schedules API never sees a custom app's non-production
//! function schedule (environments design §4.5, ruling 1): `list` leaves it
//! out, every by-id route answers `404`, and create refuses a ref naming an
//! environment. Production's row is the tenant's as before.

use std::sync::Arc;

use axum::Json;
use axum::extract::Path;
use axum::http::StatusCode;
use serde_json::Value;
use uuid::Uuid;

use crate::common::demo_workspace_id;
use crate::custom_app_functions_fixture::seeded_tenant;
use crate::env_schedules::{flip, rows, sandbox_ref, scheduled_app};

/// Every run in this binary is threadless.
struct NoThreads;

#[async_trait::async_trait]
impl agentic_pipeline::platform::ThreadOwnerLookup for NoThreads {
    async fn thread_owner(&self, _thread: Uuid) -> Result<Option<Option<Uuid>>, String> {
        Ok(None)
    }
}

async fn answer(response: axum::response::Response) -> (StatusCode, Value) {
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// The tenant's schedules API never sees an environment's row: the list
/// leaves it out, every by-id route answers `404`, and create refuses a ref
/// naming an environment. Production's row is the tenant's as before.
#[tokio::test]
async fn the_tenant_schedules_api_never_sees_an_environments_row() {
    use agentic_pipeline::scheduler::{BackfillRequest, ScheduleInput};
    use axum::extract::Extension;
    use entity::workspace_members::WorkspaceRole;
    use oxy_app::server::api::middlewares::role_guards::WorkspaceAdmin;
    use oxy_app::server::api::schedules;
    use oxy_auth::extractor::AuthenticatedUserExtractor;

    let t = seeded_tenant().await;
    let app = scheduled_app(&t, "env-sched-tenant").await;
    flip(&t, app.id, "dev-a1", true).await.expect("turn on");
    let ws = demo_workspace_id();
    let state = Extension(Arc::new(agentic_http::AgenticState::new(
        tokio_util::sync::CancellationToken::new(),
        t.db.clone(),
        Arc::new(NoThreads),
    )));
    let (platform, _dir) = crate::custom_app_functions_manual_run::platform().await;
    let user = || AuthenticatedUserExtractor(t.guest().user);
    let admin = || WorkspaceAdmin(WorkspaceRole::Admin);
    let id = agentic_pipeline::scheduler::list_schedules(&t.db, ws)
        .await
        .expect("list")
        .into_iter()
        .find(|s| s.target_ref == sandbox_ref(app.id))
        .expect("the sandbox's row")
        .id;

    let (status, listed) = answer(schedules::list(state.clone(), user(), Path(ws)).await).await;
    assert_eq!(status, StatusCode::OK);
    let refs: Vec<&str> = listed
        .as_array()
        .expect("a list")
        .iter()
        .filter_map(|s| s["target_ref"].as_str())
        .collect();
    assert!(
        refs.contains(&format!("{}/tick", app.id).as_str()),
        "production's: {refs:?}"
    );
    assert!(
        !refs.contains(&sandbox_ref(app.id).as_str()),
        "never the sandbox's"
    );

    let by_id = || Path((ws, id.clone()));
    let input = |target_ref: String| ScheduleInput {
        name: "mine".into(),
        target_kind: "function".into(),
        target_ref,
        question: None,
        variables: None,
        cron_expr: "0 * * * *".into(),
        timezone: "UTC".into(),
        enabled: true,
    };
    let production_ref = format!("{}/tick", app.id);
    let not_found = [
        schedules::get(state.clone(), user(), by_id()).await,
        schedules::update(
            admin(),
            state.clone(),
            user(),
            by_id(),
            Json(input(production_ref)),
        )
        .await,
        schedules::delete(admin(), state.clone(), user(), by_id()).await,
        schedules::run_now(
            admin(),
            state.clone(),
            Extension(platform.clone()),
            user(),
            by_id(),
        )
        .await,
        schedules::backfill(
            admin(),
            state.clone(),
            Extension(platform.clone()),
            user(),
            by_id(),
            Json(BackfillRequest {
                from: chrono::Utc::now() - chrono::Duration::hours(3),
                to: chrono::Utc::now(),
                concurrency: None,
            }),
        )
        .await,
    ];
    for response in not_found {
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
    let created = schedules::create(
        admin(),
        state.clone(),
        user(),
        Path(ws),
        Json(input(sandbox_ref(app.id))),
    )
    .await;
    assert_eq!(
        created.status(),
        StatusCode::BAD_REQUEST,
        "a ref naming an environment"
    );
    assert_eq!(
        rows(&t.db, app.id).await,
        vec![sandbox_ref(app.id), format!("{}/tick", app.id)],
        "nothing the tenant sent touched the sandbox's row"
    );
}
