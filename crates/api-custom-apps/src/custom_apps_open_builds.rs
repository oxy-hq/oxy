//! "How many open tabs are still on an old build?" — for operators.
//!
//! The platform runtime's `oxy-app-ready` and `oxy-update` rows name the build
//! each session runs (`custom_apps_beacon` validates it as a UUID). This turns
//! the grouped per-build session counts (`get_fleet_build_sessions`) into one
//! line per app, compared against the app's live build. Read by the fleet
//! health table and the per-app availability panel.
//!
//! **"Not measured" is never folded into "all live"**, for the same reason the
//! fleet table never folds an unmeasured app into operational: capture off, a
//! failed query, or no live build to compare against are open questions, and
//! answering them with a green line converts them into false ones.

use std::collections::HashMap;

use oxy_observability::types::AppBuildSessions;
use serde::Serialize;
use uuid::Uuid;

/// Sessions active in the last day. Long enough to cover a tab left open
/// overnight that noticed a morning publish; there is no "tab closed" signal,
/// so a closed session counts on its last build until it ages out of this.
pub const OPEN_BUILDS_WINDOW_MINUTES: u32 = 24 * 60;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpenBuilds {
    /// `not_measured` | `none_seen` | `all_live` | `some_older`.
    pub status: &'static str,
    /// Sessions last seen on the live build.
    pub live_sessions: u64,
    /// Sessions last seen on any other build.
    pub older_sessions: u64,
    /// How many distinct older builds those sessions run.
    pub older_builds: u64,
    pub window_minutes: u32,
}

impl OpenBuilds {
    pub fn not_measured() -> Self {
        Self {
            status: "not_measured",
            live_sessions: 0,
            older_sessions: 0,
            older_builds: 0,
            window_minutes: OPEN_BUILDS_WINDOW_MINUTES,
        }
    }
}

/// Fold one app's per-build rows against its live build.
///
/// `rows: None` is "not measured" (capture off or the query failed). `live:
/// None` — nothing is published to compare against — is "not measured" too:
/// every session would read as older, which would be a claim, not a count.
pub fn summarize(live: Option<Uuid>, rows: Option<&[AppBuildSessions]>) -> OpenBuilds {
    let (Some(live), Some(rows)) = (live, rows) else {
        return OpenBuilds::not_measured();
    };
    let live = live.to_string();
    let mut out = OpenBuilds {
        status: "none_seen",
        ..OpenBuilds::not_measured()
    };
    for row in rows {
        if row.build_id.eq_ignore_ascii_case(&live) {
            out.live_sessions += row.sessions;
        } else if row.sessions > 0 {
            out.older_sessions += row.sessions;
            out.older_builds += 1;
        }
    }
    out.status = match (out.live_sessions, out.older_sessions) {
        (0, 0) => "none_seen",
        (_, 0) => "all_live",
        _ => "some_older",
    };
    out
}

/// One app to evaluate: `(org_id, app_id, live build)`.
pub type OpenBuildsTarget = (Uuid, Uuid, Option<Uuid>);

/// [`OpenBuilds`] for each app, from one grouped query. Every requested app
/// gets an answer; a failure answers "not measured" for all of them.
pub async fn for_apps(targets: &[OpenBuildsTarget]) -> HashMap<Uuid, OpenBuilds> {
    let unmeasured = || {
        targets
            .iter()
            .map(|(_, app, _)| (*app, OpenBuilds::not_measured()))
            .collect()
    };
    let Some(store) = oxy_observability::global::get_global() else {
        return unmeasured();
    };
    let pairs: Vec<(String, String)> = targets
        .iter()
        .map(|(org, app, _)| (org.to_string(), app.to_string()))
        .collect();
    let rows = match store
        .get_fleet_build_sessions(&pairs, OPEN_BUILDS_WINDOW_MINUTES)
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!("open-builds query failed; reporting not measured: {e}");
            return unmeasured();
        }
    };
    targets
        .iter()
        .map(|(_, app, live)| {
            let found = rows.get(&app.to_string()).map(Vec::as_slice).unwrap_or(&[]);
            (*app, summarize(*live, Some(found)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(build: &str, sessions: u64) -> AppBuildSessions {
        AppBuildSessions {
            build_id: build.to_string(),
            sessions,
        }
    }

    #[test]
    fn counts_live_and_older_sessions_and_builds() {
        let live = Uuid::new_v4();
        let rows = [
            row(&live.to_string(), 7),
            row(&Uuid::new_v4().to_string(), 2),
            row(&Uuid::new_v4().to_string(), 1),
        ];
        let open = summarize(Some(live), Some(&rows));
        assert_eq!(open.status, "some_older");
        assert_eq!(
            (open.live_sessions, open.older_sessions, open.older_builds),
            (7, 3, 2)
        );
    }

    #[test]
    fn all_live_and_none_seen_are_different_answers() {
        let live = Uuid::new_v4();
        let all = summarize(Some(live), Some(&[row(&live.to_string(), 4)]));
        assert_eq!(all.status, "all_live");
        assert_eq!(summarize(Some(live), Some(&[])).status, "none_seen");
    }

    /// The whole point: an unanswered question is never green.
    #[test]
    fn unmeasured_or_nothing_live_is_not_measured_never_all_live() {
        let live = Uuid::new_v4();
        assert_eq!(summarize(Some(live), None).status, "not_measured");
        assert_eq!(
            summarize(None, Some(&[row(&live.to_string(), 4)])).status,
            "not_measured"
        );
    }
}
