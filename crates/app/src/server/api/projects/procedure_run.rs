//! `POST /api/projects/{project_id}/procedures/{procedure_id}/runs`     (start)
//! `GET  /api/projects/{project_id}/procedures/runs/{run_id}`            (poll)
//! `POST /api/projects/{project_id}/procedures/runs/{run_id}/cancel`     (cancel)
//!
//! Long-running batch surface for customer-app bundles. A procedure
//! is a `.procedure.yml` file in the project — multi-step
//! orchestration (SQL, agent calls, file writes, etc.). Bundles use
//! this to expose "Generate report" / "Recompute" buttons that
//! produce structured artifacts.
//!
//! Pipeline: reuses
//! `agentic_pipeline::workflow_run::run_inline_workflow_with_render_context`
//! (the same path CLI `oxy run` and the MCP workflow tool use). The
//! procedure runs in a spawned task; state lives in the
//! `customer_app_procedure_runs` DB table (see
//! `migration::m20260526_000001_create_customer_app_procedure_runs`)
//! so server restarts don't drop in-flight runs from the bundle's
//! point of view.
//!
//! Cancellation: the cancel endpoint stamps `cancel_requested_at` on
//! the row AND aborts the JoinHandle in the in-process registry. The
//! abort is the fast path (kills the spawn immediately); the DB
//! stamp is the durable record for cross-instance / restart cases.

use std::collections::HashMap;
use std::sync::Arc;

use agentic_pipeline::workflow_run::WorkflowRunError;
use agentic_workflow::WorkflowConfig;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use dashmap::DashMap;
use entity::customer_app_procedure_runs as proc_run;
use entity::customer_app_procedure_runs::ActiveModel as ProcRunActiveModel;
use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tokio::task::JoinHandle;
use tracing::{error, instrument, warn};
use uuid::Uuid;

use crate::server::api::customer_apps_gates::{check_customer_app_gates, parse_versioned_body};
use crate::server::router::AppState;

#[derive(Serialize)]
struct ApiErr {
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<String>,
}

fn err(status: StatusCode, msg: impl Into<String>) -> Response {
    (
        status,
        Json(ApiErr {
            message: msg.into(),
            code: None,
            hint: None,
        }),
    )
        .into_response()
}

fn err_with_code(status: StatusCode, msg: impl Into<String>, code: &'static str) -> Response {
    (
        status,
        Json(ApiErr {
            message: msg.into(),
            code: Some(code),
            hint: None,
        }),
    )
        .into_response()
}

fn err_with_hint(
    status: StatusCode,
    msg: impl Into<String>,
    code: &'static str,
    hint: impl Into<String>,
) -> Response {
    (
        status,
        Json(ApiErr {
            message: msg.into(),
            code: Some(code),
            hint: Some(hint.into()),
        }),
    )
        .into_response()
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcedureRunRequest {
    /// Bag of params to inject into the procedure's render context.
    /// Each key becomes available to procedure SQL templates as
    /// `{{ params.<key> }}`.
    #[serde(default)]
    pub params: Option<JsonValue>,
}

#[derive(Debug, Serialize)]
pub struct ProcedureRunStartResponse {
    pub run_id: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ProcedureRunPollResponse {
    Running {
        #[serde(skip_serializing_if = "Option::is_none")]
        progress: Option<ProgressFrame>,
    },
    Done {
        result: ProcedureResult,
    },
    Failed {
        error: ProcedureError,
    },
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProgressFrame {
    pub step: String,
    pub percent: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcedureError {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProcedureResult {
    pub summary: String,
    pub outputs: HashMap<String, JsonValue>,
}

/// In-process map of run_id → tokio JoinHandle. Used by the cancel
/// endpoint to abort the spawned task immediately on the same
/// instance. DB row's `cancel_requested_at` is the durable record;
/// this is just the fast path. Cleaned up when the spawned task
/// completes.
fn join_handles() -> &'static DashMap<String, JoinHandle<()>> {
    use std::sync::OnceLock;
    static HANDLES: OnceLock<DashMap<String, JoinHandle<()>>> = OnceLock::new();
    HANDLES.get_or_init(DashMap::new)
}

#[instrument(skip_all, fields(project_id = %project_id, procedure_id = %procedure_id))]
pub async fn start_procedure_run(
    State(app_state): State<AppState>,
    Path((project_id, procedure_id)): Path<(Uuid, String)>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let gates_ctx = match check_customer_app_gates(&headers, project_id).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };
    let req: ProcedureRunRequest = match parse_versioned_body(&body) {
        Ok(r) => r,
        Err(resp) => return resp,
    };

    let agentic_state = match app_state.agentic_state.as_ref() {
        Some(s) => s.clone(),
        None => {
            return err(
                StatusCode::SERVICE_UNAVAILABLE,
                "procedure runtime not configured in this deployment",
            );
        }
    };
    let db = agentic_state.db.clone();

    let proj_ctx = match gates_ctx.build_project_context().await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    let workspace_root = proj_ctx
        .workspace_manager()
        .config_manager
        .workspace_path()
        .to_path_buf();
    let candidate_paths = [
        workspace_root.join(format!("{procedure_id}.procedure.yml")),
        workspace_root.join(format!("{procedure_id}.workflow.yml")),
        workspace_root.join(format!("{procedure_id}.automation.yml")),
    ];
    let procedure_path = match candidate_paths.iter().find(|p| p.exists()) {
        Some(p) => p.clone(),
        None => {
            let tried = candidate_paths
                .iter()
                .map(|p| {
                    p.strip_prefix(&workspace_root)
                        .unwrap_or(p)
                        .display()
                        .to_string()
                })
                .collect::<Vec<_>>()
                .join(", ");
            return err_with_hint(
                StatusCode::NOT_FOUND,
                format!("procedure '{procedure_id}' not found"),
                "procedure_not_found",
                format!(
                    "Looked for: {tried}. Pass the procedure's base name without the extension \
                     (e.g. for `weekly_summary.procedure.yml`, call \
                     `useProcedureRun({{ procedureId: 'weekly_summary' }})`)."
                ),
            );
        }
    };

    // `tokio::fs::read_to_string` so we don't block the executor
    // thread on potentially-slow disk I/O. Matches the workspace
    // /workflows route's pattern. YAML parsing below is synchronous
    // but fast enough on procedure-sized files (~10 KB typical) that
    // spawn_blocking adds more overhead than it saves.
    let workflow_yaml = match tokio::fs::read_to_string(&procedure_path).await {
        Ok(s) => s,
        Err(e) => {
            error!(path = ?procedure_path, error = %e, "read procedure file failed");
            return err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not read procedure file",
            );
        }
    };
    let workflow_config: WorkflowConfig = match serde_yaml::from_str(&workflow_yaml) {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "procedure YAML parse failed");
            return err_with_code(
                StatusCode::BAD_REQUEST,
                format!("procedure YAML parse failed: {e}"),
                "procedure_invalid_yaml",
            );
        }
    };

    let run_id = Uuid::new_v4();
    let now = Utc::now().into();
    let insert = ProcRunActiveModel {
        id: ActiveValue::Set(run_id),
        workspace_id: ActiveValue::Set(project_id),
        procedure_id: ActiveValue::Set(procedure_id.clone()),
        status: ActiveValue::Set("running".to_string()),
        params: ActiveValue::Set(req.params.clone()),
        progress_step: ActiveValue::Set(None),
        progress_percent: ActiveValue::Set(None),
        result_summary: ActiveValue::Set(None),
        result_outputs: ActiveValue::Set(None),
        error_message: ActiveValue::Set(None),
        error_code: ActiveValue::Set(None),
        cancel_requested_at: ActiveValue::Set(None),
        started_at: ActiveValue::Set(now),
        completed_at: ActiveValue::Set(None),
    };
    if let Err(e) = insert.insert(&db).await {
        error!(run_id = %run_id, error = %e, "procedure run insert failed");
        return err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not register procedure run",
        );
    }

    let render_context = req
        .params
        .as_ref()
        .map(|p| serde_json::json!({ "params": p }));

    let proj_ctx_run = proj_ctx;
    let db_for_task = db.clone();
    let run_id_str = run_id.to_string();
    let run_id_for_task = run_id_str.clone();
    let handle = tokio::spawn(async move {
        let workspace: Arc<dyn agentic_workflow::WorkspaceContext> = Arc::new(proj_ctx_run);
        let result = agentic_pipeline::workflow_run::run_inline_workflow_with_render_context(
            workspace.as_ref(),
            workflow_config,
            None,
            render_context,
            None,
        )
        .await;

        // Was a cancel requested mid-run? Check the DB row before we
        // record a result so a race between user-cancel + procedure-
        // completion lands on the right terminal state.
        let cancel_seen = proc_run::Entity::find_by_id(run_id)
            .one(&db_for_task)
            .await
            .ok()
            .flatten()
            .and_then(|r| r.cancel_requested_at)
            .is_some();

        let update = match (cancel_seen, result) {
            (true, _) => set_cancelled(run_id),
            (false, Ok(outputs)) => set_done(run_id, outputs),
            (false, Err(e)) => set_failed(run_id, &e),
        };
        if let Err(db_err) = update.update(&db_for_task).await {
            error!(run_id = %run_id_for_task, error = %db_err, "procedure run completion update failed");
        }
        join_handles().remove(&run_id_for_task);
    });
    join_handles().insert(run_id_str.clone(), handle);

    let resp = ProcedureRunStartResponse { run_id: run_id_str };
    (StatusCode::ACCEPTED, Json(resp)).into_response()
}

fn set_done(run_id: Uuid, outputs: HashMap<String, JsonValue>) -> ProcRunActiveModel {
    let summary = if outputs.is_empty() {
        "Procedure completed.".to_string()
    } else {
        format!("Procedure completed — {} task outputs.", outputs.len())
    };
    let outputs_json = serde_json::to_value(&outputs).unwrap_or(JsonValue::Null);
    ProcRunActiveModel {
        id: ActiveValue::Set(run_id),
        status: ActiveValue::Set("done".into()),
        result_summary: ActiveValue::Set(Some(summary)),
        result_outputs: ActiveValue::Set(Some(outputs_json)),
        completed_at: ActiveValue::Set(Some(Utc::now().into())),
        ..Default::default()
    }
}

fn set_failed(run_id: Uuid, e: &WorkflowRunError) -> ProcRunActiveModel {
    let (code, message) = workflow_error_to_code(e);
    ProcRunActiveModel {
        id: ActiveValue::Set(run_id),
        status: ActiveValue::Set("failed".into()),
        error_message: ActiveValue::Set(Some(message)),
        error_code: ActiveValue::Set(Some(code.to_string())),
        completed_at: ActiveValue::Set(Some(Utc::now().into())),
        ..Default::default()
    }
}

fn set_cancelled(run_id: Uuid) -> ProcRunActiveModel {
    ProcRunActiveModel {
        id: ActiveValue::Set(run_id),
        status: ActiveValue::Set("cancelled".into()),
        error_message: ActiveValue::Set(Some("cancelled by user".into())),
        error_code: ActiveValue::Set(Some("procedure_run_cancelled".into())),
        completed_at: ActiveValue::Set(Some(Utc::now().into())),
        ..Default::default()
    }
}

/// Best-effort categorization of procedure-runner failures.
fn workflow_error_to_code(e: &WorkflowRunError) -> (&'static str, String) {
    let msg = e.to_string();
    let lower = msg.to_lowercase();
    let code = if lower.contains("agent")
        && (lower.contains("not configured") || lower.contains("inlineagentrunner"))
    {
        "procedure_requires_agent_runner"
    } else if lower.contains("rate limit") {
        "procedure_rate_limited"
    } else if lower.contains("timeout") || lower.contains("timed out") {
        "procedure_run_timeout"
    } else if lower.contains("connect") || lower.contains("connection") {
        "procedure_warehouse_unreachable"
    } else {
        "procedure_run_failed"
    };
    (code, msg)
}

#[instrument(skip_all, fields(project_id = %project_id, run_id = %run_id))]
pub async fn poll_procedure_run(
    State(app_state): State<AppState>,
    Path((project_id, run_id)): Path<(Uuid, String)>,
    headers: HeaderMap,
) -> Response {
    let _gates_ctx = match check_customer_app_gates(&headers, project_id).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };
    let agentic_state = match app_state.agentic_state.as_ref() {
        Some(s) => s.clone(),
        None => {
            return err(
                StatusCode::SERVICE_UNAVAILABLE,
                "procedure runtime not configured",
            );
        }
    };
    let run_uuid = match Uuid::parse_str(&run_id) {
        Ok(u) => u,
        Err(_) => {
            return err_with_code(
                StatusCode::BAD_REQUEST,
                "invalid run_id",
                "procedure_run_invalid_id",
            );
        }
    };
    let row = match proc_run::Entity::find_by_id(run_uuid)
        .one(&agentic_state.db)
        .await
    {
        Ok(Some(r)) => r,
        Ok(None) => {
            return err_with_code(
                StatusCode::NOT_FOUND,
                "procedure run not found",
                "procedure_run_not_found",
            );
        }
        Err(e) => {
            error!(run_id = %run_id, error = %e, "procedure poll lookup failed");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "lookup failed");
        }
    };
    // Cross-project leakage defense.
    if row.workspace_id != project_id {
        return err_with_code(
            StatusCode::FORBIDDEN,
            "run does not belong to this project",
            "thread_project_mismatch",
        );
    }

    let resp = match row.status.as_str() {
        "running" => ProcedureRunPollResponse::Running {
            progress: row.progress_step.as_ref().map(|step| ProgressFrame {
                step: step.clone(),
                percent: row.progress_percent.unwrap_or(0).clamp(0, 100) as u8,
            }),
        },
        "done" => {
            let summary = row
                .result_summary
                .clone()
                .unwrap_or_else(|| "Procedure completed.".to_string());
            let outputs: HashMap<String, JsonValue> = row
                .result_outputs
                .clone()
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default();
            ProcedureRunPollResponse::Done {
                result: ProcedureResult { summary, outputs },
            }
        }
        "failed" => ProcedureRunPollResponse::Failed {
            error: ProcedureError {
                message: row
                    .error_message
                    .clone()
                    .unwrap_or_else(|| "procedure failed".into()),
                code: row.error_code.clone(),
            },
        },
        "cancelled" => ProcedureRunPollResponse::Cancelled,
        other => ProcedureRunPollResponse::Failed {
            error: ProcedureError {
                message: format!("unexpected status: {other}"),
                code: Some("procedure_unknown_status".into()),
            },
        },
    };
    Json(resp).into_response()
}

/// `POST /api/projects/{project_id}/procedures/runs/{run_id}/cancel`
///
/// Two-step cancel: stamp the DB row (durable; visible cross-instance,
/// survives restart) AND abort the in-process JoinHandle (fast; kills
/// the LLM call / SQL execution immediately on this instance). Returns
/// 204 on success — pollers see the terminal state on next request.
#[instrument(skip_all, fields(project_id = %project_id, run_id = %run_id))]
pub async fn cancel_procedure_run(
    State(app_state): State<AppState>,
    Path((project_id, run_id)): Path<(Uuid, String)>,
    headers: HeaderMap,
) -> Response {
    let _gates_ctx = match check_customer_app_gates(&headers, project_id).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };
    let agentic_state = match app_state.agentic_state.as_ref() {
        Some(s) => s.clone(),
        None => return err(StatusCode::SERVICE_UNAVAILABLE, "runtime not configured"),
    };
    let run_uuid = match Uuid::parse_str(&run_id) {
        Ok(u) => u,
        Err(_) => {
            return err_with_code(
                StatusCode::BAD_REQUEST,
                "invalid run_id",
                "procedure_run_invalid_id",
            );
        }
    };
    let row = match proc_run::Entity::find_by_id(run_uuid)
        .one(&agentic_state.db)
        .await
    {
        Ok(Some(r)) => r,
        Ok(None) => {
            return err_with_code(
                StatusCode::NOT_FOUND,
                "procedure run not found",
                "procedure_run_not_found",
            );
        }
        Err(e) => {
            error!(run_id = %run_id, error = %e, "cancel: lookup failed");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "lookup failed");
        }
    };
    if row.workspace_id != project_id {
        return err_with_code(
            StatusCode::FORBIDDEN,
            "run does not belong to this project",
            "thread_project_mismatch",
        );
    }
    // Idempotent: already terminal → no-op.
    if matches!(row.status.as_str(), "done" | "failed" | "cancelled") {
        return StatusCode::NO_CONTENT.into_response();
    }

    // Stamp the durable cancel marker. The spawned task reads this
    // after the workflow returns; if the abort below kills it first,
    // we still need a non-stale DB record so a future poll sees the
    // right state.
    //
    // Race: the spawned task can flip the row to `done` between the
    // status check above and this stamp. Re-check inside an UPDATE
    // … WHERE status = 'running' so we don't stamp a terminal row
    // — the sweep filter at sweep_terminal_runs only acts on
    // running rows, so a leftover cancel_requested_at on a `done`
    // row would otherwise stick around until the 24h TTL eviction.
    let stamp_res = proc_run::Entity::update_many()
        .col_expr(
            proc_run::Column::CancelRequestedAt,
            sea_orm::sea_query::Expr::value(chrono::DateTime::<chrono::FixedOffset>::from(
                Utc::now(),
            )),
        )
        .filter(proc_run::Column::Id.eq(run_uuid))
        .filter(proc_run::Column::Status.eq("running"))
        .exec(&agentic_state.db)
        .await;
    match stamp_res {
        Ok(out) if out.rows_affected == 0 => {
            // Row reached terminal state between status read and
            // stamp. The terminal row is the right answer; nothing
            // more to do.
            return StatusCode::NO_CONTENT.into_response();
        }
        Ok(_) => {}
        Err(e) => {
            warn!(run_id = %run_id, error = %e, "cancel: DB stamp failed");
        }
    }

    // Fast path: abort the in-process spawn. Only works on the
    // instance that started the run. The set_cancelled handler in
    // the task completion path covers the cross-instance case — when
    // an instance without the handle calls cancel, the stamp above
    // is the only effect, and when the procedure naturally finishes
    // it observes cancel_requested_at and writes the cancelled row.
    if let Some((_, handle)) = join_handles().remove(&run_id) {
        handle.abort();
        // Spawned task's drop path will write the cancelled row.
        // But the task may not get a chance to run (abort) — write
        // here too so pollers see the state immediately. Idempotent
        // with whatever the task may eventually write because both
        // write 'cancelled' + completed_at.
        if let Err(e) = set_cancelled(run_uuid).update(&agentic_state.db).await {
            warn!(run_id = %run_id, error = %e, "cancel: terminal update failed");
        }
    }
    StatusCode::NO_CONTENT.into_response()
}

/// Periodic maintenance for the `customer_app_procedure_runs` table.
/// Combines three jobs into one pass so the startup loop only has
/// to schedule one task. Caller invokes this on a timer (default:
/// every 10 min from `spawn_periodic_sweep`).
///
/// 1. **Evict terminal rows older than 24h.** Bundles polling
///    after-the-fact get `procedure_run_not_found` instead of a
///    stale `done`; aligns with the spec's TTL.
///
/// 2. **Reconcile cross-instance cancels.** A row with
///    `status = 'running'` AND `cancel_requested_at` set means the
///    originating instance saw the stamp but either died before
///    observing it or the cancel was issued on a different
///    instance whose abort is a no-op here. Promote to `cancelled`
///    once the stamp is older than the abort window.
///
/// 3. **Mark stuck-running rows failed.** A row with
///    `status = 'running'` and `started_at` older than 2 hours is
///    almost certainly orphaned (originating instance crashed
///    mid-run). Mark `failed` with a clear code.
pub async fn sweep_terminal_runs(
    db: &sea_orm::DatabaseConnection,
) -> Result<SweepReport, sea_orm::DbErr> {
    use sea_orm::sea_query::Expr;

    // 1. Terminal eviction.
    let evict_cutoff = Utc::now() - chrono::Duration::hours(24);
    let evicted = proc_run::Entity::delete_many()
        .filter(proc_run::Column::CompletedAt.lt(evict_cutoff))
        .filter(Expr::col(proc_run::Column::Status).is_in(["done", "failed", "cancelled"]))
        .exec(db)
        .await?
        .rows_affected;

    // 2. Cross-instance cancel reconciliation. 30s grace lets the
    //    originating instance write its own cancelled row first.
    let cancel_grace = Utc::now() - chrono::Duration::seconds(30);
    let now: chrono::DateTime<chrono::FixedOffset> = Utc::now().into();
    let stuck_cancelled = proc_run::Entity::update_many()
        .col_expr(proc_run::Column::Status, Expr::value("cancelled"))
        .col_expr(proc_run::Column::CompletedAt, Expr::value(now))
        .col_expr(
            proc_run::Column::ErrorMessage,
            Expr::value("cancelled by user (reconciled by sweep)"),
        )
        .col_expr(
            proc_run::Column::ErrorCode,
            Expr::value("procedure_run_cancelled"),
        )
        .filter(proc_run::Column::Status.eq("running"))
        .filter(proc_run::Column::CancelRequestedAt.lt(cancel_grace))
        .exec(db)
        .await?
        .rows_affected;

    // 3. Stuck-running detection. 2-hour cutoff: longest legitimate
    //    procedures run in minutes; anything `running` for 2 hours
    //    is orphaned. Guard against double-counting rows step 2 just
    //    handled by requiring `cancel_requested_at IS NULL`.
    let stuck_cutoff = Utc::now() - chrono::Duration::hours(2);
    let stuck_failed = proc_run::Entity::update_many()
        .col_expr(proc_run::Column::Status, Expr::value("failed"))
        .col_expr(proc_run::Column::CompletedAt, Expr::value(now))
        .col_expr(
            proc_run::Column::ErrorMessage,
            Expr::value("procedure timed out (no progress for 2+ hours)"),
        )
        .col_expr(
            proc_run::Column::ErrorCode,
            Expr::value("procedure_run_orphaned"),
        )
        .filter(proc_run::Column::Status.eq("running"))
        .filter(proc_run::Column::StartedAt.lt(stuck_cutoff))
        .filter(proc_run::Column::CancelRequestedAt.is_null())
        .exec(db)
        .await?
        .rows_affected;

    Ok(SweepReport {
        evicted,
        stuck_cancelled,
        stuck_failed,
    })
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SweepReport {
    pub evicted: u64,
    pub stuck_cancelled: u64,
    pub stuck_failed: u64,
}

/// Spawn the periodic sweep task. Runs every 10 minutes — fast
/// enough that stuck cancels become terminal within typical
/// polling windows, slow enough not to load the DB with delete-
/// many sweeps. Stops cleanly on shutdown.
pub fn spawn_periodic_sweep(
    db: sea_orm::DatabaseConnection,
    shutdown: tokio_util::sync::CancellationToken,
) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(10 * 60));
        // Skip the immediate first tick — startup is busy enough.
        ticker.tick().await;
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    match sweep_terminal_runs(&db).await {
                        Ok(report) => {
                            if report.evicted + report.stuck_cancelled + report.stuck_failed > 0 {
                                tracing::info!(
                                    evicted = report.evicted,
                                    stuck_cancelled = report.stuck_cancelled,
                                    stuck_failed = report.stuck_failed,
                                    "customer-app procedure runs swept",
                                );
                            }
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "customer-app procedure sweep failed");
                        }
                    }
                }
                _ = shutdown.cancelled() => {
                    tracing::debug!("customer-app procedure sweep shutting down");
                    return;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_error_classifies_known_patterns() {
        let cases = [
            (
                "connection refused to warehouse",
                "procedure_warehouse_unreachable",
            ),
            ("OpenAI returned rate limit", "procedure_rate_limited"),
            ("query timed out", "procedure_run_timeout"),
            (
                "InlineAgentRunner not configured",
                "procedure_requires_agent_runner",
            ),
            ("something else entirely", "procedure_run_failed"),
        ];
        for (msg, want) in cases {
            let e = WorkflowRunError::Inline(msg.to_string());
            let (code, _) = workflow_error_to_code(&e);
            assert_eq!(code, want, "for message {msg:?}");
        }
    }
}
