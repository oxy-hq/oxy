//! The wire shape of `GET /api/customer-apps/{id}/releases`
//! (`web-app/src/types/appReleases.ts` mirrors it).

use serde::Serialize;
use uuid::Uuid;

use super::rule::Verdict;

#[derive(Debug, Serialize)]
pub struct ReleaseTimeline {
    pub window: Window,
    /// `included`, or `not_permitted`: staging and sandbox lanes are absent
    /// because this caller may not open the app's non-production environments.
    pub non_production: &'static str,
    /// Production, staging, sandboxes — always all three, in that order.
    pub lanes: Vec<Lane>,
    /// A lane held more events in the window than were read; the oldest were
    /// dropped (never the release serving now).
    pub events_truncated: bool,
    pub parts: Parts,
    /// Newest first.
    pub releases: Vec<Release>,
    /// Production migrations no release claims: applied by a promote that then
    /// failed, or by a build since pruned. The schema changed with no release.
    pub unreleased_migrations: Vec<Migration>,
    pub migrations_truncated: bool,
}

#[derive(Debug, Serialize)]
pub struct Window {
    pub days: i64,
    pub since: String,
    pub until: String,
}

/// Each slow read answers alone: `ok` or `unavailable`.
#[derive(Debug, Serialize)]
pub struct Parts {
    pub metrics: &'static str,
    pub vitals: &'static str,
    pub migrations: &'static str,
    pub issues: &'static str,
}

pub fn part<T>(read: &Option<T>) -> &'static str {
    if read.is_some() { "ok" } else { "unavailable" }
}

#[derive(Debug, Serialize)]
pub struct Lane {
    /// `production` | `staging` | `sandboxes`.
    pub lane: &'static str,
    pub events: Vec<LaneEvent>,
}

#[derive(Debug, Serialize)]
pub struct LaneEvent {
    pub id: Uuid,
    pub environment: String,
    pub action: String,
    pub at: String,
    pub build: Option<BuildRef>,
    /// Its build has since been deleted (`build` is null, and it was not an unpublish).
    pub pruned: bool,
    pub actor: Option<String>,
    /// Published by a sandbox agent token. Never true on production, which no
    /// promote path ships such a build to.
    pub agent_built: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct BuildRef {
    /// `app_builds.id`.
    pub id: Uuid,
    /// The publish's build id.
    pub build_id: String,
    pub commit_sha: Option<String>,
    pub source_branch: Option<String>,
    pub source_repo: Option<String>,
    pub published_via: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Release {
    pub index: usize,
    pub build: Option<BuildRef>,
    pub pruned: bool,
    pub unpublished: bool,
    /// Serving now: `ended_at` is null.
    pub current: bool,
    /// Began before the window; its numbers count from `counted_from`.
    pub clipped: bool,
    pub started_at: String,
    pub counted_from: String,
    pub ended_at: Option<String>,
    pub started_by: StartedBy,
    /// Null: the build was pruned, nothing was served, or the read was unavailable.
    pub metrics: Option<Metrics>,
    pub vitals: Vec<ScreenVitals>,
    /// Screens past the ones listed.
    pub more_screens: bool,
    pub migrations: Vec<Migration>,
    /// Failures first seen on this release's build while it served.
    pub new_issues: Vec<NewIssue>,
    /// The index of the release this one is compared with.
    pub previous: Option<usize>,
    pub verdict: Verdict,
    /// Present on a red release only.
    pub rollback: Option<Rollback>,
}

#[derive(Debug, Serialize)]
pub struct StartedBy {
    /// `promote` | `publish` | `rollback` | `made_live` | `history_start` |
    /// `unpublish` | `other`.
    pub action: &'static str,
    pub actor: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Metrics {
    pub calls: i64,
    pub failed: i64,
    pub shed: i64,
    /// `failed / calls` in basis points; null with no calls.
    pub failure_rate_bp: Option<i64>,
    pub route_calls: i64,
    pub p50_ms: Option<i64>,
    pub p95_ms: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ScreenVitals {
    pub path: String,
    pub samples: i64,
    pub lcp_p75: Option<i64>,
    /// The worst interaction in the page's first 10 s, p75 across page loads.
    pub inp_p75: Option<i64>,
    /// CLS × 1000 over the first 10 s.
    pub cls_p75: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Migration {
    pub store: String,
    pub filename: String,
    pub applied_at: String,
    pub build_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct NewIssue {
    pub function_name: String,
    pub fingerprint: String,
    pub first_seen: String,
    pub occurrences: i64,
}

/// What the console offers on a red release. A hint only: the move's dry-run
/// plan stays the authority, and the confirm shows it.
#[derive(Debug, Serialize)]
pub struct Rollback {
    pub offered: bool,
    /// The build it would go back to: the release this one was compared with.
    pub to: Option<BuildRef>,
    /// `not_current` (a later release replaced it) | `no_previous`.
    pub why_not: Option<&'static str>,
}
