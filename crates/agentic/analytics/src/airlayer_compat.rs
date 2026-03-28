//! Compatibility layer between oxy's YAML format and airlayer types.
//!
//! This module provides:
//! - YAML parsing shims for `.view.yml` / `.topic.yml` files that may differ
//!   slightly from airlayer's expected format (e.g. optional `description`).
//! - Dialect mapping from [`agentic_connector::SqlDialect`] to [`airlayer::Dialect`].
//! - Parameter substitution for airlayer's parameterised SQL output.

use std::collections::HashMap;
use std::sync::Arc;

use serde::Deserialize;

use agentic_connector::DatabaseConnector;
use airlayer::DatasourceDialectMap;

// ── YAML shim types ──────────────────────────────────────────────────────────
//
// airlayer's `View` has `description: String` (non-optional) while oxy YAML
// files may omit it.  These thin wrappers add `#[serde(default)]` where needed
// and convert into the real airlayer types.

/// Intermediate view representation for oxy YAML files.
#[derive(Debug, Deserialize)]
struct ViewShim {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    label: Option<String>,
    /// Oxy YAML may use `data_source` or `datasource`.
    #[serde(default, alias = "data_source")]
    datasource: Option<String>,
    #[serde(default)]
    dialect: Option<String>,
    #[serde(default)]
    table: Option<String>,
    #[serde(default)]
    sql: Option<String>,
    #[serde(default)]
    entities: Vec<airlayer::Entity>,
    #[serde(default)]
    dimensions: Vec<airlayer::Dimension>,
    #[serde(default)]
    measures: Option<Vec<airlayer::Measure>>,
    #[serde(default)]
    segments: Vec<airlayer::schema::models::Segment>,
}

/// Intermediate topic representation for oxy YAML files.
#[derive(Debug, Deserialize)]
struct TopicShim {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    views: Vec<String>,
    #[serde(default)]
    base_view: Option<String>,
    #[serde(default)]
    retrieval: Option<airlayer::schema::models::TopicRetrievalConfig>,
    #[serde(default)]
    default_filters: Option<Vec<airlayer::schema::models::TopicFilter>>,
}

// ── YAML parsing ─────────────────────────────────────────────────────────────

/// Parse an oxy `.view.yml` string into an `airlayer::View`.
///
/// Handles differences from airlayer's strict format:
/// - `description` defaults to `""` when absent
/// - `data_source` accepted as alias for `datasource`
pub fn parse_view_yaml(
    yaml: &str,
) -> Result<airlayer::View, Box<dyn std::error::Error + Send + Sync>> {
    let shim: ViewShim = serde_yaml::from_str(yaml)?;
    Ok(airlayer::View {
        name: shim.name,
        description: shim.description,
        label: shim.label,
        datasource: shim.datasource,
        dialect: shim.dialect,
        table: shim.table,
        sql: shim.sql,
        entities: shim.entities,
        dimensions: shim.dimensions,
        measures: shim.measures,
        segments: shim.segments,
    })
}

/// Parse an oxy `.topic.yml` string into an `airlayer::Topic`.
pub fn parse_topic_yaml(
    yaml: &str,
) -> Result<airlayer::Topic, Box<dyn std::error::Error + Send + Sync>> {
    let shim: TopicShim = serde_yaml::from_str(yaml)?;
    Ok(airlayer::Topic {
        name: shim.name,
        description: shim.description,
        views: shim.views,
        base_view: shim.base_view,
        retrieval: shim.retrieval,
        default_filters: shim.default_filters,
    })
}

// ── Dialect mapping ──────────────────────────────────────────────────────────

/// Convert an [`agentic_connector::SqlDialect`] to an [`airlayer::Dialect`].
///
/// Returns `None` for unknown / `Other` dialects that airlayer does not support.
pub fn convert_dialect(dialect: agentic_connector::SqlDialect) -> Option<airlayer::Dialect> {
    // Use airlayer's own from_str which handles aliases like "pg", "duck", etc.
    airlayer::Dialect::from_str(dialect.as_str())
}

/// Build an [`airlayer::DatasourceDialectMap`] from the solver's connector map.
///
/// Each connector's logical name is mapped to its SQL dialect.  The `default`
/// connector's dialect is set as the map's default for views that don't specify
/// a `datasource:` field.
pub fn build_dialect_map(
    connectors: &HashMap<String, Arc<dyn DatabaseConnector>>,
    default: &str,
) -> DatasourceDialectMap {
    let mut map = DatasourceDialectMap::new();

    for (name, connector) in connectors {
        if let Some(dialect) = convert_dialect(connector.dialect()) {
            map.insert(name, dialect);
        }
    }

    // Set the default connector's dialect as the map default.
    if let Some(connector) = connectors.get(default) {
        if let Some(dialect) = convert_dialect(connector.dialect()) {
            map.set_default(dialect);
        }
    } else if let Some((_name, connector)) = connectors.iter().next() {
        // Fallback: use the first connector if default is not found.
        if let Some(dialect) = convert_dialect(connector.dialect()) {
            map.set_default(dialect);
        }
    }

    map
}

// ── Parameter substitution ───────────────────────────────────────────────────

/// Substitute positional parameter placeholders (`$1`, `$2`, ... and `@p0`,
/// `@p1`, ...) or `?` placeholders with escaped string literals.
///
/// Airlayer returns parameterised SQL but the agentic connector trait sends
/// raw SQL with no separate parameter binding.
///
/// Copied from `crates/workflow/src/semantic_builder.rs`.
pub fn substitute_params(sql: &str, params: &[String]) -> String {
    if params.is_empty() {
        return sql.to_string();
    }

    let uses_positional = (0..params.len())
        .any(|i| sql.contains(&format!("${}", i + 1)) || sql.contains(&format!("@p{}", i)));

    let mut result = sql.to_string();

    if uses_positional {
        // Replace $1, $2, ... and @p0, @p1, ... (right-to-left to avoid prefix
        // collision, e.g. $1 inside $10).
        for (i, param) in params.iter().enumerate().rev() {
            let escaped = param.replace('\'', "''");
            let literal = format!("'{}'", escaped);
            result = result.replace(&format!("${}", i + 1), &literal);
            result = result.replace(&format!("@p{}", i), &literal);
        }
    } else {
        // Replace ? placeholders left-to-right (MySQL/Snowflake/SQLite).
        let mut param_index = 0;
        while result.contains('?') && param_index < params.len() {
            let escaped = params[param_index].replace('\'', "''");
            let literal = format!("'{}'", escaped);
            result = result.replacen('?', &literal, 1);
            param_index += 1;
        }
    }

    result
}
