//! `oxy cameras` CLI — operator-side fleet ops.
//!
//! V1 surface is small on purpose:
//!
//! - `oxy cameras sweep-logs` — manual / cron-driven retention
//!   sweep of `oxy_cam_device_logs`. Same code path as the
//!   in-process loop spawned by `oxy serve` but with deterministic
//!   timing, which is what an operator wants from a system cron.
//!
//! Future surfaces (claim-by-id, force-revoke, etc.) belong here
//! when they're needed. Keeping the umbrella subcommand makes them
//! easy to grow into without churning the top-level CLI.

use clap::Parser;
use oxy::database::client::establish_connection;
use oxy::theme::StyledText;
use oxy_cameras::service::log_retention::{Policy, sweep_all, sweep_workspace};
use oxy_shared::errors::OxyError;
use uuid::Uuid;

#[derive(Parser, Debug)]
pub struct CamerasArgs {
    #[clap(subcommand)]
    pub command: CamerasCommand,
}

#[derive(Parser, Debug)]
pub enum CamerasCommand {
    /// Sweep stale device log rows per the retention policy.
    ///
    /// Defaults: info/debug rows older than 7 days, warn/error
    /// older than 30 days. Override via
    /// `OXY_CAMERA_LOG_RETENTION_INFO_DAYS` and `..._WARN_DAYS`.
    SweepLogs(SweepLogsArgs),
}

#[derive(Parser, Debug)]
pub struct SweepLogsArgs {
    /// Restrict to a single workspace. When omitted, sweep every
    /// workspace that owns at least one edge box.
    #[clap(long)]
    pub workspace: Option<Uuid>,
}

pub async fn handle_cameras_command(args: CamerasArgs) -> Result<(), OxyError> {
    match args.command {
        CamerasCommand::SweepLogs(a) => cmd_sweep_logs(a).await,
    }
}

async fn cmd_sweep_logs(args: SweepLogsArgs) -> Result<(), OxyError> {
    let policy = Policy::from_env();
    println!(
        "{} retention: info/debug {}d, warn+ {}d",
        "cameras sweep-logs".info(),
        policy.info_days,
        policy.warn_days
    );

    let db = establish_connection().await?;

    let stats = match args.workspace {
        Some(wid) => sweep_workspace(wid, policy)
            .await
            .map_err(|e| OxyError::RuntimeError(format!("sweep workspace {wid}: {e}")))?,
        None => sweep_all(&db, policy)
            .await
            .map_err(|e| OxyError::RuntimeError(format!("sweep all: {e}")))?,
    };

    println!(
        "{} swept: {} info/debug, {} warn/error",
        "✓".success(),
        stats.info_deleted,
        stats.warn_deleted
    );
    Ok(())
}
