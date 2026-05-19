//! Compatibility layer between oxy's YAML format and airlayer types.
//!
//! This module provides:
//! - YAML parsing shims for `.view.yml` / `.topic.yml` files that may differ
//!   slightly from airlayer's expected format (e.g. optional `description`).
//! - Dialect mapping from [`agentic_connector::SqlDialect`] to [`airlayer::Dialect`].
//! - Parameter substitution for airlayer's parameterised SQL output.

use std::collections::HashMap;
use std::sync::Arc;

use agentic_connector::DatabaseConnector;
use airlayer::DatasourceDialectMap;

// ── YAML parsing ─────────────────────────────────────────────────────────────
//
// The oxy → airlayer YAML shim is the canonical one in `oxy-airlayer-compat`
// (infrastructure) so analytics, the builder validator, the workflow bridge
// and `oxy validate` cannot disagree. Re-exported here so existing
// `crate::airlayer_compat::parse_*_yaml` call sites are unchanged.
// See internal-docs/semantic-validation-standardization.md.
pub use oxy_airlayer_compat::{build_layer, parse_topic_yaml, parse_view_yaml};

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
