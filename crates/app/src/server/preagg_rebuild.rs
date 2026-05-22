//! Rebuild helpers for the pre-aggregation background worker.
//!
//! Each function owns one phase of the rebuild pipeline:
//! `rebuild_rollup` orchestrates them in sequence.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use tokio::sync::Mutex as TokioMutex;

use agentic_connector::{DatabaseConnector, SqlDialect};
use agentic_semantic::refresh_key_cache::RefreshKeyCache;
use agentic_workflow::workspace::WorkspaceContext;

use crate::agentic_wiring::OxyProjectContext;

// ── Shared context for a single rollup rebuild within a cycle ─────────────────

#[derive(Clone)]
pub(super) struct RebuildContext {
    pub schema: String,
    pub workspace_path: PathBuf,
    pub database_name: String,
    pub renewal_threshold: Duration,
    pub manifest_write_lock: Arc<TokioMutex<()>>,
}

// ── Orchestrator ──────────────────────────────────────────────────────────────

/// Orchestrate a full rebuild for one stale rollup: CTAS → pull → manifest + cache.
pub(super) async fn rebuild_rollup(
    view: airlayer::View,
    rollup: airlayer::preagg::RollupSpec,
    current_refresh_key_value: Option<String>,
    date_str: String,
    ctx: Arc<OxyProjectContext>,
    rebuild_ctx: RebuildContext,
    cache: Arc<RwLock<RefreshKeyCache>>,
) -> Result<(), String> {
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

    let cache_dir = oxy_shared::state_dir::get_airlayer_cache_dir(&rebuild_ctx.workspace_path);
    tokio::fs::create_dir_all(&cache_dir)
        .await
        .map_err(|e| e.to_string())?;

    let file_written = materialize_parquet(&entry, &connector, &cache_dir, &rebuild_ctx).await?;

    // Empty rollups (zero-row warehouse result) write no Parquet file. Committing
    // a manifest entry that points to a non-existent file poisons the read path:
    // the next semantic query would match the entry, resolve to `LocalParquet`,
    // and then fail with "Parquet cache file not found" — with no warehouse
    // fallback. Skip the commit until a future rebuild produces real rows.
    if !file_written {
        tracing::info!(
            rollup = %rollup.name,
            "preagg: rebuild produced zero rows; skipping manifest commit"
        );
        return Ok(());
    }

    commit_manifest_and_cache(
        &entry,
        &rollup.hash,
        &current_refresh_key_value,
        &cache_dir,
        &rebuild_ctx.database_name,
        &cache,
        &rebuild_ctx.manifest_write_lock,
    )
    .await?;

    tracing::info!(rollup = %rollup.name, "preagg: rebuild complete");
    Ok(())
}

// ── Phase 1: warehouse CTAS ───────────────────────────────────────────────────

/// Execute the warehouse CTAS statement for a single rollup.
///
/// Marks every other rollup in the view as fresh so `collect_build_sql` only
/// generates a CTAS for the target rollup, avoiding invalid SQL from other
/// rollups (e.g. aggregate-only rollups with no GROUP BY).
///
/// Returns the `ManifestEntry` for the rebuilt rollup on success.
async fn build_warehouse_table(
    view: &airlayer::View,
    rollup: &airlayer::preagg::RollupSpec,
    current_refresh_key_value: &Option<String>,
    date_str: &str,
    connector: &Arc<dyn DatabaseConnector>,
    rebuild_ctx: &RebuildContext,
) -> Result<airlayer::preagg::ManifestEntry, String> {
    let dialect = connector_to_airlayer_dialect(connector.dialect());

    let all_rollups = airlayer::preagg::resolve_rollups(view);
    let freshness: Vec<airlayer::preagg::RollupFreshness> = all_rollups
        .iter()
        .map(|r| airlayer::preagg::RollupFreshness {
            rollup_hash: r.hash.clone(),
            is_fresh: r.hash != rollup.hash,
            current_refresh_key_value: if r.hash == rollup.hash {
                current_refresh_key_value.clone()
            } else {
                None
            },
        })
        .collect();

    let plan = airlayer::preagg::collect_build_sql(
        &[view],
        &rebuild_ctx.schema,
        date_str,
        &dialect,
        None,
        Some(&freshness),
    )
    .map_err(|e| e.to_string())?;

    agentic_semantic::preagg::execute_build_plan(connector, &plan)
        .await
        .map_err(|e| e.to_string())?;

    plan.manifest_entries
        .into_iter()
        .find(|e| e.rollup_hash == rollup.hash)
        .ok_or_else(|| format!("manifest entry not found for {}", rollup.hash))
}

// ── Phase 2: Parquet pull ─────────────────────────────────────────────────────

/// Pull the warehouse rollup table into a local Parquet file via hot-swap rename.
///
/// Returns `true` if a Parquet file was written and renamed into place,
/// `false` if `pull_rollup` produced zero rows (no file on disk).
async fn materialize_parquet(
    entry: &airlayer::preagg::ManifestEntry,
    connector: &Arc<dyn DatabaseConnector>,
    cache_dir: &std::path::Path,
    rebuild_ctx: &RebuildContext,
) -> Result<bool, String> {
    let dialect = connector_to_airlayer_dialect(connector.dialect());

    let parquet_filename = format!("{}__{}.parquet", entry.view_name, entry.rollup_hash);
    let final_path = cache_dir.join(&parquet_filename);
    let temp_path = cache_dir.join(format!("{parquet_filename}.tmp"));

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
/// Called after a successful Parquet pull. The cache insert (not invalidate)
/// ensures the next heartbeat tick sees a fresh entry for this rollup.
async fn commit_manifest_and_cache(
    entry: &airlayer::preagg::ManifestEntry,
    rollup_hash: &str,
    current_refresh_key_value: &Option<String>,
    cache_dir: &std::path::Path,
    database_name: &str,
    cache: &Arc<RwLock<RefreshKeyCache>>,
    manifest_write_lock: &Arc<TokioMutex<()>>,
) -> Result<(), String> {
    let parquet_filename = format!("{}__{}.parquet", entry.view_name, entry.rollup_hash);
    let measures = serde_json::from_str(&entry.measures_json)
        .map_err(|e| format!("measures_json parse error: {e}"))?;
    let local_entry = airlayer::preagg::LocalRollupEntry {
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

    let _lock = manifest_write_lock.lock().await;

    let cache_dir_owned = cache_dir.to_path_buf();
    let database_name_owned = database_name.to_string();
    let rollup_hash_owned = rollup_hash.to_string();
    let local_entry_owned = local_entry;

    tokio::task::spawn_blocking(move || {
        let mut manifest = agentic_semantic::preagg::load_local_manifest(&cache_dir_owned)
            .unwrap_or_else(|| airlayer::preagg::LocalManifest {
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

    drop(_lock);

    {
        let mut guard = cache.write().expect("preagg cache lock poisoned");
        guard.insert(rollup_hash.to_string(), current_refresh_key_value.clone());
    }

    Ok(())
}

// ── Utility ───────────────────────────────────────────────────────────────────

pub(super) fn connector_to_airlayer_dialect(dialect: SqlDialect) -> airlayer::Dialect {
    match dialect {
        SqlDialect::Snowflake => airlayer::Dialect::Snowflake,
        SqlDialect::BigQuery => airlayer::Dialect::BigQuery,
        SqlDialect::DuckDb => airlayer::Dialect::DuckDB,
        SqlDialect::Postgres => airlayer::Dialect::Postgres,
        SqlDialect::Other(s) if s.to_lowercase().contains("clickhouse") => {
            airlayer::Dialect::ClickHouse
        }
        ref unknown => {
            tracing::warn!(
                dialect = ?unknown,
                "preagg: unrecognized connector dialect, falling back to Postgres"
            );
            airlayer::Dialect::Postgres
        }
    }
}
