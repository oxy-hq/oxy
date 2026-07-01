//! Worker shape that wraps `oxy_compile::compile_workspace` into the
//! runtime's `ExecutingTask` channel pair so the existing worker pool
//! can drive it like any other queued task.
//!
//! Compile is atomic from the queue's perspective — one TaskSpec, one
//! `revisions` row, no per-step decisions, no fan-out at the
//! coordinator. The worker therefore emits exactly three events
//! (`compile_started`, `compile_progress` on each milestone,
//! `compile_finished`) and a single terminal `TaskOutcome`.

use std::path::PathBuf;
use std::sync::Arc;

use agentic_core::delegation::TaskOutcome;
use agentic_runtime::orchestrator::worker::ExecutingTask;
use sea_orm::DatabaseConnection;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use oxy_compile::{
    CompileError, CompileOutcome, CompileRequest, RevisionKind, RevisionStatus, compile_workspace,
    compiler_version,
};

const EVENT_BUFFER: usize = 16;
const OUTCOME_BUFFER: usize = 4;

/// Inputs the worker needs to drive one compile. Translated from the
/// `TaskSpec::Compile` payload by the executor before being handed to
/// this worker.
#[derive(Debug, Clone)]
pub struct CompileSpec {
    pub workspace_id: Uuid,
    pub workspace_path: PathBuf,
    pub git_sha: Option<String>,
    pub branch: Option<String>,
    pub promote: bool,
    pub kind: RevisionKind,
    pub owner_user_id: Option<Uuid>,
}

pub struct CompileWorker {
    db: Arc<DatabaseConnection>,
}

impl CompileWorker {
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Self { db }
    }

    /// Spawn a compile against the supplied spec. The returned
    /// `ExecutingTask` carries event + outcome receivers + a cancel
    /// token; the worker pool will plumb them through the normal
    /// queue lifecycle.
    pub fn execute(&self, spec: CompileSpec) -> ExecutingTask {
        let (event_tx, event_rx) = mpsc::channel::<(String, Value)>(EVENT_BUFFER);
        let (outcome_tx, outcome_rx) = mpsc::channel::<TaskOutcome>(OUTCOME_BUFFER);
        let cancel = CancellationToken::new();

        let db = self.db.clone();
        let cancel_clone = cancel.clone();
        // Outer task watches the inner one so a panic / abort always
        // synthesizes a terminal Failed outcome rather than leaving
        // the coordinator waiting on a dead channel. Mirrors the
        // `AirwayWorker` pattern.
        let outcome_tx_watch = outcome_tx.clone();
        tokio::spawn(async move {
            let handle = tokio::spawn(drive(spec, db, event_tx, cancel_clone, outcome_tx));
            if let Err(join_err) = handle.await {
                let msg = if join_err.is_panic() {
                    "compile worker panicked (internal error)".to_string()
                } else {
                    "compile worker task aborted".to_string()
                };
                let _ = outcome_tx_watch.send(TaskOutcome::Failed(msg)).await;
            }
        });

        ExecutingTask {
            events: event_rx,
            outcomes: outcome_rx,
            cancel,
            answers: None,
        }
    }
}

#[tracing::instrument(
    skip_all,
    fields(workspace_id = %spec.workspace_id, promote = spec.promote)
)]
async fn drive(
    spec: CompileSpec,
    db: Arc<DatabaseConnection>,
    event_tx: mpsc::Sender<(String, Value)>,
    cancel: CancellationToken,
    outcome_tx: mpsc::Sender<TaskOutcome>,
) {
    let started_ts = chrono::Utc::now();
    let _ = event_tx
        .send((
            "compile_started".to_string(),
            json!({
                "workspace_id": spec.workspace_id,
                "git_sha": spec.git_sha,
                "branch": spec.branch,
                "promote": spec.promote,
                "kind": spec.kind.as_str(),
                "started_at": started_ts.to_rfc3339(),
            }),
        ))
        .await;

    // Cancel is checked once up front rather than mid-compile; the
    // compile primitive itself is unit-of-work atomic — interrupting
    // it half-way through could leave a `compiling` row in Postgres
    // (the reaper sweep handles that path).
    if cancel.is_cancelled() {
        let _ = outcome_tx
            .send(TaskOutcome::Failed(
                "compile cancelled before start".to_string(),
            ))
            .await;
        return;
    }

    let outcome = compile_workspace(CompileRequest {
        db: &db,
        workspace_id: spec.workspace_id,
        workspace_path: &spec.workspace_path,
        git_sha: spec.git_sha.clone(),
        branch: spec.branch.clone(),
        compiler_version: compiler_version(),
        promote: spec.promote,
        kind: spec.kind,
        owner_user_id: spec.owner_user_id,
        // Reject a compiled config the stateless fleet couldn't read (#2520):
        // it fails the compile instead of promoting a revision that 503s.
        config_gate: Some(crate::server::compile_config_gate::runtime_config_gate()),
    })
    .await;

    match outcome {
        Ok(o) => {
            let _ = event_tx
                .send(("compile_finished".to_string(), summarise_outcome(&o)))
                .await;
            let answer = format!(
                "revision {} {} ({} files compiled, {} failed)",
                o.revision_id,
                o.status.as_str(),
                o.file_count_compiled,
                o.file_count_failed
            );
            let task_outcome = if matches!(o.status, RevisionStatus::Ready) {
                // config.yml is the source of truth for the per-workspace health
                // cadence. A promoted compile is the sync point: refresh the
                // workspace's `health_eval` schedule row from the freshly
                // compiled `health_check`. Best-effort — never fail the compile.
                if spec.promote {
                    reconcile_health_from_compiled(&db, spec.workspace_id).await;
                }
                TaskOutcome::Done {
                    answer,
                    metadata: Some(summarise_outcome(&o)),
                }
            } else {
                TaskOutcome::Failed(format!(
                    "compile recorded {} failed file(s); revision_id={}",
                    o.file_count_failed, o.revision_id
                ))
            };
            let _ = outcome_tx.send(task_outcome).await;
        }
        Err(e) => {
            let _ = event_tx
                .send((
                    "compile_finished".to_string(),
                    json!({ "error": format!("{e}") }),
                ))
                .await;
            let _ = outcome_tx
                .send(TaskOutcome::Failed(compile_error_to_string(&e)))
                .await;
        }
    }
}

/// Derive the health-eval `(interval, enabled)` from a compiled config JSON
/// value's `health_check` section. Absent / unparseable → default cadence,
/// enabled. Pure (no IO) so it's directly unit-testable.
fn health_settings_from_config(config: Option<&Value>) -> (std::time::Duration, bool) {
    let hc = config.and_then(|v| v.get("health_check")).and_then(|hc| {
        serde_json::from_value::<oxy::config::health_check::HealthCheckConfig>(hc.clone()).ok()
    });
    let interval = oxy::config::health_check::resolve_interval(hc.as_ref());
    let enabled = hc.as_ref().map(|h| h.enabled).unwrap_or(true);
    (interval, enabled)
}

/// Read the workspace's promoted compiled config and reconcile its `health_eval`
/// schedule row to the configured cadence. Best-effort: a read error or missing
/// config falls back to the default cadence; a reconcile error is logged.
pub(crate) async fn reconcile_health_from_compiled(
    db: &DatabaseConnection,
    workspace_id: uuid::Uuid,
) {
    let config = crate::server::api::compiled_reader::resolve_workspace_config(workspace_id, None)
        .await
        .ok()
        .flatten();
    let (interval, enabled) = health_settings_from_config(config.as_ref());
    if let Err(e) =
        agentic_pipeline::scheduler::reconcile_health_schedule(db, workspace_id, interval, enabled)
            .await
    {
        tracing::warn!(
            target: "health_eval",
            error = %e,
            %workspace_id,
            "failed to reconcile health schedule from compiled config"
        );
    }
}

fn summarise_outcome(o: &CompileOutcome) -> Value {
    json!({
        "revision_id": o.revision_id,
        "status": o.status.as_str(),
        "git_sha": o.git_sha,
        "branch": o.branch,
        "started_at": o.started_at.to_rfc3339(),
        "finished_at": o.finished_at.to_rfc3339(),
        "file_count_seen": o.file_count_seen,
        "file_count_compiled": o.file_count_compiled,
        "file_count_failed": o.file_count_failed,
        // Truncate per-file failures to the first 10 here so the queue
        // event stream stays cheap; the full list is queryable via the
        // revisions.error_summary JSONB column.
        "failures_sample": o.failures.iter().take(10).collect::<Vec<_>>(),
    })
}

fn compile_error_to_string(e: &CompileError) -> String {
    format!("compile error: {e}")
}

/// Parse a `TaskSpec::Compile` payload into a `CompileSpec` the worker
/// can drive. Stays in this module so the executor stays clean of
/// payload-shape decisions.
pub fn spec_from_taskspec(
    workspace_id: Uuid,
    workspace_path: PathBuf,
    git_sha: Option<String>,
    branch: Option<String>,
    promote: bool,
    kind: Option<&str>,
    owner_user_id: Option<Uuid>,
) -> Result<CompileSpec, String> {
    let kind = match kind {
        None | Some("main") => RevisionKind::Main,
        Some("draft") => RevisionKind::Draft,
        Some(other) => return Err(format!("unknown revision kind: {other}")),
    };
    if matches!(kind, RevisionKind::Draft) && owner_user_id.is_none() {
        return Err("draft revision requires owner_user_id".to_string());
    }
    Ok(CompileSpec {
        workspace_id,
        workspace_path,
        git_sha,
        branch,
        promote,
        kind,
        owner_user_id,
    })
}

#[cfg(test)]
mod health_settings_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn absent_config_is_default_cadence_enabled() {
        let (interval, enabled) = health_settings_from_config(None);
        assert_eq!(interval, std::time::Duration::from_secs(600));
        assert!(enabled);
    }

    #[test]
    fn absent_health_check_section_is_default() {
        let cfg = json!({ "databases": [] });
        let (interval, enabled) = health_settings_from_config(Some(&cfg));
        assert_eq!(interval, std::time::Duration::from_secs(600));
        assert!(enabled);
    }

    #[test]
    fn reads_interval_and_enabled() {
        let cfg = json!({ "health_check": { "interval": "45m", "enabled": true } });
        let (interval, enabled) = health_settings_from_config(Some(&cfg));
        assert_eq!(interval, std::time::Duration::from_secs(2700));
        assert!(enabled);
    }

    #[test]
    fn disabled_is_respected() {
        let cfg = json!({ "health_check": { "enabled": false } });
        let (interval, enabled) = health_settings_from_config(Some(&cfg));
        assert_eq!(interval, std::time::Duration::from_secs(600));
        assert!(!enabled);
    }
}
