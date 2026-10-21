//! The watcher table, `app_environment_tails`: which `(app, environment)`
//! pairs someone is tailing, as leases a stream renews while it lives.
//!
//! The stream's side is [`Watch`]: [`Watch::register`] writes the row, a task
//! renews it every [`RENEW_EVERY`], and dropping the `Watch` (the stream
//! ended, the client went) deletes it. A process that dies mid-stream leaves
//! a row that stops counting at `expires_at` — [`LEASE`] after its last
//! renewal — and the next registration anywhere sweeps it.
//!
//! The producers' side is [`watched_pairs`], read through [`super::gate`] at
//! most once a second per process. Lease times are the database's clock on
//! both sides, so no two replicas' clocks are compared.

use std::collections::HashSet;
use std::time::Duration;

use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, DbErr, Statement};
use sentry::SentryFutureExt;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// How long a row counts after its last renewal.
pub const LEASE: Duration = Duration::from_secs(30);

/// How often a live stream renews its row: three renewals per lease, so one
/// slow or failed renewal does not unwatch a stream that is still open.
pub const RENEW_EVERY: Duration = Duration::from_secs(10);

/// A row this long past its lease is deleted by the next registration.
const SWEEP_AFTER: &str = "1 hour";

fn statement(sql: &str, values: Vec<sea_orm::Value>) -> Statement {
    Statement::from_sql_and_values(DatabaseBackend::Postgres, sql, values)
}

fn lease_secs() -> sea_orm::Value {
    (LEASE.as_secs() as i64).into()
}

/// Write a lease for `environment` of `app_id`, and sweep leases long dead.
async fn insert(db: &impl ConnectionTrait, app_id: Uuid, environment: &str) -> Result<Uuid, DbErr> {
    let id = Uuid::new_v4();
    db.execute_raw(statement(
        "INSERT INTO app_environment_tails (id, app_id, environment, expires_at) \
         VALUES ($1, $2, $3, now() + make_interval(secs => $4))",
        vec![id.into(), app_id.into(), environment.into(), lease_secs()],
    ))
    .await?;
    // Best effort: a stream whose process died never deleted its row. It
    // already stopped counting at its lease; this only keeps the table small.
    let sweep = format!(
        "DELETE FROM app_environment_tails WHERE expires_at < now() - interval '{SWEEP_AFTER}'"
    );
    if let Err(e) = db.execute_unprepared(&sweep).await {
        tracing::debug!(error = %e, "tail: sweeping dead leases failed");
    }
    Ok(id)
}

/// Push a lease's expiry out by [`LEASE`]. `false` when the row is gone.
pub async fn renew(db: &impl ConnectionTrait, id: Uuid) -> Result<bool, DbErr> {
    let done = db
        .execute_raw(statement(
            "UPDATE app_environment_tails \
             SET expires_at = now() + make_interval(secs => $2) WHERE id = $1",
            vec![id.into(), lease_secs()],
        ))
        .await?;
    Ok(done.rows_affected() > 0)
}

/// Delete a lease.
pub async fn release(db: &impl ConnectionTrait, id: Uuid) -> Result<(), DbErr> {
    db.execute_raw(statement(
        "DELETE FROM app_environment_tails WHERE id = $1",
        vec![id.into()],
    ))
    .await?;
    Ok(())
}

/// Every `(app, environment)` with a live lease.
pub async fn watched_pairs(db: &impl ConnectionTrait) -> Result<HashSet<(Uuid, String)>, DbErr> {
    let rows = db
        .query_all_raw(statement(
            "SELECT DISTINCT app_id, environment FROM app_environment_tails \
             WHERE expires_at > now()",
            Vec::new(),
        ))
        .await?;
    rows.iter()
        .map(|row| -> Result<(Uuid, String), DbErr> {
            Ok((row.try_get("", "app_id")?, row.try_get("", "environment")?))
        })
        .collect()
}

/// One stream's lease, held for as long as the stream lives.
pub struct Watch {
    id: Uuid,
    stop: CancellationToken,
}

impl Watch {
    /// Register a lease for `environment` of `app_id` and start renewing it.
    pub async fn register(
        db: &DatabaseConnection,
        app_id: Uuid,
        environment: &str,
    ) -> Result<Self, DbErr> {
        let id = insert(db, app_id, environment).await?;
        let stop = CancellationToken::new();
        tokio::spawn(
            hold(db.clone(), id, stop.clone())
                // The renewals belong to the request that opened the stream.
                .bind_hub(sentry::Hub::current()),
        );
        Ok(Self { id, stop })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

/// Renew `id` every [`RENEW_EVERY`] until `stop`, then delete it.
async fn hold(db: DatabaseConnection, id: Uuid, stop: CancellationToken) {
    let mut every = tokio::time::interval(RENEW_EVERY);
    every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // The first tick is immediate, and the row was written just now.
    every.tick().await;
    loop {
        tokio::select! {
            _ = stop.cancelled() => break,
            _ = every.tick() => match renew(&db, id).await {
                Ok(true) => {}
                // Swept, or deleted by hand: write it again would need the
                // pair, and the stream is the one place that holds it. The
                // stream goes on; producers stop sending until it reopens.
                Ok(false) => tracing::warn!(%id, "tail: lease row is gone; producers will stop sending"),
                Err(e) => tracing::warn!(%id, error = %e, "tail: lease renewal failed; retrying"),
            },
        }
    }
    if let Err(e) = release(&db, id).await {
        tracing::debug!(%id, error = %e, "tail: lease release failed; it expires on its own");
    }
}
