//! Shaping the reads into the wire answer: one [`Release`] per stretch, each
//! judged against its `previous`, and the three lanes. Pure.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::dto::{
    BuildRef, Lane, LaneEvent, Metrics, Migration, NewIssue, Release, Rollback, ScreenVitals,
    StartedBy,
};
use super::metrics::MetricsRow;
use super::reads::EventRow;
use super::rule::{self, Kind, Screen, Side, Verdict};
use super::timeline::{self, Applied, FirstSeen, Stretch};
use super::vitals::{MAX_SCREENS, VitalsRow};

/// What the bounded reads returned; `None` is a part that was unavailable.
pub struct Reads {
    pub metrics: Option<Vec<MetricsRow>>,
    pub vitals: Option<Vec<VitalsRow>>,
    pub migrations: Option<(Vec<Applied>, bool)>,
    pub issues: Option<Vec<FirstSeen>>,
}

/// The builds and the actors' labels the events named.
pub struct Names {
    pub builds: HashMap<Uuid, BuildRef>,
    pub labels: HashMap<Uuid, String>,
}

impl Names {
    pub fn from_rows<'a>(
        rows: impl Iterator<Item = &'a EventRow>,
        labels: HashMap<Uuid, String>,
    ) -> Self {
        let builds = rows
            .filter_map(|r| Some((r.build_id?, build_ref(r)?)))
            .collect();
        Self { builds, labels }
    }

    fn build(&self, id: Option<Uuid>) -> Option<BuildRef> {
        id.and_then(|id| self.builds.get(&id).cloned())
    }

    fn label(&self, id: Option<Uuid>) -> Option<String> {
        id.and_then(|id| self.labels.get(&id).cloned())
    }
}

fn build_ref(r: &EventRow) -> Option<BuildRef> {
    Some(BuildRef {
        id: r.build_id?,
        build_id: r.publish_id.clone()?,
        commit_sha: r.commit_sha.clone(),
        source_branch: r.source_branch.clone(),
        source_repo: r.source_repo.clone(),
        published_via: r.published_via.clone(),
    })
}

fn rfc(t: DateTime<Utc>) -> String {
    t.to_rfc3339()
}

/// The three lanes from the in-window events, oldest first in each.
pub fn lanes(production: &[EventRow], other: &[EventRow], names: &Names) -> Vec<Lane> {
    let mut lanes = [
        ("production", Vec::new()),
        ("staging", Vec::new()),
        ("sandboxes", Vec::new()),
    ];
    for row in production.iter().chain(other) {
        let k = match row.environment.as_str() {
            "production" => 0,
            "staging" => 1,
            _ => 2,
        };
        lanes[k].1.push(LaneEvent {
            id: row.id,
            environment: row.environment.clone(),
            action: row.action.clone(),
            at: row.at.to_rfc3339(),
            build: names.build(row.build_id),
            // The FK is `ON DELETE SET NULL`: a pruned build leaves a NULL.
            pruned: row.build_id.is_none() && row.action != "unpublish",
            actor: names.label(row.actor),
            agent_built: row.agent_built.unwrap_or(false),
        });
    }
    lanes
        .into_iter()
        .map(|(lane, events)| Lane { lane, events })
        .collect()
}

/// One release per stretch, newest first, and the migrations none claims.
pub fn releases(
    stretches: &[Stretch],
    reads: Reads,
    names: &Names,
) -> (Vec<Release>, Vec<Migration>) {
    let (migrations, unreleased) = match &reads.migrations {
        Some((rows, _)) => timeline::attribute_migrations(stretches, rows.clone()),
        None => (vec![Vec::new(); stretches.len()], Vec::new()),
    };
    let issues = timeline::attribute_issues(stretches, reads.issues.clone().unwrap_or_default());
    let sides: Vec<Side> = stretches.iter().map(|s| side(s.index, &reads)).collect();
    let mut out: Vec<Release> = stretches
        .iter()
        .zip(migrations)
        .zip(issues)
        .map(|((s, migrations), issues)| {
            let verdict = verdict(s, &sides, reads.metrics.is_some());
            Release {
                index: s.index,
                build: names.build(s.build_id),
                pruned: s.pruned,
                unpublished: s.unpublished,
                current: s.current,
                clipped: s.clipped,
                started_at: rfc(s.began_at),
                counted_from: rfc(s.starts_at),
                ended_at: (!s.current).then(|| rfc(s.ends_at)),
                started_by: StartedBy {
                    action: s.started_by.as_str(),
                    actor: names.label(s.actor),
                },
                metrics: metrics_of(s, &reads),
                vitals: screens_of(s.index, &reads),
                more_screens: more_screens(s.index, &reads),
                migrations: migrations.into_iter().map(migration).collect(),
                new_issues: issues.into_iter().map(new_issue).collect(),
                previous: s.previous,
                rollback: rollback(s, &verdict, stretches, names),
                verdict,
            }
        })
        .collect();
    out.reverse();
    (out, unreleased.into_iter().map(migration).collect())
}

fn metrics_row(index: usize, reads: &Reads) -> Option<&MetricsRow> {
    reads
        .metrics
        .as_ref()?
        .iter()
        .find(|r| r.idx == index as i64)
}

fn metrics_of(s: &Stretch, reads: &Reads) -> Option<Metrics> {
    if !s.comparable() {
        return None;
    }
    reads.metrics.as_ref()?;
    let row = metrics_row(s.index, reads).cloned().unwrap_or_default();
    Some(Metrics {
        calls: row.calls,
        failed: row.failed,
        shed: row.shed,
        failure_rate_bp: rule::rate_bp(row.failed, row.calls),
        route_calls: row.route_calls,
        p50_ms: row.p50_ms,
        p95_ms: row.p95_ms,
    })
}

fn vitals_rows(index: usize, reads: &Reads) -> impl Iterator<Item = &VitalsRow> {
    reads
        .vitals
        .iter()
        .flatten()
        .filter(move |r| r.idx == index as i64)
}

fn screens_of(index: usize, reads: &Reads) -> Vec<ScreenVitals> {
    vitals_rows(index, reads)
        .filter(|r| r.rank <= MAX_SCREENS as i64)
        .map(|r| ScreenVitals {
            path: r.path.clone(),
            samples: r.samples,
            lcp_p75: r.lcp_p75,
            inp_p75: r.inp_p75,
            cls_p75: r.cls_p75,
        })
        .collect()
}

fn more_screens(index: usize, reads: &Reads) -> bool {
    vitals_rows(index, reads).any(|r| r.rank > MAX_SCREENS as i64)
}

fn side(index: usize, reads: &Reads) -> Side {
    let row = metrics_row(index, reads).cloned().unwrap_or_default();
    Side {
        calls: row.calls,
        failed: row.failed,
        route_calls: row.route_calls,
        p95_ms: row.p95_ms,
        screens: screens_of(index, reads)
            .into_iter()
            .map(|v| Screen {
                path: v.path,
                samples: v.samples,
                lcp_p75: v.lcp_p75,
                cls_p75: v.cls_p75,
            })
            .collect(),
    }
}

fn verdict(s: &Stretch, sides: &[Side], metrics_read: bool) -> Verdict {
    if s.unpublished {
        return Verdict::not_comparable("production served nothing");
    }
    if s.pruned {
        return Verdict::not_comparable("the build was pruned, and its calls with it");
    }
    if !metrics_read {
        return Verdict::not_comparable("the function calls could not be read");
    }
    let Some(previous) = s.previous else {
        return Verdict::not_comparable("no earlier release in this window to compare with");
    };
    rule::judge(&sides[s.index], &sides[previous])
}

/// On a red release: roll back to the release it was compared with, when it
/// is the one serving now.
fn rollback(s: &Stretch, verdict: &Verdict, all: &[Stretch], names: &Names) -> Option<Rollback> {
    if verdict.kind != Kind::Worse {
        return None;
    }
    let to = s.previous.and_then(|p| names.build(all[p].build_id));
    let why_not = if !s.current {
        Some("not_current")
    } else if to.is_none() {
        Some("no_previous")
    } else {
        None
    };
    Some(Rollback {
        offered: why_not.is_none(),
        to: to.filter(|_| why_not.is_none()),
        why_not,
    })
}

fn migration(a: Applied) -> Migration {
    Migration {
        store: a.store,
        filename: a.filename,
        applied_at: rfc(a.applied_at),
        build_id: a.build_id,
    }
}

fn new_issue(f: FirstSeen) -> NewIssue {
    NewIssue {
        function_name: f.function_name,
        fingerprint: f.fingerprint,
        first_seen: rfc(f.first_seen),
        occurrences: f.occurrences,
    }
}
