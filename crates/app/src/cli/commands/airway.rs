//! `oxy airway` CLI — run airway ELT pipelines.
//!
//! Shares the same domain path as the HTTP `/agentic-airway/runs`
//! endpoint via `agentic_pipeline::airway_run`: seed the queue, drive
//! the coordinator + worker, stream events to the terminal.

use std::sync::Arc;
use std::time::Duration;

use agentic_pipeline::airway_run::{
    AirwayRunError, StartAirwayRequest, spawn_airway_run_drive, start_airway_run,
};
use agentic_pipeline::backfill::{
    ChunkDisposition, ChunkGranularity, ChunkProgress, drive_backfill_range,
    find_or_create_backfill_range, load_coverage,
};
use agentic_pipeline::{
    AIRWAY_SOURCE_TYPE, AirwayMigrator, AnalyticsMigrator, AutomationMigrator, airway_event_handler,
};
use agentic_runtime::crud;
use agentic_runtime::event_registry::EventRegistry;
use agentic_runtime::migration::RuntimeMigrator;
use agentic_runtime::state::RuntimeState;
use chrono::{DateTime, Utc};
use clap::Parser;
use migration::MigratorTrait;
use oxy::adapters::workspace::builder::WorkspaceBuilder;
use oxy::config::resolve_local_workspace_path;
use oxy::database::client::establish_connection;
use oxy::theme::StyledText;
use oxy_shared::errors::OxyError;
use sea_orm::DatabaseConnection;
use serde_json::{Value, json};
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

#[derive(Parser, Debug)]
pub struct AirwayArgs {
    #[clap(subcommand)]
    pub command: AirwayCommand,
}

#[derive(Parser, Debug)]
pub enum AirwayCommand {
    /// Run an airway pipeline defined by a `.airway.yml` file.
    Run(AirwayRunArgs),
    /// Chunked, resumable backfill of a pipeline over a date range. Splits
    /// `[from, to)` into period chunks, runs each as a bounded backfill, and
    /// records every chunk in `backfill_checkpoints` — so a crash/cancel resumes
    /// by skipping `done` chunks and re-running failed ones.
    Backfill(AirwayBackfillArgs),
    /// Show backfill coverage for a pipeline (done / failed / pending chunks)
    /// from `backfill_checkpoints` — i.e. what period is loaded vs missing.
    Coverage(AirwayCoverageArgs),
}

#[derive(Parser, Debug)]
pub struct AirwayRunArgs {
    /// Path to the `.airway.yml`, relative to the workspace root.
    pub pipeline_ref: String,
    /// Emit one JSON object per event instead of pretty output.
    #[clap(long)]
    pub json: bool,
}

#[derive(Parser, Debug)]
pub struct AirwayBackfillArgs {
    /// Path to the `.airway.yml`, relative to the workspace root.
    pub pipeline_ref: String,
    /// Inclusive window start (RFC3339, e.g. `2020-01-01T00:00:00Z`).
    #[clap(long)]
    pub from: String,
    /// Exclusive window end (RFC3339).
    #[clap(long)]
    pub to: String,
    /// Chunk size: `month` (default), `week`, or `day`.
    #[clap(long, default_value = "month")]
    pub granularity: String,
    /// Max chunks to run concurrently. Each chunk is a full airway run writing
    /// the same table, and DuckLake serializes catalog commits per table — so a
    /// high value trades parallel extract for commit conflict-retries (safe:
    /// resumable). ≈4 is a good default; 1 is fully sequential.
    #[clap(long, default_value = "4")]
    pub concurrency: usize,
    /// Emit one JSON object per event instead of pretty output.
    #[clap(long)]
    pub json: bool,
}

#[derive(Parser, Debug)]
pub struct AirwayCoverageArgs {
    /// Path to the `.airway.yml`, relative to the workspace root.
    pub pipeline_ref: String,
    /// Emit JSON instead of a table.
    #[clap(long)]
    pub json: bool,
}

pub async fn handle_airway_command(args: AirwayArgs) -> Result<(), OxyError> {
    match args.command {
        AirwayCommand::Run(a) => cmd_run(a).await,
        AirwayCommand::Backfill(a) => cmd_backfill(a).await,
        AirwayCommand::Coverage(a) => cmd_coverage(a).await,
    }
}

async fn connect_db() -> Result<sea_orm::DatabaseConnection, OxyError> {
    let db = establish_connection().await?;
    migration::Migrator::up(&db, None)
        .await
        .map_err(|e| OxyError::RuntimeError(format!("migrations: {e}")))?;
    RuntimeMigrator::up(&db, None)
        .await
        .map_err(|e| OxyError::RuntimeError(format!("runtime migrations: {e}")))?;
    AnalyticsMigrator::up(&db, None)
        .await
        .map_err(|e| OxyError::RuntimeError(format!("analytics migrations: {e}")))?;
    AutomationMigrator::up(&db, None)
        .await
        .map_err(|e| OxyError::RuntimeError(format!("workflow migrations: {e}")))?;
    AirwayMigrator::up(&db, None)
        .await
        .map_err(|e| OxyError::RuntimeError(format!("airway migrations: {e}")))?;
    Ok(db)
}

fn build_event_registry() -> EventRegistry {
    let mut registry = EventRegistry::new();
    registry.register(AIRWAY_SOURCE_TYPE, airway_event_handler());
    registry
}

fn is_terminal(status: Option<&str>) -> bool {
    matches!(
        status,
        Some("done") | Some("failed") | Some("cancelled") | Some("timed_out")
    )
}

/// Build the airway `variables` map from the process environment,
/// filtered to only the variables the rendered YAML actually references
/// (via a minijinja undeclared-vars scan).
///
/// Why filter: the variables we return get persisted in
/// `agentic_task_queue` AND the run-metadata JSONB. Passing the whole
/// `std::env::vars()` would bleed unrelated secrets into both stores
/// indefinitely (AWS_SECRET_ACCESS_KEY, OPENAI_API_KEY, OXY_DATABASE_URL,
/// the entire *_var surface), bypassing the *_var → secret-manager →
/// strip flow the rest of agentic-pipeline maintains. Scanning the YAML
/// for the variables it actually uses keeps the env-pass-through useful
/// (`{{ YELP_API_KEY }}` still works) without bleeding everything else
/// into storage.
///
/// On any failure (file not readable, template not compilable), we
/// return an empty map. That surfaces as a clear "airway variable
/// substitution failed: undefined variable" at render time if the YAML
/// references something, instead of silently regressing to the old
/// "pass everything" behaviour.
fn build_env_vars_for_yaml(
    project_path: &std::path::Path,
    pipeline_ref: &str,
) -> serde_json::Map<String, serde_json::Value> {
    let yaml_path = project_path.join(pipeline_ref);
    let yaml = match std::fs::read_to_string(&yaml_path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                error = %e,
                path = %yaml_path.display(),
                "env-var scan: could not read pipeline YAML; passing no env vars. \
                 If the pipeline references `{{ FOO }}` you'll see a substitution \
                 error at render time."
            );
            return serde_json::Map::new();
        }
    };

    // `track_nested = true` returns root variable names for nested accesses
    // (`{{ config.api.key }}` → returns `config`). For env vars that's
    // exactly what we want — process env is flat-keyed.
    let env = minijinja::Environment::new();
    let referenced: std::collections::HashSet<String> =
        match env.template_from_named_str("pipeline", &yaml) {
            Ok(tpl) => tpl.undeclared_variables(true).into_iter().collect(),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    path = %yaml_path.display(),
                    "env-var scan: minijinja could not parse pipeline YAML as a \
                     template; passing no env vars."
                );
                return serde_json::Map::new();
            }
        };

    std::env::vars()
        .filter(|(k, _)| referenced.contains(k))
        .map(|(k, v)| (k, serde_json::Value::String(v)))
        .collect()
}

async fn cmd_run(args: AirwayRunArgs) -> Result<(), OxyError> {
    let db = connect_db().await?;
    let project_path = resolve_local_workspace_path()?;
    let workspace_manager = WorkspaceBuilder::new(Uuid::nil())
        .with_workspace_path(&project_path)
        .await?
        .with_runs_manager(oxy::adapters::runs::RunsManager::noop())
        .build()
        .await?;

    let project_ctx = Arc::new(crate::agentic_wiring::OxyProjectContext::new(
        workspace_manager,
    ));
    let platform: Arc<dyn agentic_pipeline::platform::PlatformContext> = project_ctx.clone();
    let workspace: Arc<dyn agentic_pipeline::WorkflowWorkspaceContext> = project_ctx;

    // Surface process env vars into the airway minijinja context so
    // pipeline YAMLs can reference secrets with `{{ MY_API_KEY }}`. The
    // CLI already loaded `.env` via `dotenv::from_path` higher up the
    // binary, so this picks up project-local secrets too without
    // re-implementing dotenv parsing. The HTTP path receives variables
    // from the request body and is unaffected.
    //
    // ⚠ Filter to only the variables the YAML actually references via
    // minijinja's undeclared-vars scan. Dumping the whole process env
    // unfiltered would persist unrelated secrets (AWS_SECRET_ACCESS_KEY,
    // OPENAI_API_KEY, OXY_DATABASE_URL, …) in `agentic_task_queue` and
    // run-metadata JSONB — both indefinitely retained at rest, both
    // bypassing the `*_var` → secret-manager → strip flow the rest of
    // the pipeline maintains. Scanning the rendered YAML keeps the
    // env-pass-through useful (`{{ YELP_API_KEY }}` still works) without
    // bleeding everything else into storage.
    let env_vars: serde_json::Map<String, serde_json::Value> =
        build_env_vars_for_yaml(&project_path, &args.pipeline_ref);
    let request = StartAirwayRequest {
        pipeline_ref: args.pipeline_ref.clone(),
        variables: Some(serde_json::Value::Object(env_vars)),
        thread_id: None,
        resources: Vec::new(),
        schedule_id: None,
        trigger: None,
        logical_date: None,
        retry_of: None,
        backfill_from: None,
        backfill_to: None,
    };

    // Single-process CLI: a co-located scoped coordinator drives this run.
    // Local workspace == Uuid::nil() (LOCAL_WORKSPACE_ID).
    let run_id = match start_airway_run(
        &db,
        workspace.as_ref(),
        request,
        agentic_pipeline::TaskScope::Scoped,
        Uuid::nil(),
    )
    .await
    {
        Ok(id) => id,
        Err(AirwayRunError::InvalidInput(msg)) | Err(AirwayRunError::Io(msg)) => {
            return Err(OxyError::ConfigurationError(msg));
        }
        Err(AirwayRunError::Airway(e)) => {
            return Err(OxyError::ConfigurationError(format!("airway spec: {e}")));
        }
        Err(e) => return Err(OxyError::RuntimeError(format!("start: {e}"))),
    };

    if !args.json {
        println!("{}", format!("Airway run started: {run_id}").info());
    }

    let state = Arc::new(RuntimeState::new());
    let (_answer_tx, _answer_rx) = mpsc::channel::<String>(1);
    let (cancel_tx, cancel_rx) = watch::channel(false);
    // Airway has no HITL; only the cancel side of the pair is wired.
    state.register(&run_id, mpsc::channel::<String>(1).0, cancel_tx);

    spawn_airway_run_drive(
        db.clone(),
        Arc::clone(&state),
        run_id.clone(),
        platform,
        cancel_rx,
        // One-shot CLI: no long-lived LISTEN/NOTIFY router needed; the
        // worker's backstop poll claims the single queued task.
        Arc::new(agentic_runtime::router::NoopTaskRouter),
    );

    let registry = build_event_registry();
    let mut processor = registry.stream_processor(AIRWAY_SOURCE_TYPE);
    let mut last_seq: i64 = -1;

    loop {
        tokio::time::sleep(Duration::from_millis(50)).await;

        let rows = crud::get_events_after(&db, &run_id, last_seq)
            .await
            .unwrap_or_default();
        for row in rows {
            last_seq = row.seq;
            for (event_type, payload) in processor.process(&row.event_type, &row.payload) {
                emit(args.json, row.seq, &event_type, &payload);
            }
        }

        // Termination is keyed off the run row's task_status rather than
        // a join handle — `spawn_airway_run_drive` owns its tasks
        // internally and doesn't hand one back.
        let terminal = match crud::get_run(&db, &run_id).await {
            Ok(Some(run)) => is_terminal(run.task_status.as_deref()),
            Ok(None) => true,
            Err(_) => false,
        };
        if terminal {
            // Final sweep so the last batch of events isn't dropped.
            let rows = crud::get_events_after(&db, &run_id, last_seq)
                .await
                .unwrap_or_default();
            for row in rows {
                for (event_type, payload) in processor.process(&row.event_type, &row.payload) {
                    emit(args.json, row.seq, &event_type, &payload);
                }
            }
            break;
        }
    }

    Ok(())
}

// ─── chunked backfill (resumable) ───────────────────────────────────────────
//
// The orchestration (chunk enumeration, per-chunk windowed runs, checkpointing,
// coverage rollup) lives in `agentic_pipeline::backfill` so the HTTP
// `/agentic-airway/chunked-backfill` + `/coverage` handlers share it. The CLI
// here is a thin wrapper: build the local context, drive, print.

fn parse_rfc3339(s: &str, field: &str) -> Result<DateTime<Utc>, OxyError> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| OxyError::ConfigurationError(format!("invalid --{field} `{s}`: {e}")))
}

/// Set up the same db + platform/workspace context `cmd_run` uses, plus the
/// pipeline's env-var variables, so a chunk run is identical to a normal run.
async fn airway_context(
    pipeline_ref: &str,
) -> Result<
    (
        DatabaseConnection,
        Arc<dyn agentic_pipeline::platform::PlatformContext>,
        Arc<dyn agentic_pipeline::WorkflowWorkspaceContext>,
        Value,
    ),
    OxyError,
> {
    let db = connect_db().await?;
    let project_path = resolve_local_workspace_path()?;
    let workspace_manager = WorkspaceBuilder::new(Uuid::nil())
        .with_workspace_path(&project_path)
        .await?
        .with_runs_manager(oxy::adapters::runs::RunsManager::noop())
        .build()
        .await?;
    let project_ctx = Arc::new(crate::agentic_wiring::OxyProjectContext::new(
        workspace_manager,
    ));
    let platform: Arc<dyn agentic_pipeline::platform::PlatformContext> = project_ctx.clone();
    let workspace: Arc<dyn agentic_pipeline::WorkflowWorkspaceContext> = project_ctx;
    let variables = Value::Object(build_env_vars_for_yaml(&project_path, pipeline_ref));
    Ok((db, platform, workspace, variables))
}

async fn cmd_backfill(args: AirwayBackfillArgs) -> Result<(), OxyError> {
    let granularity = ChunkGranularity::parse(&args.granularity).ok_or_else(|| {
        OxyError::ConfigurationError(format!(
            "invalid --granularity `{}` (expected month|week|day)",
            args.granularity
        ))
    })?;
    let from = parse_rfc3339(&args.from, "from")?;
    let to = parse_rfc3339(&args.to, "to")?;
    if from >= to {
        return Err(OxyError::ConfigurationError(
            "empty window — --from must be before --to".to_string(),
        ));
    }

    let (db, platform, _workspace, variables) = airway_context(&args.pipeline_ref).await?;
    if !args.json {
        println!(
            "{}",
            format!(
                "Backfill {} ({} chunks)",
                args.pipeline_ref, args.granularity
            )
            .info()
        );
    }

    // Single-tenant local: LOCAL_WORKSPACE_ID (Uuid::nil()), no initiating user.
    // find-or-create so re-running the same window resumes that range (drives
    // only its not-`done` chunks) instead of spawning a duplicate.
    let range_id = find_or_create_backfill_range(
        &db,
        Uuid::nil(),
        &args.pipeline_ref,
        from,
        to,
        granularity,
        args.concurrency as i32,
        None,
    )
    .await
    .map_err(|e| OxyError::RuntimeError(format!("resolve backfill range: {e}")))?;

    let json = args.json;
    let summary = drive_backfill_range(
        &db,
        platform,
        range_id,
        Some(variables),
        |p: ChunkProgress| {
            if json {
                return;
            }
            match p.disposition {
                ChunkDisposition::Resumed => {
                    println!("  {}", format!("↪ {} (already done)", p.label).secondary())
                }
                ChunkDisposition::Done => println!("  {}", format!("✓ {}", p.label).success()),
                ChunkDisposition::Degraded | ChunkDisposition::Failed => {
                    let note = p.note.unwrap_or_default();
                    println!("  {}", format!("✗ {} ({note})", p.label).error());
                }
            }
        },
    )
    .await
    .map_err(|e| OxyError::RuntimeError(format!("chunked backfill: {e}")))?;

    println!(
        "{}",
        format!(
            "done={} resumed={} degraded={} failed={}",
            summary.done, summary.resumed, summary.degraded, summary.failed
        )
        .info()
    );
    if summary.degraded + summary.failed > 0 {
        println!(
            "{}",
            "re-run the same command to retry failed / partial chunks.".secondary()
        );
    }
    Ok(())
}

async fn cmd_coverage(args: AirwayCoverageArgs) -> Result<(), OxyError> {
    let db = connect_db().await?;
    // Single-tenant local: LOCAL_WORKSPACE_ID (Uuid::nil()), matching the
    // `airway_context` platform that cmd_backfill drives with.
    let report = load_coverage(&db, Uuid::nil(), &args.pipeline_ref)
        .await
        .map_err(|e| OxyError::RuntimeError(format!("coverage: {e}")))?;

    if args.json {
        println!("{}", json!(report));
        return Ok(());
    }
    if report.chunks.is_empty() {
        println!("no backfill checkpoints for `{}`", args.pipeline_ref);
        return Ok(());
    }
    println!(
        "{}",
        format!(
            "Coverage {} — {}/{} chunks done",
            args.pipeline_ref, report.summary.done, report.summary.total
        )
        .info()
    );
    if let (Some(f), Some(t)) = (report.summary.loaded_from, report.summary.loaded_to) {
        println!("  loaded range: {} → {}", f.date_naive(), t.date_naive());
    }
    let missing: Vec<_> = report
        .chunks
        .iter()
        .filter(|r| r.status != "done")
        .collect();
    if missing.is_empty() {
        println!("  {}", "no missing periods.".success());
    } else {
        println!("  {} period(s) not done:", missing.len());
        for r in missing {
            let note = r
                .error
                .as_deref()
                .map(|e| format!(": {e}"))
                .unwrap_or_default();
            let mark = match r.status.as_str() {
                "failed" | "cancelled" | "timed_out" => "✗",
                "completed_with_errors" => "⚠",
                _ => "•", // pending / running
            };
            println!(
                "    {mark} {} → {} [{}] (attempts {}){note}",
                r.period_start.date_naive(),
                r.period_end.date_naive(),
                r.status,
                r.attempts,
            );
        }
    }
    Ok(())
}

fn emit(as_json: bool, seq: i64, event_type: &str, payload: &Value) {
    if as_json {
        println!(
            "{}",
            json!({ "seq": seq, "event_type": event_type, "payload": payload })
        );
        return;
    }
    match event_type {
        "load_started" => println!("{}", "▶ load started".info()),
        "extract_completed" => {
            let table = payload.get("table").and_then(Value::as_str).unwrap_or("?");
            let rows = payload
                .get("rows_extracted")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            println!("  extracted {table} ({rows} rows)");
        }
        "normalize_completed" => {
            let table = payload.get("table").and_then(Value::as_str).unwrap_or("?");
            println!("  normalized {table}");
        }
        "destination_load_started" => println!("  loading into destination…"),
        "load_completed" => {
            let ms = payload
                .get("duration_ms")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            println!("{}", format!("✓ load completed ({ms} ms)").success());
        }
        "schema_evolved" => println!("{}", "• schema evolved".secondary()),
        "pipeline_error" => {
            let err = payload.get("error").and_then(Value::as_str).unwrap_or("?");
            println!("{}", format!("✗ pipeline error: {err}").error());
        }
        "cancelled" => println!("{}", "■ cancelled".secondary()),
        other => println!("  {other}: {payload}"),
    }
}
