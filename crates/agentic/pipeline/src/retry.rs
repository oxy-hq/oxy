//! Retry path — clone a terminal-failed run into a fresh run that flows
//! through the queue normally.
//!
//! Semantics: clone-and-reseed. The original run row stays as-is; a brand
//! new run is seeded with the same target (workflow_ref or pipeline_ref),
//! the same variables, the same `schedule_id` (so per-job history threads
//! both runs together), and is tagged `trigger="retry"` with
//! `metadata.retry_of=<original>` so the dashboard can link them.
//!
//! Only `workflow` and `airway` source types are retryable in v1 —
//! analytics and builder runs need per-domain reconstruction that doesn't
//! exist yet (the original interactive context is lost the moment the
//! original session ends).

use sea_orm::{DatabaseConnection, DbErr};

use agentic_runtime::crud::{TaskScope, get_run};
use agentic_runtime::entity::run;

use crate::airway_run::{StartAirwayRequest, start_airway_run};
use crate::workflow_run::{StartWorkflowRequest, start_workflow_run};

#[derive(Debug)]
pub enum RetryError {
    /// The originating run id doesn't resolve.
    NotFound,
    /// The original run isn't in a state we can retry from, or its source
    /// type lacks a reconstruction path.
    NotRetryable(String),
    /// Reconstructing succeeded but seeding the new run failed.
    SeedFailed(String),
    Db(DbErr),
}

impl std::fmt::Display for RetryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RetryError::NotFound => write!(f, "run not found"),
            RetryError::NotRetryable(m) => write!(f, "{m}"),
            RetryError::SeedFailed(m) => write!(f, "seed failed: {m}"),
            RetryError::Db(e) => write!(f, "db error: {e}"),
        }
    }
}

impl From<DbErr> for RetryError {
    fn from(e: DbErr) -> Self {
        RetryError::Db(e)
    }
}

/// Clone `run_id` into a fresh run. Returns the new `run_id`.
///
/// **Workspace-scoped**: a foreign run id is reported as `NotFound`
/// rather than `NotRetryable`, so the surface mirrors `get_schedule`'s
/// cross-workspace handling — the existence of a run in another tenant
/// isn't probeable.
///
/// `workspace` is needed for the airway path (it resolves the pipeline file
/// off the workspace filesystem); workflow retries don't actually read it
/// but the signature stays uniform with `run_schedule_now`.
pub async fn retry_run(
    db: &DatabaseConnection,
    workspace_id: uuid::Uuid,
    workspace: &dyn crate::WorkflowWorkspaceContext,
    run_id: &str,
) -> Result<String, RetryError> {
    let original = get_run(db, run_id).await?.ok_or(RetryError::NotFound)?;
    if original.workspace_id != workspace_id {
        return Err(RetryError::NotFound);
    }

    if !is_terminal_failed(&original) {
        return Err(RetryError::NotRetryable(format!(
            "run is {:?}; only failed / cancelled / timed_out runs can be retried",
            original.task_status.as_deref().unwrap_or("(none)")
        )));
    }

    let source = original.source_type.as_deref().unwrap_or("");
    match source {
        "workflow" => retry_workflow(db, &original).await,
        "airway" => retry_airway(db, workspace, &original).await,
        other => Err(RetryError::NotRetryable(format!(
            "retry not supported for source_type {other:?}"
        ))),
    }
}

fn is_terminal_failed(r: &run::Model) -> bool {
    matches!(
        r.task_status.as_deref(),
        Some("failed") | Some("cancelled") | Some("timed_out")
    )
}

async fn retry_workflow(
    db: &DatabaseConnection,
    original: &run::Model,
) -> Result<String, RetryError> {
    let metadata = original.metadata.as_ref();
    let workflow_ref = metadata
        .and_then(|m| m.get("workflow_ref"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| RetryError::NotRetryable("workflow_ref missing on original run".into()))?
        .to_string();
    let variables = metadata
        .and_then(|m| m.get("variables"))
        .filter(|v| !v.is_null())
        .cloned();

    let req = StartWorkflowRequest {
        workflow_ref,
        variables,
        retry_from_run_id: None,
        cache_enabled: false,
        invalidate_steps: None,
        invalidate_iterations: None,
        thread_id: None,
        schedule_id: original.schedule_id.clone(),
        trigger: Some("retry".to_string()),
        logical_date: None,
        retry_of: Some(original.id.clone()),
    };
    start_workflow_run(db, req, TaskScope::Global, original.workspace_id)
        .await
        .map_err(|e| RetryError::SeedFailed(e.to_string()))
}

async fn retry_airway(
    db: &DatabaseConnection,
    workspace: &dyn crate::WorkflowWorkspaceContext,
    original: &run::Model,
) -> Result<String, RetryError> {
    let metadata = original.metadata.as_ref();
    let pipeline_ref = metadata
        .and_then(|m| m.get("pipeline_ref"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| RetryError::NotRetryable("pipeline_ref missing on original run".into()))?
        .to_string();
    let variables = metadata
        .and_then(|m| m.get("variables"))
        .filter(|v| !v.is_null())
        .cloned();
    // Reproduce the original run's table scope. Absent on pre-existing runs
    // (treated as a full-spec run); empty for a run that already targeted the
    // whole spec.
    let resources = metadata
        .and_then(|m| m.get("resources"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let req = StartAirwayRequest {
        pipeline_ref,
        variables,
        thread_id: None,
        resources,
        schedule_id: original.schedule_id.clone(),
        trigger: Some("retry".to_string()),
        logical_date: None,
        retry_of: Some(original.id.clone()),
        // A retry re-runs as a normal incremental; the original backfill
        // window is not re-applied (the source's cursor was frozen during the
        // backfill, so the live position is unchanged). Re-backfill explicitly
        // via /backfill if a bounded replay is wanted again.
        backfill_from: None,
        backfill_to: None,
    };
    start_airway_run(db, workspace, req, TaskScope::Global, original.workspace_id)
        .await
        .map_err(|e| RetryError::SeedFailed(e.to_string()))
}
