//! `AppFunctionTaskExecutor`: runs a **scheduled** customer-app Oxy Function as a
//! `TaskSpec::Custom { kind: "app_function" }` on the global-run fleet. Registered
//! into the `CustomTaskRegistry` by `server::router::recovery`;
//! `PipelineTaskExecutor` delegates the `app_function` kind here. The actual run
//! is `customer_apps_functions::run_scheduled_function` (org-owner identity,
//! `mode="schedule"` invocation record). See
//! `internal-docs/2026-07-07-scheduled-oxy-functions-design.md`.

use agentic_core::delegation::{TaskAssignment, TaskOutcome, TaskSpec};
use agentic_runtime::worker::{ExecutingTask, TaskExecutor};
use async_trait::async_trait;
use sea_orm::DatabaseConnection;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// The `kind` discriminator for a scheduled customer-app function Custom task.
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

            #[cfg(feature = "customer-app-functions")]
            let outcome = match crate::server::api::customer_apps_functions::run_scheduled_function(
                &db,
                app_id,
                &function_name,
                cancel_child,
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
            #[cfg(not(feature = "customer-app-functions"))]
            let outcome = {
                let _ = (&db, app_id, &function_name, cancel_child);
                TaskOutcome::Failed("customer-app-functions feature not enabled".to_string())
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
