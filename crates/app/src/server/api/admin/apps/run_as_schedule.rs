//! **Run now `as=schedule`** — `POST …/{id}/functions/{name}/runs?as=schedule`
//! with an optional `&environment=<name>`, on both staff surfaces
//! (`internal-docs/2026-09-10-custom-app-environments-design.md` §4.5,
//! ruling 3).
//!
//! It runs a scheduled function once, as its schedule would: no input (a body
//! is `400 input_refused`), the system identity (the org owner, `appRole:
//! admin`, as a cron fire), the function's retry policy from its manifest,
//! and the build and policy of the environment it names — whether or not that
//! environment's schedules switch is on, because a person asked. The function
//! must declare a `schedule` in that build, production included
//! (`409 not_scheduled`).
//!
//! The run's `trigger` is `schedule_now`; its invocation's `mode` is `manual`
//! (`app_function_executor::invocation_mode`), so it counts as a sandbox's
//! activity where a cron fire does not. Who may ask is the plain Run now's
//! rule (`environment_scope::admit`): a publish token in production only, and
//! only for a check; a sandbox agent token in a sandbox it created.

use axum::http::StatusCode;
use oxy_app_core::audit::RequestActor;
use oxy_auth::types::AppPublishTokenAuth;
use uuid::Uuid;

use super::dto::RunFunctionJobResponse;
use super::environment_scope::{self, RouteError, ScopeError};
use crate::server::api::custom_apps_functions::FunctionJobTrigger;
use crate::server::api::custom_apps_functions::check_run::{self, Asked};

/// The query of `POST …/functions/{name}/runs`: the environment, and how to
/// run the function there.
#[derive(Debug, Default, serde::Deserialize)]
pub struct RunJobQuery {
    #[serde(default)]
    pub environment: Option<String>,
    /// `schedule` — the one value; anything else is `400 invalid_as`.
    #[serde(default, rename = "as")]
    pub run_as: Option<String>,
}

/// The one value `?as=` takes.
pub const AS_SCHEDULE: &str = "schedule";

/// What the request asked: the app and function, the environment as named,
/// and `?as=` with the body it came with.
pub(super) struct ScheduleNowRequest<'a> {
    pub id: Uuid,
    pub name: &'a str,
    pub environment: Option<&'a str>,
    pub run_as: &'a str,
    pub body: &'a [u8],
}

/// `?as=` is `schedule`, and the request carries no input.
fn validate(run_as: &str, body: &[u8]) -> Result<(), ScopeError> {
    if run_as != AS_SCHEDULE {
        return Err(ScopeError::new(
            StatusCode::BAD_REQUEST,
            "invalid_as",
            format!("{run_as:?} is not a way to run a function: the one value is \"schedule\""),
        ));
    }
    if !body.is_empty() {
        return Err(ScopeError::new(
            StatusCode::BAD_REQUEST,
            "input_refused",
            "a run as the schedule takes no input: a cron fire has none; send no body",
        ));
    }
    Ok(())
}

/// Queue the run. The body of `handlers::run_function_job` when `?as=` is
/// present.
pub(super) async fn queue(
    actor: &RequestActor,
    marker: Option<&AppPublishTokenAuth>,
    request: ScheduleNowRequest<'_>,
) -> Result<RunFunctionJobResponse, RouteError> {
    validate(request.run_as, request.body)?;
    let db = environment_scope::connect().await?;
    if marker.is_some_and(|marker| !super::handlers::token_names_app(marker, request.id)) {
        return Err(StatusCode::FORBIDDEN.into());
    }
    let named = request.environment.unwrap_or_default();
    let (app, environment) =
        environment_scope::admit(&db, request.id, &actor.user, marker, named).await?;
    // A publish token is admitted to production alone, and runs a check there
    // and nothing else — as for the plain Run now.
    if let Some(marker) = marker {
        super::handlers::machine_may_run(&db, marker, request.id, request.name).await?;
    }
    let asked = Asked {
        trigger: FunctionJobTrigger::ScheduleNow,
        environment: &environment,
        credential_token_id: super::agent_scope::token_of(&actor.user),
    };
    let run_id = check_run::trigger(&db, app.id, request.name, None, asked)
        .await
        .map_err(ScopeError::from)?;
    let audited = super::run_audit::Asked { actor, marker };
    let run = (request.name, run_id.as_str());
    super::run_audit::run_queued(&db, audited, &app, &environment, run).await;
    Ok(RunFunctionJobResponse {
        run_id,
        environment: environment.name(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_is_schedule_or_refused() {
        assert!(validate("schedule", b"").is_ok());
        for other in ["", "Schedule", "cron", "manual"] {
            let refused = validate(other, b"").expect_err(other);
            assert_eq!(
                (refused.status, refused.code),
                (StatusCode::BAD_REQUEST, "invalid_as")
            );
        }
    }

    /// A cron fire has no input, so neither does a run as one — `{}` included.
    #[test]
    fn a_body_is_refused() {
        for body in [&b"{}"[..], b"null", b"{\"store\":7}"] {
            let refused = validate("schedule", body).expect_err("a body");
            assert_eq!(
                (refused.status, refused.code),
                (StatusCode::BAD_REQUEST, "input_refused")
            );
        }
    }

    #[test]
    fn the_query_reads_as_and_environment() {
        let q: RunJobQuery =
            serde_urlencoded::from_str("environment=dev-a1&as=schedule").expect("parses");
        assert_eq!(q.environment.as_deref(), Some("dev-a1"));
        assert_eq!(q.run_as.as_deref(), Some(AS_SCHEDULE));
        let bare: RunJobQuery = serde_urlencoded::from_str("").expect("parses");
        assert_eq!((bare.environment, bare.run_as), (None, None));
    }
}
