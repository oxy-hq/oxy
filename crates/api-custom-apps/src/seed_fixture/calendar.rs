//! Putting the fixture's release history on a seven-day calendar.
//!
//! The publishes and promotes are real calls (`releases`) and so happen within
//! seconds of each other; a timeline would stack five releases on one minute.
//! After them, one transaction re-dates the rows those calls wrote — the
//! pointer events, the builds, their function rows, the app row and its two
//! environment rows — onto the calendar below. Every statement is keyed by the
//! fixture's own deterministic app id, and this runs on a local database only
//! (the gates in `mod.rs`). The timestamps are the one non-real fact in the
//! history. Audit rows keep their real times: the audit log is evidence.
//!
//! | Release | Publish (staging) | Promote (production) |
//! | --- | --- | --- |
//! | r1 | T-7d | T-7d+10m |
//! | r2 | T-5d | T-5d+30m |
//! | r3 | T-3d-2h | T-3d |
//! | r4 (bad) | T-7h | T-6h, serving now |
//! | r5 (fix) | T-1h | — staging only |

use chrono::{DateTime, Duration, DurationRound, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseBackend, DatabaseConnection, DbErr, Statement, TransactionTrait, Value,
};
use uuid::Uuid;

use super::bundle::RELEASES;

/// The release that production serves at the end of the seed: the bad one.
pub const LIVE: usize = 4;

/// When each release was published and promoted, relative to `t`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Calendar {
    /// Seed time: the end of the seeded traffic.
    pub t: DateTime<Utc>,
    pub publish: [DateTime<Utc>; RELEASES],
    /// `None`: never promoted (r5).
    pub promote: [Option<DateTime<Utc>>; RELEASES],
}

/// One production stretch: a release serving production over `[start, end)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stretch {
    /// 1-based release index.
    pub release: usize,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl Stretch {
    pub fn hours(&self) -> f64 {
        (self.end - self.start).num_seconds() as f64 / 3600.0
    }
}

impl Calendar {
    /// The calendar for a seed at `t` (truncated to the minute).
    pub fn at(t: DateTime<Utc>) -> Self {
        let t = t.duration_trunc(Duration::minutes(1)).unwrap_or(t);
        let d = Duration::days;
        let h = Duration::hours;
        let m = Duration::minutes;
        Self {
            t,
            publish: [t - d(7), t - d(5), t - d(3) - h(2), t - h(7), t - h(1)],
            promote: [
                Some(t - d(7) + m(10)),
                Some(t - d(5) + m(30)),
                Some(t - d(3)),
                Some(t - h(6)),
                None,
            ],
        }
    }

    /// Production's stretches, oldest first; the last one ends at `t`.
    pub fn stretches(&self) -> Vec<Stretch> {
        let promoted: Vec<(usize, DateTime<Utc>)> = self
            .promote
            .iter()
            .enumerate()
            .filter_map(|(i, at)| at.map(|at| (i + 1, at)))
            .collect();
        promoted
            .iter()
            .enumerate()
            .map(|(k, &(release, start))| Stretch {
                release,
                start,
                end: promoted.get(k + 1).map_or(self.t, |&(_, next)| next),
            })
            .collect()
    }
}

/// Re-date what the release script wrote. `builds[k]` is release `k + 1`'s
/// `app_builds.id`.
pub async fn redate(
    conn: &DatabaseConnection,
    app_id: Uuid,
    cal: &Calendar,
    builds: &[Uuid],
) -> Result<(), DbErr> {
    let txn = conn.begin().await?;
    for (k, build) in builds.iter().enumerate() {
        let published = cal.publish[k];
        exec(
            &txn,
            EVENT_SQL,
            vec![
                at(published),
                app_id.into(),
                "staging".into(),
                (*build).into(),
                "publish".into(),
            ],
        )
        .await?;
        exec(
            &txn,
            BUILD_SQL,
            vec![at(published), app_id.into(), (*build).into()],
        )
        .await?;
        exec(
            &txn,
            FUNCTIONS_SQL,
            vec![at(published), app_id.into(), (*build).into()],
        )
        .await?;
        if let Some(promoted) = cal.promote[k] {
            exec(
                &txn,
                EVENT_SQL,
                vec![
                    at(promoted),
                    app_id.into(),
                    "production".into(),
                    (*build).into(),
                    "promote".into(),
                ],
            )
            .await?;
        }
    }
    let first = cal.publish[0] - Duration::minutes(5);
    let live = cal.promote[LIVE - 1].expect("the live release was promoted");
    let last_publish = cal.publish[RELEASES - 1];
    exec(
        &txn,
        APP_SQL,
        vec![at(first), at(live), at(last_publish), app_id.into()],
    )
    .await?;
    exec(
        &txn,
        ENV_SQL,
        vec![at(first), at(live), app_id.into(), "production".into()],
    )
    .await?;
    exec(
        &txn,
        ENV_SQL,
        vec![at(first), at(last_publish), app_id.into(), "staging".into()],
    )
    .await?;
    txn.commit().await
}

const EVENT_SQL: &str = "UPDATE app_environment_events SET at = $1 \
     WHERE app_id = $2 AND environment = $3 AND build_id = $4 AND action = $5";
const BUILD_SQL: &str = "UPDATE app_builds SET created_at = $1 WHERE app_id = $2 AND id = $3";
const FUNCTIONS_SQL: &str =
    "UPDATE app_functions SET created_at = $1 WHERE app_id = $2 AND build_id = $3";
const APP_SQL: &str = "UPDATE apps SET created_at = $1, published_at = $2, \
     last_promoted_at = $2, last_synced_at = $3, updated_at = $3 WHERE id = $4";
const ENV_SQL: &str = "UPDATE app_environments SET created_at = $1, updated_at = $2 \
     WHERE app_id = $3 AND name = $4";

fn at(t: DateTime<Utc>) -> Value {
    t.fixed_offset().into()
}

async fn exec<C: ConnectionTrait>(conn: &C, sql: &str, values: Vec<Value>) -> Result<(), DbErr> {
    conn.execute_raw(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        values,
    ))
    .await
    .map(|_| ())
}
