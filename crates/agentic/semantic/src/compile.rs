//! Semantic query compilation using airlayer.
//!
//! `.view.yml` / `.topic.yml` discovery + parsing goes through the canonical
//! `oxy-airlayer-compat` shim (NOT airlayer's native directory loader, which
//! rejects oxy's `data_source` alias) so the automation path agrees with
//! analytics and the builder validator. See
//! `internal-docs/semantic-validation-standardization.md`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use airlayer::engine::query::{
    FilterOperator, OrderBy, QueryFilter, QueryRequest, TimeDimensionQuery,
};
use airlayer::schema::models::TopicFilterType;
use chrono::{Local, NaiveDate};
use serde_json::Value as JsonValue;

use oxy_shared::substitute_params;

use crate::config::{SemanticFilterType, SemanticQueryConfig, TimeGranularity};
use crate::error::SemanticError;
use crate::refresh_key_cache::RefreshKeyCache;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Result of compiling a semantic query.
///
/// - `Warehouse` — run the SQL against the named warehouse connector.
/// - `Preaggregation` — run `preagg_sql` against an in-memory DuckDB that reads the local pre-aggregated Parquet file.
#[derive(Debug)]
pub enum CompiledQuery {
    Warehouse {
        sql: String,
        database_name: String,
    },
    Preaggregation {
        /// DuckDB rewrite that reads the local Parquet cache.
        preagg_sql: String,
        /// Path to the cached Parquet file backing `preagg_sql`.
        parquet_path: PathBuf,
        /// Warehouse SQL that would have been executed without the preagg
        /// short-circuit. Surfaced to users/agents so they see the logical
        /// query, not the DuckDB rewrite.
        warehouse_sql: String,
        /// Logical warehouse the query targets (from the view datasource).
        /// Surfaced for display even when execution short-circuits to
        /// local DuckDB.
        warehouse_database: String,
    },
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Resolve a semantic query against the semantic layer and compile to SQL.
///
/// `cache` is the optional Layer-1 in-process refresh key cache. When `None`
/// (CLI, tests), local Parquet is served without freshness validation.
pub fn resolve_and_compile(
    scan_path: &Path,
    databases: &[airlayer::DatabaseConfig],
    task: &SemanticQueryConfig,
    cache: Option<Arc<RwLock<RefreshKeyCache>>>,
    renewal_threshold_secs: u64,
    pre_loaded_layer: Option<airlayer::SemanticLayer>,
) -> Result<CompiledQuery, SemanticError> {
    let dialects = airlayer::DatasourceDialectMap::from_config_databases(databases);

    // Use the caller-supplied layer when available (avoids re-reading the
    // workspace from disk on hot paths). Fall back to the canonical
    // shim-based discovery + parse when not provided.
    let layer = match pre_loaded_layer {
        Some(l) => l,
        None => oxy_airlayer_compat::load_layer_from_dir(scan_path)
            .map_err(|e| SemanticError::Runtime(format!("semantic engine error: {e}")))?,
    };
    let engine = airlayer::SemanticEngine::from_semantic_layer(layer, dialects)
        .map_err(|e| SemanticError::Runtime(format!("semantic engine error: {e}")))?;

    let semantic_layer = engine.semantic_layer();

    let topic = resolve_topic(semantic_layer, task)?;

    // Get database from views.
    let views: Vec<&airlayer::View> = semantic_layer
        .views
        .iter()
        .filter(|v| topic.views.contains(&v.name))
        .collect();

    let database_name = views
        .iter()
        .find_map(|v| v.datasource.clone())
        .ok_or_else(|| {
            SemanticError::Validation(format!("No datasource found for topic '{}'", topic.name))
        })?;

    // Build date fields for filter normalization.
    let date_fields = collect_date_fields(&views);

    let request = build_query_request(
        task,
        &topic.name,
        topic.base_view.as_ref(),
        topic.default_filters.as_ref(),
        &date_fields,
    )?;

    // Compile to SQL.
    let result = engine
        .compile_query(&request)
        .map_err(|e| SemanticError::Runtime(format!("query compilation error: {e}")))?;

    let sql = substitute_params(&result.sql, &result.params);

    // Check local Parquet cache with freshness validation (Layer 1).
    // Only attempt this path when a cache (and therefore a background worker) is present.
    // Without a running worker there is no guarantee the local Parquet is up-to-date,
    // so CLI, tests, and the builder context always compile to warehouse SQL.
    if let Some(ref cache_arc) = cache
        && let Some(local) = try_resolve_local_parquet(
            scan_path,
            &request,
            cache_arc,
            renewal_threshold_secs,
            &sql,
            &database_name,
        )
    {
        return Ok(local);
    }

    Ok(CompiledQuery::Warehouse { sql, database_name })
}

/// Compile a semantic query using a pre-built `SemanticEngine`.
///
/// Use this when compiling multiple queries against the same schema — build
/// the engine once with [`airlayer::SemanticEngine::from_semantic_layer`] and
/// call this for each query so the expensive engine-build cost is paid once.
/// Returns only the SQL string; no preagg check is performed.
pub fn compile_with_engine(
    engine: &airlayer::SemanticEngine,
    task: &SemanticQueryConfig,
) -> Result<String, SemanticError> {
    let semantic_layer = engine.semantic_layer();
    let topic = resolve_topic(semantic_layer, task)?;
    let views: Vec<&airlayer::View> = semantic_layer
        .views
        .iter()
        .filter(|v| topic.views.contains(&v.name))
        .collect();
    let date_fields = collect_date_fields(&views);
    let request = build_query_request(
        task,
        &topic.name,
        topic.base_view.as_ref(),
        topic.default_filters.as_ref(),
        &date_fields,
    )?;
    let result = engine
        .compile_query(&request)
        .map_err(|e| SemanticError::Runtime(format!("query compilation error: {e}")))?;
    Ok(substitute_params(&result.sql, &result.params))
}

/// Look up the local-Parquet manifest, check coverage + freshness, and
/// return a `CompiledQuery::Preaggregation` if all conditions are met.
///
/// Extracted so the analytics solver can reuse the freshness/seed dance
/// without duplicating the manifest-loading code.
pub fn try_resolve_local_parquet(
    scan_path: &Path,
    request: &QueryRequest,
    cache: &Arc<RwLock<RefreshKeyCache>>,
    renewal_threshold_secs: u64,
    warehouse_sql: &str,
    warehouse_database: &str,
) -> Option<CompiledQuery> {
    let cache_dir = oxy_shared::state_dir::get_airlayer_cache_dir(scan_path);
    let manifest_path = cache_dir.join("manifest.json");
    let content = std::fs::read_to_string(&manifest_path).ok()?;
    let manifest: airlayer::preagg::LocalManifest = serde_json::from_str(&content).ok()?;

    let covering_entry = airlayer::preagg::check_coverage(request, &manifest.rollups)?;
    let rollup_hash = covering_entry.rollup_hash.clone();
    let manifest_value = covering_entry.refresh_key_value.clone();
    let is_fresh = check_and_seed_freshness(
        cache,
        &rollup_hash,
        manifest_value.as_deref(),
        renewal_threshold_secs,
    );

    let resolution = airlayer::preagg::resolve_local(request, &manifest, &cache_dir)?;
    if let airlayer::preagg::PreaggResolution::LocalParquet {
        reagg_sql: preagg_sql,
        parquet_path,
    } = resolution
    {
        if !is_fresh {
            tracing::debug!(
                rollup_hash = %rollup_hash,
                "preagg: serving stale Parquet, background rebuild pending"
            );
        }
        return Some(CompiledQuery::Preaggregation {
            preagg_sql,
            parquet_path: PathBuf::from(parquet_path),
            warehouse_sql: warehouse_sql.to_string(),
            warehouse_database: warehouse_database.to_string(),
        });
    }
    None
}

/// Decide whether the cached refresh-key entry is still fresh against the
/// manifest's stored value. On a cache miss, seeds the cache from the
/// manifest and reports stale so the background worker picks it up next
/// heartbeat.
pub fn check_and_seed_freshness(
    cache: &Arc<RwLock<RefreshKeyCache>>,
    rollup_hash: &str,
    manifest_value: Option<&str>,
    renewal_threshold_secs: u64,
) -> bool {
    let threshold = Duration::from_secs(renewal_threshold_secs);
    let guard = cache.read().expect("preagg cache lock poisoned");
    if let Some(entry) = guard.get(rollup_hash, threshold) {
        return entry.value.as_deref() == manifest_value;
    }
    drop(guard);
    let mut wguard = cache.write().expect("preagg cache lock poisoned");
    wguard.insert(rollup_hash.to_string(), manifest_value.map(String::from));
    false
}

/// Get the database (datasource) name from the first view that has one.
pub fn get_database_from_views(views: &[airlayer::View]) -> Option<String> {
    views.iter().find_map(|v| v.datasource.clone())
}

// ---------------------------------------------------------------------------
// Internal: topic resolution
// ---------------------------------------------------------------------------

fn resolve_topic(
    semantic_layer: &airlayer::SemanticLayer,
    task: &SemanticQueryConfig,
) -> Result<airlayer::Topic, SemanticError> {
    let empty = Vec::new();
    let topics = semantic_layer.topics.as_ref().unwrap_or(&empty);

    if let Some(topic_name) = &task.topic {
        topics
            .iter()
            .find(|t| t.name == *topic_name)
            .cloned()
            .ok_or_else(|| {
                let available: Vec<_> = topics.iter().map(|t| t.name.clone()).collect();
                SemanticError::Validation(format!(
                    "Topic '{}' not found. Available: {:?}",
                    topic_name, available
                ))
            })
    } else {
        let mut view_names = HashSet::new();
        for dim in &task.dimensions {
            if let Some((view, _)) = dim.split_once('.') {
                view_names.insert(view.to_string());
            }
        }
        for td in &task.time_dimensions {
            if let Some((view, _)) = td.dimension.split_once('.') {
                view_names.insert(view.to_string());
            }
        }
        for m in &task.measures {
            if let Some((view, _)) = m.split_once('.') {
                view_names.insert(view.to_string());
            }
        }
        if view_names.is_empty() {
            return Err(SemanticError::Validation(
                "No dimensions or measures specified".to_string(),
            ));
        }
        Ok(airlayer::Topic {
            name: "adhoc_query".to_string(),
            description: Some("Ad-hoc query inferred from views".to_string()),
            views: view_names.into_iter().collect(),
            base_view: None,
            retrieval: None,
            default_filters: None,
            meta: None,
        })
    }
}

// ---------------------------------------------------------------------------
// Internal: date field tracking
// ---------------------------------------------------------------------------

fn collect_date_fields(views: &[&airlayer::View]) -> HashSet<String> {
    use airlayer::schema::models::DimensionType;
    let mut date_fields = HashSet::new();
    for view in views {
        for dim in &view.dimensions {
            if matches!(
                dim.dimension_type,
                DimensionType::Date | DimensionType::Datetime
            ) {
                date_fields.insert(format!("{}.{}", view.name, dim.name));
            }
        }
    }
    date_fields
}

fn normalize_date_value(date: &str) -> Result<String, SemanticError> {
    if NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok() {
        return Ok(date.to_string());
    }
    let result = chrono_english::parse_date_string(date, Local::now(), chrono_english::Dialect::Us)
        .map_err(|e| {
            SemanticError::Runtime(format!(
                "Failed to parse date '{}': {}. Expected YYYY-MM-DD or relative expression.",
                date, e
            ))
        })?;
    Ok(result.format("%Y-%m-%d").to_string())
}

// ---------------------------------------------------------------------------
// Internal: query request building
// ---------------------------------------------------------------------------

fn build_query_request(
    task: &SemanticQueryConfig,
    topic_name: &str,
    base_view: Option<&String>,
    default_filters: Option<&Vec<airlayer::schema::models::TopicFilter>>,
    date_fields: &HashSet<String>,
) -> Result<QueryRequest, SemanticError> {
    let mut filters = Vec::new();

    if let Some(defaults) = default_filters {
        for df in defaults {
            let field = qualify_field(&df.field, topic_name);
            let (op, vals) = convert_topic_filter_type(&df.filter_type, &field, date_fields)?;
            filters.push(QueryFilter {
                member: Some(field),
                operator: Some(op),
                values: vals,
                and: None,
                or: None,
            });
        }
    }

    for f in &task.filters {
        let field = qualify_field(&f.field, topic_name);
        let (op, vals) = convert_semantic_filter_type(&f.filter_type, &field, date_fields)?;
        filters.push(QueryFilter {
            member: Some(field),
            operator: Some(op),
            values: vals,
            and: None,
            or: None,
        });
    }

    let order: Vec<OrderBy> = task
        .orders
        .iter()
        .map(|o| OrderBy {
            id: qualify_field(&o.field, topic_name),
            desc: o.direction.to_lowercase() == "desc",
        })
        .collect();

    let time_dimensions: Vec<TimeDimensionQuery> = task
        .time_dimensions
        .iter()
        .map(|td| TimeDimensionQuery {
            dimension: qualify_field(&td.dimension, topic_name),
            granularity: td.granularity.as_ref().map(granularity_to_string),
            date_range: None,
        })
        .collect();

    let through = base_view.map(|bv| vec![bv.clone()]).unwrap_or_default();

    Ok(QueryRequest {
        measures: task.measures.clone(),
        dimensions: task.dimensions.clone(),
        filters,
        segments: vec![],
        time_dimensions,
        order,
        limit: task.limit,
        offset: task.offset,
        timezone: None,
        ungrouped: false,
        through,
        motif: None,
        motif_params: Default::default(),
    })
}

fn qualify_field(field: &str, topic_name: &str) -> String {
    if field.contains('.') {
        field.to_string()
    } else {
        format!("{topic_name}.{field}")
    }
}

fn granularity_to_string(g: &TimeGranularity) -> String {
    match g {
        TimeGranularity::Year => "year",
        TimeGranularity::Quarter => "quarter",
        TimeGranularity::Month => "month",
        TimeGranularity::Week => "week",
        TimeGranularity::Day => "day",
        TimeGranularity::Hour => "hour",
        TimeGranularity::Minute => "minute",
        TimeGranularity::Second => "second",
    }
    .to_string()
}

// ---------------------------------------------------------------------------
// Internal: filter conversion
// ---------------------------------------------------------------------------

fn convert_topic_filter_type(
    ft: &TopicFilterType,
    field: &str,
    date_fields: &HashSet<String>,
) -> Result<(FilterOperator, Vec<String>), SemanticError> {
    match ft {
        TopicFilterType::Eq(f) => Ok((
            FilterOperator::Equals,
            vec![jv2s(&f.value, field, date_fields)?],
        )),
        TopicFilterType::Neq(f) => Ok((
            FilterOperator::NotEquals,
            vec![jv2s(&f.value, field, date_fields)?],
        )),
        TopicFilterType::Gt(f) => Ok((
            FilterOperator::Gt,
            vec![jv2s(&f.value, field, date_fields)?],
        )),
        TopicFilterType::Gte(f) => Ok((
            FilterOperator::Gte,
            vec![jv2s(&f.value, field, date_fields)?],
        )),
        TopicFilterType::Lt(f) => Ok((
            FilterOperator::Lt,
            vec![jv2s(&f.value, field, date_fields)?],
        )),
        TopicFilterType::Lte(f) => Ok((
            FilterOperator::Lte,
            vec![jv2s(&f.value, field, date_fields)?],
        )),
        TopicFilterType::In(f) => {
            let v = f
                .values
                .iter()
                .map(|v| jv2s(v, field, date_fields))
                .collect::<Result<Vec<_>, _>>()?;
            Ok((FilterOperator::Equals, v))
        }
        TopicFilterType::NotIn(f) => {
            let v = f
                .values
                .iter()
                .map(|v| jv2s(v, field, date_fields))
                .collect::<Result<Vec<_>, _>>()?;
            Ok((FilterOperator::NotEquals, v))
        }
        TopicFilterType::InDateRange(f) => Ok((
            FilterOperator::InDateRange,
            vec![
                jv2s(&f.from, field, date_fields)?,
                jv2s(&f.to, field, date_fields)?,
            ],
        )),
        TopicFilterType::NotInDateRange(f) => Ok((
            FilterOperator::NotInDateRange,
            vec![
                jv2s(&f.from, field, date_fields)?,
                jv2s(&f.to, field, date_fields)?,
            ],
        )),
    }
}

fn convert_semantic_filter_type(
    ft: &SemanticFilterType,
    field: &str,
    date_fields: &HashSet<String>,
) -> Result<(FilterOperator, Vec<String>), SemanticError> {
    match ft {
        SemanticFilterType::Eq(f) => Ok((
            FilterOperator::Equals,
            vec![jv2s(&f.value, field, date_fields)?],
        )),
        SemanticFilterType::Neq(f) => Ok((
            FilterOperator::NotEquals,
            vec![jv2s(&f.value, field, date_fields)?],
        )),
        SemanticFilterType::Gt(f) => Ok((
            FilterOperator::Gt,
            vec![jv2s(&f.value, field, date_fields)?],
        )),
        SemanticFilterType::Gte(f) => Ok((
            FilterOperator::Gte,
            vec![jv2s(&f.value, field, date_fields)?],
        )),
        SemanticFilterType::Lt(f) => Ok((
            FilterOperator::Lt,
            vec![jv2s(&f.value, field, date_fields)?],
        )),
        SemanticFilterType::Lte(f) => Ok((
            FilterOperator::Lte,
            vec![jv2s(&f.value, field, date_fields)?],
        )),
        SemanticFilterType::In(f) => {
            let v = f
                .values
                .iter()
                .map(|v| jv2s(v, field, date_fields))
                .collect::<Result<Vec<_>, _>>()?;
            Ok((FilterOperator::Equals, v))
        }
        SemanticFilterType::NotIn(f) => {
            let v = f
                .values
                .iter()
                .map(|v| jv2s(v, field, date_fields))
                .collect::<Result<Vec<_>, _>>()?;
            Ok((FilterOperator::NotEquals, v))
        }
        SemanticFilterType::InDateRange(f) => Ok((
            FilterOperator::InDateRange,
            vec![
                jv2s(&f.from, field, date_fields)?,
                jv2s(&f.to, field, date_fields)?,
            ],
        )),
        SemanticFilterType::NotInDateRange(f) => Ok((
            FilterOperator::NotInDateRange,
            vec![
                jv2s(&f.from, field, date_fields)?,
                jv2s(&f.to, field, date_fields)?,
            ],
        )),
        SemanticFilterType::Contains(f) => Ok((
            FilterOperator::Contains,
            vec![jv2s(&f.value, field, date_fields)?],
        )),
    }
}

/// JSON value to string, with date normalization for date fields.
fn jv2s(
    value: &JsonValue,
    field: &str,
    date_fields: &HashSet<String>,
) -> Result<String, SemanticError> {
    let s = match value {
        JsonValue::String(s) => s.clone(),
        JsonValue::Number(n) => n.to_string(),
        JsonValue::Bool(b) => b.to_string(),
        JsonValue::Null => {
            return Err(SemanticError::Runtime(format!(
                "NULL filter value for '{field}'"
            )));
        }
        other => serde_json::to_string(other).unwrap_or_default(),
    };
    if date_fields.contains(field) {
        return normalize_date_value(&s);
    }
    Ok(s)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qualify_field() {
        assert_eq!(qualify_field("revenue", "orders"), "orders.revenue");
        assert_eq!(qualify_field("orders.revenue", "orders"), "orders.revenue");
    }

    #[test]
    fn test_get_database_from_views() {
        let views = vec![
            airlayer::View {
                name: "v1".into(),
                description: None,
                label: None,
                datasource: None,
                dialect: None,
                table: Some("t".into()),
                sql: None,
                entities: vec![],
                dimensions: vec![],
                measures: None,
                segments: vec![],
                pre_aggregations: None,
                refresh_key: None,
                meta: None,
            },
            airlayer::View {
                name: "v2".into(),
                description: None,
                label: None,
                datasource: Some("my_db".into()),
                dialect: None,
                table: Some("t".into()),
                sql: None,
                entities: vec![],
                dimensions: vec![],
                measures: None,
                segments: vec![],
                pre_aggregations: None,
                refresh_key: None,
                meta: None,
            },
        ];
        assert_eq!(get_database_from_views(&views), Some("my_db".to_string()));
    }
}
