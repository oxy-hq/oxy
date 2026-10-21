//! Which build each recent custom-app session runs — the operator's answer to
//! "how many open tabs are still on an old build".
//!
//! Read from the client rows the platform runtime writes (`kind = 'client'`):
//! `oxy-app-ready` names the build a document loaded, and `oxy-update` names
//! the build a tab was running when it noticed (or applied, or gave up on) a
//! newer one. A session's most recent such row is the build it was last seen
//! on. Rows with no build (written before the runtime reported one) are left
//! out rather than counted as any build.
//!
//! Counts are SESSIONS (the tracking cookie), not devices, and there is no
//! "tab closed" signal: a session that closed on an old build counts as on it
//! until it ages out of the window. A tab left open behind the live build
//! writes an `oxy-update` `stale` row every 6 h, so a 24 h window keeps it —
//! as long as its probe reaches the server (offline or signed out, it ages out). An operational signal, not an audit trail —
//! the rows are page-written (see `custom_apps_beacon`).

use std::collections::HashMap;

use clickhouse::Row;
use oxy_shared::errors::OxyError;
use serde::Deserialize;

use super::ClickHouseObservabilityStorage;
use super::schema::PRODUCTION_ONLY;
use crate::types::AppBuildSessions;

fn escape_sql_literal(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "''")
}

#[derive(Debug, Deserialize, Row)]
struct BuildSessionsRow {
    app_id: String,
    build_id: String,
    sessions: u64,
}

/// One grouped statement for a page of apps: per app, per build, how many
/// sessions were last seen on it within `window_minutes`. Pure, so the shape is
/// pinned without a live ClickHouse.
fn build_sessions_sql(apps: &[(String, String)], window_minutes: u32) -> String {
    let pairs = apps
        .iter()
        .map(|(org, app)| {
            format!(
                "('{}','{}')",
                escape_sql_literal(org),
                escape_sql_literal(app)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    // The inner alias must not be `build_id`: ClickHouse resolves the WHERE's
    // `build_id != ''` to a same-named SELECT alias before the column, and an
    // aggregate in WHERE is Code 184 (ILLEGAL_AGGREGATION).
    format!(
        "SELECT app_id, latest_build AS build_id, count() AS sessions FROM (\
         SELECT app_id, session_id, argMax(build_id, timestamp) AS latest_build \
         FROM custom_app_events \
         WHERE (org_id, app_id) IN ({pairs}) \
         AND kind = 'client' \
         AND error_kind IN ('oxy-app-ready', 'oxy-update') \
         AND build_id != '' \
         AND timestamp >= now() - INTERVAL {window_minutes} MINUTE \
         AND {PRODUCTION_ONLY} \
         GROUP BY app_id, session_id) \
         GROUP BY app_id, latest_build"
    )
}

/// Sessions per build for each app, keyed by `app_id`. An app absent from the
/// map had no session report a build in the window — "none seen", which is not
/// the same as "not measured": that is an `Err` or capture being off.
pub(super) async fn get_fleet_build_sessions(
    storage: &ClickHouseObservabilityStorage,
    apps: &[(String, String)],
    window_minutes: u32,
) -> Result<HashMap<String, Vec<AppBuildSessions>>, OxyError> {
    if apps.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = storage
        .read_client()
        .query(&build_sessions_sql(apps, window_minutes))
        .fetch_all::<BuildSessionsRow>()
        .await
        .map_err(|e| {
            OxyError::RuntimeError(format!("ClickHouse build sessions query failed: {e}"))
        })?;
    let mut out: HashMap<String, Vec<AppBuildSessions>> = HashMap::new();
    for row in rows {
        out.entry(row.app_id).or_default().push(AppBuildSessions {
            build_id: row.build_id,
            sessions: row.sessions,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apps() -> Vec<(String, String)> {
        vec![
            ("org-1".into(), "app-1".into()),
            ("org-2".into(), "app-'2".into()),
        ]
    }

    /// Org-first tuple predicate, like the availability fleet query: the table's
    /// sort key is `(org_id, app_id, timestamp)`.
    #[test]
    fn leads_with_the_sort_key_and_escapes_ids() {
        let sql = build_sessions_sql(&apps(), 1440);
        assert!(sql.contains("(org_id, app_id) IN (('org-1','app-1'),('org-2','app-''2'))"));
        assert!(sql.contains("INTERVAL 1440 MINUTE"));
    }

    /// Only the two platform rows that name a running build, only client rows,
    /// only production, and never a row without a build.
    #[test]
    fn reads_only_build_naming_client_rows_in_production() {
        let sql = build_sessions_sql(&apps(), 60);
        assert!(sql.contains("kind = 'client'"));
        assert!(sql.contains("error_kind IN ('oxy-app-ready', 'oxy-update')"));
        assert!(sql.contains("build_id != ''"));
        assert!(sql.contains(PRODUCTION_ONLY));
    }

    /// A session counts once, on the build of its LATEST row — a tab that
    /// reloaded onto the new build must stop counting as stale.
    #[test]
    fn a_session_counts_once_on_its_latest_build() {
        let sql = build_sessions_sql(&apps(), 60);
        assert!(sql.contains("argMax(build_id, timestamp) AS latest_build"));
        assert!(sql.contains("GROUP BY app_id, session_id"));
        assert!(sql.starts_with("SELECT app_id, latest_build AS build_id, count() AS sessions"));
        assert!(sql.ends_with("GROUP BY app_id, latest_build"));
    }

    /// ClickHouse resolves a WHERE identifier to a SELECT alias of the same
    /// name before the table's column, so `argMax(build_id, …) AS build_id`
    /// beside `WHERE build_id != ''` puts an aggregate in WHERE: Code 184
    /// ILLEGAL_AGGREGATION on every call, and every open-builds line read
    /// "not measured". No alias of the inner SELECT may name a word its WHERE
    /// reads.
    #[test]
    fn no_inner_alias_shadows_a_column_the_where_reads() {
        let words = |s: &str| -> Vec<String> {
            s.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .filter(|w| !w.is_empty())
                .map(str::to_string)
                .collect()
        };
        let sql = build_sessions_sql(&apps(), 60);
        let inner = sql.split_once("FROM (").expect("an inner query").1;
        let (select, rest) = inner
            .split_once(" FROM custom_app_events ")
            .expect("the inner FROM");
        let filter = rest
            .split_once("WHERE ")
            .expect("a WHERE")
            .1
            .split_once(" GROUP BY ")
            .expect("a GROUP BY")
            .0;
        let aliases: Vec<String> = select
            .split(" AS ")
            .skip(1)
            .filter_map(|s| words(s).into_iter().next())
            .collect();
        assert!(!aliases.is_empty(), "{select}");
        let read = words(filter);
        for alias in aliases {
            assert!(
                !read.contains(&alias),
                "inner alias `{alias}` shadows a column the WHERE reads: {filter}"
            );
        }
    }
}
