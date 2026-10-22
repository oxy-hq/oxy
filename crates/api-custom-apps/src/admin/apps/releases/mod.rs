//! The release timeline: `GET /api/customer-apps/{id}/releases?days=`.
//!
//! Three lanes — production, staging, sandboxes — of an app's pointer moves
//! over the window, and production cut into **releases** (one stretch per
//! build it served, `timeline`). Each release carries what it measured: its
//! function calls' failure rate and route p95 (`metrics`), its Web Vitals per
//! screen (`vitals`), the migrations it ran and the failures first seen on it,
//! all compared with the release before it by one pure rule (`rule`). A red
//! release that is serving now names the build a rollback would go back to;
//! the console's Roll back then goes through the same dry-run plan `oxyc
//! rollback` prints.
//!
//! ## Who sees what
//!
//! The mount's guard stack has decided the app (`admin::apps::guarded`).
//! Production's rows are readable by every caller it admits, publish tokens
//! for their own app included (as `/builds` and `/issues` are; actor labels
//! go to them as `/builds`' `published_by_email` does — this is not one of
//! the people routes). Staging's and sandboxes' events are read only for a
//! caller with non-production reach (`Caller::has_reach`, oxy-authz
//! `Action::AppNonProduction`); otherwise those lanes are empty and the answer
//! says `not_permitted` rather than refusing the production half. A sandbox
//! agent token is answered 404 by `app_grant_scope` before this runs.
//!
//! ## Bounded
//!
//! Postgres only (FleetOk by the `/customer-apps/{*rest}` wildcard). The
//! window is 1–30 days (default 7). Event reads are capped and keep the
//! newest. The four slow parts — metrics, vitals, migrations, new failures —
//! each run alone under a statement timeout and a budget, and one that fails
//! answers `unavailable` without taking the others down (`metrics::bounded`).

mod compose;
mod dto;
mod metrics;
mod reads;
mod rule;
mod timeline;
mod vitals;

use axum::Json;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query};
use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use entity::apps;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use oxy_auth::types::AppPublishTokenAuth;
use sea_orm::DatabaseConnection;
use serde::Deserialize;
use uuid::Uuid;

pub use dto::ReleaseTimeline;
pub use metrics::bounded;
pub use rule::Kind;

use super::environment_scope::{self, Caller, ScopeError};
use super::issues::{MAX_DAYS, parse_days};
use compose::{Names, Reads};
use dto::{Parts, Window, part};

/// `?days=` on the release timeline.
#[derive(Debug, Default, Deserialize)]
pub struct ReleaseQuery {
    pub days: Option<i64>,
}

fn rejected(rejection: QueryRejection) -> ScopeError {
    ScopeError::new(
        StatusCode::BAD_REQUEST,
        "invalid_days",
        format!("days must be a whole number between 1 and {MAX_DAYS}: {rejection}"),
    )
}

/// `app`'s release timeline over the `days` before `now`; the staging and
/// sandbox lanes only when `reach`.
pub async fn releases_of(
    db: &DatabaseConnection,
    app: &apps::Model,
    days: i64,
    now: DateTime<Utc>,
    reach: bool,
) -> Result<ReleaseTimeline, ScopeError> {
    let since = now - Duration::days(days);
    let history = reads::history(db, app.id, since, reach)
        .await
        .map_err(|e| ScopeError::internal("release history read failed", e))?;
    let stretches = timeline::stretches(
        history.anchor.as_ref().map(reads::EventRow::event),
        history
            .production
            .iter()
            .map(reads::EventRow::event)
            .collect(),
        since,
        now,
        history.production_truncated,
    );
    let all_rows = || {
        history
            .production
            .iter()
            .chain(&history.other)
            .chain(&history.anchor)
    };
    let actors: Vec<Uuid> = all_rows().filter_map(|r| r.actor).collect();
    let labels = reads::actor_labels(db, actors)
        .await
        .map_err(|e| ScopeError::internal("release actor lookup failed", e))?;
    let names = Names::from_rows(all_rows(), labels);
    let reads = read_parts(db, app.id, since, &stretches).await;
    let parts = Parts {
        metrics: part(&reads.metrics),
        vitals: part(&reads.vitals),
        migrations: part(&reads.migrations),
        issues: part(&reads.issues),
    };
    let migrations_truncated = reads.migrations.as_ref().is_some_and(|(_, cut)| *cut);
    let lanes = compose::lanes(&history.production, &history.other, &names);
    let (releases, unreleased_migrations) = compose::releases(&stretches, reads, &names);
    Ok(ReleaseTimeline {
        window: Window {
            days,
            since: since.to_rfc3339(),
            until: now.to_rfc3339(),
        },
        non_production: if reach { "included" } else { "not_permitted" },
        lanes,
        events_truncated: history.production_truncated || history.other_truncated,
        parts,
        releases,
        unreleased_migrations,
        migrations_truncated,
    })
}

/// The four slow parts, each alone: one that fails is `None`.
async fn read_parts(
    db: &DatabaseConnection,
    app_id: Uuid,
    since: DateTime<Utc>,
    stretches: &[timeline::Stretch],
) -> Reads {
    let comparable = stretches.iter().any(timeline::Stretch::comparable);
    Reads {
        metrics: if comparable {
            metrics::metrics(db, app_id, stretches).await
        } else {
            Some(Vec::new())
        },
        vitals: vitals::release_vitals(db, app_id, stretches).await,
        migrations: metrics::migrations(db, app_id, since).await,
        issues: metrics::new_issues(db, app_id, since).await,
    }
}

/// `GET /api/customer-apps/{id}/releases` — the app's releases over the last
/// `?days=` (1–30, default 7), each with its numbers beside the release before
/// it, and the three environments' pointer moves. Derived on every read.
pub async fn list_app_releases(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    marker: Option<axum::Extension<AppPublishTokenAuth>>,
    Path(id): Path<Uuid>,
    query: Result<Query<ReleaseQuery>, QueryRejection>,
) -> Result<Json<ReleaseTimeline>, ScopeError> {
    let Query(q) = query.map_err(rejected)?;
    let days = parse_days(q.days)?;
    let db = environment_scope::connect().await?;
    let app = environment_scope::load_app(&db, id).await?;
    let reach = Caller::new(&user, marker.as_ref())
        .has_reach(&db, &app)
        .await;
    Ok(Json(releases_of(&db, &app, days, Utc::now(), reach).await?))
}
