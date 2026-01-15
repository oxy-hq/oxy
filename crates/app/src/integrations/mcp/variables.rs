// MCP Variables Support
//
// This module provides functionality to extract, merge, and validate variables
// from MCP tool call requests according to the MCP specification.
//
// Variables are passed in the `_meta` object of tool call requests and are
// merged with tool arguments following a specific precedence order.

use std::collections::HashMap;

use serde_json::Value;
use tracing::debug;

/// Extracts variables from MCP meta parameter.
///
/// According to the MCP specification, variables can be passed in the
/// `_meta` object of tool call requests under the "variables" key.
///
/// # Arguments
///
/// * `meta` - Optional reference to the MCP meta object containing variables
///
/// # Returns
///
/// * `Ok(HashMap<String, Value>)` - Variables extracted from meta (empty if none)
/// * `Err(rmcp::ErrorData)` - Invalid variables format
///
/// # Errors
///
/// Returns `rmcp::ErrorData::invalid_params` if:
/// - Variables format is invalid (not a JSON object)
pub fn extract_meta_variables(
    meta: Option<&serde_json::Map<String, Value>>,
) -> Result<HashMap<String, Value>, rmcp::ErrorData> {
    // Extract variables value from meta object
    let variables_value = meta.and_then(|m| m.get("variables")).cloned();

    if let Some(value) = variables_value {
        // Parse variables into HashMap
        serde_json::from_value(value).map_err(|e| {
            rmcp::ErrorData::invalid_params(
                format!("Invalid variables format in _meta: {}", e),
                None,
            )
        })
    } else {
        Ok(HashMap::new())
    }
}

/// Merges variables from multiple sources with proper precedence.
///
/// Variables are merged in the following order (later overrides earlier):
/// 1. Default values from resource definition
/// 2. Variables from tool arguments
/// 3. Variables from _meta["variables"]
///
/// # Arguments
///
/// * `defaults` - Default variable values from resource definition
/// * `meta_vars` - Variables from MCP _meta parameter
/// * `arg_vars` - Variables from tool arguments
///
/// # Returns
///
/// * `HashMap<String, Value>` - Merged variables with proper precedence applied
///
/// # Examples
///
/// ```ignore
/// let defaults = HashMap::from([("currency".to_string(), json!("USD"))]);
/// let meta_vars = HashMap::from([("currency".to_string(), json!("EUR"))]);
/// let arg_vars = HashMap::from([("include_tax".to_string(), json!(false))]);
///
/// let merged = merge_variables(defaults, meta_vars, arg_vars);
/// // Result: {"currency": "EUR", "include_tax": false}
/// ```
pub fn merge_variables(
    defaults: HashMap<String, Value>,
    meta_vars: HashMap<String, Value>,
    arg_vars: HashMap<String, Value>,
) -> HashMap<String, Value> {
    let mut merged = defaults;

    // Apply argument variables (override defaults)
    for (key, value) in arg_vars {
        merged.insert(key, value);
    }

    // Apply meta variables (override both defaults and arguments)
    for (key, value) in meta_vars {
        merged.insert(key, value);
    }

    debug!(
        variables = ?merged.keys().collect::<Vec<_>>(),
        "Merged variables from defaults, arguments, and meta"
    );

    merged
}

/// Validates merged variables against a resource schema.
///
/// This function validates that:
/// - All required variables are present
/// - Variable types match the schema (if type information is available)
/// - Unknown variables are handled appropriately
///
/// # Arguments
///
/// * `variables` - The merged variables to validate
/// * `required_vars` - Optional list of required variable names
///
/// # Returns
///
/// * `Ok(())` - Variables are valid
/// * `Err(rmcp::ErrorData)` - Validation failed
///
/// # Errors
///
/// Returns `rmcp::ErrorData::invalid_params` if:
/// - Required variables are missing
/// - Variable types don't match schema expectations (future enhancement)
///
/// # Note
///
/// Currently, this function only validates required variables. Full type
/// validation will be added in a future enhancement when we have better
/// schema introspection capabilities.
pub fn validate_variables(
    variables: &HashMap<String, Value>,
    required_vars: Option<&[String]>,
) -> Result<(), rmcp::ErrorData> {
    if let Some(required) = required_vars {
        for var_name in required {
            if !variables.contains_key(var_name) {
                return Err(rmcp::ErrorData::invalid_params(
                    format!("Required variable '{}' is missing", var_name),
                    None,
                ));
            }
        }
    }

    // TODO: Implement type validation when schema introspection is available
    // For now, we rely on the execution layer to handle type mismatches

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_meta_variables_with_valid_variables() {
        let mut meta = serde_json::Map::new();
        meta.insert(
            "variables".to_string(),
            json!({
                "user_id": "user_123",
                "organization_id": "org_456"
            }),
        );

        let result = extract_meta_variables(Some(&meta)).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result.get("user_id").unwrap(), &json!("user_123"));
        assert_eq!(result.get("organization_id").unwrap(), &json!("org_456"));
    }

    #[test]
    fn test_extract_meta_variables_with_no_variables() {
        let meta = serde_json::Map::new();

        let result = extract_meta_variables(Some(&meta)).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_extract_meta_variables_with_none() {
        let result = extract_meta_variables(None).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_extract_meta_variables_with_invalid_format() {
        let mut meta = serde_json::Map::new();
        meta.insert("variables".to_string(), json!("invalid")); // Should be object

        let result = extract_meta_variables(Some(&meta));

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .message
                .contains("Invalid variables format")
        );
    }

    #[test]
    fn test_merge_variables_with_all_sources() {
        let defaults = HashMap::from([
            ("currency".to_string(), json!("USD")),
            ("include_tax".to_string(), json!(true)),
        ]);

        let meta_vars = HashMap::from([
            ("currency".to_string(), json!("EUR")),
            ("organization_id".to_string(), json!("org_123")),
        ]);

        let arg_vars = HashMap::from([("include_tax".to_string(), json!(false))]);

        let merged = merge_variables(defaults, meta_vars, arg_vars);

        // Currency should be from meta (overrides both default and args)
        assert_eq!(merged.get("currency").unwrap(), &json!("EUR"));
        // include_tax should be from args (overrides default, but meta doesn't set it)
        assert_eq!(merged.get("include_tax").unwrap(), &json!(false));
        // organization_id should be from meta (new variable)
        assert_eq!(merged.get("organization_id").unwrap(), &json!("org_123"));
    }

    #[test]
    fn test_merge_variables_precedence_order() {
        // Test that meta overrides arguments, and arguments override defaults
        let defaults = HashMap::from([("var".to_string(), json!("default"))]);
        let meta_vars = HashMap::from([("var".to_string(), json!("meta"))]);
        let arg_vars = HashMap::from([("var".to_string(), json!("arg"))]);

        let merged = merge_variables(defaults, meta_vars, arg_vars);

        assert_eq!(merged.get("var").unwrap(), &json!("meta"));
    }

    #[test]
    fn test_merge_variables_empty_sources() {
        let defaults = HashMap::new();
        let meta_vars = HashMap::new();
        let arg_vars = HashMap::new();

        let merged = merge_variables(defaults, meta_vars, arg_vars);

        assert!(merged.is_empty());
    }

    #[test]
    fn test_validate_variables_with_all_required_present() {
        let variables = HashMap::from([
            ("user_id".to_string(), json!("user_123")),
            ("organization_id".to_string(), json!("org_456")),
        ]);

        let required = vec!["user_id".to_string(), "organization_id".to_string()];

        let result = validate_variables(&variables, Some(&required));

        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_variables_with_missing_required() {
        let variables = HashMap::from([("user_id".to_string(), json!("user_123"))]);

        let required = vec!["user_id".to_string(), "organization_id".to_string()];

        let result = validate_variables(&variables, Some(&required));

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .message
                .contains("Required variable 'organization_id' is missing")
        );
    }

    #[test]
    fn test_validate_variables_with_no_required() {
        let variables = HashMap::from([("user_id".to_string(), json!("user_123"))]);

        let result = validate_variables(&variables, None);

        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_variables_empty_with_no_required() {
        let variables = HashMap::new();

        let result = validate_variables(&variables, None);

        assert!(result.is_ok());
    }
}
