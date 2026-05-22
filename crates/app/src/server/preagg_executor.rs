//! PreaggTaskExecutor: runs preagg_cycle tasks via the agentic Worker infrastructure.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use agentic_core::delegation::{TaskAssignment, TaskOutcome, TaskSpec};
use agentic_runtime::worker::{ExecutingTask, TaskExecutor};
use agentic_semantic::refresh_key_cache::RefreshKeyCache;
use agentic_workflow::preagg_event::PreaggEvent;
use agentic_workflow::workspace::WorkspaceContext;
use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::preagg_rebuild::{RebuildContext, connector_to_airlayer_dialect, rebuild_rollup};
use super::preagg_worker::PreaggWorkerConfig;
use crate::agentic_wiring::OxyProjectContext;

pub struct PreaggTaskExecutor {
    pub config: Arc<PreaggWorkerConfig>,
    pub cache: Arc<RwLock<RefreshKeyCache>>,
    pub ctx: Arc<OxyProjectContext>,
}

#[async_trait]
impl TaskExecutor for PreaggTaskExecutor {
    async fn execute(&self, assignment: TaskAssignment) -> Result<ExecutingTask, String> {
        let TaskSpec::Custom { kind, .. } = &assignment.spec else {
            return Err(format!(
                "unexpected spec for PreaggTaskExecutor: {:?}",
                assignment.spec
            ));
        };
        if kind != "preagg_cycle" {
            return Err(format!("unknown preagg kind: {kind}"));
        }

        let (event_tx, event_rx) = mpsc::channel(256);
        let (outcome_tx, outcome_rx) = mpsc::channel(4);
        let cancel = CancellationToken::new();

        let config = Arc::clone(&self.config);
        let cache = Arc::clone(&self.cache);
        let ctx = Arc::clone(&self.ctx);
        let cancel_clone = cancel.clone();

        tokio::spawn(async move {
            run_preagg_task(config, cache, ctx, event_tx, outcome_tx, cancel_clone).await;
        });

        Ok(ExecutingTask {
            events: event_rx,
            outcomes: outcome_rx,
            cancel,
            answers: None,
        })
    }
}

// ── Main task function ────────────────────────────────────────────────────────

async fn run_preagg_task(
    config: Arc<PreaggWorkerConfig>,
    cache: Arc<RwLock<RefreshKeyCache>>,
    ctx: Arc<OxyProjectContext>,
    event_tx: mpsc::Sender<(String, serde_json::Value)>,
    outcome_tx: mpsc::Sender<TaskOutcome>,
    cancel: CancellationToken,
) {
    use tokio::task::JoinSet;

    // Evict stale entries for rollups that may no longer exist in views.
    // Use 2× the renewal_threshold as max_age so entries older than that
    // are certainly from prior cycles and won't be re-validated this tick.
    {
        let mut guard = cache.write().expect("preagg cache lock poisoned");
        guard.sweep(config.renewal_threshold * 2);
    }

    let views = match load_view_files(&config.workspace_path, config.database.as_deref()).await {
        Ok(v) => v,
        Err(e) => {
            let _ = outcome_tx
                .send(TaskOutcome::Failed(format!("load_view_files: {e}")))
                .await;
            return;
        }
    };

    let today = chrono::Utc::now().format("%Y%m%dT%H%M%S").to_string();

    struct StaleWork {
        view: airlayer::View,
        rollup: airlayer::preagg::RollupSpec,
        current_value: Option<String>,
        database_name: String,
    }

    let mut stale_work: Vec<StaleWork> = Vec::new();
    let mut total_rollups: usize = 0;
    let mut skipped_no_key: usize = 0;
    for (view, database_name) in &views {
        let rollups = airlayer::preagg::resolve_rollups(view);
        for rollup in rollups {
            let Some(rk) = rollup_refresh_key(&rollup, view) else {
                skipped_no_key += 1;
                let _ = event_tx
                    .send(
                        PreaggEvent::RollupSkippedNoRefreshKey {
                            view: view.name.clone(),
                            rollup: rollup.name.clone(),
                        }
                        .to_wire(),
                    )
                    .await;
                continue;
            };
            total_rollups += 1;

            let (current_value, is_stale, rk_err) =
                evaluate_refresh_key(rk, &rollup.hash, &config, &cache, &ctx, database_name).await;

            if let Some(err_msg) = rk_err {
                let _ = event_tx
                    .send(
                        PreaggEvent::RefreshKeyError {
                            rollup_hash: rollup.hash.clone(),
                            error: err_msg,
                        }
                        .to_wire(),
                    )
                    .await;
            } else if is_stale {
                stale_work.push(StaleWork {
                    view: view.clone(),
                    rollup,
                    current_value,
                    database_name: database_name.clone(),
                });
            } else {
                let _ = event_tx
                    .send(
                        PreaggEvent::RollupFresh {
                            view: view.name.clone(),
                            rollup: rollup.name.clone(),
                        }
                        .to_wire(),
                    )
                    .await;
            }
        }
    }

    if stale_work.is_empty() {
        let answer = if skipped_no_key > 0 {
            format!("all rollups are up to date ({skipped_no_key} skipped: no refresh_key)")
        } else {
            "all rollups are up to date".into()
        };
        let _ = outcome_tx
            .send(TaskOutcome::Done {
                answer,
                metadata: None,
            })
            .await;
        return;
    }

    let total = total_rollups;
    let mut set: JoinSet<(String, String, Result<(), String>)> = JoinSet::new();

    for work in stale_work {
        if cancel.is_cancelled() {
            break;
        }

        let rebuild_ctx = RebuildContext {
            schema: config.schema.clone(),
            workspace_path: config.workspace_path.clone(),
            database_name: work.database_name.clone(),
            renewal_threshold: config.renewal_threshold,
            manifest_write_lock: Arc::clone(&config.manifest_write_lock),
        };
        let ctx_clone = Arc::clone(&ctx);
        let cache_clone = Arc::clone(&cache);
        let event_tx_clone = event_tx.clone();
        let view_name = work.view.name.clone();
        let rollup_name = work.rollup.name.clone();
        let today_clone = today.clone();

        let _ = event_tx_clone
            .send(
                PreaggEvent::RollupStarted {
                    view: view_name.clone(),
                    rollup: rollup_name.clone(),
                }
                .to_wire(),
            )
            .await;

        set.spawn(async move {
            let result = rebuild_rollup(
                work.view,
                work.rollup,
                work.current_value,
                today_clone,
                ctx_clone,
                rebuild_ctx,
                cache_clone,
            )
            .await;
            (view_name, rollup_name, result)
        });
    }

    let mut succeeded = 0usize;
    let mut failed = 0usize;

    while let Some(join_result) = set.join_next().await {
        match join_result {
            Ok((view_name, rollup_name, Ok(()))) => {
                succeeded += 1;
                let _ = event_tx
                    .send(
                        PreaggEvent::RollupDone {
                            view: view_name,
                            rollup: rollup_name,
                        }
                        .to_wire(),
                    )
                    .await;
            }
            Ok((view_name, rollup_name, Err(e))) => {
                failed += 1;
                tracing::warn!(
                    view = %view_name,
                    rollup = %rollup_name,
                    error = %e,
                    "preagg rollup rebuild failed"
                );
                let _ = event_tx
                    .send(
                        PreaggEvent::RollupFailed {
                            view: view_name,
                            rollup: rollup_name,
                            error: e,
                        }
                        .to_wire(),
                    )
                    .await;
            }
            Err(join_err) => {
                failed += 1;
                tracing::warn!(error = %join_err, "preagg rollup task panicked");
            }
        }
    }

    let skipped_suffix = if skipped_no_key > 0 {
        format!(" ({skipped_no_key} skipped: no refresh_key)")
    } else {
        String::new()
    };
    let outcome = if failed == 0 {
        TaskOutcome::Done {
            answer: format!("rebuilt {succeeded} of {total} rollups{skipped_suffix}"),
            metadata: None,
        }
    } else {
        TaskOutcome::Failed(format!(
            "{failed} of {total} rollups failed{skipped_suffix}"
        ))
    };
    let _ = outcome_tx.send(outcome).await;
}

// ── Helpers (moved from preagg_worker.rs) ────────────────────────────────────

fn rollup_refresh_key<'a>(
    rollup: &'a airlayer::preagg::RollupSpec,
    view: &'a airlayer::View,
) -> Option<&'a airlayer::RefreshKey> {
    if let Some(ref preaggs) = view.pre_aggregations {
        for pa in preaggs {
            if pa.name == rollup.name {
                if let Some(ref k) = pa.refresh_key {
                    return Some(k);
                }
                break;
            }
        }
    }
    view.refresh_key.as_ref()
}

/// Dispatch to the appropriate refresh-key evaluator based on key kind.
///
/// Returns `(current_value, is_stale, error_msg)`.
async fn evaluate_refresh_key(
    rk: &airlayer::RefreshKey,
    rollup_hash: &str,
    config: &PreaggWorkerConfig,
    cache: &Arc<RwLock<RefreshKeyCache>>,
    ctx: &OxyProjectContext,
    database_name: &str,
) -> (Option<String>, bool, Option<String>) {
    match rk {
        airlayer::RefreshKey::Every(interval_str) => {
            let (value, is_stale) =
                eval_every_refresh_key(interval_str, rollup_hash, config, cache);
            (value, is_stale, None)
        }
        airlayer::RefreshKey::Sql(sql) => {
            eval_sql_refresh_key(sql, rollup_hash, config, cache, ctx, database_name).await
        }
    }
}

/// Evaluate an `Every`-interval refresh key.
///
/// Returns `(None, is_stale)`. `is_stale` is false if the in-memory cache or
/// the manifest confirms the rollup was built within the interval.
fn eval_every_refresh_key(
    interval_str: &str,
    rollup_hash: &str,
    config: &PreaggWorkerConfig,
    cache: &Arc<RwLock<RefreshKeyCache>>,
) -> (Option<String>, bool) {
    let Ok(interval) = airlayer::preagg::parse_interval(interval_str) else {
        // Unparsable interval → treat as always stale so operator notices.
        tracing::warn!(interval = %interval_str, rollup_hash, "preagg: unparsable Every interval");
        return (None, true);
    };

    // Layer 1: in-memory cache (survives heartbeats within the same process).
    {
        let guard = cache.read().expect("preagg cache lock poisoned");
        if guard.get(rollup_hash, interval).is_some() {
            return (None, false);
        }
    }

    // Layer 2: manifest's build_date (survives server restarts).
    // If the rollup was built within the interval, seed the cache and skip rebuild.
    let manifest_build_date = agentic_semantic::preagg::load_local_manifest(
        &oxy::state_dir::get_airlayer_cache_dir(&config.workspace_path),
    )
    .and_then(|m| {
        m.rollups
            .iter()
            .find(|r| r.rollup_hash == rollup_hash)
            .map(|r| r.build_date.clone())
    });

    if let Some(build_date_str) = manifest_build_date {
        if let Ok(built_at) =
            chrono::NaiveDateTime::parse_from_str(&build_date_str, "%Y-%m-%d %H:%M:%S")
        {
            let built_at_utc = built_at.and_utc();
            let age = chrono::Utc::now().signed_duration_since(built_at_utc);
            if age < chrono::Duration::from_std(interval).unwrap_or(chrono::Duration::zero()) {
                let mut guard = cache.write().expect("preagg cache lock poisoned");
                guard.insert(rollup_hash.to_string(), None);
                return (None, false);
            }
        }
    }

    (None, true)
}

/// Evaluate a SQL-based refresh key by running it against the warehouse.
///
/// Returns `(current_value, is_stale, error_msg)`. On connector/query error,
/// `error_msg` is `Some(...)` and the rollup is treated as fresh (not stale)
/// to avoid rebuild thrashing while the warehouse is unavailable.
async fn eval_sql_refresh_key(
    sql: &str,
    rollup_hash: &str,
    config: &PreaggWorkerConfig,
    cache: &Arc<RwLock<RefreshKeyCache>>,
    ctx: &OxyProjectContext,
    database_name: &str,
) -> (Option<String>, bool, Option<String>) {
    let connector = match ctx.get_connector(database_name).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("get_connector failed for {database_name}: {e}");
            return (None, false, Some(format!("get_connector failed: {e}")));
        }
    };

    let current = match connector.execute_query(sql, 1).await {
        Ok(result) => result
            .result
            .rows
            .first()
            .and_then(|r| r.0.first())
            .map(|cell| match cell {
                agentic_core::result::CellValue::Text(s) => s.clone(),
                agentic_core::result::CellValue::Number(n) => n.to_string(),
                agentic_core::result::CellValue::Null => String::new(),
            }),
        Err(e) => {
            tracing::warn!("refresh_key SQL evaluation failed: {e}");
            return (None, false, Some(format!("refresh_key SQL failed: {e}")));
        }
    };

    let last_value = agentic_semantic::preagg::load_local_manifest(
        &oxy::state_dir::get_airlayer_cache_dir(&config.workspace_path),
    )
    .and_then(|m| {
        m.rollups
            .iter()
            .find(|r| r.rollup_hash == rollup_hash)
            .and_then(|r| r.refresh_key_value.clone())
    });

    let is_stale = current.as_deref() != last_value.as_deref();
    (current, is_stale, None)
}

pub(super) async fn load_view_files(
    workspace_path: &Path,
    database_override: Option<&str>,
) -> Result<Vec<(airlayer::View, String)>, String> {
    let workspace_path = workspace_path.to_path_buf();
    let database_override = database_override.map(|s| s.to_string());
    tokio::task::spawn_blocking(move || {
        load_view_files_sync(&workspace_path, database_override.as_deref())
    })
    .await
    .map_err(|e| format!("load_view_files task panicked: {e}"))?
}

fn load_view_files_sync(
    workspace_path: &Path,
    database_override: Option<&str>,
) -> Result<Vec<(airlayer::View, String)>, String> {
    let mut views = Vec::new();
    let pattern = workspace_path
        .join("**/*.view.yml")
        .to_str()
        .ok_or("non-UTF8 workspace path")?
        .to_string();

    for entry in glob::glob(&pattern).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?;
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("read {} failed: {e}", path.display()))?;
        match serde_yaml::from_str::<airlayer::View>(&content) {
            Ok(view) => {
                let db = database_override
                    .map(|s| s.to_string())
                    .or_else(|| view.datasource.clone())
                    .unwrap_or_else(|| "default".to_string());
                views.push((view, db));
            }
            Err(e) => tracing::warn!("preagg: skip {}: parse error: {e}", path.display()),
        }
    }
    Ok(views)
}
