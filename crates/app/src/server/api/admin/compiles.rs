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

use axum::Json;
use axum::Router;
use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use chrono::{DateTime, Utc};
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::server::router::AppState;

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_compiles))
        .route("/{revision_id}", get(get_compile))
        .route("/run", post(run_compile_now))
}

// ---------------------------------------------------------------------------
// DB connect helper
// ---------------------------------------------------------------------------

async fn connect() -> Result<DatabaseConnection, Response> {
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
// GET /admin/compiles
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    /// Cap on rows returned. Defaults to 50, clamped to 200.
    #[serde(default)]
    pub limit: Option<u32>,
    /// Filter to one workspace.
    #[serde(default)]
    pub workspace_id: Option<Uuid>,
    /// Filter by status. One of `compiling` / `ready` / `failed`.
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct CompileRow {
    pub revision_id: Uuid,
    pub workspace_id: Uuid,
    pub git_sha: String,
    pub branch: Option<String>,
    pub status: String,
    pub kind: String,
    pub owner_user_id: Option<Uuid>,
    pub compiler_version: String,
    pub schema_version: i32,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
    pub file_count_seen: i32,
    pub file_count_compiled: i32,
    pub file_count_failed: i32,
    pub is_current_for_workspace: bool,
}

#[derive(Serialize, Debug)]
pub struct ListResponse {
    pub rows: Vec<CompileRow>,
    pub total_returned: usize,
}

async fn list_compiles(
    Query(query): Query<ListQuery>,
) -> Result<Json<ListResponse>, Response> {
    let db = connect().await?;
    let limit = query.limit.unwrap_or(50).clamp(1, 200);

    let mut find = entity::revisions::Entity::find()
        .order_by_desc(entity::revisions::Column::StartedAt);
    if let Some(ws) = query.workspace_id {
        find = find.filter(entity::revisions::Column::WorkspaceId.eq(ws));
    }
    if let Some(ref status) = query.status {
        find = find.filter(entity::revisions::Column::Status.eq(status.clone()));
    }
    let rows = find
        .limit(limit as u64)
        .all(&db)
        .await
        .map_err(db_err)?;

    // For each revision_id seen, mark whether its workspace points at
    // it as current. Lookup is bounded by `limit` so the IN list stays
    // small.
    let revision_ids: Vec<Uuid> = rows.iter().map(|r| r.revision_id).collect();
    let workspace_ids: Vec<Uuid> = rows
        .iter()
        .map(|r| r.workspace_id)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut current_for: std::collections::HashSet<Uuid> = std::collections::HashSet::new();
    if !revision_ids.is_empty() || !workspace_ids.is_empty() {
        let workspaces = entity::workspaces::Entity::find()
            .filter(entity::workspaces::Column::Id.is_in(workspace_ids))
            .all(&db)
            .await
            .map_err(db_err)?;
        for ws in workspaces {
            if let Some(rid) = ws.current_revision_id {
                current_for.insert(rid);
            }
        }
    }

    let out: Vec<CompileRow> = rows
        .into_iter()
        .map(|r| compile_row_from_model(r, &current_for))
        .collect();
    let total_returned = out.len();
    Ok(Json(ListResponse {
        rows: out,
        total_returned,
    }))
}

fn compile_row_from_model(
    r: entity::revisions::Model,
    current_for: &std::collections::HashSet<Uuid>,
) -> CompileRow {
    let started_at = r.started_at.with_timezone(&Utc);
    let finished_at = r.finished_at.map(|f| f.with_timezone(&Utc));
    let duration_ms = finished_at
        .map(|f| (f - started_at).num_milliseconds())
        .map(|d| d.max(0));
    CompileRow {
        revision_id: r.revision_id,
        workspace_id: r.workspace_id,
        git_sha: r.git_sha,
        branch: r.branch,
        status: r.status,
        kind: r.kind,
        owner_user_id: r.owner_user_id,
        compiler_version: r.compiler_version,
        schema_version: r.schema_version,
        started_at,
        finished_at,
        duration_ms,
        file_count_seen: r.file_count_seen,
        file_count_compiled: r.file_count_compiled,
        file_count_failed: r.file_count_failed,
        is_current_for_workspace: current_for.contains(&r.revision_id),
    }
}

// ---------------------------------------------------------------------------
// GET /admin/compiles/{revision_id}
// ---------------------------------------------------------------------------

#[derive(Serialize, Debug)]
pub struct CompileDetail {
    #[serde(flatten)]
    pub row: CompileRow,
    /// Full `error_summary` JSONB from the revisions row. Null on
    /// success.
    pub error_summary: Option<serde_json::Value>,
}

async fn get_compile(
    Path(revision_id): Path<Uuid>,
) -> Result<Json<CompileDetail>, Response> {
    let db = connect().await?;
    let row = entity::revisions::Entity::find_by_id(revision_id)
        .one(&db)
        .await
        .map_err(db_err)?
        .ok_or_else(|| {
            error_body(
                StatusCode::NOT_FOUND,
                "not_found",
                Some(format!("revision {revision_id} not found")),
            )
        })?;

    let workspace = entity::workspaces::Entity::find_by_id(row.workspace_id)
        .one(&db)
        .await
        .map_err(db_err)?;
    let mut current_for = std::collections::HashSet::new();
    if let Some(ws) = workspace
        && let Some(rid) = ws.current_revision_id
    {
        current_for.insert(rid);
    }

    let error_summary = row.error_summary.clone();
    let cr = compile_row_from_model(row, &current_for);
    Ok(Json(CompileDetail {
        row: cr,
        error_summary,
    }))
}

// ---------------------------------------------------------------------------
// POST /admin/compiles/run
// ---------------------------------------------------------------------------

/// Operator action: enqueue a Compile TaskSpec for the given
/// workspace. Used from the admin UI's "Run compile now" button. The
/// task lands in `agentic_task_queue` and is drained by the standard
/// worker fleet.
///
/// `promote` defaults to `false` — the operator can opt into promoting
/// the resulting revision to `workspaces.current_revision_id` (cloud
/// production uses this on the webhook path).
#[derive(Deserialize, Debug)]
pub struct RunCompileRequest {
    pub workspace_id: Uuid,
    #[serde(default)]
    pub git_sha: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub promote: bool,
}

#[derive(Serialize, Debug)]
pub struct RunCompileResponse {
    pub task_id: String,
    pub workspace_id: Uuid,
    pub promote: bool,
}

async fn run_compile_now(
    Json(req): Json<RunCompileRequest>,
) -> Result<Json<RunCompileResponse>, Response> {
    let db = connect().await?;

    // Make sure the workspace exists before we enqueue — otherwise
    // the worker will fail with a confusing FK violation when it
    // tries to insert the revisions row.
    let exists = entity::workspaces::Entity::find_by_id(req.workspace_id)
        .one(&db)
        .await
        .map_err(db_err)?;
    if exists.is_none() {
        return Err(error_body(
            StatusCode::NOT_FOUND,
            "workspace_not_found",
            Some(format!("workspace {} not found", req.workspace_id)),
        ));
    }

    let task_id = Uuid::new_v4().to_string();
    let spec = agentic_core::delegation::TaskSpec::Compile {
        workspace_id: req.workspace_id,
        git_sha: req.git_sha.clone(),
        branch: req.branch.clone(),
        promote: req.promote,
        kind: Some("main".to_string()),
        owner_user_id: None,
    };

    agentic_runtime::crud::enqueue_task(
        &db,
        &task_id,
        &task_id,
        None,
        &spec,
        None,
        agentic_runtime::orchestrator::crud::queue::TaskScope::Global,
    )
    .await
    .map_err(|e| {
        tracing::error!(?e, "admin/compiles: enqueue failed");
        error_body(
            StatusCode::INTERNAL_SERVER_ERROR,
            "enqueue_failed",
            Some(format!("{e}")),
        )
    })?;

    Ok(Json(RunCompileResponse {
        task_id,
        workspace_id: req.workspace_id,
        promote: req.promote,
    }))
}

// ---------------------------------------------------------------------------
// Error helpers
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: Option<String>,
}

fn error_body(status: StatusCode, code: &'static str, message: Option<String>) -> Response {
    (status, Json(ErrorBody { code, message })).into_response()
}

fn db_err(e: sea_orm::DbErr) -> Response {
    tracing::error!(?e, "admin/compiles: DB query failed");
    error_body(
        StatusCode::INTERNAL_SERVER_ERROR,
        "db_error",
        Some(format!("{e}")),
    )
}
