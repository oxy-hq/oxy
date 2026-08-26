//! Deciding whether a rollup is STALE.
//!
//! One question, two kinds of answer, and a two-layer cache under each. An
//! `every:` key is a cadence — measured against the last build, wherever that
//! is recorded; a `sql:` key is a probe — a value compared against the one the
//! last build was for. Both read through the same three places, in the same
//! order, because the manifest is not the only record of a build: a rollup
//! that rebuilt to ZERO rows has no manifest entry at all (`preagg_retract`
//! removed it), and the node-local ledger is the only surviving evidence it
//! ran.
//!
//! Split out of `preagg_executor` for size, and because every function here
//! takes a `cache_dir` rather than reaching for the process's state dir —
//! which is what makes the "second cycle after a zero-row rebuild" case
//! testable at all.

use std::sync::{Arc, RwLock};

use agentic_automation::workspace::WorkspaceContext;
use agentic_semantic::refresh_key_cache::RefreshKeyCache;

use super::preagg_ledger;
use crate::agentic_wiring::OxyProjectContext;

/// The refresh key that governs one rollup: the per-rollup `refresh_key:` if
/// the declaration carries one, else the view-level key. `None` means the
/// worker skips this rollup — `api::semantic::build_preagg_status` filters on
/// the same rule so the IDE never lists a rollup that will never be built.
pub(crate) fn rollup_refresh_key<'a>(
    rollup: &'a airlayer::preagg::RollupSpec,
    view: &'a airlayer::View,
) -> Option<&'a airlayer::RefreshKey> {
    if let Some(ref preaggs) = view.pre_aggregations {
        for pa in preaggs {
            if pa.name == rollup.name {
                if let Some(ref k) = pa.refresh_key {
                    return Some(k);
                }
                break;
            }
        }
    }
    view.refresh_key.as_ref()
}

/// Dispatch to the appropriate refresh-key evaluator based on key kind.
///
/// Returns `(current_value, is_stale, error_msg)`.
pub(super) async fn evaluate_refresh_key(
    rk: &airlayer::RefreshKey,
    rollup_hash: &str,
    cache_dir: &std::path::Path,
    cache: &Arc<RwLock<RefreshKeyCache>>,
    ctx: &OxyProjectContext,
    database_name: &str,
) -> (Option<String>, bool, Option<String>) {
    match rk {
        airlayer::RefreshKey::Every(interval_str) => {
            let (value, is_stale) =
                eval_every_refresh_key(interval_str, rollup_hash, cache_dir, cache);
            (value, is_stale, None)
        }
        airlayer::RefreshKey::Sql(sql) => {
            eval_sql_refresh_key(sql, rollup_hash, cache_dir, ctx, database_name).await
        }
    }
}

/// Evaluate an `Every`-interval refresh key.
///
/// Returns `(None, is_stale)`. `is_stale` is false if the in-memory cache or
/// the manifest confirms the rollup was built within the interval.
pub(super) fn eval_every_refresh_key(
    interval_str: &str,
    rollup_hash: &str,
    cache_dir: &std::path::Path,
    cache: &Arc<RwLock<RefreshKeyCache>>,
) -> (Option<String>, bool) {
    let Ok(interval) = airlayer::preagg::parse_interval(interval_str) else {
        // Unparsable interval → treat as always stale so operator notices.
        tracing::warn!(interval = %interval_str, rollup_hash, "preagg: unparsable Every interval");
        return (None, true);
    };

    // Layer 1: in-memory cache (survives heartbeats within the same process).
    {
        let guard = cache.read().expect("preagg cache lock poisoned");
        if guard.get(rollup_hash, interval).is_some() {
            return (None, false);
        }
    }

    // Layer 2: manifest's build_date (survives server restarts).
    // If the rollup was built within the interval, seed the cache and skip rebuild.
    let manifest_build_date =
        agentic_semantic::preagg::load_local_manifest(cache_dir).and_then(|m| {
            m.rollups
                .iter()
                .find(|r| r.rollup_hash == rollup_hash)
                .map(|r| r.build_date.clone())
        });

    if let Some(build_date_str) = manifest_build_date
        && let Ok(built_at) =
            chrono::NaiveDateTime::parse_from_str(&build_date_str, "%Y-%m-%d %H:%M:%S")
    {
        let built_at_utc = built_at.and_utc();
        let age = chrono::Utc::now().signed_duration_since(built_at_utc);
        let chrono_interval = match chrono::Duration::from_std(interval) {
            Ok(d) => d,
            Err(_) => {
                tracing::warn!(
                    interval = %interval_str,
                    rollup_hash,
                    "preagg: configured Every interval overflows chrono::Duration; \
                     treating rollup as always fresh to avoid spurious rebuilds"
                );
                chrono::Duration::milliseconds(i64::MAX)
            }
        };
        if age < chrono_interval {
            let mut guard = cache.write().expect("preagg cache lock poisoned");
            guard.insert(rollup_hash.to_string(), None);
            return (None, false);
        }
    }

    // Layer 3: the node-local ledger's zero-row record (survives restarts).
    // A rollup that rebuilt to nothing has NO manifest entry — the retraction
    // removed it — so layer 2 reads it as never-built and layer 1 only covers
    // this process. Without this, a legitimately empty rollup would rebuild on
    // every cadence tick for as long as it stayed empty, reported as "Not
    // built" the whole time. The attempt is what the interval measures.
    let empty_at = preagg_ledger::RollupLedger::load(cache_dir)
        .empty_record(rollup_hash)
        .and_then(|e| chrono::DateTime::parse_from_rfc3339(&e.at).ok());

    if let Some(at) = empty_at
        && let Ok(chrono_interval) = chrono::Duration::from_std(interval)
        && chrono::Utc::now().signed_duration_since(at.with_timezone(&chrono::Utc))
            < chrono_interval
    {
        let mut guard = cache.write().expect("preagg cache lock poisoned");
        guard.insert(rollup_hash.to_string(), None);
        return (None, false);
    }

    (None, true)
}

/// Evaluate a SQL-based refresh key by running it against the warehouse.
///
/// Returns `(current_value, is_stale, error_msg)`. On connector/query error,
/// `error_msg` is `Some(...)` and the rollup is treated as fresh (not stale)
/// to avoid rebuild thrashing while the warehouse is unavailable.
pub(super) async fn eval_sql_refresh_key(
    sql: &str,
    rollup_hash: &str,
    cache_dir: &std::path::Path,
    ctx: &OxyProjectContext,
    database_name: &str,
) -> (Option<String>, bool, Option<String>) {
    let connector = match ctx.get_connector(database_name).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("get_connector failed for {database_name}: {e}");
            return (None, false, Some(format!("get_connector failed: {e}")));
        }
    };

    let current = match connector.execute_query(sql, 1).await {
        Ok(result) => result
            .result
            .rows
            .first()
            .and_then(|r| r.0.first())
            .map(|cell| match cell {
                agentic_core::result::CellValue::Text(s) => s.clone(),
                agentic_core::result::CellValue::Number(n) => n.to_string(),
                agentic_core::result::CellValue::Null => String::new(),
            }),
        Err(e) => {
            tracing::warn!("refresh_key SQL evaluation failed: {e}");
            return (None, false, Some(format!("refresh_key SQL failed: {e}")));
        }
    };

    let last_value = agentic_semantic::preagg::load_local_manifest(cache_dir)
        .and_then(|m| {
            m.rollups
                .iter()
                .find(|r| r.rollup_hash == rollup_hash)
                .and_then(|r| r.refresh_key_value.clone())
        })
        // No manifest entry does not mean nothing was ever built: a zero-row
        // rebuild retracts its entry, taking `refresh_key_value` with it. The
        // ledger keeps the probe that empty answer was for, so an unchanged
        // key still says "fresh" instead of rebuilding to nothing every tick.
        .or_else(|| {
            preagg_ledger::RollupLedger::load(cache_dir)
                .empty_record(rollup_hash)
                .and_then(|e| e.refresh_key_value.clone())
        });

    let is_stale = current.as_deref() != last_value.as_deref();
    (current, is_stale, None)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, RwLock};

    use agentic_semantic::refresh_key_cache::RefreshKeyCache;

    use super::preagg_ledger;

    // ── Freshness after a zero-row rebuild ────────────────────────────────

    /// The regression finding #1 named: the retraction that stops an empty
    /// rollup serving stale rows also erases the manifest fields both
    /// staleness evaluators read. Nothing else in the suite exercises a SECOND
    /// cycle after a zero-row rebuild, which is why a green run and a rollup
    /// rebuilding every 600s were consistent.
    #[tokio::test]
    async fn a_second_cycle_does_not_rebuild_a_rollup_that_just_came_back_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        // What `rebuild_rollup`'s zero-row path leaves behind: no manifest
        // entry at all, and the attempt on record.
        preagg_ledger::record_empty(dir.path(), "empty", 1, "orders", "daily", None).await;

        // A cold process, so only the durable record can answer.
        let cache = Arc::new(RwLock::new(RefreshKeyCache::new()));
        let (_, stale) = super::eval_every_refresh_key("24h", "empty", dir.path(), &cache);
        assert!(
            !stale,
            "the interval still gates the rollup; without the record it rebuilds every tick"
        );
    }

    /// ...and the interval still expires, or the record would be a permanent
    /// mute rather than a cadence.
    #[tokio::test]
    async fn an_empty_rollup_is_stale_again_once_its_interval_elapses() {
        let dir = tempfile::tempdir().expect("tempdir");
        preagg_ledger::record_empty(dir.path(), "empty", 1, "orders", "daily", None).await;

        let cache = Arc::new(RwLock::new(RefreshKeyCache::new()));
        let (_, stale) = super::eval_every_refresh_key("1ms", "empty", dir.path(), &cache);
        assert!(stale, "a 1ms interval has elapsed by now");
    }

    /// A rollup nothing has ever touched is stale, so a fresh workspace still
    /// builds on its first cycle.
    #[tokio::test]
    async fn a_never_built_rollup_is_stale() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cache = Arc::new(RwLock::new(RefreshKeyCache::new()));
        let (_, stale) = super::eval_every_refresh_key("24h", "fresh", dir.path(), &cache);
        assert!(stale);
    }
}
