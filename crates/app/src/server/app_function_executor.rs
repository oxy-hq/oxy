//! `AppFunctionTaskExecutor`: runs a **scheduled** custom-app Oxy Function as a
//! `TaskSpec::Custom { kind: "app_function" }` on the global-run fleet. Registered
//! into the `CustomTaskRegistry` by `server::router::recovery`;
//! `PipelineTaskExecutor` delegates the `app_function` kind here. The actual run
//! is `custom_apps_functions::run_scheduled_function` (org-owner identity,
//! `mode="schedule"` invocation record). See
//! `internal-docs/customer-apps-functions.md`.

use agentic_core::delegation::{TaskAssignment, TaskOutcome, TaskSpec};
use agentic_runtime::worker::{ExecutingTask, TaskExecutor};
use async_trait::async_trait;
use sea_orm::DatabaseConnection;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

/// The `kind` discriminator for a scheduled custom-app function Custom task.
pub const APP_FUNCTION_KIND: &str = "app_function";

pub struct AppFunctionTaskExecutor {
    pub db: DatabaseConnection,
    /// The node's Layer-1 preagg cache, injected by `build_custom_task_registry`
    /// so a scheduled function's `ctx.semantic` resolves rollups the same way an
    /// HTTP-invoked one does. Default (no cache) compiles to warehouse SQL.
    pub preagg: crate::server::api::middlewares::workspace_context::PreaggCacheCtx,
}

#[async_trait]
impl TaskExecutor for AppFunctionTaskExecutor {
    async fn execute(&self, assignment: TaskAssignment) -> Result<ExecutingTask, String> {
        let TaskSpec::Custom { kind, payload } = &assignment.spec else {
            return Err(format!(
                "unexpected spec for AppFunctionTaskExecutor: {:?}",
                assignment.spec
            ));
        };
        if kind != APP_FUNCTION_KIND {
            return Err(format!("unknown app_function kind: {kind}"));
        }
        let app_id: uuid::Uuid = payload
            .get("app_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "app_function payload missing string app_id".to_string())?
            .parse()
            .map_err(|e| format!("bad app_id: {e}"))?;
        let function_name = payload
            .get("function_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "app_function payload missing string function_name".to_string())?
            .to_string();
        // The trigger stamped on the task by the seeding path drives the
        // invocation `mode`, so the invocation history agrees with the run's
        // `metadata.trigger`: a run-now / API trigger records `mode="manual"`, a
        // cron fire records `mode="schedule"`. A legacy payload without the field
        // (pre-this-change queued tasks) defaults to `schedule`.
        let mode = match payload.get("trigger").and_then(|v| v.as_str()) {
            Some("manual") => "manual".to_string(),
            _ => "schedule".to_string(),
        };
        // Optional input params (JSON) supplied at trigger time — serialized back
        // to the request-body bytes the isolate receives as `req`. Absent (cron
        // fire) → empty body.
        let input: Vec<u8> = match payload.get("input").filter(|v| !v.is_null()) {
            // Propagate a serialize failure instead of silently defaulting to an
            // empty body: running the isolate with `req.body = ""` would execute
            // the function against no input and mask the malformed trigger.
            Some(v) => serde_json::to_vec(v)
                .map_err(|e| format!("failed to serialize app_function input: {e}"))?,
            None => Vec::new(),
        };

        // The worker's run is a root trace of its own; this span is that root,
        // linked — not parented — to the request that enqueued the task when
        // the payload carries its `traceparent`. HyperDX shows the link both
        // ways; the request's latency stays its own.
        let job_span = tracing::info_span!(
            "custom_app_function.job",
            otel.name = %format!("job fn {function_name}"),
            app_id = %app_id,
            function = %function_name,
            mode = %mode,
            faas.trigger = if mode == "schedule" { "timer" } else { "other" },
        );
        if let Some(traceparent) = payload.get("traceparent").and_then(|v| v.as_str())
            && !oxy_telemetry::propagation::link_from_traceparent(&job_span, traceparent)
        {
            tracing::debug!(
                traceparent,
                "app_function: unusable traceparent on the task payload"
            );
        }

        let (event_tx, event_rx) = mpsc::channel(16);
        let (outcome_tx, outcome_rx) = mpsc::channel(4);
        let cancel = CancellationToken::new();
        // The runtime trips `cancel` (returned in ExecutingTask) to stop this
        // task; hand a clone to the run so the isolate actually terminates
        // mid-flight instead of running to its timeout.
        let job = JobArgs {
            db: self.db.clone(),
            preagg: self.preagg.clone(),
            app_id,
            function_name,
            mode,
            input,
            cancel: cancel.clone(),
            event_tx,
            outcome_tx,
        };
        tokio::spawn(run_job(job).instrument(job_span));

        Ok(ExecutingTask {
            events: event_rx,
            outcomes: outcome_rx,
            cancel,
            answers: None,
        })
    }
}

/// Everything one job carries onto its spawned task.
struct JobArgs {
    db: DatabaseConnection,
    preagg: crate::server::api::middlewares::workspace_context::PreaggCacheCtx,
    app_id: uuid::Uuid,
    function_name: String,
    mode: String,
    input: Vec<u8>,
    cancel: CancellationToken,
    event_tx: mpsc::Sender<(String, serde_json::Value)>,
    outcome_tx: mpsc::Sender<TaskOutcome>,
}

/// The job body: announce the start on the run's event log, run the function
/// under the org owner's identity, report the outcome. Runs under the
/// `custom_app_function.job` span `execute` builds.
async fn run_job(args: JobArgs) {
    let JobArgs {
        db,
        preagg,
        app_id,
        function_name,
        mode,
        input,
        cancel,
        event_tx,
        outcome_tx,
    } = args;
    let _ = event_tx
        .send((
            "app_function_started".into(),
            serde_json::json!({ "app_id": app_id, "function_name": function_name }),
        ))
        .await;

    #[cfg(feature = "custom-app-functions")]
    let outcome = match crate::server::api::custom_apps_functions::run_scheduled_function(
        &db,
        app_id,
        &function_name,
        &mode,
        input,
        cancel,
        // Stream the run's log lines onto the run's event log so a
        // scheduled/manual function's output is persisted + observable.
        Some(event_tx.clone()),
        // The worker's own composition root: hand the runtime the shared
        // data-plane query executor as a trait object, so the function
        // module never imports `projects::query`.
        std::sync::Arc::new(crate::server::api::projects::query::DataPlaneQueryExecutor)
            as std::sync::Arc<
                dyn crate::server::api::custom_apps_functions::seam::FunctionQueryExecutor,
            >,
        preagg,
    )
    .await
    {
        Ok(body) => TaskOutcome::Done {
            answer: body,
            metadata: Some(serde_json::json!({ "app_id": app_id, "function_name": function_name })),
        },
        Err(e) => TaskOutcome::Failed(e),
    };
    #[cfg(not(feature = "custom-app-functions"))]
    let outcome = {
        let _ = (&db, app_id, &function_name, &mode, input, cancel, &preagg);
        TaskOutcome::Failed("custom-app-functions feature not enabled".to_string())
    };

    let _ = outcome_tx.send(outcome).await;
}
