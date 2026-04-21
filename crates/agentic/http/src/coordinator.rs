//! Coordinator dashboard routes.
//!
//! GET  /coordinator/active-runs      — list currently active (non-terminal) root runs
//! GET  /coordinator/runs             — list recent runs (paginated)
//! GET  /coordinator/runs/:id/tree    — full task tree for a run
//! GET  /coordinator/recovery         — recovery & reliability stats
//! GET  /coordinator/queue            — task queue health
//! GET  /coordinator/live             — SSE stream of run status changes

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    Json,
    extract::{Extension, Path},
    http::StatusCode,
    response::{
        IntoResponse, Response,
        sse::{Event as SseEvent, KeepAlive, Sse},
    },
};
use serde::{Deserialize, Serialize};

use crate::{
    db,
    state::{AgenticState, RunStatus},
};

// ── Response types ──────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct ActiveRunEntry {
    pub run_id: String,
    pub status: String,
    pub question: String,
    pub agent_id: String,
    pub source_type: String,
    pub attempt: i32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize)]
pub struct ActiveRunsResponse {
    pub runs: Vec<ActiveRunEntry>,
    pub total: usize,
}

#[derive(Deserialize)]
pub struct ListRunsQuery {
    #[serde(default = "default_limit")]
    pub limit: u64,
    #[serde(default)]
    pub offset: u64,
    /// Comma-separated user-facing statuses: running, suspended, done, failed, cancelled
    #[serde(default)]
    pub status: Option<String>,
    /// Filter by source_type: analytics, builder
    #[serde(default)]
    pub source_type: Option<String>,
}

fn default_limit() -> u64 {
    50
}

#[derive(Serialize)]
pub struct RunHistoryEntry {
    pub run_id: String,
    pub status: String,
    pub question: String,
    pub agent_id: String,
    pub source_type: String,
    pub answer: Option<String>,
    pub error_message: Option<String>,
    pub attempt: i32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize)]
pub struct RunHistoryResponse {
    pub runs: Vec<RunHistoryEntry>,
    pub total: usize,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn extract_agent_id(metadata: &Option<serde_json::Value>) -> String {
    metadata
        .as_ref()
        .and_then(|m| m.get("agent_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

// ── GET /coordinator/active-runs ─────────────────────────────────────────────

pub async fn list_active_runs(Extension(state): Extension<Arc<AgenticState>>) -> Response {
    let db = state.db.clone();

    let runs = match db::list_active_runs(&db).await {
        Ok(r) => r,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("db error: {e}")).into_response();
        }
    };

    let entries: Vec<ActiveRunEntry> = runs
        .into_iter()
        .map(|r| {
            // Prefer in-memory status (more up-to-date) over DB status.
            let live_status = state
                .statuses
                .get(&r.id)
                .map(|s| match s.value() {
                    RunStatus::Running => "running",
                    RunStatus::Suspended { .. } => "suspended",
                    RunStatus::Done => "done",
                    RunStatus::Failed(_) => "failed",
                    RunStatus::Cancelled => "cancelled",
                })
                .unwrap_or_else(|| db::user_facing_status(r.task_status.as_deref()));

            ActiveRunEntry {
                run_id: r.id,
                status: live_status.to_string(),
                question: r.question,
                agent_id: extract_agent_id(&r.metadata),
                source_type: r.source_type.unwrap_or_default(),
                attempt: r.attempt,
                created_at: r.created_at.to_rfc3339(),
                updated_at: r.updated_at.to_rfc3339(),
            }
        })
        .collect();

    let total = entries.len();
    Json(ActiveRunsResponse {
        runs: entries,
        total,
    })
    .into_response()
}

// ── GET /coordinator/runs ────────────────────────────────────────────────────

pub async fn list_runs(
    Extension(state): Extension<Arc<AgenticState>>,
    axum::extract::Query(query): axum::extract::Query<ListRunsQuery>,
) -> Response {
    let db = state.db.clone();

    let limit = query.limit.min(200);

    // Parse comma-separated status filter.
    let status_strings: Vec<String> = query
        .status
        .as_deref()
        .map(|s| s.split(',').map(|v| v.trim().to_string()).collect())
        .unwrap_or_default();
    let status_refs: Vec<&str> = status_strings.iter().map(|s| s.as_str()).collect();
    let status_filter = if status_refs.is_empty() {
        None
    } else {
        Some(status_refs.as_slice())
    };

    let (runs, total_count) = match db::list_runs_filtered(
        &db,
        status_filter,
        query.source_type.as_deref(),
        query.offset,
        limit,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("db error: {e}")).into_response();
        }
    };

    let mut entries = Vec::with_capacity(runs.len());
    for r in runs {
        let (status, error_message) =
            db::get_effective_run_state(&db, &r)
                .await
                .unwrap_or_else(|_| {
                    (
                        db::user_facing_status(r.task_status.as_deref()).to_string(),
                        r.error_message.clone(),
                    )
                });

        entries.push(RunHistoryEntry {
            run_id: r.id,
            status,
            question: r.question,
            agent_id: extract_agent_id(&r.metadata),
            source_type: r.source_type.unwrap_or_default(),
            answer: r.answer,
            error_message,
            attempt: r.attempt,
            created_at: r.created_at.to_rfc3339(),
            updated_at: r.updated_at.to_rfc3339(),
        });
    }

    Json(RunHistoryResponse {
        runs: entries,
        total: total_count as usize,
    })
    .into_response()
}

// ── GET /coordinator/runs/:id/tree ────────────────────────────────────────────

#[derive(Deserialize)]
pub struct RunIdPath {
    id: String,
}

#[derive(Serialize)]
pub struct TaskTreeNode {
    pub run_id: String,
    pub parent_run_id: Option<String>,
    pub status: String,
    pub question: String,
    pub agent_id: String,
    pub source_type: String,
    pub answer: Option<String>,
    pub error_message: Option<String>,
    pub attempt: i32,
    pub task_status: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// Outcome as recorded by the parent coordinator (from agentic_task_outcomes).
    pub outcome_status: Option<String>,
}

#[derive(Serialize)]
pub struct TaskTreeResponse {
    pub root_id: String,
    pub nodes: Vec<TaskTreeNode>,
}

pub async fn get_run_tree(
    Path(RunIdPath { id: run_id }): Path<RunIdPath>,
    Extension(state): Extension<Arc<AgenticState>>,
) -> Response {
    let db = state.db.clone();

    let runs = match db::load_task_tree(&db, &run_id).await {
        Ok(r) if r.is_empty() => {
            return (StatusCode::NOT_FOUND, "run not found").into_response();
        }
        Ok(r) => r,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("db error: {e}")).into_response();
        }
    };

    // Collect all task outcomes for runs that have children.
    let parent_ids: Vec<String> = runs.iter().map(|r| r.id.clone()).collect();
    let mut outcome_map: HashMap<String, String> = HashMap::new();
    for pid in &parent_ids {
        if let Ok(outcomes) = db::get_outcomes_for_parent(&db, pid).await {
            for o in outcomes {
                outcome_map.insert(o.child_id, o.status);
            }
        }
    }

    let nodes: Vec<TaskTreeNode> = runs
        .into_iter()
        .map(|r| {
            let live_status = state
                .statuses
                .get(&r.id)
                .map(|s| match s.value() {
                    RunStatus::Running => "running",
                    RunStatus::Suspended { .. } => "suspended",
                    RunStatus::Done => "done",
                    RunStatus::Failed(_) => "failed",
                    RunStatus::Cancelled => "cancelled",
                })
                .unwrap_or_else(|| db::user_facing_status(r.task_status.as_deref()));

            TaskTreeNode {
                outcome_status: outcome_map.get(&r.id).cloned(),
                run_id: r.id,
                parent_run_id: r.parent_run_id,
                status: live_status.to_string(),
                question: r.question,
                agent_id: extract_agent_id(&r.metadata),
                source_type: r.source_type.unwrap_or_default(),
                answer: r.answer,
                error_message: r.error_message,
                attempt: r.attempt,
                task_status: r.task_status,
                created_at: r.created_at.to_rfc3339(),
                updated_at: r.updated_at.to_rfc3339(),
            }
        })
        .collect();

    Json(TaskTreeResponse {
        root_id: run_id,
        nodes,
    })
    .into_response()
}

// ── GET /coordinator/recovery ─────────────────────────────────────────────────

#[derive(Serialize)]
pub struct AgentStats {
    pub agent_id: String,
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub recovered: usize,
}

#[derive(Serialize)]
pub struct RecoveredRunEntry {
    pub run_id: String,
    pub status: String,
    pub question: String,
    pub agent_id: String,
    pub attempt: i32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize)]
pub struct RecoveryResponse {
    /// Total root runs in the window.
    pub total_runs: usize,
    /// Runs that were recovered (attempt > 0).
    pub recovered_count: usize,
    /// Runs that ended in failure.
    pub failed_count: usize,
    /// Runs that ended in cancellation.
    pub cancelled_count: usize,
    /// Runs that completed successfully.
    pub succeeded_count: usize,
    /// Per-agent breakdown.
    pub agents: Vec<AgentStats>,
    /// Recovered runs (attempt > 0), most recent first.
    pub recovered_runs: Vec<RecoveredRunEntry>,
}

pub async fn get_recovery_stats(
    Extension(state): Extension<Arc<AgenticState>>,
    axum::extract::Query(query): axum::extract::Query<ListRunsQuery>,
) -> Response {
    let db = state.db.clone();

    // Fetch recent root runs.
    let limit = query.limit.min(500);
    let runs = match db::list_recent_runs(&db, limit).await {
        Ok(r) => r,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("db error: {e}")).into_response();
        }
    };

    // Only root runs (no children).
    let root_runs: Vec<_> = runs
        .into_iter()
        .filter(|r| r.parent_run_id.is_none())
        .collect();

    let mut recovered_count = 0usize;
    let mut failed_count = 0usize;
    let mut cancelled_count = 0usize;
    let mut succeeded_count = 0usize;
    let mut agent_map: HashMap<String, (usize, usize, usize, usize)> = HashMap::new(); // (total, ok, fail, recovered)
    let mut recovered_runs = Vec::new();

    for r in &root_runs {
        let agent_id = extract_agent_id(&r.metadata);
        let status = db::user_facing_status(r.task_status.as_deref());

        match status {
            "done" => succeeded_count += 1,
            "failed" => failed_count += 1,
            "cancelled" => cancelled_count += 1,
            _ => {}
        }

        let entry = agent_map.entry(agent_id.clone()).or_insert((0, 0, 0, 0));
        entry.0 += 1;
        if status == "done" {
            entry.1 += 1;
        }
        if status == "failed" {
            entry.2 += 1;
        }

        if r.attempt > 0 {
            recovered_count += 1;
            entry.3 += 1;
            recovered_runs.push(RecoveredRunEntry {
                run_id: r.id.clone(),
                status: status.to_string(),
                question: r.question.clone(),
                agent_id,
                attempt: r.attempt,
                created_at: r.created_at.to_rfc3339(),
                updated_at: r.updated_at.to_rfc3339(),
            });
        }
    }

    let agents: Vec<AgentStats> = agent_map
        .into_iter()
        .map(
            |(agent_id, (total, succeeded, failed, recovered))| AgentStats {
                agent_id,
                total,
                succeeded,
                failed,
                recovered,
            },
        )
        .collect();

    Json(RecoveryResponse {
        total_runs: root_runs.len(),
        recovered_count,
        failed_count,
        cancelled_count,
        succeeded_count,
        agents,
        recovered_runs,
    })
    .into_response()
}

// ── GET /coordinator/queue ────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct QueueTaskEntry {
    pub task_id: String,
    pub run_id: String,
    pub queue_status: String,
    pub worker_id: Option<String>,
    pub claim_count: i32,
    pub max_claims: i32,
    pub last_heartbeat: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize)]
pub struct QueueHealthResponse {
    pub queued: u64,
    pub claimed: u64,
    pub completed: u64,
    pub failed: u64,
    pub cancelled: u64,
    pub dead: u64,
    pub stale_tasks: Vec<QueueTaskEntry>,
    pub dead_tasks: Vec<QueueTaskEntry>,
}

fn queue_row_to_entry(m: db::QueueTaskRow) -> QueueTaskEntry {
    QueueTaskEntry {
        task_id: m.task_id,
        run_id: m.run_id,
        queue_status: m.queue_status,
        worker_id: m.worker_id,
        claim_count: m.claim_count,
        max_claims: m.max_claims,
        last_heartbeat: m.last_heartbeat.map(|t| t.to_rfc3339()),
        created_at: m.created_at.to_rfc3339(),
        updated_at: m.updated_at.to_rfc3339(),
    }
}

pub async fn get_queue_health(Extension(state): Extension<Arc<AgenticState>>) -> Response {
    let db = state.db.clone();

    let stats = match db::get_queue_stats(&db).await {
        Ok(s) => s,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("db error: {e}")).into_response();
        }
    };

    Json(QueueHealthResponse {
        queued: stats.queued,
        claimed: stats.claimed,
        completed: stats.completed,
        failed: stats.failed,
        cancelled: stats.cancelled,
        dead: stats.dead,
        stale_tasks: stats
            .stale_tasks
            .into_iter()
            .map(queue_row_to_entry)
            .collect(),
        dead_tasks: stats
            .dead_tasks
            .into_iter()
            .map(queue_row_to_entry)
            .collect(),
    })
    .into_response()
}

// ── GET /coordinator/live (SSE) ──────────────────────────────────────────────
//
// Streams run status snapshots every time any run's status changes.
// The client receives a periodic snapshot of all in-memory run statuses.

#[derive(Serialize)]
struct LiveStatusEntry {
    run_id: String,
    status: String,
}

pub async fn live_stream(Extension(state): Extension<Arc<AgenticState>>) -> Response {
    let stream = async_stream::stream! {
        // Send an initial snapshot immediately.
        let snapshot = build_snapshot(&state);
        let event = SseEvent::default()
            .event("snapshot")
            .data(serde_json::to_string(&snapshot).unwrap_or_default());
        yield Ok::<_, std::convert::Infallible>(event);

        // Poll every 2 seconds for status changes.
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
        let mut last_snapshot = snapshot;

        loop {
            tokio::select! {
                _ = interval.tick() => {},
                _ = state.shutdown_token.cancelled() => break,
            }

            let current = build_snapshot(&state);
            if current != last_snapshot {
                let event = SseEvent::default()
                    .event("snapshot")
                    .data(serde_json::to_string(&current).unwrap_or_default());
                yield Ok(event);
                last_snapshot = current;
            }
        }
    };

    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

fn build_snapshot(state: &AgenticState) -> Vec<LiveStatusEntry> {
    state
        .statuses
        .iter()
        .map(|entry| {
            let status = match entry.value() {
                RunStatus::Running => "running",
                RunStatus::Suspended { .. } => "suspended",
                RunStatus::Done => "done",
                RunStatus::Failed(_) => "failed",
                RunStatus::Cancelled => "cancelled",
            };
            LiveStatusEntry {
                run_id: entry.key().clone(),
                status: status.to_string(),
            }
        })
        .collect()
}

impl PartialEq for LiveStatusEntry {
    fn eq(&self, other: &Self) -> bool {
        self.run_id == other.run_id && self.status == other.status
    }
}
