//! A build a sandbox agent token published is never made live as it is, nor
//! made staging's by a promote or a rollback: every new path answers `409
//! draft_published_by_agent`, a dry run included. And the token itself is
//! refused both routes.

use axum::http::StatusCode;
use serde_json::{Value, json};

use super::{draft, pointers, promote, rollback};
use crate::sandbox_agent_token::fixture::{Agent, agent, tarball};
use crate::sandbox_agent_token::staging_draft::staging_agent;

const REFUSED: &str = "draft_published_by_agent";

fn assert_refused(status: StatusCode, body: &Value, why: &str) {
    assert_eq!(status, StatusCode::CONFLICT, "{why}: {body}");
    assert_eq!(body["code"], REFUSED, "{why}: {body}");
}

/// The token publishes `build_id` with `fields` (`environment` among them).
async fn token_publish(agent: &Agent, build_id: &str, environment: &str) {
    let fields = [("build_id", build_id), ("environment", environment)];
    let (status, body) = agent.publish("by-token", &fields).await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

/// The token's draft is staging's build: promoting staging to production is
/// refused, dry run or act. A person's draft then moves staging on, and
/// rolling staging back to the token's draft is refused too.
#[tokio::test]
async fn a_tokens_draft_is_never_promoted_or_rolled_back_to() {
    let agent = staging_agent().await;
    let before = pointers(&agent).await;
    token_publish(&agent, "agent-draft", "staging").await;

    for dry_run in [true, false] {
        let body = json!({ "from": "staging", "dry_run": dry_run });
        let (status, refused) = promote(&agent, "production", body).await;
        assert_refused(status, &refused, "staging → production");
    }
    assert_eq!(pointers(&agent).await.0, before.0, "production never moved");

    let slug = agent.app.slug.clone();
    draft(&agent, "persons-draft", tarball(&slug, "person")).await;
    let (status, refused) = rollback(&agent, "staging", json!({ "to": "agent-draft" })).await;
    assert_refused(status, &refused, "a staging rollback");
}

/// A token's sandbox build cannot become staging's by a person's promote.
#[tokio::test]
async fn a_tokens_sandbox_build_is_never_promoted_to_staging() {
    let agent = agent().await;
    let (status, created) = agent.create("dev-tok").await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    token_publish(&agent, "sbx-by-token", "dev-tok").await;
    let before = pointers(&agent).await;

    let (status, refused) = promote(&agent, "staging", json!({ "from": "dev-tok" })).await;
    assert_refused(status, &refused, "dev-tok → staging");
    assert_eq!(pointers(&agent).await, before, "nothing moved");
}

/// The token is refused both routes outright: by the route allow-list in
/// front of them (`404`), whatever it names.
#[tokio::test]
async fn the_token_is_refused_both_routes() {
    let agent = staging_agent().await;
    let app = agent.app.id;
    for (uri, body) in [
        (
            format!("/customer-apps/{app}/environments/production/promote"),
            json!({ "from": "staging" }),
        ),
        (
            format!("/customer-apps/{app}/environments/staging/rollback"),
            json!({}),
        ),
    ] {
        let (status, refused) = agent.api("POST", &uri, Some(body)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}: {refused}");
    }
}
