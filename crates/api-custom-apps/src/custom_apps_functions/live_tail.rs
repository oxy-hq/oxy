//! One run's half of the live tail (`custom_apps_tail::producer`): which
//! invocation it is, and how it ended. Kept out of `run_with_runtime` so the
//! instrumented wrapper stays a wrapper.

use std::time::Duration;

use super::{RunArgs, RunOutcome};
use crate::custom_apps_tail::producer::{Follower, Invocation, Outcome};

/// Start following the run `args` describes, in the environment it was
/// admitted to.
pub(super) async fn follow(args: &RunArgs<'_>) -> Follower {
    let invocation = Invocation {
        app_id: args.app.id,
        environment: args.policy.environment().name(),
        invocation_id: args.invocation_id,
        function: args.function_name.to_string(),
        mode: args.mode.to_string(),
        build_id: args.build_id,
        request_id: args.request_id,
    };
    Follower::start(args.db, invocation, args.logs.clone()).await
}

/// The run returned `outcome` after `ran`; `failed` is the pager's verdict.
pub(super) fn finish(follower: Follower, outcome: &RunOutcome, failed: bool, ran: Duration) {
    let (status, error, _, http_status, _) = outcome;
    follower.finish(&Outcome {
        status: (*status).to_string(),
        failed,
        http_status: *http_status,
        duration_ms: u64::try_from(ran.as_millis()).unwrap_or(u64::MAX),
        error: error.clone(),
    });
}
