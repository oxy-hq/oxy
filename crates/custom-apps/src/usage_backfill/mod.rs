//! Copies a workspace's custom-app usage history from Oxy's Postgres into its
//! own Airhouse table `oxy_custom_app_usage`, so the Activity tab and the
//! weekly report can read it there ([`usage_reads`](crate::usage_reads)).
//!
//! Since W2.3b the usage drain appends every new page view and beacon event to
//! Airhouse as well as to Postgres, and [`record_event`](crate::tracking::record_event)
//! does the same for SDK events. Everything written before that, and anything
//! the best-effort append dropped meanwhile, exists only in Postgres. This pass
//! hands those rows to the same sink, through the same idempotent insert
//! ([`insert_sql`](crate::usage_airhouse::insert_sql)), so a row that is
//! already in Airhouse is skipped on its event id.
//!
//! # One row per workspace
//!
//! `custom_app_usage_backfill` records `started_at`, `finished_at`, how many
//! rows were handed over, and the position of the last one. `finished_at` is
//! the only thing the read side looks at: unset, the workspace keeps reading
//! Postgres.
//!
//! # Bounded batches, resumable
//!
//! Rows are read [`BATCH_ROWS`] at a time in a fixed order: every `view` row,
//! then every `event` row; within a kind by app, then time, then id. After each
//! batch's insert returned, the row's position moves to the batch's last row in
//! one `UPDATE`. A pass that fails, runs out of its time budget or loses its
//! process picks up after that position; the batch it died in is read again and
//! the idempotent insert skips what already landed.
//!
//! # A pass never reads Airhouse back
//!
//! Whether a row already exists is the insert's own check. This module never
//! counts what is in Airhouse, so `rows_copied` is progress through Postgres,
//! not the number of new Airhouse rows.
//!
//! # Running it again
//!
//! A finished workspace is left alone: [`run`] returns
//! [`Pass::AlreadyFinished`] without reading either table. Resetting
//! `finished_at` and the position by hand makes the next pass walk the whole
//! history again; the insert then writes nothing that is already there.

use std::time::{Duration, Instant};

use chrono::Utc;
use entity::custom_app_usage_backfill;
use sea_orm::{
    ConnectionTrait, DatabaseBackend, DatabaseConnection, DbErr, EntityTrait, FromQueryResult,
    Statement,
};
use uuid::Uuid;

use crate::usage_airhouse::{self as airhouse_sink, SinkError, UsageSink};

mod batch;
use batch::{Position, app_homes, event_batch, save, view_batch};

/// Most rows read from Postgres, and sent in one insert, at a time.
pub const BATCH_ROWS: u64 = 200;
/// Longest one batch's insert may take.
pub const BATCH_TIMEOUT: Duration = Duration::from_secs(30);

pub(super) const VIEW: &str = "view";
pub(super) const EVENT: &str = "event";

/// How a pass ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pass {
    /// The workspace has no Airhouse (or the integration is off). Nothing was
    /// recorded: it keeps reading Postgres, and a later pass looks again.
    NoAirhouse,
    /// The workspace was finished by an earlier pass. Nothing was read.
    AlreadyFinished,
    /// This pass reached the end of both tables and marked the workspace
    /// finished. `rows` is what this pass handed to the insert.
    Finished { rows: u64 },
    /// The time budget ran out first. The position is saved; the next pass
    /// carries on. `rows` is what this pass handed to the insert.
    Paused { rows: u64 },
}

#[derive(Debug, thiserror::Error)]
pub enum BackfillError {
    #[error("database: {0}")]
    Db(#[from] DbErr),
    #[error("airhouse: {0}")]
    Sink(String),
}

/// Run a pass over `workspace_id` on the process's sink, for at most `budget`
/// (checked between batches).
pub async fn run(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    budget: Duration,
) -> Result<Pass, BackfillError> {
    run_with(
        db,
        airhouse_sink::sink().as_ref(),
        workspace_id,
        budget,
        BATCH_ROWS,
    )
    .await
}

/// [`run`] on a given sink and batch size.
pub async fn run_with(
    db: &DatabaseConnection,
    sink: &dyn UsageSink,
    workspace_id: Uuid,
    budget: Duration,
    batch_rows: u64,
) -> Result<Pass, BackfillError> {
    if state(db, workspace_id)
        .await?
        .is_some_and(|row| row.finished_at.is_some())
    {
        return Ok(Pass::AlreadyFinished);
    }
    match sink.prepare(db, workspace_id).await {
        Ok(()) => {}
        Err(SinkError::NotConfigured) => return Ok(Pass::NoAirhouse),
        Err(SinkError::Failed(why)) => return Err(BackfillError::Sink(why)),
    }
    db.execute_raw(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "INSERT INTO custom_app_usage_backfill (workspace_id) VALUES ($1) \
         ON CONFLICT (workspace_id) DO NOTHING",
        [workspace_id.into()],
    ))
    .await?;
    let position = state(db, workspace_id)
        .await?
        .map(|row| Position::of(&row))
        .unwrap_or_default();

    let homes = app_homes(db, workspace_id).await?;
    let deadline = Instant::now() + budget;
    let mut handed = 0u64;
    let mut at = position;
    for kind in [VIEW, EVENT] {
        // A position in `event` means every `view` row is done.
        if kind == VIEW && at.kind == Some(EVENT) {
            continue;
        }
        if at.kind != Some(kind) {
            at = Position::start_of(kind);
        }
        loop {
            if Instant::now() >= deadline {
                return Ok(Pass::Paused { rows: handed });
            }
            let recorded_at = Utc::now();
            let (facts, last) = match kind {
                VIEW => view_batch(db, &homes, &at, batch_rows, recorded_at).await?,
                _ => event_batch(db, &homes, &at, batch_rows, recorded_at).await?,
            };
            let Some(last) = last else { break };
            tokio::time::timeout(BATCH_TIMEOUT, sink.append(db, workspace_id, &facts))
                .await
                .map_err(|_| {
                    BackfillError::Sink(format!(
                        "an insert of {} rows timed out after {}s",
                        facts.len(),
                        BATCH_TIMEOUT.as_secs()
                    ))
                })?
                .map_err(|e| match e {
                    SinkError::NotConfigured => {
                        BackfillError::Sink("the workspace's Airhouse went away".to_string())
                    }
                    SinkError::Failed(why) => BackfillError::Sink(why),
                })?;
            handed += facts.len() as u64;
            at = last;
            save(db, workspace_id, facts.len() as u64, &at).await?;
        }
        // This kind is done. The next starts from its beginning; a pass cut
        // short right here resumes at it rather than re-reading this one.
        if kind == VIEW {
            at = Position::start_of(EVENT);
            save(db, workspace_id, 0, &at).await?;
        }
    }
    db.execute_raw(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "UPDATE custom_app_usage_backfill \
         SET finished_at = now(), updated_at = now() WHERE workspace_id = $1",
        [workspace_id.into()],
    ))
    .await?;
    Ok(Pass::Finished { rows: handed })
}

/// The workspace's row, if a pass ever began.
pub async fn state(
    db: &DatabaseConnection,
    workspace_id: Uuid,
) -> Result<Option<custom_app_usage_backfill::Model>, DbErr> {
    custom_app_usage_backfill::Entity::find_by_id(workspace_id)
        .one(db)
        .await
}

/// Workspaces a pass should be queued for: they have at least one app and an
/// active Airhouse tenant, and no row says finished. At most `limit`, the one
/// attempted longest ago first (never attempted before any), so a workspace
/// whose passes keep failing is tried again after the others, not instead of
/// them. A queued pass counts as an attempt ([`note_attempt`]).
pub async fn unfinished_workspaces(
    db: &DatabaseConnection,
    limit: u64,
) -> Result<Vec<Uuid>, DbErr> {
    #[derive(FromQueryResult)]
    struct Row {
        workspace_id: Uuid,
    }
    let rows = Row::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "SELECT t.workspace_id FROM airhouse_tenants t \
         LEFT JOIN custom_app_usage_backfill b ON b.workspace_id = t.workspace_id \
         WHERE t.status = 'active' \
           AND b.finished_at IS NULL \
           AND EXISTS (SELECT 1 FROM apps a WHERE a.project_id = t.workspace_id) \
         ORDER BY b.updated_at NULLS FIRST, t.created_at, t.workspace_id LIMIT $1",
        [(limit as i64).into()],
    ))
    .all(db)
    .await?;
    Ok(rows.into_iter().map(|r| r.workspace_id).collect())
}

/// Record that a pass was queued for `workspace_id`: creates its row, or moves
/// `updated_at`, which is what [`unfinished_workspaces`] orders by.
pub async fn note_attempt(db: &DatabaseConnection, workspace_id: Uuid) -> Result<(), DbErr> {
    db.execute_raw(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "INSERT INTO custom_app_usage_backfill (workspace_id) VALUES ($1) \
         ON CONFLICT (workspace_id) DO UPDATE SET updated_at = now()",
        [workspace_id.into()],
    ))
    .await?;
    Ok(())
}

/// Apps whose workspace has no Airhouse (no active tenant): their usage can
/// only live in Postgres, so while this is above zero the Postgres usage writes
/// cannot stop. Quoted verbatim in `internal-docs/data-placement.md`, which a
/// test holds to this text.
pub const APPS_WITHOUT_AIRHOUSE_SQL: &str = "\
SELECT count(*) AS apps_without_airhouse
FROM apps a
WHERE NOT EXISTS (
    SELECT 1 FROM airhouse_tenants t
    WHERE t.workspace_id = a.project_id AND t.status = 'active'
)";

/// Apps whose workspace has an Airhouse that has not finished copying its
/// history: the second thing that has to reach zero. Quoted verbatim in
/// `internal-docs/data-placement.md`.
pub const APPS_NOT_YET_COPIED_SQL: &str = "\
SELECT count(*) AS apps_not_yet_copied
FROM apps a
WHERE EXISTS (
    SELECT 1 FROM airhouse_tenants t
    WHERE t.workspace_id = a.project_id AND t.status = 'active'
)
AND NOT EXISTS (
    SELECT 1 FROM custom_app_usage_backfill b
    WHERE b.workspace_id = a.project_id AND b.finished_at IS NOT NULL
)";
