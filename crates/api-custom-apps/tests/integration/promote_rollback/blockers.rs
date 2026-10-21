//! What blocks a production move: an unset `required` key (a promote only),
//! and retiring a registered schedule nobody named — and the schedules a move
//! leaves registered.

use axum::http::StatusCode;
use oxy_api_custom_apps::custom_apps_publish::publish;
use serde_json::{Value, json};

use super::{draft, fixture_builds, person, pointers, promote, rollback};
use crate::common::demo_workspace_id;
use crate::sandbox_agent_token::fixture::{Agent, agent};
use crate::sandbox_publish::{bundle, input, noop};

/// The app's production function schedules, as `fn:<app>/<function>` names.
async fn registered(agent: &Agent) -> Vec<String> {
    let mut names: Vec<String> =
        agentic_pipeline::scheduler::list_schedules(&agent.t.db, demo_workspace_id())
            .await
            .expect("read the schedules")
            .into_iter()
            .filter(|s| s.target_ref.starts_with(&format!("{}/", agent.app.id)))
            .map(|s| s.target_ref)
            .collect();
    names.sort();
    names
}

fn codes(body: &Value) -> Vec<String> {
    body["blockers"]
        .as_array()
        .map(|b| {
            b.iter()
                .filter_map(|b| b["code"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// A build declaring a `required` key production holds no value for: the plan
/// lists it and blocks, the act is refused with the plan's blockers, and once
/// the key is set in production the promote goes through.
#[tokio::test]
async fn an_unset_required_key_blocks_a_promote_until_it_is_set() {
    let agent = agent().await;
    let (prod, _) = fixture_builds(&agent).await;
    let slug = agent.app.slug.clone();
    let needs = json!({ "env": { "STRIPE_KEY": { "required": true } } });
    let tarball = bundle(&slug, &[noop("noop", json!({ "route": true }))], needs, &[]);
    let wanting = draft(&agent, "needs-key", tarball).await;

    let (status, plan) = promote(
        &agent,
        "production",
        json!({ "from": "staging", "dry_run": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{plan}");
    assert_eq!(
        plan["required_secrets_unset"],
        json!(["STRIPE_KEY"]),
        "{plan}"
    );
    assert_eq!(codes(&plan), vec!["required_secrets_unset"], "{plan}");

    let (status, refused) = promote(&agent, "production", json!({ "from": "staging" })).await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    assert_eq!(refused["error"], "required_secrets_unset", "{refused}");
    assert_eq!(codes(&refused), vec!["required_secrets_unset"], "{refused}");
    assert_eq!(pointers(&agent).await.0, Some(prod));

    let uri = format!("/customer-apps/{}/secrets", agent.app.id);
    let secret = json!({ "key": "STRIPE_KEY", "value": "sk_test_promote" });
    let (status, body) = person(&agent, "POST", &uri, Some(secret)).await;
    assert!(status.is_success(), "set the key: {status} {body}");
    let (status, acted) = promote(&agent, "production", json!({ "from": "staging" })).await;
    assert_eq!(status, StatusCode::OK, "{acted}");
    assert_eq!(pointers(&agent).await.0, Some(wanting));
}

/// Production runs a scheduled function. A promote to a build without it
/// would retire the schedule: blocked until the function is named; then the
/// schedule is gone. Rolling back re-registers it from the target's manifest.
#[tokio::test]
async fn retiring_a_schedule_must_be_confirmed_and_a_rollback_restores_it() {
    let agent = agent().await;
    let slug = agent.app.slug.clone();
    let nightly = noop("nightly", json!({ "schedule": "0 0 * * *" }));
    let mut scheduled = input(
        &agent.t,
        &slug,
        "with-nightly",
        bundle(&slug, &[nightly], json!({}), &[]),
    );
    scheduled.promote = true;
    publish(scheduled)
        .await
        .expect("a promoted build with a schedule");
    let nightly_ref = format!("{}/nightly", agent.app.id);
    assert_eq!(registered(&agent).await, vec![nightly_ref.clone()]);

    let plain = bundle(
        &slug,
        &[noop("noop", json!({ "route": true }))],
        json!({}),
        &[],
    );
    let unscheduled = draft(&agent, "no-nightly", plain).await;
    let (status, plan) = promote(
        &agent,
        "production",
        json!({ "from": "staging", "dry_run": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{plan}");
    assert_eq!(
        plan["schedules"]["removed"][0]["function"], "nightly",
        "{plan}"
    );
    assert_eq!(codes(&plan), vec!["schedule_removal_unconfirmed"], "{plan}");
    let (status, refused) = promote(&agent, "production", json!({ "from": "staging" })).await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    assert_eq!(
        refused["error"], "schedule_removal_unconfirmed",
        "{refused}"
    );
    assert_eq!(
        registered(&agent).await,
        vec![nightly_ref.clone()],
        "nothing retired"
    );

    let confirmed = json!({ "from": "staging", "confirm_schedule_removals": ["nightly"] });
    let (status, acted) = promote(&agent, "production", confirmed).await;
    assert_eq!(status, StatusCode::OK, "{acted}");
    assert_eq!(pointers(&agent).await.0, Some(unscheduled));
    assert!(
        registered(&agent).await.is_empty(),
        "retired with the build"
    );

    let (status, plan) = rollback(&agent, "production", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{plan}");
    assert_eq!(plan["build"]["build_id"], "with-nightly", "{plan}");
    assert_eq!(
        plan["schedules"]["added"][0]["function"], "nightly",
        "{plan}"
    );
    assert_eq!(
        registered(&agent).await,
        vec![nightly_ref],
        "the target's schedules"
    );
}
