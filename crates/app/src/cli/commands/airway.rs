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
use agentic_pipeline::{
    AIRWAY_SOURCE_TYPE, AirwayMigrator, AnalyticsMigrator, WorkflowMigrator, airway_event_handler,
};
use agentic_runtime::crud;
use agentic_runtime::event_registry::EventRegistry;
use agentic_runtime::migration::RuntimeMigrator;
use agentic_runtime::state::RuntimeState;
use clap::Parser;
use migration::MigratorTrait;
use oxy::adapters::workspace::builder::WorkspaceBuilder;
use oxy::config::resolve_local_workspace_path;
use oxy::database::client::establish_connection;
use oxy::theme::StyledText;
use oxy_shared::errors::OxyError;
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
}

#[derive(Parser, Debug)]
pub struct AirwayRunArgs {
    /// Path to the `.airway.yml`, relative to the workspace root.
    pub pipeline_ref: String,
    /// Emit one JSON object per event instead of pretty output.
    #[clap(long)]
    pub json: bool,
}

pub async fn handle_airway_command(args: AirwayArgs) -> Result<(), OxyError> {
    match args.command {
        AirwayCommand::Run(a) => cmd_run(a).await,
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
    WorkflowMigrator::up(&db, None)
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

    let request = StartAirwayRequest {
        pipeline_ref: args.pipeline_ref.clone(),
        variables: None,
        thread_id: None,
        resources: Vec::new(),
        schedule_id: None,
        trigger: None,
        logical_date: None,
        retry_of: None,
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
