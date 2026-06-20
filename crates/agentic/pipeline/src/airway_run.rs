//! Shared start helper for airway runs.
//!
//! Mirrors [`crate::workflow_run::start_workflow_run`]: seeds a fresh
//! `agentic_runs` row, populates the airway-specific extension row,
//! and enqueues a [`TaskSpec::Airway`] for the coordinator to claim.
//! HTTP, CLI, MCP, and eval all converge on this primitive.
//!
//! [`start_airway_run`] only seeds the DB; the runtime's existing
//! coordinator + worker pair drives the queued task to completion
//! through [`crate::executor::PipelineTaskExecutor::execute_airway`].

use std::sync::Arc;

use agentic_airway::AirwayPipelineSpec;
use agentic_airway::extension::run_extension;
use agentic_core::delegation::TaskSpec;
use agentic_core::transport::{CoordinatorTransport, WorkerTransport};
use agentic_runtime::coordinator::Coordinator;
use agentic_runtime::crud;
use agentic_runtime::state::RuntimeState;
use agentic_runtime::transport::DurableTransport;
use agentic_runtime::worker::Worker;
use agentic_workflow::WorkspaceContext;
use sea_orm::{DatabaseConnection, DbErr};
use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use crate::executor::PipelineTaskExecutor;
use crate::platform::PlatformContext;

/// Inputs for [`start_airway_run`]. Doubles as the HTTP request body
/// for `POST /agentic-airway/runs`.
#[derive(Debug, Clone, Deserialize)]
pub struct StartAirwayRequest {
    /// Path to a `.airway.yml`, relative to the workspace root.
    pub pipeline_ref: String,
    /// Optional variables to render into the pipeline YAML at run
    /// time. Carried on the queue spec; not yet applied (templating
    /// is a follow-up).
    #[serde(default)]
    pub variables: Option<Value>,
    /// Conversation thread to associate this run with, if any.
    #[serde(default)]
    pub thread_id: Option<Uuid>,
    /// Explicit subset of resources (tables) to run, overriding the
    /// spec's `resources`. Caller-settable (HTTP/CLI) — used by "retry
    /// failed tables" to re-run only the streams that failed. Empty =
    /// run the whole spec.
    #[serde(default)]
    pub resources: Vec<String>,
    /// Soft FK → `agentic_schedules.id`. Internal-only — only the scheduler
    /// fire path sets this; HTTP/CLI input cannot, so callers can't spoof
    /// which schedule a run "came from".
    #[serde(skip_deserializing, default)]
    pub schedule_id: Option<String>,
    /// How this run was triggered: `"scheduled"`, `"manual"`, `"backfill"`.
    /// Internal-only — stamped onto `agentic_runs.metadata.trigger`.
    #[serde(skip_deserializing, default)]
    pub trigger: Option<String>,
    /// The cron-scheduled time this run is replaying (UTC). Set by the
    /// backfill path; stamped onto `agentic_runs.metadata.logical_date`.
    #[serde(skip_deserializing, default)]
    pub logical_date: Option<chrono::DateTime<chrono::Utc>>,
    /// Run id this run is a retry of. Set by `retry_run`; stamped onto
    /// `agentic_runs.metadata.retry_of`.
    #[serde(skip_deserializing, default)]
    pub retry_of: Option<String>,
    /// Bounded-backfill window `[from, to)` (RFC3339), set by the backfill
    /// endpoint. Threaded onto the queue spec and applied to the
    /// date-windowed sources (toast, quickbooks). Internal-only — the public
    /// `/runs` body cannot set it (only `/backfill` does).
    #[serde(skip_deserializing, default)]
    pub backfill_from: Option<String>,
    #[serde(skip_deserializing, default)]
    pub backfill_to: Option<String>,
}

/// One row in the run-history list for a pipeline.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AirwayRunSummary {
    pub run_id: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub updated_at: chrono::DateTime<chrono::FixedOffset>,
}

/// Errors from seeding an airway run.
#[derive(Debug, Error)]
pub enum AirwayRunError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("airway: {0}")]
    Airway(#[from] agentic_airway::AirwayError),
    #[error("database error: {0}")]
    Db(#[from] DbErr),
    #[error("io error reading airway spec: {0}")]
    Io(String),
}

/// Discover the tables a source exposes, for the pipeline-create UI.
///
/// Connects with the live credentials carried in `config` and returns
/// the tables (with columns) so the wizard can offer a picker instead
/// of making the user hand-type table names. Stateless: nothing is
/// persisted, and no DB/platform context is required.
pub async fn discover_airway_source_tables(
    kind: String,
    config: serde_json::Value,
) -> Result<Vec<agentic_airway::DiscoveredTable>, AirwayRunError> {
    let source = agentic_airway::config::SourceConfig { kind, config };
    Ok(agentic_airway::discover_source_tables(&source).await?)
}

/// Insert an `agentic_runs` row + `airway_run_extensions` row and
/// enqueue a [`TaskSpec::Airway`] for the coordinator to drive.
///
/// Returns the fresh `run_id`. The caller is responsible for any
/// runtime-state side effects (registering cancel watches, spawning
/// SSE subscribers); this function only touches the database.
/// `scope` records who will drive the seeded run: [`TaskScope::Scoped`] when
/// a co-located coordinator is spawned right after (every HTTP/CLI caller
/// today), [`TaskScope::Global`] when the Phase 2 scheduler seeds it for the
/// standalone/recovery loop to pick up.
pub async fn start_airway_run(
    db: &DatabaseConnection,
    workspace: &dyn WorkspaceContext,
    request: StartAirwayRequest,
    scope: crud::TaskScope,
    workspace_id: Uuid,
) -> Result<String, AirwayRunError> {
    if request.pipeline_ref.trim().is_empty() {
        return Err(AirwayRunError::InvalidInput(
            "pipeline_ref must not be empty".into(),
        ));
    }

    // Resolve + render + parse the spec up front so the user gets a
    // clear error at submit time rather than from a queued worker
    // failure later. The same `variables` ride the queue spec so the
    // worker renders an identical document at run time.
    // Contain `pipeline_ref` to the workspace (untrusted HTTP input):
    // reject absolute/`..`/empty + canonical-containment. Errors quote
    // only the ref, never the resolved absolute path.
    let path = crate::pipeline_ref::resolve_pipeline_ref(
        workspace.workspace_path(),
        &request.pipeline_ref,
    )
    .map_err(AirwayRunError::InvalidInput)?;
    let yaml = tokio::fs::read_to_string(&path).await.map_err(|e| {
        AirwayRunError::Io(format!("read pipeline_ref `{}`: {e}", request.pipeline_ref))
    })?;
    let spec = AirwayPipelineSpec::from_yaml_with_vars(&yaml, request.variables.as_ref())?;

    let run_id = Uuid::new_v4().to_string();
    // Lineage labels stamped at run-start so the dashboard can label
    // the Source / Destination cards even before `pipeline_plan` fires
    // (or for legacy runs that predated it). `source_kind` is the
    // connector kind from the YAML (e.g. `"postgres_cdc"` / `"stripe"`).
    // `destination_label` reads either the referenced database name
    // (most common — users write `destination: { database: foo, ... }`)
    // or the inline connector kind (test fixtures / pre-resolved specs).
    let source_kind = spec.source.kind.clone();
    let destination_label = match &spec.destination {
        agentic_airway::config::DestinationSpec::Reference(r) => r.database.clone(),
        agentic_airway::config::DestinationSpec::Inline(c) => c.kind.clone(),
    };
    let mut metadata = serde_json::json!({
        "pipeline_ref": request.pipeline_ref,
        "pipeline_name": spec.name,
        "concurrency": spec.concurrency,
        "source_kind": source_kind,
        "destination_label": destination_label,
        // `null` when omitted; the executor passes whatever lands here
        // through to the worker on resume.
        "variables": request.variables,
        // Subset of tables this run targeted (empty = whole spec). Persisted
        // so a retry of this run reproduces the same scope instead of
        // silently widening a "retry failed tables" run back to the full
        // pipeline — see `retry_airway`.
        "resources": request.resources.clone(),
        // Backfill window for observability in the run history (null for
        // normal runs). The effective window still rides the queue spec below.
        "backfill_from": request.backfill_from.clone(),
        "backfill_to": request.backfill_to.clone(),
    });
    crate::scheduler::stamp_trigger_metadata(
        &mut metadata,
        &request.trigger,
        &request.logical_date,
        &request.retry_of,
    );

    let question = format!("airway: {}", spec.name);
    if let Some(schedule_id) = request.schedule_id.as_deref() {
        crud::insert_run_with_schedule(
            db,
            &run_id,
            &question,
            request.thread_id,
            agentic_airway::SOURCE_TYPE,
            Some(metadata),
            schedule_id,
            workspace_id,
        )
        .await?;
    } else {
        crud::insert_run(
            db,
            &run_id,
            &question,
            request.thread_id,
            agentic_airway::SOURCE_TYPE,
            Some(metadata),
            workspace_id,
        )
        .await?;
    }

    run_extension::insert_run_extension(db, &run_id, &spec, Some(&request.pipeline_ref)).await?;

    let task_spec = TaskSpec::Airway {
        pipeline_ref: request.pipeline_ref,
        variables: request.variables,
        resources: request.resources,
        backfill_from: request.backfill_from,
        backfill_to: request.backfill_to,
    };
    crud::enqueue_task(db, &run_id, &run_id, None, &task_spec, None, scope).await?;

    Ok(run_id)
}

/// Spawn the coordinator + worker pair that drives a queued airway run
/// to completion.
///
/// Pairs with [`start_airway_run`]. Airway is a single atomic task —
/// no child delegation, no decision chain — so unlike
/// `spawn_workflow_run_drive` this attaches no completion policy or
/// delegation resolver: the default coordinator terminates the run
/// when the root `TaskSpec::Airway` returns `Done` / `Failed`.
///
/// The transport is scoped to this run's id so the worker can't poach
/// a sibling run's queued root task (same race the workflow drive
/// guards against).
pub fn spawn_airway_run_drive(
    db: DatabaseConnection,
    state: Arc<RuntimeState>,
    run_id: String,
    platform: Arc<dyn PlatformContext>,
    mut cancel_rx: tokio::sync::watch::Receiver<bool>,
    router: Arc<dyn agentic_runtime::router::TaskRouter>,
) {
    let executor = Arc::new(PipelineTaskExecutor {
        platform,
        builder_bridges: None,
        schema_cache: None,
        builder_test_runner: None,
        builder_app_runner: None,
        db: db.clone(),
        state: Some(state.clone()),
    });

    let transport = DurableTransport::with_router(db.clone(), router, Some(run_id.clone()));

    let mut coordinator = Coordinator::new(
        db,
        state.clone(),
        transport.clone() as Arc<dyn CoordinatorTransport>,
    );
    // `register_root` (not `submit_root`) — the queue row already
    // exists from `start_airway_run`'s `enqueue_task`.
    coordinator.register_root(run_id.clone(), 0);

    let worker = Worker::new(transport.clone() as Arc<dyn WorkerTransport>, executor);

    // Forward `state.cancel(run_id)` into the transport so the worker's
    // in-flight `CancellationToken` fires and the queued row is marked
    // cancelled. The coordinator's `handle_cancelled` then propagates
    // to the run row.
    let transport_for_cancel = transport.clone();
    let cancel_task_id = run_id.clone();
    let cancel_forwarder = tokio::spawn(async move {
        while cancel_rx.changed().await.is_ok() {
            if *cancel_rx.borrow() {
                let _ = transport_for_cancel.cancel_subtree(&cancel_task_id).await;
                break;
            }
        }
    });

    let worker_task = tokio::spawn(async move {
        worker.run().await;
    });
    let cleanup_run_id = run_id.clone();
    let cleanup_state = state;
    tokio::spawn(async move {
        let mut coord = coordinator;
        coord.run().await;
        // Close the SSE stream cleanly once the run terminates —
        // mirrors the workflow drive's shutdown sequence.
        cleanup_state.notify(&cleanup_run_id);
        cancel_forwarder.abort();
        worker_task.abort();
        cleanup_state.deregister(&cleanup_run_id);
    });
}

/// List recent airway runs for a `pipeline_ref`, newest first, capped
/// at `limit`. Backs the run-history dropdown.
///
/// Filters `agentic_runs` on `source_type = 'airway'` and the
/// `metadata->>'pipeline_ref'` that `start_airway_run` stamps — no
/// join needed since the ref lives in the run row's metadata.
pub async fn list_airway_runs(
    db: &DatabaseConnection,
    pipeline_ref: &str,
    limit: u64,
) -> Result<Vec<AirwayRunSummary>, AirwayRunError> {
    use sea_orm::{FromQueryResult, Statement};

    #[derive(FromQueryResult)]
    struct Row {
        id: String,
        task_status: Option<String>,
        created_at: chrono::DateTime<chrono::FixedOffset>,
        updated_at: chrono::DateTime<chrono::FixedOffset>,
    }

    let stmt = Statement::from_sql_and_values(
        sea_orm::DatabaseBackend::Postgres,
        r#"
        SELECT id, task_status, created_at, updated_at
        FROM agentic_runs
        WHERE source_type = $1
          AND parent_run_id IS NULL
          AND metadata->>'pipeline_ref' = $2
        ORDER BY created_at DESC
        LIMIT $3
        "#,
        [
            agentic_airway::SOURCE_TYPE.into(),
            pipeline_ref.into(),
            (limit as i64).into(),
        ],
    );

    let rows = Row::find_by_statement(stmt).all(db).await?;
    Ok(rows
        .into_iter()
        .map(|r| AirwayRunSummary {
            run_id: r.id,
            status: r.task_status.unwrap_or_else(|| "unknown".into()),
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
        .collect())
}

// Happy-path coverage lives in the testcontainers suite alongside the
// worker's integration tests — `start_airway_run` is mostly persistence
// (one runtime row + one extension row + one queue row) and the
// validation surface is a single empty-string check.
