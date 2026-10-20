//! The batch endpoints write the same audit rows as the single ones — one per
//! app, for the same actor — in the transaction that moves the app, so an app
//! whose move is refused has none.
//!
//! Through the real router in a staff person's session. The tenant has two
//! apps, each published to production (`agent-prod`) and to staging
//! (`agent-stg`, newer, staging served it).

use axum::http::StatusCode;
use entity::{apps, audit_events};
use oxy_app_core::custom_app_environment::AppEnvironment;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{audit_rows, fixture_builds, person, pointers, sandbox_build};
use crate::sandbox_agent_token::fixture::{Agent, agent, published, tarball};

/// A second app in the agent's org, published like the first.
async fn second_app(agent: &Agent) -> apps::Model {
    published(&agent.t, "batch-audit-second").await
}

/// `POST /customer-apps/batch/{action}` for `ids`, as the person.
async fn batch(agent: &Agent, action: &str, ids: &[Uuid]) -> Value {
    let uri = format!("/customer-apps/batch/{action}");
    let (status, body) = person(agent, "POST", &uri, Some(json!({ "ids": ids }))).await;
    assert_eq!(status, StatusCode::OK, "{action}: {body}");
    body
}

/// What a row says apart from when it was written and the request it came
/// from: the same for a single handler's row and a batch handler's.
fn content(row: &audit_events::Model) -> Value {
    json!({
        "action": row.action,
        "actor": [row.actor_user_id, row.actor_email, row.actor_type],
        "org": row.org_id,
        "workspace": row.workspace_id,
        "target_type": row.target_type,
        "environment": row.environment,
        "outcome": row.outcome,
        "metadata_keys": row.metadata.as_object().map(|m| {
            let mut keys: Vec<_> = m.keys().cloned().collect();
            keys.sort();
            keys
        }),
    })
}

/// The one row of `action` that targets `target_id`.
fn row_for<'a>(
    rows: &'a [audit_events::Model],
    target_id: &str,
) -> Option<&'a audit_events::Model> {
    let mut found = rows
        .iter()
        .filter(|row| row.target_id.as_deref() == Some(target_id));
    let row = found.next();
    assert!(found.next().is_none(), "one row per app: {rows:?}");
    row
}

#[tokio::test]
async fn batch_publish_writes_an_app_published_row_per_app_like_the_single_publish() {
    let agent = agent().await;
    let second = second_app(&agent).await;
    let (first_id, second_id) = (agent.app.id, second.id);

    let body = batch(&agent, "publish", &[first_id, second_id]).await;
    assert_eq!(body["succeeded"], 2, "{body}");

    let rows = audit_rows(&agent, "app.published").await;
    assert_eq!(rows.len(), 2, "one row per app: {rows:?}");
    for (id, name) in [(first_id, &agent.app.name), (second_id, &second.name)] {
        let row = row_for(&rows, &id.to_string()).expect("a row for the app");
        assert_eq!(row.actor_user_id, Some(agent.t.guest_id));
        assert_eq!(row.actor_type, "user");
        assert_eq!(row.org_id, Some(agent.app.org_id));
        assert_eq!(row.target_type.as_deref(), Some("app"));
        assert_eq!(row.target_label.as_ref(), Some(name));
    }

    // The single handler, on the same app, writes the same row.
    let uri = format!("/customer-apps/{first_id}/publish");
    let (status, answered) = person(&agent, "POST", &uri, None).await;
    assert_eq!(status, StatusCode::OK, "{answered}");
    let rows = audit_rows(&agent, "app.published").await;
    assert_eq!(rows.len(), 3, "{rows:?}");
    let batched = rows
        .iter()
        .find(|r| r.target_id.as_deref() == Some(&first_id.to_string()))
        .expect("the batch's row");
    let single = rows.last().expect("the single's row");
    assert_eq!(content(batched), content(single));
}

#[tokio::test]
async fn batch_promote_latest_writes_an_app_promoted_row_per_app_naming_the_build() {
    let agent = agent().await;
    let second = second_app(&agent).await;
    let (prod, stg) = fixture_builds(&agent).await;

    let body = batch(&agent, "promote-latest", &[agent.app.id, second.id]).await;
    assert_eq!(body["succeeded"], 2, "{body}");
    assert_eq!(pointers(&agent).await.0, Some(stg), "production moved");

    let rows = audit_rows(&agent, "app.promoted").await;
    assert_eq!(rows.len(), 2, "one row per app: {rows:?}");
    let row = row_for(&rows, &format!("{}/production", agent.app.id)).expect("the first app's row");
    assert_eq!(row.environment, "production");
    assert_eq!(row.actor_user_id, Some(agent.t.guest_id));
    assert_eq!(row.actor_type, "user");
    assert_eq!(row.org_id, Some(agent.app.org_id));
    assert_eq!(row.workspace_id, Some(agent.app.project_id));
    assert_eq!(row.target_type.as_deref(), Some("custom_app_environment"));
    assert_eq!(
        row.target_label,
        Some(format!("{}/production", agent.app.slug))
    );
    assert_eq!(row.metadata["build"], json!(stg));
    assert_eq!(row.metadata["previous"], json!(prod));
    assert!(row.metadata["from"].is_null(), "{}", row.metadata);
    assert!(row_for(&rows, &format!("{}/production", second.id)).is_some());

    // `oxyc promote` of a third app writes the same row, naming its source.
    let third = published(&agent.t, "batch-audit-third").await;
    let uri = format!(
        "/customer-apps/{}/environments/production/promote",
        third.id
    );
    let (status, plan) = person(&agent, "POST", &uri, Some(json!({ "from": "staging" }))).await;
    assert_eq!(status, StatusCode::OK, "{plan}");
    let rows = audit_rows(&agent, "app.promoted").await;
    let single = row_for(&rows, &format!("{}/production", third.id)).expect("the single's row");
    let batched = row_for(&rows, &format!("{}/production", agent.app.id)).expect("the batch's row");
    assert_eq!(content(batched), content(single));
    assert_eq!(single.metadata["from"], "staging");
}

#[tokio::test]
async fn batch_unpublish_writes_an_app_unpublished_row_per_app_like_the_single_unpublish() {
    let agent = agent().await;
    let second = second_app(&agent).await;
    let third = published(&agent.t, "batch-audit-third").await;

    let body = batch(&agent, "unpublish", &[agent.app.id, second.id]).await;
    assert_eq!(body["succeeded"], 2, "{body}");
    assert_eq!(pointers(&agent).await.0, None, "production emptied");

    let rows = audit_rows(&agent, "app.unpublished").await;
    assert_eq!(rows.len(), 2, "one row per app: {rows:?}");
    for (id, name) in [(agent.app.id, &agent.app.name), (second.id, &second.name)] {
        let row = row_for(&rows, &id.to_string()).expect("a row for the app");
        assert_eq!(row.actor_user_id, Some(agent.t.guest_id));
        assert_eq!(row.actor_type, "user");
        assert_eq!(row.org_id, Some(agent.app.org_id));
        assert_eq!(row.target_type.as_deref(), Some("app"));
        assert_eq!(row.target_label.as_ref(), Some(name));
    }

    let uri = format!("/customer-apps/{}/publish", third.id);
    let (status, _) = person(&agent, "DELETE", &uri, None).await;
    assert_eq!(status, StatusCode::OK);
    let rows = audit_rows(&agent, "app.unpublished").await;
    let single = row_for(&rows, &third.id.to_string()).expect("the single's row");
    let batched = row_for(&rows, &agent.app.id.to_string()).expect("the batch's row");
    assert_eq!(content(batched), content(single));
}

/// An app whose move is refused — its draft or its newest build is one staging
/// never served — is reported as failed and has no row; the app beside it,
/// moved in the same request, has its own.
#[tokio::test]
async fn a_refused_move_in_a_batch_writes_no_row() {
    let agent = agent().await;
    let second = second_app(&agent).await;
    let slug = agent.app.slug.clone();
    let unstaged = sandbox_build(&agent, "q1", "sbx-q1", tarball(&slug, "unstaged"), true).await;

    // Make Live: the first app's draft is a pointer no publish wrote.
    crate::app_environments::set_pointer(
        &agent.t.db,
        agent.app.id,
        &AppEnvironment::Staging,
        Some(unstaged),
    )
    .await;
    let body = batch(&agent, "publish", &[agent.app.id, second.id]).await;
    assert_eq!(body["results"][0]["code"], "staging_never_served", "{body}");
    let rows = audit_rows(&agent, "app.published").await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].target_id, Some(second.id.to_string()));

    // Promote latest: the first app's newest build is the sandbox's.
    let body = batch(&agent, "promote-latest", &[agent.app.id, second.id]).await;
    assert_eq!(body["results"][0]["code"], "staging_never_served", "{body}");
    let rows = audit_rows(&agent, "app.promoted").await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].target_id, Some(format!("{}/production", second.id)));

    // Unpublish: an id that names no app.
    let missing = Uuid::new_v4();
    let body = batch(&agent, "unpublish", &[missing, second.id]).await;
    assert_eq!(body["failed"], 1, "{body}");
    let rows = audit_rows(&agent, "app.unpublished").await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].target_id, Some(second.id.to_string()));
}
