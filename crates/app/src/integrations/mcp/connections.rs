use oxy::config::{ConfigManager, model::ConnectionOverrides};
use serde_json::Value;

/// Extracts and validates connection overrides from MCP meta parameter.
///
/// According to the MCP specification, connection overrides are passed in the
/// `_meta` object of tool call requests under the "connections" key.
///
/// # Arguments
///
/// * `meta` - Optional reference to the MCP meta object containing connection overrides
/// * `config_manager` - Configuration manager for accessing database configuration
///
/// # Returns
///
/// * `Ok(Some(ConnectionOverrides))` - Valid connection overrides were found and validated
/// * `Ok(None)` - No connection overrides were provided in meta
/// * `Err(rmcp::ErrorData)` - Invalid connection overrides format or validation failed
///
/// # Errors
///
/// Returns `rmcp::ErrorData::invalid_params` if:
/// - Connection overrides format is invalid (not a JSON object)
/// - Database name in override doesn't exist in configuration
/// - Override type doesn't match the database type (e.g., ClickHouse override for Snowflake database)
pub fn extract_connection_overrides(
    meta: Option<&serde_json::Map<String, Value>>,
    config_manager: &ConfigManager,
) -> Result<Option<ConnectionOverrides>, rmcp::ErrorData> {
    let connections_value = meta.and_then(|m| m.get("connections")).cloned();

    if let Some(value) = connections_value {
        let connection_overrides: ConnectionOverrides =
            serde_json::from_value(value).map_err(|e| {
                tracing::warn!("Invalid connection overrides format in MCP request: {}", e);
                rmcp::ErrorData::invalid_params(
                    format!("Invalid connection overrides format: {}", e),
                    None,
                )
            })?;

        tracing::debug!(
            databases = ?connection_overrides.keys().collect::<Vec<_>>(),
            "Extracted connection overrides from MCP request"
        );

        // Validate that all database names exist in configuration
        validate_connection_overrides(&connection_overrides, config_manager)?;

        Ok(Some(connection_overrides))
    } else {
        tracing::debug!("No connection overrides provided in MCP request");
        Ok(None)
    }
}

/// Validates connection overrides against the project configuration.
///
/// Ensures that:
/// 1. All database names in overrides exist in the project configuration
/// 2. Override types match the database types (e.g., ClickHouse override for ClickHouse database)
///
/// # Arguments
///
/// * `overrides` - Connection overrides to validate
/// * `config_manager` - Configuration manager for accessing database configuration
///
/// # Returns
///
/// * `Ok(())` - All connection overrides are valid
/// * `Err(rmcp::ErrorData)` - Validation failed
fn validate_connection_overrides(
    overrides: &ConnectionOverrides,
    config_manager: &ConfigManager,
) -> Result<(), rmcp::ErrorData> {
    use oxy::config::model::{ConnectionOverride, DatabaseType};

    let config = config_manager.get_config();
    let databases = &config.databases;

    for (db_name, override_value) in overrides {
        let database = databases
            .iter()
            .find(|db| &db.name == db_name)
            .ok_or_else(|| {
                tracing::warn!(
                    database = db_name,
                    "Database not found in configuration for connection override"
                );
                rmcp::ErrorData::invalid_params(
                    format!("Database '{}' not found in configuration", db_name),
                    None,
                )
            })?;

        // Validate override type matches database type
        match (&database.database_type, override_value) {
            (DatabaseType::ClickHouse(_), ConnectionOverride::ClickHouse(_)) => {
                tracing::debug!(database = db_name, "Valid ClickHouse connection override");
            }
            (DatabaseType::Snowflake(_), ConnectionOverride::Snowflake(_)) => {
                tracing::debug!(database = db_name, "Valid Snowflake connection override");
            }
            (DatabaseType::ClickHouse(_), ConnectionOverride::Snowflake(_)) => {
                return Err(rmcp::ErrorData::invalid_params(
                    format!(
                        "Invalid override type for database '{}': expected ClickHouse override, got Snowflake",
                        db_name
                    ),
                    None,
                ));
            }
            (DatabaseType::Snowflake(_), ConnectionOverride::ClickHouse(_)) => {
                return Err(rmcp::ErrorData::invalid_params(
                    format!(
                        "Invalid override type for database '{}': expected Snowflake override, got ClickHouse",
                        db_name
                    ),
                    None,
                ));
            }
            _ => {
                return Err(rmcp::ErrorData::invalid_params(
                    format!(
                        "Unsupported database type for connection overrides: {}",
                        db_name
                    ),
                    None,
                ));
            }
        }
    }

    Ok(())
}
