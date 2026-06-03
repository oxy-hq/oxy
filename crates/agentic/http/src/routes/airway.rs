//! Airway run lifecycle handlers.
//!
//! Airway is queue-driven like workflow: `POST /runs` seeds an
//! `agentic_runs` row + `airway_run_extensions` row and enqueues a
//! `TaskSpec::Airway`; the per-request coordinator + worker claim it
//! and drive it to completion. SSE just streams whatever lands in
//! `agentic_run_events` — the registry routes by `source_type =
//! "airway"`, so the shared `stream_events` handler needs no airway
//! awareness.
//!
//! ## Routes
//!
//! | Method | Path                              | Purpose |
//! |--------|-----------------------------------|---------|
//! | POST   | `/agentic-airway/runs`            | Start a run |
//! | GET    | `/agentic-airway/runs/:id/events` | SSE stream (shared handler) |
//! | POST   | `/agentic-airway/runs/:id/cancel` | Cancel a running pipeline |

use std::sync::Arc;

use axum::{
    Json,
    extract::{Extension, Path},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::Deserialize;
use serde::Serialize;
use tokio::sync::{mpsc, watch};

use agentic_pipeline::WorkflowWorkspaceContext;
use agentic_pipeline::airway_run::{
    AirwayRunError, StartAirwayRequest, list_airway_runs, spawn_airway_run_drive, start_airway_run,
};
use agentic_pipeline::platform::PlatformContext;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use uuid::Uuid;

use crate::state::AgenticState;

#[derive(Serialize)]
pub struct CreateAirwayRunResponse {
    pub run_id: String,
}

#[derive(Deserialize)]
pub struct AirwayRunIdPath {
    id: String,
}

#[derive(Deserialize)]
pub struct ListRunsQuery {
    pub pipeline_ref: String,
    /// Hard cap to keep responses bounded; the dropdown only shows the
    /// most-recent N. Defaults to 50, clamped to 200.
    #[serde(default)]
    pub limit: Option<u64>,
}

// ── GET /agentic-airway/runs?pipeline_ref=... ──────────────────────────────

pub async fn list_runs_for_pipeline(
    Extension(state): Extension<Arc<AgenticState>>,
    axum::extract::Query(q): axum::extract::Query<ListRunsQuery>,
) -> Response {
    let limit = q.limit.unwrap_or(50).min(200);
    match list_airway_runs(&state.db, &q.pipeline_ref, limit).await {
        Ok(runs) => Json(runs).into_response(),
        Err(e) => {
            tracing::error!(%e, "list_runs_for_pipeline failed");
            (StatusCode::INTERNAL_SERVER_ERROR, format!("list runs: {e}")).into_response()
        }
    }
}

// ── POST /agentic-airway/runs ──────────────────────────────────────────────

pub async fn create_airway_run(
    Extension(state): Extension<Arc<AgenticState>>,
    Extension(platform): Extension<Arc<dyn PlatformContext>>,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    Json(body): Json<StartAirwayRequest>,
) -> Response {
    // Write-side authz: a caller-supplied `thread_id` is persisted on
    // the run and surfaces in that thread's feed. Without this check
    // any authed user could attach a run to someone else's thread
    // (read-side is already guarded by `ensure_run_access`).
    if let Some(tid) = body.thread_id {
        match state.thread_owner.thread_owner(tid).await {
            Ok(None) => return (StatusCode::NOT_FOUND, "thread not found").into_response(),
            Ok(Some(Some(owner_id))) if owner_id != user.id => {
                return (StatusCode::FORBIDDEN, "access denied").into_response();
            }
            Ok(_) => {}
            Err(e) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, format!("db error: {e}"))
                    .into_response();
            }
        }
    }

    // `PlatformContext: WorkflowWorkspaceContext`, so this coercion is
    // free — `start_airway_run` only needs the workspace surface.
    let workspace: Arc<dyn WorkflowWorkspaceContext> = platform.clone();

    // Interactive run: a co-located scoped coordinator is spawned right
    // below via `spawn_airway_run_drive`, so it owns this run's tree.
    // workspace_id from the platform context so out-of-process drivers
    // can route this row back to its workspace.
    let workspace_id = platform.workspace_id();
    let run_id = match start_airway_run(
        &state.db,
        workspace.as_ref(),
        body,
        agentic_pipeline::TaskScope::Scoped,
        workspace_id,
    )
    .await
    {
        Ok(id) => id,
        Err(AirwayRunError::InvalidInput(msg)) | Err(AirwayRunError::Io(msg)) => {
            return (StatusCode::BAD_REQUEST, msg).into_response();
        }
        Err(AirwayRunError::Airway(e)) => {
            // Spec parse / validation failure — caller's input problem.
            return (StatusCode::BAD_REQUEST, format!("airway spec: {e}")).into_response();
        }
        Err(e) => {
            tracing::error!(%e, "create_airway_run: start failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("start: {e}")).into_response();
        }
    };

    // Register cancel + answer channels so the Stop button works. Airway
    // never consumes answers (no HITL), but `register` wants the pair;
    // the answer_rx is simply dropped.
    let (answer_tx, _answer_rx) = mpsc::channel::<String>(1);
    let (cancel_tx, cancel_rx) = watch::channel(false);
    state.register(&run_id, answer_tx, cancel_tx);

    // Drive the queued task — without this the `TaskSpec::Airway` row
    // sits in `agentic_task_queue` forever.
    spawn_airway_run_drive(
        state.db.clone(),
        state.runtime.clone(),
        run_id.clone(),
        platform,
        cancel_rx,
        state.router.clone(),
    );

    Json(CreateAirwayRunResponse { run_id }).into_response()
}

// ── POST /agentic-airway/runs/:id/cancel ───────────────────────────────────

pub async fn cancel_airway_run(
    Path(AirwayRunIdPath { id: run_id }): Path<AirwayRunIdPath>,
    Extension(state): Extension<Arc<AgenticState>>,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
) -> Response {
    if let Err(resp) = ensure_run_access(&state, &user.id, &run_id).await {
        return resp;
    }
    // Durable cross-process cancel signal (see cancel_workflow_run).
    agentic_runtime::crud::request_cancel(&state.db, &run_id)
        .await
        .ok();
    if !state.cancel(&run_id) {
        // No live cancel channel. Same race as the workflow cancel
        // handler: distinguish "stuck queue row" (defensive fail) from
        // "just finished cleanly" (must not rewrite a `done` run to
        // `failed`). Gate the defensive write on non-terminal status.
        let already_terminal = match agentic_runtime::crud::get_run(&state.db, &run_id).await {
            Ok(Some(run)) => matches!(
                run.task_status.as_deref(),
                Some("done") | Some("failed") | Some("cancelled") | Some("timed_out")
            ),
            Ok(None) => true,
            Err(e) => {
                tracing::warn!(%run_id, error = %e, "airway cancel: status lookup failed");
                true
            }
        };
        if !already_terminal
            && let Err(e) =
                agentic_runtime::crud::update_run_failed(&state.db, &run_id, "cancelled by user")
                    .await
        {
            tracing::warn!(%run_id, error = %e, "airway cancel: defensive DB update failed");
        }
        state.notify(&run_id);
        state.notifiers.remove(&run_id);
    }
    StatusCode::NO_CONTENT.into_response()
}

/// Verify `user_id` may operate on `run_id`. Runs not linked to a
/// thread (the common case for airway — pipelines are usually not
/// chat-scoped) are allowed through, matching the workflow handler's
/// policy. Single chokepoint if that policy changes.
async fn ensure_run_access(
    state: &AgenticState,
    user_id: &Uuid,
    run_id: &str,
) -> Result<(), Response> {
    let run = match agentic_runtime::crud::get_run(&state.db, run_id).await {
        Ok(Some(run)) => run,
        Ok(None) => return Err((StatusCode::NOT_FOUND, "run not found").into_response()),
        Err(e) => {
            return Err(
                (StatusCode::INTERNAL_SERVER_ERROR, format!("db error: {e}")).into_response(),
            );
        }
    };
    let Some(thread_uuid) = run.thread_id else {
        return Ok(());
    };
    match state.thread_owner.thread_owner(thread_uuid).await {
        Ok(None) => Err((StatusCode::NOT_FOUND, "run not found").into_response()),
        Ok(Some(Some(owner_id))) if &owner_id != user_id => {
            Err((StatusCode::FORBIDDEN, "access denied").into_response())
        }
        Ok(_) => Ok(()),
        Err(e) => {
            Err((StatusCode::INTERNAL_SERVER_ERROR, format!("db error: {e}")).into_response())
        }
    }
}

// ── GET /agentic-airway/files ─────────────────────
//
// Mirrors `/agentic-workflows/files`: lists `.airway.yml` pipeline
// files as { path, path_b64 } so the Schedules UI target picker can be
// populated for airway schedules.

#[derive(Serialize)]
pub struct AirwayFile {
    /// Workspace-relative path, usable as a schedule `target_ref`.
    pub path: String,
    /// URL-safe base64 of `path` (parity with the workflow files shape).
    pub path_b64: String,
}

pub async fn list_airway_files(
    Extension(platform): Extension<Arc<dyn PlatformContext>>,
) -> Response {
    let workspace: Arc<dyn WorkflowWorkspaceContext> = platform.clone();
    let paths = match workspace.list_airway_files().await {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("list files: {e}"),
            )
                .into_response();
        }
    };
    let root = workspace.workspace_path().to_path_buf();
    let files: Vec<AirwayFile> = paths
        .into_iter()
        .map(|abs| {
            let rel = abs
                .strip_prefix(&root)
                .unwrap_or(&abs)
                .to_string_lossy()
                .to_string();
            let path_b64 = URL_SAFE_NO_PAD.encode(rel.as_bytes());
            AirwayFile {
                path: rel,
                path_b64,
            }
        })
        .collect();
    Json(files).into_response()
}

// ── POST /agentic-airway/sources/discover ──────────────────────────────────
//
// Connect to a SQL source with the live credentials supplied at wizard
// time and return its tables (with columns) so the New Pipeline UI can
// offer a table picker instead of hand-typed table names. Stateless —
// nothing is persisted. Authed: a caller can already author pipelines
// that connect anywhere, so this grants no new privilege; the auth gate
// is kept because the handler makes an outbound connection to a
// caller-specified host.
//
// KNOWN / DEFERRED (intentional, not a review gap): this dials a
// caller-controlled host/port on the API thread and surfaces the
// connector error verbatim. It only matters on multi-tier deployments
// where the API tier's network reach differs from the worker tier.
// Hardening (reject RFC1918 / link-local / 169.254.169.254 metadata IPs
// + sanitise the error) is tracked separately, not done here.

#[derive(Deserialize)]
pub struct DiscoverSourceRequest {
    /// Source kind — only introspectable kinds (`clickhouse`) are wired.
    pub kind: String,
    /// Live connector credentials (e.g. host/port/database/username/
    /// password/secure for ClickHouse). Not persisted.
    #[serde(default)]
    pub config: serde_json::Value,
}

#[derive(Serialize)]
pub struct DiscoverSourceResponse {
    pub tables: Vec<agentic_pipeline::DiscoveredTable>,
}

pub async fn discover_source_tables(
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Json(req): Json<DiscoverSourceRequest>,
) -> Response {
    match agentic_pipeline::airway_run::discover_airway_source_tables(req.kind, req.config).await {
        Ok(tables) => Json(DiscoverSourceResponse { tables }).into_response(),
        Err(AirwayRunError::Airway(e)) => {
            // Bad credentials / unreachable host / unsupported kind —
            // the caller's input, surfaced verbatim so the wizard can show it.
            (StatusCode::BAD_GATEWAY, format!("discovery failed: {e}")).into_response()
        }
        Err(e) => {
            tracing::warn!(%e, "discover_source_tables failed");
            (StatusCode::BAD_REQUEST, format!("discovery: {e}")).into_response()
        }
    }
}
