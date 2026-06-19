//! User-facing compile endpoints (the "Compile" button in the IDE
//! header). Mounted under the workspace router as:
//!
//!   * `POST /{workspace_id}/compile`        — enqueue a main compile
//!   * `GET  /{workspace_id}/compile/status` — poll the latest revision
//!
//! These differ from the admin `/admin/compiles` endpoints in two ways:
//!
//!   1. Auth: `WorkspaceEditor` (any workspace member above Viewer) can
//!      ship from main. Admin endpoints require `OXY_OWNER` /
//!      `OXY_APP_ADMIN` and operate cross-workspace.
//!   2. Gating: the POST refuses unless the active branch is the
//!      workspace's default branch. Compile is the shipping action;
//!      pushing a non-default branch state doesn't promote
//!      `current_revision_id` so it shouldn't be initiated from the
//!      Compile button.

use axum::Json;
use axum::extract::{Path, Query};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use oxy_git::GitClient;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::server::api::middlewares::role_guards::WorkspaceEditor;

#[derive(Deserialize)]
pub struct CompileQuery {
    /// Active branch the IDE is on. Required so the handler can verify
    /// it matches the workspace's default branch before enqueuing.
    pub branch: String,
}

#[derive(Serialize, Debug)]
pub struct EnqueueResponse {
    pub task_id: String,
    pub workspace_id: Uuid,
    pub branch: String,
}

#[derive(Serialize, Debug)]
pub struct CompileStatus {
    pub workspace_id: Uuid,
    /// The currently-promoted revision, if any. Reads serve this.
    pub current_revision_id: Option<Uuid>,
    /// The latest revision for this workspace regardless of status —
    /// gives the UI something to display while a compile is in flight.
    pub latest: Option<RevisionSummary>,
    /// Whether the IDE Compile button should be enabled. Mirrors the
    /// server-side gate so the FE doesn't need to re-implement it.
    pub can_compile: bool,
    /// HEAD commit SHA on the workspace's default branch. `None` for
    /// blank / demo / no-remote workspaces. The FE compares this
    /// against `latest.git_sha` to label the button as
    /// up-to-date / stale / never-compiled without a second request.
    pub head_sha: Option<String>,
    /// Workspace's default branch (`"main"` / `"master"` / custom).
    /// `None` matches `head_sha = None`.
    pub default_branch: Option<String>,
    /// Whether this is a multi-instance (split-fleet) deployment where manual
    /// compile is meaningful — i.e. compiling promotes the revision a separate
    /// `serve` fleet reads. `false` on a single `all` instance, which serves
    /// reads from the working copy directly, so a manual compile changes nothing
    /// it serves. The IDE **hides** the Compile button when this is `false`.
    ///
    /// Derived from the role of the node answering this (FleetOk) request: in a
    /// split fleet that's a `serve` replica (`!= all`); on a single instance
    /// it's the `all` node itself.
    pub boundary_active: bool,
}

#[derive(Serialize, Debug)]
pub struct RevisionSummary {
    pub revision_id: Uuid,
    pub status: String,
    pub kind: String,
    pub branch: Option<String>,
    /// SHA the revision was compiled against. The IDE Compile button
    /// compares this to the workspace's HEAD SHA to label itself as
    /// up-to-date or stale; the operator surface shows it for triage.
    pub git_sha: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
}

/// POST /{workspace_id}/compile?branch=<active>
pub async fn enqueue_compile(
    _: WorkspaceEditor,
    Path(workspace_id): Path<Uuid>,
    Query(q): Query<CompileQuery>,
) -> Result<Json<EnqueueResponse>, (StatusCode, String)> {
    let db = oxy::database::client::establish_connection()
        .await
        .map_err(|e| {
            tracing::error!(?e, "compile: DB connect failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "database temporarily unavailable".into(),
            )
        })?;

    let workspace = entity::workspaces::Entity::find_by_id(workspace_id)
        .one(&db)
        .await
        .map_err(internal)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("workspace {workspace_id} not found"),
            )
        })?;

    let workspace_path = workspace.path.as_deref().ok_or_else(|| {
        (
            StatusCode::CONFLICT,
            "workspace has no on-disk path — compile is not yet supported here".into(),
        )
    })?;

    // Two compile paths:
    //
    //   * Git-backed workspace (default_branch resolves to Some): require
    //     the active branch to match the default, then ship that branch's
    //     HEAD commit as the revision's `git_sha`. Distinct SHAs let the
    //     partial unique index on (workspace_id, git_sha) WHERE
    //     status='ready' AND kind='main' track every promotion.
    //
    //   * Blank / demo / no-remote workspace (default_branch returns
    //     None): there is no committed identity for the workspace. Each
    //     Compile click is a fresh snapshot of the working tree, so we
    //     mint a synthetic per-click identity (`local-<uuid>`) and skip
    //     the branch gate entirely. The compile worker treats any SHA
    //     that doesn't equal the literal "local" string as participating
    //     in idempotency — but since our synthetic SHAs are unique per
    //     click, the idempotency lookup never finds a match and each
    //     compile lands as its own row. The partial unique index also
    //     doesn't collide because the SHAs differ.
    let default_branch =
        crate::server::default_branch::resolve_default_branch(&db, workspace_id).await;

    let (target_branch, git_sha) = match default_branch.as_deref() {
        Some(default) => {
            if q.branch != default {
                return Err((
                    StatusCode::CONFLICT,
                    format!(
                        "compile is enabled only on the default branch ({default}); current branch is {}",
                        q.branch
                    ),
                ));
            }
            let git_client = oxy::github::default_git_client();
            let (sha, _subject) = git_client
                .get_branch_commit(std::path::Path::new(workspace_path), default)
                .await;
            if sha.is_empty() {
                return Err((
                    StatusCode::CONFLICT,
                    format!(
                        "could not resolve HEAD commit on {default} — commit your changes first"
                    ),
                ));
            }
            (default.to_string(), sha)
        }
        None => {
            tracing::debug!(
                workspace_id = %workspace_id,
                branch = %q.branch,
                "compile: no default branch resolved — using synthetic local SHA"
            );
            (q.branch.clone(), format!("local-{}", Uuid::new_v4()))
        }
    };

    let task_id = Uuid::new_v4().to_string();
    let spec = agentic_core::delegation::TaskSpec::Compile {
        workspace_id,
        git_sha: Some(git_sha.clone()),
        branch: Some(target_branch.clone()),
        promote: true,
        kind: Some("main".to_string()),
        owner_user_id: None,
    };

    // `agentic_task_queue.run_id` has a FK to `agentic_runs.id`; the
    // task insert below fails with `agentic_task_queue_run_id_fkey`
    // unless we materialise the run row first. Task and run share the
    // same UUID for compile tasks (1:1, no fan-out).
    agentic_runtime::crud::insert_run(
        &db,
        &task_id,
        &format!("compile main @ {git_sha}"),
        None,
        "compile",
        Some(serde_json::json!({
            "workspace_id": workspace_id,
            "git_sha": git_sha,
            "branch": target_branch,
        })),
        workspace_id,
    )
    .await
    .map_err(|e| {
        tracing::error!(?e, "compile: insert_run failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed to register compile run".into(),
        )
    })?;

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
        tracing::error!(?e, "compile: enqueue failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed to enqueue compile task".into(),
        )
    })?;

    tracing::info!(
        workspace_id = %workspace_id,
        %task_id,
        branch = %target_branch,
        git_sha = %git_sha,
        "compile: enqueued via IDE button"
    );

    let _ = workspace;
    Ok(Json(EnqueueResponse {
        task_id,
        workspace_id,
        branch: target_branch,
    }))
}

/// GET /{workspace_id}/compile/status?branch=<active>
pub async fn compile_status(
    Path(workspace_id): Path<Uuid>,
    Query(q): Query<CompileQuery>,
) -> Result<Json<CompileStatus>, (StatusCode, String)> {
    let db = oxy::database::client::establish_connection()
        .await
        .map_err(|e| {
            tracing::error!(?e, "compile_status: DB connect failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "database temporarily unavailable".into(),
            )
        })?;

    let workspace = entity::workspaces::Entity::find_by_id(workspace_id)
        .one(&db)
        .await
        .map_err(internal)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("workspace {workspace_id} not found"),
            )
        })?;

    let latest = entity::revisions::Entity::find()
        .filter(entity::revisions::Column::WorkspaceId.eq(workspace_id))
        .order_by_desc(entity::revisions::Column::StartedAt)
        .one(&db)
        .await
        .map_err(internal)?
        .map(summarise);

    // Allow Compile when the workspace has no resolvable default
    // branch (blank / demo / no-remote): the working tree is the
    // identity and there's no branch state to gate against. When a
    // default branch IS known, restrict Compile to it — non-default
    // branches read from FS and don't need to update Postgres.
    let default_branch =
        crate::server::default_branch::resolve_default_branch(&db, workspace_id).await;
    let can_compile = match default_branch.as_deref() {
        Some(default) => q.branch == default,
        None => true,
    };

    // Resolve current HEAD SHA on the default branch (when known) so
    // the FE can compare against `latest.git_sha` to label the button
    // up-to-date / stale without a follow-up request. Cheap — same
    // `get_branch_commit` call used at enqueue time, and we already
    // have the workspace path.
    let head_sha = match (&default_branch, workspace.path.as_deref()) {
        (Some(default), Some(path)) => {
            let (sha, _subject) = oxy::github::default_git_client()
                .get_branch_commit(std::path::Path::new(path), default)
                .await;
            if sha.is_empty() { None } else { Some(sha) }
        }
        _ => None,
    };

    Ok(Json(CompileStatus {
        workspace_id,
        current_revision_id: workspace.current_revision_id,
        latest,
        can_compile,
        head_sha,
        default_branch,
        boundary_active: crate::server::role_manifest::current_process_role()
            != crate::server::role_manifest::Role::All,
    }))
}

fn summarise(r: entity::revisions::Model) -> RevisionSummary {
    let started_at = r.started_at.with_timezone(&Utc);
    let finished_at = r.finished_at.map(|f| f.with_timezone(&Utc));
    let duration_ms = finished_at
        .map(|f| (f - started_at).num_milliseconds())
        .map(|d| d.max(0));
    RevisionSummary {
        revision_id: r.revision_id,
        status: r.status,
        kind: r.kind,
        branch: r.branch,
        git_sha: r.git_sha,
        started_at,
        finished_at,
        duration_ms,
    }
}

fn internal<E: std::fmt::Debug>(err: E) -> (StatusCode, String) {
    tracing::error!(?err, "compile endpoint internal error");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal server error".into(),
    )
}
