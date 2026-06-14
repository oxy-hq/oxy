//! `/api/admin/compiles/*` — operator visibility for the compile
//! boundary (Phase 1.6a + the work this PR adds on top: TaskSpec,
//! webhook, promotion). Pattern mirrors `internal_jobs`:
//!
//!   - Flat routes nested by `router/global.rs` under
//!     `/admin/compiles` so the more permissive
//!     `oxy_owner_or_app_admin_guard` layer can wrap the whole tree
//!     without dragging the rest of `/admin/*` along.
//!   - DB access is on-demand via
//!     `oxy::database::client::establish_connection()` — no AppState
//!     threading.

mod batch;
mod crud;
mod workspaces;

use axum::Json;
use axum::Router;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, EntityTrait, Statement};
use serde::Serialize;
use uuid::Uuid;

use crate::server::router::AppState;

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(crud::list_compiles))
        .route("/workspaces", get(workspaces::list_workspaces))
        .route("/{revision_id}", get(crud::get_compile))
        .route("/run", post(crud::run_compile_now))
        .route("/backfill", post(crud::backfill_uncompiled))
        .route("/batch/run", post(batch::batch_run_compile))
        .route("/batch/promote", post(batch::batch_promote))
        .route("/{revision_id}/promote", post(crud::promote_to_revision))
}

// ---------------------------------------------------------------------------
// DB connect helper
// ---------------------------------------------------------------------------

pub(super) async fn connect() -> Result<DatabaseConnection, Response> {
    oxy::database::client::establish_connection()
        .await
        .map_err(|e| {
            tracing::error!(?e, "admin/compiles: DB connect failed");
            error_body(
                StatusCode::SERVICE_UNAVAILABLE,
                "db_unavailable",
                Some("Database connection failed".into()),
            )
        })
}

// ---------------------------------------------------------------------------
// Shared enqueue helper
// ---------------------------------------------------------------------------

/// Materialise the `agentic_runs` row, then enqueue the Compile task.
/// `agentic_task_queue.run_id` FKs to `agentic_runs.id`, so the run row MUST
/// exist before the task insert or it fails with `agentic_task_queue_run_id_fkey`
/// (mirrors the IDE/webhook path in `api::compile`). Returns the shared task/run id.
pub(super) async fn insert_run_and_enqueue_compile(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    git_sha: Option<String>,
    branch: Option<String>,
    promote: bool,
) -> Result<String, sea_orm::DbErr> {
    let task_id = Uuid::new_v4().to_string();
    agentic_runtime::crud::insert_run(
        db,
        &task_id,
        &format!("compile main ({})", git_sha.as_deref().unwrap_or("local")),
        None,
        "compile",
        Some(serde_json::json!({
            "workspace_id": workspace_id,
            "git_sha": git_sha,
            "branch": branch,
        })),
        workspace_id,
    )
    .await?;
    let spec = agentic_core::delegation::TaskSpec::Compile {
        workspace_id,
        git_sha,
        branch,
        promote,
        kind: Some("main".to_string()),
        owner_user_id: None,
    };
    agentic_runtime::crud::enqueue_task(
        db,
        &task_id,
        &task_id,
        None,
        &spec,
        None,
        agentic_runtime::orchestrator::crud::queue::TaskScope::Global,
    )
    .await?;
    Ok(task_id)
}

// ---------------------------------------------------------------------------
// Promotion (shared by single + batch promote handlers)
// ---------------------------------------------------------------------------

/// Outcome of a single promote, kept separate from HTTP concerns so the
/// batch path can collect successes/failures without short-circuiting.
pub(super) enum PromoteError {
    NotFound,
    NotPromotable(String),
    Db(sea_orm::DbErr),
}

impl PromoteError {
    /// Flat, operator-facing string used in batch result rows.
    pub(super) fn message(&self, revision_id: Uuid) -> String {
        match self {
            PromoteError::NotFound => format!("revision {revision_id} not found"),
            PromoteError::NotPromotable(m) => m.clone(),
            PromoteError::Db(e) => format!("{e}"),
        }
    }
}

/// Core rollback: repoint `workspaces.current_revision_id` at an existing
/// `ready`/`main` revision after validating it is promotable. Shared by the
/// single-revision and batch promote handlers. Returns the workspace whose
/// pointer moved.
pub(super) async fn promote_one(
    db: &DatabaseConnection,
    revision_id: Uuid,
) -> Result<Uuid, PromoteError> {
    let rev = entity::revisions::Entity::find_by_id(revision_id)
        .one(db)
        .await
        .map_err(PromoteError::Db)?;
    let Some(rev) = rev else {
        return Err(PromoteError::NotFound);
    };
    if rev.status != "ready" || rev.kind != "main" {
        return Err(PromoteError::NotPromotable(format!(
            "only a ready main revision can be promoted (got status={}, kind={})",
            rev.status, rev.kind
        )));
    }

    db.execute(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "UPDATE workspaces SET current_revision_id = $1 WHERE id = $2",
        [revision_id.into(), rev.workspace_id.into()],
    ))
    .await
    .map_err(PromoteError::Db)?;

    tracing::warn!(
        %revision_id,
        workspace_id = %rev.workspace_id,
        "admin/compiles: workspace repointed to revision (manual rollback)"
    );

    Ok(rev.workspace_id)
}

// ---------------------------------------------------------------------------
// Batch limits
// ---------------------------------------------------------------------------

/// Hard cap on ids accepted per batch request — bounds the per-request
/// wall-clock (sequential enqueues) and the resulting compile herd. Shared by
/// both batch endpoints.
pub(super) const BATCH_MAX_IDS: usize = 200;

// ---------------------------------------------------------------------------
// Error helpers
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: Option<String>,
}

pub(super) fn error_body(
    status: StatusCode,
    code: &'static str,
    message: Option<String>,
) -> Response {
    (status, Json(ErrorBody { code, message })).into_response()
}

pub(super) fn db_err(e: sea_orm::DbErr) -> Response {
    tracing::error!(?e, "admin/compiles: DB query failed");
    error_body(
        StatusCode::INTERNAL_SERVER_ERROR,
        "db_error",
        Some(format!("{e}")),
    )
}
