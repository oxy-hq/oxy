//! Cron schedule CRUD + run-now (multi-tenant, §12 FU4b).
//!
//! Lives in the `app` crate (not `agentic-http`) so the handlers can use
//! `WorkspaceAdmin` from `role_guards` — the agentic-http layer must not
//! depend on `app` (backend boundary rule). Routes are mounted under
//! `/{workspace_id}/agentic-schedules` in `workspace.rs`, behind the
//! standard `workspace_middleware`. All operations are scoped to
//! `workspace_id` (extracted from the parent path); mutations require
//! workspace-admin role.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Extension, Path},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use uuid::Uuid;

use agentic_http::AgenticState;
use agentic_pipeline::WorkflowWorkspaceContext;
use agentic_pipeline::platform::PlatformContext;
use agentic_pipeline::scheduler::{
    ScheduleError, ScheduleInput, create_schedule, delete_schedule, get_schedule, list_schedules,
    run_schedule_now, update_schedule,
};
use oxy_auth::extractor::AuthenticatedUserExtractor;

use crate::server::api::middlewares::role_guards::WorkspaceAdmin;

fn map_err(e: ScheduleError) -> Response {
    match e {
        ScheduleError::Invalid(m) => (StatusCode::BAD_REQUEST, m).into_response(),
        ScheduleError::NotFound => (StatusCode::NOT_FOUND, "schedule not found").into_response(),
        ScheduleError::Db(e) => {
            tracing::error!(target: "scheduler", error = %e, "schedule CRUD db error");
            (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
        }
    }
}

pub async fn list(
    Extension(state): Extension<Arc<AgenticState>>,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Path(workspace_id): Path<Uuid>,
) -> Response {
    match list_schedules(&state.db, workspace_id).await {
        Ok(rows) => Json(rows).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn get(
    Extension(state): Extension<Arc<AgenticState>>,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Path((workspace_id, id)): Path<(Uuid, String)>,
) -> Response {
    match get_schedule(&state.db, workspace_id, &id).await {
        Ok(m) => Json(m).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn create(
    _: WorkspaceAdmin,
    Extension(state): Extension<Arc<AgenticState>>,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Path(workspace_id): Path<Uuid>,
    Json(input): Json<ScheduleInput>,
) -> Response {
    match create_schedule(&state.db, workspace_id, input).await {
        Ok(m) => (StatusCode::CREATED, Json(m)).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn update(
    _: WorkspaceAdmin,
    Extension(state): Extension<Arc<AgenticState>>,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Path((workspace_id, id)): Path<(Uuid, String)>,
    Json(input): Json<ScheduleInput>,
) -> Response {
    match update_schedule(&state.db, workspace_id, &id, input).await {
        Ok(m) => Json(m).into_response(),
        Err(e) => map_err(e),
    }
}

pub async fn delete(
    _: WorkspaceAdmin,
    Extension(state): Extension<Arc<AgenticState>>,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Path((workspace_id, id)): Path<(Uuid, String)>,
) -> Response {
    match delete_schedule(&state.db, workspace_id, &id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => map_err(e),
    }
}

#[derive(Serialize)]
pub struct RunNowResponse {
    pub run_id: String,
}

pub async fn run_now(
    _: WorkspaceAdmin,
    Extension(state): Extension<Arc<AgenticState>>,
    Extension(platform): Extension<Arc<dyn PlatformContext>>,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Path((workspace_id, id)): Path<(Uuid, String)>,
) -> Response {
    let workspace: Arc<dyn WorkflowWorkspaceContext> = platform.clone();
    match run_schedule_now(&state.db, workspace_id, workspace.as_ref(), &id).await {
        Ok(run_id) => Json(RunNowResponse { run_id }).into_response(),
        Err(e) => map_err(e),
    }
}
