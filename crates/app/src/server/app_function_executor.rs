//! `AppFunctionTaskExecutor`: runs a **scheduled** custom-app Oxy Function as a
//! `TaskSpec::Custom { kind: "app_function" }` on the global-run fleet. Registered
//! into the `CustomTaskRegistry` by `server::router::recovery`;
//! `PipelineTaskExecutor` delegates the `app_function` kind here. The actual run
//! is `custom_apps_functions::run_scheduled_function` (org-owner identity,
//! `mode="schedule"` invocation record). See
//! `internal-docs/2026-07-07-scheduled-oxy-functions-design.md`.

use agentic_core::delegation::{TaskAssignment, TaskOutcome, TaskSpec};
use agentic_runtime::worker::{ExecutingTask, TaskExecutor};
use async_trait::async_trait;
use sea_orm::DatabaseConnection;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// The `kind` discriminator for a scheduled custom-app function Custom task.
pub const APP_FUNCTION_KIND: &str = "app_function";

pub struct AppFunctionTaskExecutor {
    pub db: DatabaseConnection,
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

        let (event_tx, event_rx) = mpsc::channel(16);
        let (outcome_tx, outcome_rx) = mpsc::channel(4);
        let cancel = CancellationToken::new();
        // The runtime trips `cancel` (returned in ExecutingTask) to stop this
        // task; hand a clone to the run so the isolate actually terminates
        // mid-flight instead of running to its timeout.
        let cancel_child = cancel.clone();
        let db = self.db.clone();

        tokio::spawn(async move {
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
                cancel_child,
                // Stream the run's log lines onto the run's event log so a
                // scheduled/manual function's output is persisted + observable.
                Some(event_tx.clone()),
                // The worker's own composition root: hand the runtime the shared
                // data-plane query executor as a trait object, so the function
                // module never imports `projects::query`.
                std::sync::Arc::new(crate::server::api::projects::query::DataPlaneQueryExecutor)
                    as std::sync::Arc<
                        dyn crate::server::api::custom_apps_functions::runtime::FunctionQueryExecutor,
                    >,
            )
            .await
            {
                Ok(body) => TaskOutcome::Done {
                    answer: body,
                    metadata: Some(
                        serde_json::json!({ "app_id": app_id, "function_name": function_name }),
                    ),
                },
                Err(e) => TaskOutcome::Failed(e),
            };
            #[cfg(not(feature = "custom-app-functions"))]
            let outcome = {
                let _ = (&db, app_id, &function_name, &mode, input, cancel_child);
                TaskOutcome::Failed("custom-app-functions feature not enabled".to_string())
            };

            let _ = outcome_tx.send(outcome).await;
        });

        Ok(ExecutingTask {
            events: event_rx,
            outcomes: outcome_rx,
            cancel,
            answers: None,
        })
    }
}
