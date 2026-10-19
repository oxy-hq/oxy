//! **Run now `as=schedule`**
//! (`POST …/functions/{name}/runs?environment=<env>&as=schedule`,
//! environments design §4.5 ruling 3), end to end through the admin route,
//! the production driver entry point and executor, as in the parent module.
//!
//! - in a sandbox whose schedules switch is off it runs anyway — a person
//!   asked — on the sandbox's build, records one `manual` invocation there and
//!   nowhere else, and counts as the sandbox's activity;
//! - production's runs the same way, labelled `schedule_now`;
//! - a function with no schedule is `409 not_scheduled`, production
//!   included; a body is `400 input_refused`; `as` takes `schedule` alone.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use oxy_app::server::api::custom_apps_sandboxes::activity;
use serde_json::{Value, json};

use super::{platform, post_admin, send, spawn_driver, wait_for_run};
use crate::custom_app_functions_fixture::{invocations, seeded_tenant};
use crate::env_schedules::{rows, scheduled_app};

#[tokio::test]
async fn run_now_as_schedule_runs_in_the_named_environment_only() {
    let t = seeded_tenant().await;
    let app = scheduled_app(&t, "fn-sched-now").await;
    let (platform, _platform_dir) = platform().await;
    let driver = spawn_driver(t.db.clone(), platform);

    let (status, queued) = post_admin(&format!(
        "/apps/{}/functions/tick/runs?environment=dev-a1&as=schedule",
        app.id
    ))
    .await;
    assert_eq!(status, StatusCode::OK, "{queued}");
    assert_eq!(queued["environment"], "dev-a1");
    let sandbox_run = queued["run_id"].as_str().expect("a run id").to_string();
    let (status, queued) =
        post_admin(&format!("/apps/{}/functions/tick/runs?as=schedule", app.id)).await;
    assert_eq!(status, StatusCode::OK, "{queued}");
    assert_eq!(queued["environment"], "production");
    let production_run = queued["run_id"].as_str().expect("a run id").to_string();

    let sandbox = wait_for_run(app.id, &sandbox_run).await;
    let production = wait_for_run(app.id, &production_run).await;
    driver.abort();

    for (run, environment) in [(&sandbox, "dev-a1"), (&production, "production")] {
        assert_eq!(run["status"], "done", "{run}");
        assert_eq!(run["trigger"], "schedule_now");
        assert_eq!(run["environment"], environment);
        let answer: Value =
            serde_json::from_str(run["answer"].as_str().expect("an answer")).expect("JSON");
        assert_eq!(
            answer["channel"], environment,
            "that environment's build ran"
        );
    }
    let rows_seen = invocations(&t.db, app.id, "tick").await;
    let mut seen: Vec<(&str, &str)> = rows_seen
        .iter()
        .map(|r| (r.environment.as_str(), r.mode.as_str()))
        .collect();
    seen.sort();
    assert_eq!(seen, vec![("dev-a1", "manual"), ("production", "manual")]);
    assert!(
        activity::last_invocation(&t.db, app.id, "dev-a1")
            .await
            .expect("read")
            .is_some(),
        "Run now is a person's: it counts as the sandbox's activity"
    );
    assert_eq!(
        rows(&t.db, app.id).await,
        vec![format!("{}/tick", app.id)],
        "Run now neither needs nor turns on the sandbox's switch"
    );
}

#[tokio::test]
async fn run_now_as_schedule_refuses_what_a_schedule_would_not_run() {
    let t = seeded_tenant().await;
    let app = scheduled_app(&t, "fn-sched-now-no").await;
    let runs = |query: &str| format!("/apps/{}/functions/{query}", app.id);

    for environment in ["", "&environment=staging", "&environment=dev-a1"] {
        let (status, body) =
            post_admin(&runs(&format!("plain/runs?as=schedule{environment}"))).await;
        assert_eq!(status, StatusCode::CONFLICT, "{environment}: {body}");
        assert_eq!(body["error"], "not_scheduled");
    }
    let (status, body) = post_admin(&runs("tick/runs?as=cron")).await;
    assert_eq!(
        (status, body["error"].clone()),
        (StatusCode::BAD_REQUEST, json!("invalid_as"))
    );

    let with_body = Request::post(format!(
        "/api/admin{}",
        runs("tick/runs?environment=dev-a1&as=schedule")
    ))
    .header("content-type", "application/json")
    .body(Body::from("{}"))
    .unwrap();
    let (status, body) = send(with_body).await;
    assert_eq!(
        (status, body["error"].clone()),
        (StatusCode::BAD_REQUEST, json!("input_refused"))
    );
    assert!(invocations(&t.db, app.id, "plain").await.is_empty());
}
