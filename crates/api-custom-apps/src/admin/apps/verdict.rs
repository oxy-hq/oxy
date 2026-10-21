//! One answer to "is this app OK?" at the API edge: the `verdict` field on the
//! `/customer-apps` list rows and on the launcher's `CustomAppSummary`.
//!
//! The product had five vocabularies for it — `AppHealth` (fleet health),
//! `AvailabilityVerdict`, the launcher card's free-text `status`, a partner's
//! `live`/`draft`, and the admin home's findings — and the web-app reconciled
//! them client-side. This is the server's half: [`ListVerdict`] is `AppHealth`
//! plus `draft`, and it is computed by the **same evaluator** as
//! `GET /customer-apps/fleet-health` ([`super::fleet_health::evaluate_fleet`]),
//! so the table, the list and the launcher cannot disagree.
//!
//! ## What it will not say
//!
//! - **No reading is never healthy.** An app the evaluator did not measure —
//!   capture off, the query failed, or this module ran out of [`BUDGET`] —
//!   is `not_measured`, never `operational`. `quiet` (traffic below the floor
//!   the ratio rules need) is its own value and survives to the wire.
//! - **A draft has no health.** It serves nobody, so it is `draft` and is not
//!   sent to the observability store at all.
//! - **It never fails the request.** A list that 500s because ClickHouse is slow
//!   is worse than a list whose rows say `not_measured`.
//!
//! ## Cost
//!
//! Two grouped ClickHouse reads for the whole page, never one per app
//! ([`super::fleet_health`]). A short per-process cache ([`TTL`]) keeps a
//! launcher that refetches on every window focus from turning into a query per
//! focus; a verdict this old is still a verdict about the last few minutes of
//! traffic, which is the window it describes. Evaluations run concurrently up to
//! [`MAX_CONCURRENT_EVALUATIONS`]; only `?verdict=true` callers pay for any of it.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::Query;
use axum::http::StatusCode;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use oxy_observability::fleet_health::AppHealth;
use serde::Serialize;
use uuid::Uuid;

use super::dto::{ListAppsQuery, ListAppsResponse};
use super::fleet_health::{AppKey, evaluate_fleet};
use super::handlers;

/// `AppHealth` plus `draft`. The wire values are `AppHealth`'s snake_case ones,
/// plus `"draft"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ListVerdict {
    Down,
    Degraded,
    NotMeasured,
    Quiet,
    Operational,
    Draft,
}

impl From<AppHealth> for ListVerdict {
    fn from(health: AppHealth) -> Self {
        match health {
            AppHealth::Down => Self::Down,
            AppHealth::Degraded => Self::Degraded,
            AppHealth::NotMeasured => Self::NotMeasured,
            AppHealth::Quiet => Self::Quiet,
            AppHealth::Operational => Self::Operational,
        }
    }
}

/// An app the caller wants a verdict for.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AppRef {
    pub id: Uuid,
    pub org_id: Uuid,
    pub published: bool,
}

/// The longest this module waits on the observability store. Past it every
/// measured-eligible app reads `not_measured`: the store did not answer, and
/// that is not a verdict about the app (`fleet_health`'s own words).
const BUDGET: Duration = Duration::from_secs(3);

/// How long a computed verdict is reused. Short: it describes a window of the
/// last few minutes, and the point is to bound the load, not to hide a change.
const TTL: Duration = Duration::from_secs(30);

/// Past this many entries the cache is dropped whole, rather than tracked per
/// entry: it is a load shield, and a rebuild costs two queries.
const CACHE_CAP: usize = 4096;

/// Most apps one evaluation covers (`fleet_health`'s page cap, which bounds the
/// `IN` list sent to ClickHouse). A longer list is evaluated in chunks.
const CHUNK: usize = 200;

static CACHE: LazyLock<Mutex<HashMap<Uuid, (Instant, AppHealth)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// A verdict per app in `apps`. Every app asked about is in the result.
pub(crate) async fn verdicts_for(apps: &[AppRef]) -> HashMap<Uuid, ListVerdict> {
    let mut out: HashMap<Uuid, ListVerdict> = apps
        .iter()
        .filter(|a| !a.published)
        .map(|a| (a.id, ListVerdict::Draft))
        .collect();

    let (cached, wanted) = split_cached(apps.iter().filter(|a| a.published).copied());
    out.extend(cached.into_iter().map(|(id, h)| (id, h.into())));
    if wanted.is_empty() {
        return out;
    }

    match tokio::time::timeout(BUDGET, evaluate_bounded(&wanted)).await {
        Ok(measured) => out.extend(measured.into_iter().map(|(id, h)| (id, h.into()))),
        Err(_) => {
            // Not remembered: a timeout says nothing about the app, and the next
            // request should try the store again rather than inherit the miss.
            tracing::warn!(
                "app verdicts: the observability store did not answer in {BUDGET:?}; \
                 reporting not_measured"
            );
            out.extend(wanted.iter().map(|(id, _)| (*id, ListVerdict::NotMeasured)));
        }
    }
    out
}

/// How many evaluations may be in flight at once. A cap, not a queue of one:
/// requests for different workspaces share no app, so serializing them would only
/// make them take turns (and a slow store would starve the ones at the back of
/// [`BUDGET`]). Eight keeps a burst after the cache expires from becoming a
/// stampede on the store; past it, a request waits for a permit within its budget.
const MAX_CONCURRENT_EVALUATIONS: usize = 8;

static EVALUATIONS: tokio::sync::Semaphore =
    tokio::sync::Semaphore::const_new(MAX_CONCURRENT_EVALUATIONS);

async fn evaluate_bounded(wanted: &[AppKey]) -> HashMap<Uuid, AppHealth> {
    // Never closed, so acquiring cannot fail.
    let _permit = EVALUATIONS.acquire().await;
    // A request that waited for its permit finds what the one ahead of it measured:
    // one cache expiry costs one evaluation per distinct set of apps, not one per
    // request.
    let (mut all, still_wanted) = split_cached(wanted.iter().map(|(id, org_id)| AppRef {
        id: *id,
        org_id: *org_id,
        published: true,
    }));
    if !still_wanted.is_empty() {
        let measured = evaluate_chunks(&still_wanted).await;
        remember(&measured);
        all.extend(measured);
    }
    all
}

async fn evaluate_chunks(keys: &[AppKey]) -> HashMap<Uuid, AppHealth> {
    let mut all = HashMap::with_capacity(keys.len());
    for chunk in keys.chunks(CHUNK) {
        all.extend(evaluate_fleet(chunk).await.health_of(chunk));
    }
    all
}

/// Cached verdicts that are still fresh, and the keys that need evaluating.
fn split_cached(apps: impl Iterator<Item = AppRef>) -> (HashMap<Uuid, AppHealth>, Vec<AppKey>) {
    let now = Instant::now();
    let cache = CACHE.lock().unwrap_or_else(|p| p.into_inner());
    let mut fresh = HashMap::new();
    let mut wanted = Vec::new();
    for app in apps {
        match cache.get(&app.id) {
            Some((at, health)) if now.duration_since(*at) < TTL => {
                fresh.insert(app.id, *health);
            }
            _ => wanted.push((app.id, app.org_id)),
        }
    }
    (fresh, wanted)
}

fn remember(measured: &HashMap<Uuid, AppHealth>) {
    let now = Instant::now();
    let mut cache = CACHE.lock().unwrap_or_else(|p| p.into_inner());
    if cache.len() + measured.len() > CACHE_CAP {
        cache.clear();
    }
    cache.extend(measured.iter().map(|(id, h)| (*id, (now, *h))));
}

/// `GET /customer-apps` — the registry list, each row carrying its `verdict`.
///
/// The list handler itself is unchanged; this wraps it, so the verdict is one
/// batched read added to a response that already makes four.
pub async fn list_apps_with_verdict(
    user: AuthenticatedUserExtractor,
    query: Query<ListAppsQuery>,
) -> Result<Json<ListAppsResponse>, StatusCode> {
    let Json(mut page) = handlers::list_apps(user, query).await?;
    let refs: Vec<AppRef> = page
        .items
        .iter()
        .map(|a| AppRef {
            id: a.id,
            org_id: a.org_id,
            published: a.published_at.is_some(),
        })
        .collect();
    let verdicts = verdicts_for(&refs).await;
    for item in &mut page.items {
        item.verdict = verdicts.get(&item.id).copied();
    }
    Ok(Json(page))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(published: bool) -> AppRef {
        AppRef {
            id: Uuid::new_v4(),
            org_id: Uuid::new_v4(),
            published,
        }
    }

    #[test]
    fn the_wire_values_are_app_health_plus_draft() {
        let wire = |v: ListVerdict| serde_json::to_string(&v).unwrap();
        assert_eq!(wire(ListVerdict::Down), "\"down\"");
        assert_eq!(wire(ListVerdict::NotMeasured), "\"not_measured\"");
        assert_eq!(wire(ListVerdict::Quiet), "\"quiet\"");
        assert_eq!(wire(ListVerdict::Draft), "\"draft\"");
        // Every `AppHealth` value maps to the same word, so the evaluator's
        // vocabulary reaches the wire unchanged.
        for health in [
            AppHealth::Down,
            AppHealth::Degraded,
            AppHealth::NotMeasured,
            AppHealth::Quiet,
            AppHealth::Operational,
        ] {
            assert_eq!(
                wire(health.into()),
                serde_json::to_string(&health).unwrap(),
                "{health:?}"
            );
        }
    }

    /// A draft is never sent to the store and never reads as healthy; with
    /// capture off (no global store in a unit test) a published app is
    /// `not_measured` — the one place "no reading" could have become a green
    /// tick.
    #[tokio::test]
    async fn a_draft_is_draft_and_an_unmeasured_app_is_not_operational() {
        let (draft, live) = (app(false), app(true));
        let verdicts = verdicts_for(&[draft, live]).await;
        assert_eq!(verdicts[&draft.id], ListVerdict::Draft);
        assert_eq!(verdicts[&live.id], ListVerdict::NotMeasured);
        assert!(verdicts.len() == 2, "every app asked about is answered");
    }

    #[tokio::test]
    async fn an_empty_list_is_an_empty_answer() {
        assert!(verdicts_for(&[]).await.is_empty());
    }
}
