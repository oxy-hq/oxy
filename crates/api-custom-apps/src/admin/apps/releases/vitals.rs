//! The release timeline's Web Vitals, per production release and screen.
//!
//! **The one reader of vitals.** It reads Postgres `custom_app_event`, a
//! Backlog table (`data_placement.rs`: "custom_app_event") headed for the
//! workspace's Airhouse `oxy_custom_app_usage`, which already receives the
//! same rows under the same ids (`usage_airhouse.rs`). When the table moves,
//! this SQL is the only thing that changes: `json_extract_string(payload,
//! '$.build')` and a `QUALIFY row_number() OVER (PARTITION BY event_id …) = 1`
//! dedupe in place of the jsonb operators.
//!
//! Only rows that name their build (`payload.build`, sent by `runtime.js`
//! since vitals carried it) are read, and a row counts only inside its own
//! build's release: there is no guessing a page's build from its session.
//!
//! The payload is written by the page's own JavaScript, so every value is
//! bounded **in SQL**, before the percentile: a non-number, a negative, an
//! LCP over a minute or a `1e400` is not a sample. A bound applied after the
//! aggregate could not take one forged value back out of a p75, and a cast of
//! `1e400` to `float8` would fail the whole statement.

use sea_orm::{DatabaseConnection, FromQueryResult};
use uuid::Uuid;

use super::metrics::{STATEMENT_TIMEOUT_MS, STRETCHES_CTE, bounded, stretch_arrays};
use super::timeline::Stretch;

/// Screens shown per release, most page loads first.
pub const MAX_SCREENS: usize = 8;

/// The p75 of payload key `key`, its samples bounded to `0..=max`. Nested
/// `CASE`s, because SQL does not promise the order an `AND` is evaluated in,
/// and the cast must only ever see a jsonb number.
fn p75(key: &str, max: u32) -> String {
    format!(
        "round(percentile_cont(0.75) WITHIN GROUP (ORDER BY \
           CASE WHEN jsonb_typeof(e.payload->'{key}') = 'number' THEN \
             CASE WHEN (e.payload->>'{key}')::numeric BETWEEN 0 AND {max} \
               THEN (e.payload->>'{key}')::numeric::float8 END END))::bigint AS {key}_p75"
    )
}

pub fn vitals_sql() -> String {
    format!(
        "WITH {STRETCHES_CTE}, \
         g AS ( \
           SELECT s.idx::bigint AS idx, left(e.payload->>'path', 256) AS path, \
                  COUNT(*)::bigint AS samples, {lcp}, {inp}, {cls} \
           FROM s JOIN custom_app_event e \
             ON e.app_id = $1 AND e.event_name = 'oxy-web-vitals' \
            AND e.environment = 'production' \
            AND e.occurred_at >= s.starts_at AND e.occurred_at < s.ends_at \
            AND e.payload->>'build' = s.build_id::text \
           WHERE jsonb_typeof(e.payload->'path') = 'string' \
           GROUP BY 1, 2), \
         r AS (SELECT g.*, row_number() OVER (PARTITION BY idx ORDER BY samples DESC, path) \
               AS rank FROM g) \
         SELECT idx, path, samples, lcp_p75, inp_p75, cls_p75, rank FROM r \
         WHERE rank <= {limit} ORDER BY idx, rank",
        lcp = p75("lcp", 60_000),
        inp = p75("inp", 30_000),
        cls = p75("cls", 10_000),
        limit = MAX_SCREENS + 1,
    )
}

/// One screen of one release. `cls_p75` is CLS × 1000, as the runtime sends it.
#[derive(Debug, Clone, PartialEq, Eq, FromQueryResult)]
pub struct VitalsRow {
    pub idx: i64,
    pub path: String,
    pub samples: i64,
    pub lcp_p75: Option<i64>,
    pub inp_p75: Option<i64>,
    pub cls_p75: Option<i64>,
    pub rank: i64,
}

/// Every release's screens, up to [`MAX_SCREENS`] each, plus one more where
/// there are more (`rank > MAX_SCREENS`), which the caller reports as a count.
/// `None`: the read failed or ran out of time.
pub async fn release_vitals(
    db: &DatabaseConnection,
    app_id: Uuid,
    stretches: &[Stretch],
) -> Option<Vec<VitalsRow>> {
    if !stretches.iter().any(Stretch::comparable) {
        return Some(Vec::new());
    }
    let [idx, builds, starts, ends] = stretch_arrays(stretches);
    let values = vec![app_id.into(), idx, builds, starts, ends];
    bounded(db, "vitals", &vitals_sql(), values, STATEMENT_TIMEOUT_MS).await
}
