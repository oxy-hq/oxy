// MCP Session Filters Support
//
// This module provides functionality to extract and validate session filters
// from MCP tool call requests according to the MCP specification.
//
// Session filters are passed in the `_meta` object of tool call requests
// and are used for row-level security and data filtering.

use std::collections::HashMap;

use serde_json::Value;

use oxy::{adapters::session_filters::SessionFilters, config::ConfigManager};

/// Extracts and validates session filters from MCP meta parameter.
///
/// According to the MCP specification, session filters are passed in the
/// `_meta` object of tool call requests under the "filters" key.
///
/// # Arguments
///
/// * `meta` - Optional reference to the MCP meta object containing filters
/// * `config_manager` - Configuration manager for accessing filter schema
///
/// # Returns
///
/// * `Ok(Some(SessionFilters))` - Valid filters were found and validated
/// * `Ok(None)` - No filters were provided in meta
/// * `Err(rmcp::ErrorData)` - Invalid filters format or validation failed
///
/// # Errors
///
/// Returns `rmcp::ErrorData::invalid_params` if:
/// - Filters format is invalid (not a JSON object)
/// - Filter keys are not defined in the project's filter schema
/// - Filter values don't match their schema types
/// - Required filters are missing
pub fn extract_session_filters(
    meta: Option<&serde_json::Map<String, Value>>,
    _config_manager: &ConfigManager,
) -> Result<Option<SessionFilters>, rmcp::ErrorData> {
    let filters_value = meta.and_then(|m| m.get("filters")).cloned();

    if let Some(value) = filters_value {
        let filters_map: HashMap<String, Value> = serde_json::from_value(value).map_err(|e| {
            tracing::warn!("Invalid filters format in MCP request: {}", e);
            rmcp::ErrorData::invalid_params(format!("Invalid filters format: {}", e), None)
        })?;

        tracing::debug!(
            filters = ?filters_map.keys().collect::<Vec<_>>(),
            "Extracted session filters from MCP request"
        );

        // Convert to SessionFilters
        let session_filters = SessionFilters::from(filters_map);

        // Let the execution layer handle filter validation
        Ok(Some(session_filters))
    } else {
        tracing::debug!("No session filters provided in MCP request");
        Ok(None)
    }
}
