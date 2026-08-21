//! Batch operator actions: enqueue a compile per workspace
//! (`/batch/run`) and repoint many workspaces at once (`/batch/promote`).

use axum::Json;
use axum::http::StatusCode;
use axum::response::Response;
use sea_orm::{DatabaseConnection, EntityTrait};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{BATCH_MAX_IDS, connect, error_body, insert_run_and_enqueue_compile, promote_one};

// POST /admin/compiles/batch/run — enqueue a compile per workspace

#[derive(Deserialize, Debug)]
pub struct BatchRunRequest {
    pub workspace_ids: Vec<Uuid>,
    #[serde(default)]
    pub promote: bool,
}

#[derive(Serialize, Debug)]
pub struct BatchRunResultRow {
    pub workspace_id: Uuid,
    pub task_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct BatchRunResponse {
    pub enqueued: usize,
    pub results: Vec<BatchRunResultRow>,
}

/// Enqueue a compile for each requested workspace. A single bad id (missing
/// workspace, enqueue failure) is reported in its result row and does not abort
/// the rest of the batch.
pub(super) async fn batch_run_compile(
    Json(req): Json<BatchRunRequest>,
) -> Result<Json<BatchRunResponse>, Response> {
    if req.workspace_ids.len() > BATCH_MAX_IDS {
        return Err(error_body(
            StatusCode::BAD_REQUEST,
            "too_many",
            Some(format!(
                "at most {BATCH_MAX_IDS} workspace_ids per batch (got {})",
                req.workspace_ids.len()
            )),
        ));
    }

    // Dedupe so a duplicated id can't enqueue two compiles for the same
    // workspace. Keep first-seen order.
    let mut seen = std::collections::HashSet::new();
    let workspace_ids: Vec<Uuid> = req
        .workspace_ids
        .into_iter()
        .filter(|id| seen.insert(*id))
        .collect();

    let db = connect().await?;
    let mut results = Vec::with_capacity(workspace_ids.len());
    let mut enqueued = 0usize;
    for workspace_id in workspace_ids {
        let row = match run_one_compile(&db, workspace_id, req.promote).await {
            Ok(task_id) => {
                enqueued += 1;
                BatchRunResultRow {
                    workspace_id,
                    task_id: Some(task_id),
                    error: None,
                }
            }
            Err(error) => BatchRunResultRow {
                workspace_id,
                task_id: None,
                error: Some(error),
            },
        };
        results.push(row);
    }

    Ok(Json(BatchRunResponse { enqueued, results }))
}

/// Confirm the workspace exists, then enqueue a compile. Returns the task id or
/// a flat operator-facing error string for the batch result row.
async fn run_one_compile(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    promote: bool,
) -> Result<String, String> {
    let exists = entity::workspaces::Entity::find_by_id(workspace_id)
        .one(db)
        .await
        .map_err(|e| format!("{e}"))?;
    if exists.is_none() {
        return Err(format!("workspace {workspace_id} not found"));
    }
    insert_run_and_enqueue_compile(db, workspace_id, None, None, promote)
        .await
        .map_err(|e| {
            tracing::error!(?e, %workspace_id, "admin/compiles: batch enqueue failed");
            format!("{e}")
        })
}

// POST /admin/compiles/batch/promote — repoint many workspaces at once

#[derive(Deserialize, Debug)]
pub struct BatchPromoteRequest {
    pub revision_ids: Vec<Uuid>,
}

#[derive(Serialize, Debug)]
pub struct BatchPromoteResultRow {
    pub revision_id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub error: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct BatchPromoteResponse {
    pub promoted: usize,
    pub results: Vec<BatchPromoteResultRow>,
}

/// Promote each requested revision via `promote_one`. A single failure
/// (not found, not promotable, DB error) is captured in its result row without
/// aborting the rest of the batch.
pub(super) async fn batch_promote(
    Json(req): Json<BatchPromoteRequest>,
) -> Result<Json<BatchPromoteResponse>, Response> {
    if req.revision_ids.len() > BATCH_MAX_IDS {
        return Err(error_body(
            StatusCode::BAD_REQUEST,
            "too_many",
            Some(format!(
                "at most {BATCH_MAX_IDS} revision_ids per batch (got {})",
                req.revision_ids.len()
            )),
        ));
    }

    // Dedupe so a duplicated revision id is promoted at most once.
    let mut seen = std::collections::HashSet::new();
    let revision_ids: Vec<Uuid> = req
        .revision_ids
        .into_iter()
        .filter(|id| seen.insert(*id))
        .collect();

    let db = connect().await?;
    let mut results = Vec::with_capacity(revision_ids.len());
    let mut promoted = 0usize;
    for revision_id in revision_ids {
        let row = match promote_one(&db, revision_id).await {
            Ok(workspace_id) => {
                promoted += 1;
                BatchPromoteResultRow {
                    revision_id,
                    workspace_id: Some(workspace_id),
                    error: None,
                }
            }
            Err(e) => BatchPromoteResultRow {
                revision_id,
                workspace_id: None,
                error: Some(e.message(revision_id)),
            },
        };
        results.push(row);
    }

    Ok(Json(BatchPromoteResponse { promoted, results }))
}
