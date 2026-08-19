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
// (infrastructure) so analytics, the builder validator, the automation bridge
// and `oxy validate` cannot disagree. Re-exported here so existing
// `crate::airlayer_compat::parse_*_yaml` call sites are unchanged.
// See internal-docs/semantic-validation-standardization.md.
pub use oxy_airlayer_compat::{build_layer, parse_topic_yaml, parse_view_yaml};

// ── Dialect mapping ──────────────────────────────────────────────────────────

/// Convert an [`agentic_connector::SqlDialect`] to an [`airlayer::Dialect`].
///
/// Returns `None` for unknown / `Other` dialects that airlayer does not support.
pub fn convert_dialect(dialect: agentic_connector::SqlDialect) -> Option<airlayer::Dialect> {
    let result = airlayer::Dialect::from_str(dialect.as_str());
    if result.is_none() {
        tracing::warn!(
            connector_dialect = %dialect.as_str(),
            "convert_dialect: unrecognized dialect, skipping connector in dialect map"
        );
    }
    result
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

    if let Some(connector) = connectors.get(default) {
        if let Some(dialect) = convert_dialect(connector.dialect()) {
            map.set_default(dialect);
        }
    } else if let Some((fallback_name, connector)) = connectors.iter().next() {
        tracing::warn!(
            missing = default,
            chosen = %fallback_name,
            "build_dialect_map: named default connector not found, using first available as fallback"
        );
        if let Some(dialect) = convert_dialect(connector.dialect()) {
            map.set_default(dialect);
        }
    }

    map
}

// ── Parameter substitution ───────────────────────────────────────────────────

pub use oxy_shared::substitute_params;
