//! Periodic background job that marks long-silent edge boxes as
//! `offline`.
//!
//! The auth middleware flips `status=active` and updates `last_seen_at`
//! on every `/control/*` call from a box. There is no inverse signal
//! today — a box that loses network or crashes silently stays
//! `active` in Oxy forever, which is misleading for operators looking
//! at the dashboard.
//!
//! This loop closes that gap by scanning every `STALE_TICK_INTERVAL`
//! for rows where `last_seen_at < now - STALE_THRESHOLD` and the
//! status isn't already `offline`, and bulk-flipping them. The next
//! successful auth call from the box flips it right back to
//! `active` (see `auth::middleware::require_device_token`), so this
//! is a strict one-way "stale" signal — not a state machine we
//! need to model separately.

use std::time::Duration;

use chrono::Utc;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::entities::edge_boxes;

/// How often the loop scans for stale boxes. 60s keeps the UI's
/// "online/offline" indicator within a minute of reality without
/// hammering Postgres — the queries are cheap (one indexed-range
/// SELECT + at most one UPDATE WHERE) but there's no reason to run
/// them at health-check cadence.
pub const STALE_TICK_INTERVAL: Duration = Duration::from_secs(60);

/// A box is considered offline once we haven't heard from it for
/// 3× the worker's default `HEALTH_INTERVAL_S` (30s). 90s strikes the
/// balance between false positives during a single dropped poll and
/// surfacing real outages quickly.
pub const STALE_THRESHOLD: Duration = Duration::from_secs(90);

/// Spawn the stale checker on the current Tokio runtime. The returned
/// handle isn't useful to the caller — the loop exits when the passed
/// `shutdown` token is cancelled, matching the other process-level
/// background jobs.
pub fn spawn(db: DatabaseConnection, shutdown: CancellationToken) {
    tokio::spawn(async move {
        info!(
            tick_secs = STALE_TICK_INTERVAL.as_secs(),
            threshold_secs = STALE_THRESHOLD.as_secs(),
            "cameras.stale_checker: started"
        );
        let mut ticker = tokio::time::interval(STALE_TICK_INTERVAL);
        // Skip the immediate tick — no point scanning before any edge
        // box has had a chance to check in after a fresh boot.
        ticker.tick().await;
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    info!("cameras.stale_checker: shutdown");
                    return;
                }
                _ = ticker.tick() => {
                    if let Err(e) = sweep_once(&db, STALE_THRESHOLD).await {
                        warn!(error = %e, "cameras.stale_checker: sweep failed");
                    }
                }
            }
        }
    });
}

/// One pass of the stale-detection logic. Public so tests can drive it
/// deterministically without waiting for the ticker.
///
/// Returns the number of rows flipped to `offline` on this pass.
pub async fn sweep_once(
    db: &DatabaseConnection,
    threshold: Duration,
) -> Result<u64, sea_orm::DbErr> {
    let cutoff = Utc::now() - chrono::Duration::from_std(threshold).unwrap_or_default();

    // Single bulk UPDATE — we don't need per-row diff logging; the
    // operator-facing signal is the count, and the auth middleware
    // already logs the inverse transition (`status flipped to active`)
    // whenever a box comes back.
    let res = edge_boxes::Entity::update_many()
        .col_expr(
            edge_boxes::Column::Status,
            sea_orm::sea_query::Expr::value("offline"),
        )
        .col_expr(
            edge_boxes::Column::UpdatedAt,
            sea_orm::sea_query::Expr::value(chrono::Utc::now()),
        )
        .filter(edge_boxes::Column::Status.ne("offline"))
        // Retired boxes must stay retired forever. Without this
        // guard, the operator's "Remove" click flipped status to
        // `retired`, then the next sweep flipped it to `offline`,
        // and every list filter that excluded `retired` started
        // matching the row again. Cameras-edge dropdown, topology,
        // and the boxes table all regressed in the same way.
        .filter(edge_boxes::Column::Status.ne("retired"))
        .filter(edge_boxes::Column::LastSeenAt.is_not_null())
        .filter(edge_boxes::Column::LastSeenAt.lt(cutoff))
        .exec(db)
        .await?;

    if res.rows_affected > 0 {
        info!(
            count = res.rows_affected,
            cutoff = %cutoff,
            "cameras.stale_checker: marked edge boxes offline"
        );
    }
    Ok(res.rows_affected)
}

// Tests live in `crates/cameras/tests/smoke_e2e.rs` so they can share
// the testcontainer + migration setup with the other Tier-A coverage.
