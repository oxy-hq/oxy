//! Rebuild helpers for the pre-aggregation background worker.
//!
//! Each function owns one phase of the rebuild pipeline:
//! `rebuild_rollup` orchestrates them in sequence.

use std::sync::{Arc, RwLock};

use tokio::sync::Mutex as TokioMutex;
use uuid::Uuid;

use agentic_automation::workspace::WorkspaceContext;
use agentic_connector::{DatabaseConnector, SqlDialect};
use agentic_semantic::refresh_key_cache::RefreshKeyCache;

use crate::agentic_wiring::OxyProjectContext;

use super::preagg_ledger;
use super::preagg_retract::{Retraction, retract_under_publish_lock};

// ── Shared context for a single rollup rebuild within a cycle ─────────────────

#[derive(Clone)]
pub(super) struct RebuildContext {
    pub schema: String,
    /// The pre-aggregation cache key — see `state_dir::airlayer_cache_key`.
    pub workspace_id: Uuid,
    pub database_name: String,
    pub manifest_write_lock: Arc<TokioMutex<()>>,
    /// An engine over EVERY view the cycle loaded, not just the one being
    /// rebuilt. Planning a rollup's SQL has to see the whole layer: a view
    /// whose entity declares `parent:` on an entity owned by another view, or
    /// whose measure reaches through another view, is only resolvable against
    /// the full set. Planning against a one-view layer fails validation
    /// ("the hierarchy dead-ends", "references unknown entity/view") for
    /// schemas that are in fact valid.
    ///
    /// Built ONCE PER CYCLE (per dialect) by the executor and shared by
    /// pointer, not per rollup. Two reasons, and both are correctness as much
    /// as cost: `SemanticEngine::from_semantic_layer` validates the layer, so
    /// building it per rollup turns one malformed view into a failure of every
    /// rollup in the workspace — including well-formed ones that built
    /// yesterday — reported N times; and it deep-copied every view for each of
    /// an unbounded fan-out of rebuild tasks.
    pub engine: Arc<oxy_airlayer_compat::SemanticEngine>,
    /// `PREAGG_BUILDER_GENERATION` as of this cycle. Recorded per committed
    /// hash in the node-local ledger so a later sweep can CHECK whether this
    /// node's artifact is the current builder's rather than infer it from a
    /// manifest the fleet shares — see `preagg_ledger`.
    pub builder_generation: u32,
}

// ── Orchestrator ──────────────────────────────────────────────────────────────

/// Orchestrate a full rebuild for one stale rollup: CTAS → pull → manifest + cache.
///
/// `Ok(true)` means the manifest was committed — a new Parquet is on disk and
/// the entry points at it. `Ok(false)` is the zero-row case: the rollup is
/// genuinely empty now, so the entry and its Parquet are RETRACTED rather than
/// left behind. Either way the caller can rely on nothing older still being
/// served for this hash (see `PREAGG_BUILDER_GENERATION`).
pub(super) async fn rebuild_rollup(
    view: oxy_airlayer_compat::View,
    rollup: oxy_airlayer_compat::preagg::RollupSpec,
    current_refresh_key_value: Option<String>,
    date_str: String,
    ctx: Arc<OxyProjectContext>,
    rebuild_ctx: RebuildContext,
    cache: Arc<RwLock<RefreshKeyCache>>,
) -> Result<bool, String> {
    let connector = ctx
        .get_connector(&rebuild_ctx.database_name)
        .await
        .map_err(|e| format!("get_connector failed: {e}"))?;

    let entry = build_warehouse_table(
        &view,
        &rollup,
        &current_refresh_key_value,
        &date_str,
        &connector,
        &rebuild_ctx,
    )
    .await?;

    let cache_dir = oxy_shared::state_dir::get_airlayer_cache_dir(rebuild_ctx.workspace_id);
    tokio::fs::create_dir_all(&cache_dir)
        .await
        .map_err(|e| e.to_string())?;

    // The per-workspace lock covers the hot-swap AND the manifest write, not
    // just the manifest write. They are one publish: a concurrent cycle for
    // the same workspace that renamed its Parquet into place between our
    // rename and our manifest write would leave the manifest describing one
    // build and the file on disk another — the read path trusts the manifest's
    // `refresh_key_value` for freshness, so the mismatch is silent. Taking the
    // lock first makes "file replaced, manifest updated" indivisible.
    let publish = rebuild_ctx.manifest_write_lock.lock().await;

    let file_written = materialize_parquet(&entry, &connector, &cache_dir, &rebuild_ctx).await?;

    // Empty rollups (zero-row warehouse result) write no Parquet file. Committing
    // a manifest entry that points to a non-existent file poisons the read path:
    // the next semantic query would match the entry, resolve to `LocalParquet`,
    // and then fail with "Parquet cache file not found" — with no warehouse
    // fallback. Skip the commit until a future rebuild produces real rows.
    if !file_written {
        // Retract rather than leave the last good build in place. Zero rows is
        // the rollup's CURRENT answer, so an entry pointing at the previous
        // build serves last period's numbers under the Pre-aggregated badge —
        // a freshness lie with no generation bump involved. With the entry
        // gone the query falls back to the warehouse, which is right, and the
        // hash stops being something a builder-generation sweep waits on
        // forever (see `PREAGG_BUILDER_GENERATION`).
        //
        // `Retraction::Empty`, not `Wrong`: the artifact goes, but the ATTEMPT
        // stays on record. It is the only surviving evidence that this rollup
        // was evaluated — the manifest's `build_date` and `refresh_key_value`
        // are what the retraction just deleted — and without it a legitimately
        // empty rollup reads as never-built and rebuilds on every cadence tick
        // for as long as it stays empty.
        let retracted = retract_under_publish_lock(
            &rollup.hash,
            &cache_dir,
            rebuild_ctx.builder_generation,
            Retraction::Empty {
                view: view.name.clone(),
                rollup: rollup.name.clone(),
                refresh_key_value: current_refresh_key_value.clone(),
            },
            &cache,
        )
        .await;
        drop(publish);
        if retracted? {
            mirror_manifest_to_s3(&cache_dir).await;
        }
        tracing::info!(
            rollup = %rollup.name,
            "preagg: rebuild produced zero rows; retracted the previous build and \
             recorded the empty result so the refresh key still gates the next cycle"
        );
        return Ok(false);
    }

    commit_manifest_and_cache(
        &entry,
        &rollup.hash,
        &current_refresh_key_value,
        &cache_dir,
        &rebuild_ctx.database_name,
        &cache,
    )
    .await?;
    // Inside the publish lock with the manifest write: the ledger entry and the
    // manifest entry are one publish, and a sweep reading a half-published pair
    // would draw the wrong conclusion about whose artifact is on disk.
    preagg_ledger::record_built(
        &cache_dir,
        &rollup.hash,
        rebuild_ctx.builder_generation,
        &view.name,
        &rollup.name,
    )
    .await;
    drop(publish);

    // Mirroring is outside the lock: it is best-effort network I/O against a
    // manifest already committed locally, and holding a per-workspace lock
    // across an S3 round-trip would serialize every rollup in the workspace
    // behind the slowest upload.
    mirror_parquet_to_s3(&entry, &cache_dir).await;
    mirror_manifest_to_s3(&cache_dir).await;

    tracing::info!(rollup = %rollup.name, "preagg: rebuild complete");
    Ok(true)
}

/// Best-effort S3 mirror of a rollup's just-written Parquet file, so a query
/// or the status tab on a DIFFERENT node than the one that just built this
/// finds it too. `cache_key` is the local directory name — the workspace id
/// both sides of the read/write split derive from
/// `oxy_shared::state_dir::get_airlayer_cache_dir`, so this can't drift from
/// what a reader looks up under, and it is the identity that belongs in a
/// shared multi-tenant bucket prefix.
async fn mirror_parquet_to_s3(
    entry: &oxy_airlayer_compat::preagg::ManifestEntry,
    cache_dir: &std::path::Path,
) {
    let Some(cache_key) = cache_dir.file_name().and_then(|n| n.to_str()) else {
        return;
    };
    let file_name = format!("{}__{}.parquet", entry.view_name, entry.rollup_hash);
    let path = cache_dir.join(&file_name);
    match tokio::fs::read(&path).await {
        Ok(bytes) => oxy_compile::preagg_blob::mirror_parquet(cache_key, &file_name, bytes).await,
        Err(e) => tracing::warn!(
            error = %e,
            file = %file_name,
            "preagg: could not re-read just-written parquet to mirror it; cross-node reads \
             of this rollup will miss until the next rebuild"
        ),
    }
}

/// Best-effort S3 mirror of the manifest after `commit_manifest_and_cache`
/// updates it — every rebuild rewrites the whole file, so this always mirrors
/// the current, complete manifest, not just the rollup that just changed.
pub(super) async fn mirror_manifest_to_s3(cache_dir: &std::path::Path) {
    let Some(cache_key) = cache_dir.file_name().and_then(|n| n.to_str()) else {
        return;
    };
    let path = cache_dir.join("manifest.json");
    match tokio::fs::read(&path).await {
        Ok(bytes) => oxy_compile::preagg_blob::mirror_manifest(cache_key, bytes).await,
        Err(e) => tracing::warn!(
            error = %e,
            "preagg: could not re-read just-written manifest to mirror it; cross-node \
             reads of this workspace's cache will miss until the next rebuild"
        ),
    }
}

// ── Phase 1: warehouse CTAS ───────────────────────────────────────────────────

/// Execute the warehouse CTAS statement for a single rollup.
///
/// Marks every other rollup in the view as fresh so the planner only
/// generates a CTAS for the target rollup, avoiding invalid SQL from other
/// rollups (e.g. aggregate-only rollups with no GROUP BY).
///
/// The engine is built from the cycle's whole layer while generation stays
/// scoped to this one view — see `RebuildContext::engine` for why the two
/// scopes have to differ.
///
/// Returns the `ManifestEntry` for the rebuilt rollup on success.
async fn build_warehouse_table(
    view: &oxy_airlayer_compat::View,
    rollup: &oxy_airlayer_compat::preagg::RollupSpec,
    current_refresh_key_value: &Option<String>,
    date_str: &str,
    connector: &Arc<dyn DatabaseConnector>,
    rebuild_ctx: &RebuildContext,
) -> Result<oxy_airlayer_compat::preagg::ManifestEntry, String> {
    let dialect = connector_to_airlayer_dialect(connector.dialect());

    let plan = plan_rollup_build(
        view,
        rollup,
        current_refresh_key_value,
        &rebuild_ctx.engine,
        &rebuild_ctx.schema,
        date_str,
        &dialect,
    )?;

    agentic_semantic::preagg::execute_build_plan(connector, &plan)
        .await
        .map_err(|e| e.to_string())?;

    plan.manifest_entries
        .into_iter()
        .find(|e| e.rollup_hash == rollup.hash)
        .ok_or_else(|| format!("manifest entry not found for {}", rollup.hash))
}

/// Build the cycle's shared planning engine over the whole layer.
///
/// Separate from [`plan_rollup_build`] on purpose: this is the step that
/// validates the layer, and it must happen ONCE per cycle so a single
/// malformed view is one reported failure rather than a failure attributed to
/// every rollup in the workspace. See [`RebuildContext::engine`].
pub(super) fn build_layer_engine(
    layer_views: Vec<oxy_airlayer_compat::View>,
    dialect: &oxy_airlayer_compat::Dialect,
) -> Result<Arc<oxy_airlayer_compat::SemanticEngine>, String> {
    let layer = oxy_airlayer_compat::SemanticLayer::new(layer_views, None);
    let dialects = oxy_airlayer_compat::DatasourceDialectMap::with_default(dialect.clone());
    oxy_airlayer_compat::SemanticEngine::from_semantic_layer(layer, dialects)
        .map(Arc::new)
        .map_err(|e| e.to_string())
}

/// Plan the CTAS for one rollup against the cycle's layer-wide engine,
/// generating SQL for this view alone.
///
/// The two scopes are deliberately different. The engine sees every view the
/// cycle loaded, because that is what resolves a `parent:` chain or a measure
/// that reaches into another view; `views` passed to the generator is just
/// this one, because only this rollup is being rebuilt. Handing the generator
/// the whole layer would build every view's rollups.
fn plan_rollup_build(
    view: &oxy_airlayer_compat::View,
    rollup: &oxy_airlayer_compat::preagg::RollupSpec,
    current_refresh_key_value: &Option<String>,
    engine: &oxy_airlayer_compat::SemanticEngine,
    schema: &str,
    date_str: &str,
    dialect: &oxy_airlayer_compat::Dialect,
) -> Result<oxy_airlayer_compat::preagg::BuildPlan, String> {
    let all_rollups = oxy_airlayer_compat::preagg::resolve_rollups(view);
    let freshness: Vec<oxy_airlayer_compat::preagg::RollupFreshness> = all_rollups
        .iter()
        .map(|r| oxy_airlayer_compat::preagg::RollupFreshness {
            rollup_hash: r.hash.clone(),
            is_fresh: r.hash != rollup.hash,
            current_refresh_key_value: if r.hash == rollup.hash {
                current_refresh_key_value.clone()
            } else {
                None
            },
        })
        .collect();

    oxy_airlayer_compat::preagg::collect_build_sql_with_engine(
        engine,
        &[view],
        schema,
        date_str,
        dialect,
        None,
        Some(&freshness),
    )
    .map_err(|e| e.to_string())
}

// ── Phase 2: Parquet pull ─────────────────────────────────────────────────────

/// Monotonic per-process counter, so two builds of the same rollup in the same
/// process never pick the same staging filename.
fn next_build_seq() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    SEQ.fetch_add(1, Ordering::Relaxed)
}

/// Pull the warehouse rollup table into a local Parquet file via hot-swap rename.
///
/// Returns `true` if a Parquet file was written and renamed into place,
/// `false` if `pull_rollup` produced zero rows (no file on disk).
async fn materialize_parquet(
    entry: &oxy_airlayer_compat::preagg::ManifestEntry,
    connector: &Arc<dyn DatabaseConnector>,
    cache_dir: &std::path::Path,
    rebuild_ctx: &RebuildContext,
) -> Result<bool, String> {
    let dialect = connector_to_airlayer_dialect(connector.dialect());

    let parquet_filename = format!("{}__{}.parquet", entry.view_name, entry.rollup_hash);
    let final_path = cache_dir.join(&parquet_filename);
    // Run-scoped, not a fixed `.tmp`: two cycles rebuilding the same rollup —
    // a schedule tick racing a Rebuild click, or two nodes draining the same
    // workspace — would otherwise write the same temp file concurrently, and
    // whichever renamed second would publish a Parquet the other was still
    // writing into. The pid+counter suffix makes each build's staging file its
    // own, so the rename is the only thing they contend on.
    let temp_path = cache_dir.join(format!(
        "{parquet_filename}.{}.{}.tmp",
        std::process::id(),
        next_build_seq()
    ));

    let fq_table = if let Some((s, t)) = entry.table_name.split_once('.') {
        dialect.qualify_table(s, t)
    } else {
        dialect.qualify_table(&rebuild_ctx.schema, &entry.table_name)
    };

    let file_written = agentic_semantic::preagg::pull_rollup(connector, &fq_table, &temp_path)
        .await
        .map_err(|e| e.to_string())?;

    if file_written {
        agentic_semantic::preagg::hot_swap_parquet(&temp_path, &final_path)
            .map_err(|e| e.to_string())?;
    }

    Ok(file_written)
}

// ── Phase 3: manifest + cache ─────────────────────────────────────────────────

/// Update the local manifest and seed the in-memory refresh-key cache.
///
/// Called after a successful Parquet pull, with the caller already holding the
/// per-workspace publish lock — the hot-swap and this write are one atomic
/// publish, so the lock cannot be taken here (see `rebuild_rollup`).
async fn commit_manifest_and_cache(
    entry: &oxy_airlayer_compat::preagg::ManifestEntry,
    rollup_hash: &str,
    current_refresh_key_value: &Option<String>,
    cache_dir: &std::path::Path,
    database_name: &str,
    cache: &Arc<RwLock<RefreshKeyCache>>,
) -> Result<(), String> {
    let parquet_filename = format!("{}__{}.parquet", entry.view_name, entry.rollup_hash);
    let measures = serde_json::from_str(&entry.measures_json)
        .map_err(|e| format!("measures_json parse error: {e}"))?;
    let local_entry = oxy_airlayer_compat::preagg::LocalRollupEntry {
        view_name: entry.view_name.clone(),
        rollup_name: entry.rollup_name.clone(),
        rollup_hash: entry.rollup_hash.clone(),
        file: parquet_filename,
        dimensions: entry.dimensions.clone(),
        measures,
        time_dimension: entry.time_dimension.clone(),
        granularity: entry.granularity.clone(),
        build_date: entry.build_date.clone(),
        refresh_key_value: current_refresh_key_value.clone(),
        refresh_key_checked_at: Some(chrono::Utc::now().to_rfc3339()),
    };

    let cache_dir_owned = cache_dir.to_path_buf();
    let database_name_owned = database_name.to_string();
    let rollup_hash_owned = rollup_hash.to_string();
    let local_entry_owned = local_entry;

    tokio::task::spawn_blocking(move || {
        let mut manifest = agentic_semantic::preagg::load_local_manifest(&cache_dir_owned)
            .unwrap_or_else(|| oxy_airlayer_compat::preagg::LocalManifest {
                pulled_at: chrono::Utc::now().to_rfc3339(),
                source_database: database_name_owned,
                rollups: vec![],
            });

        if let Some(existing) = manifest
            .rollups
            .iter_mut()
            .find(|r| r.rollup_hash == rollup_hash_owned)
        {
            *existing = local_entry_owned;
        } else {
            manifest.rollups.push(local_entry_owned);
        }
        manifest.pulled_at = chrono::Utc::now().to_rfc3339();

        agentic_semantic::preagg::save_local_manifest(&cache_dir_owned, &manifest)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("manifest write task panicked: {e}"))??;

    {
        let mut guard = cache.write().expect("preagg cache lock poisoned");
        guard.insert(rollup_hash.to_string(), current_refresh_key_value.clone());
    }

    Ok(())
}

// ── Utility ───────────────────────────────────────────────────────────────────

pub(super) fn connector_to_airlayer_dialect(dialect: SqlDialect) -> oxy_airlayer_compat::Dialect {
    match dialect {
        SqlDialect::Snowflake => oxy_airlayer_compat::Dialect::Snowflake,
        SqlDialect::BigQuery => oxy_airlayer_compat::Dialect::BigQuery,
        SqlDialect::DuckDb => oxy_airlayer_compat::Dialect::DuckDB,
        SqlDialect::Postgres => oxy_airlayer_compat::Dialect::Postgres,
        SqlDialect::Other(s) if s.to_lowercase().contains("clickhouse") => {
            oxy_airlayer_compat::Dialect::ClickHouse
        }
        ref unknown => {
            tracing::warn!(
                dialect = ?unknown,
                "preagg: unrecognized connector dialect, falling back to Postgres"
            );
            oxy_airlayer_compat::Dialect::Postgres
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{build_layer_engine, plan_rollup_build};

    /// `orders.order` rolls up to `customers.customer`: the parent entity is
    /// owned by a DIFFERENT view, which is the case the planner used to be
    /// blind to.
    fn orders_view() -> oxy_airlayer_compat::View {
        serde_yaml::from_str(
            r#"
name: orders
table: orders
refresh_key:
  every: "1h"
pre_aggregations:
  - name: orders_by_month
    dimensions: [status]
    measures: [order_count]
    time_dimension: ordered_at
    granularity: month
entities:
  - name: order
    type: primary
    key: order_id
    parent: customer
  - name: customer
    type: foreign
    key: customer_id
dimensions:
  - name: order_id
    type: string
    expr: order_id
  - name: customer_id
    type: string
    expr: customer_id
  - name: status
    type: string
    expr: status
  - name: ordered_at
    type: datetime
    expr: ordered_at
measures:
  - name: order_count
    type: count
    expr: order_id
"#,
        )
        .expect("orders view fixture parses")
    }

    fn customers_view() -> oxy_airlayer_compat::View {
        serde_yaml::from_str(
            r#"
name: customers
table: customers
entities:
  - name: customer
    type: primary
    key: customer_id
dimensions:
  - name: customer_id
    type: string
    expr: customer_id
measures:
  - name: customer_count
    type: count
    expr: customer_id
"#,
        )
        .expect("customers view fixture parses")
    }

    fn plan_orders_rollup(layer_views: Vec<oxy_airlayer_compat::View>) -> Result<usize, String> {
        let view = orders_view();
        let rollup = oxy_airlayer_compat::preagg::resolve_rollups(&view)
            .into_iter()
            .find(|r| r.name == "orders_by_month")
            .expect("declared rollup resolves");
        let engine = build_layer_engine(layer_views, &oxy_airlayer_compat::Dialect::DuckDB)?;
        plan_rollup_build(
            &view,
            &rollup,
            &None,
            &engine,
            "preagg",
            "20260825T000000",
            &oxy_airlayer_compat::Dialect::DuckDB,
        )
        .map(|plan| plan.manifest_entries.len())
    }

    #[test]
    fn a_rollup_on_a_view_whose_entity_has_a_cross_view_parent_plans() {
        // The regression: `order` declares `parent: customer`, and `customer`
        // is a primary entity on another view. Planning must see both.
        let entries =
            plan_orders_rollup(vec![orders_view(), customers_view()]).expect("plan succeeds");
        assert_eq!(entries, 1, "exactly the targeted rollup is planned");
    }

    #[test]
    fn planning_that_view_alone_is_what_used_to_fail() {
        // Pins the cause, so a future refactor that quietly narrows the engine
        // back to one view fails here rather than in production. The failure
        // now lands in `build_layer_engine` — which is the point of hoisting
        // it: a layer that will not validate says so ONCE per cycle, not once
        // per rollup.
        let err = plan_orders_rollup(vec![orders_view()])
            .expect_err("a one-view layer cannot resolve the parent");
        assert!(
            err.contains("customer"),
            "expected the dead-end hierarchy error, got: {err}"
        );
    }

    #[test]
    fn only_the_targeted_view_is_built_even_though_the_engine_sees_the_layer() {
        // Engine scope ≠ generation scope: `customers` is in the layer for
        // resolution, but nothing of its own is built.
        let view = orders_view();
        let rollup = oxy_airlayer_compat::preagg::resolve_rollups(&view)
            .into_iter()
            .find(|r| r.name == "orders_by_month")
            .expect("declared rollup resolves");
        let engine = build_layer_engine(
            vec![orders_view(), customers_view()],
            &oxy_airlayer_compat::Dialect::DuckDB,
        )
        .expect("the layer validates");
        let plan = plan_rollup_build(
            &view,
            &rollup,
            &None,
            &engine,
            "preagg",
            "20260825T000000",
            &oxy_airlayer_compat::Dialect::DuckDB,
        )
        .expect("plan succeeds");
        assert!(
            plan.manifest_entries
                .iter()
                .all(|e| e.view_name == "orders"),
            "no other view's rollups leaked into the plan"
        );
    }
}
