//! Production's pointer history cut into **releases**: one stretch of time
//! per build production served, and which earlier stretch each is compared
//! with. Pure — the events come in already read (`reads`).
//!
//! - Consecutive events naming the same build are one release: a re-promote
//!   of the serving build (every `just up` seed makes one) is a dot on the
//!   lane, not a release compared with itself.
//! - The event serving at the window's start (the anchor) begins a `clipped`
//!   release counted from the window's start. When the window held more
//!   events than were read, the oldest kept one is `clipped` instead and the
//!   anchor is not used: it would claim the gap.
//! - A `backfill` event is where history begins, not a release anyone made.
//! - `previous` is the nearest earlier release with a build that is not
//!   pruned, not an unpublish and not the same build: production down for an
//!   hour is not "the release before".

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// One pointer move, as `app_environment_events` holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub id: Uuid,
    pub environment: String,
    pub action: String,
    pub at: DateTime<Utc>,
    pub build_id: Option<Uuid>,
    pub actor: Option<Uuid>,
}

/// How a release began.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartedBy {
    Promote,
    Publish,
    /// Back to a build production served before.
    Rollback,
    /// A `rollback` event to a build production never served: the legacy
    /// console's "Make live", which is a forward move.
    MadeLive,
    /// A `backfill` row: serving since before history was kept.
    HistoryStart,
    Unpublish,
    Other,
}

impl StartedBy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Promote => "promote",
            Self::Publish => "publish",
            Self::Rollback => "rollback",
            Self::MadeLive => "made_live",
            Self::HistoryStart => "history_start",
            Self::Unpublish => "unpublish",
            Self::Other => "other",
        }
    }
}

/// One release: a stretch of production serving one build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stretch {
    /// 0 is the oldest.
    pub index: usize,
    pub build_id: Option<Uuid>,
    /// When the event that began it happened.
    pub began_at: DateTime<Utc>,
    /// Where its numbers are counted from: `began_at`, or the window's start.
    pub starts_at: DateTime<Utc>,
    /// The next release's `began_at`, or the window's end.
    pub ends_at: DateTime<Utc>,
    /// Serving now.
    pub current: bool,
    /// Began before the window, or before the oldest event read.
    pub clipped: bool,
    pub started_by: StartedBy,
    pub actor: Option<Uuid>,
    pub unpublished: bool,
    /// A move whose build has since been deleted: its calls went with it.
    pub pruned: bool,
    pub previous: Option<usize>,
}

impl Stretch {
    /// Has a build whose numbers can be read and compared.
    pub fn comparable(&self) -> bool {
        self.build_id.is_some() && !self.pruned && !self.unpublished
    }
}

/// Cut `events` (production's, oldest first) into releases over `[since, until)`.
pub fn stretches(
    anchor: Option<Event>,
    events: Vec<Event>,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
    truncated: bool,
) -> Vec<Stretch> {
    let all = anchor.filter(|_| !truncated).into_iter().chain(events);
    let mut out: Vec<Stretch> = Vec::new();
    for (k, e) in all.enumerate() {
        if let Some(last) = out.last()
            && continues(last, &e)
        {
            continue;
        }
        let unpublished = e.action == "unpublish";
        let build_id = e.build_id.filter(|_| !unpublished);
        let started_by = started_by(&out, &e, build_id);
        out.push(Stretch {
            index: out.len(),
            build_id,
            began_at: e.at,
            starts_at: e.at.max(since),
            ends_at: until,
            current: false,
            clipped: e.at < since || (k == 0 && truncated),
            started_by,
            actor: e.actor,
            unpublished,
            pruned: !unpublished && build_id.is_none(),
            previous: None,
        });
    }
    for k in 1..out.len() {
        out[k - 1].ends_at = out[k].began_at.max(since);
    }
    if let Some(last) = out.last_mut() {
        last.current = true;
    }
    for k in 0..out.len() {
        out[k].previous = previous_of(&out, k);
    }
    out
}

/// `e` names the build `last` already serves: the same release goes on.
fn continues(last: &Stretch, e: &Event) -> bool {
    last.build_id.is_some() && e.action != "unpublish" && e.build_id == last.build_id
}

fn started_by(earlier: &[Stretch], e: &Event, build: Option<Uuid>) -> StartedBy {
    match e.action.as_str() {
        "promote" => StartedBy::Promote,
        "publish" => StartedBy::Publish,
        "backfill" => StartedBy::HistoryStart,
        "unpublish" => StartedBy::Unpublish,
        "rollback" if build.is_some() && earlier.iter().any(|s| s.build_id == build) => {
            StartedBy::Rollback
        }
        "rollback" => StartedBy::MadeLive,
        _ => StartedBy::Other,
    }
}

fn previous_of(all: &[Stretch], k: usize) -> Option<usize> {
    let this = &all[k];
    if !this.comparable() {
        return None;
    }
    all[..k]
        .iter()
        .rev()
        .find(|s| s.comparable() && s.build_id != this.build_id)
        .map(|s| s.index)
}

/// One production migration ledger row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    pub store: String,
    pub filename: String,
    pub applied_at: DateTime<Utc>,
    pub build_id: Option<Uuid>,
}

/// Each row to the earliest release of its build that began at or after it
/// was applied (production's files run before the pointer moves). A row no
/// release claims — a promote that applied files and then failed, or a build
/// since pruned — is returned apart: the schema changed with no release.
pub fn attribute_migrations(
    stretches: &[Stretch],
    rows: Vec<Applied>,
) -> (Vec<Vec<Applied>>, Vec<Applied>) {
    let mut per = vec![Vec::new(); stretches.len()];
    let mut unreleased = Vec::new();
    for row in rows {
        let owner = stretches.iter().find(|s| {
            row.build_id.is_some() && s.build_id == row.build_id && s.began_at >= row.applied_at
        });
        match owner {
            Some(s) => per[s.index].push(row),
            None => unreleased.push(row),
        }
    }
    (per, unreleased)
}

/// A failure first seen in the window, and on which build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirstSeen {
    pub function_name: String,
    pub fingerprint: String,
    pub build_id: Uuid,
    pub first_seen: DateTime<Utc>,
    pub occurrences: i64,
}

/// Each new failure to the release serving when it first happened, when that
/// release's build is the one it failed on. A call that ran on the old build
/// after a move belongs to neither.
pub fn attribute_issues(stretches: &[Stretch], issues: Vec<FirstSeen>) -> Vec<Vec<FirstSeen>> {
    let mut per = vec![Vec::new(); stretches.len()];
    for issue in issues {
        let owner = stretches.iter().find(|s| {
            s.build_id == Some(issue.build_id)
                && s.starts_at <= issue.first_seen
                && issue.first_seen < s.ends_at
        });
        if let Some(s) = owner {
            per[s.index].push(issue);
        }
    }
    per
}

#[cfg(test)]
#[path = "timeline_tests.rs"]
mod tests;
