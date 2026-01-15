use crate::config::schema_type_converter;
use oxy_shared::errors::OxyError;
use schemars::schema::SchemaObject;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, ops::Deref};
use utoipa::ToSchema;

/// Type-safe wrapper for session filters that have been validated against JSON schemas
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SessionFilters(pub HashMap<String, Value>);

impl From<HashMap<String, Value>> for SessionFilters {
    fn from(map: HashMap<String, Value>) -> Self {
        Self(map)
    }
}

/// Allows compiler to convert SessionFilters to HashMap<String, Value>, while still
/// enforcing type safety. Necessary for more ergonomic API - i.e. `filters.get("key")` vs `filters.0.get("key")`
impl Deref for SessionFilters {
    type Target = HashMap<String, Value>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Helper struct to validate and process filters using JSON Schema
pub struct FilterProcessor {
    schemas: HashMap<String, SchemaObject>,
}

impl FilterProcessor {
    /// Create a new FilterProcessor with the given filter schemas
    pub fn new(schemas: HashMap<String, SchemaObject>) -> Self {
        Self { schemas }
    }

    /// Validate and convert filter values according to their schemas
    pub fn process_filters(&self, filters: SessionFilters) -> Result<SessionFilters, OxyError> {
        let mut result = HashMap::new();

        // Check for required filters (those without default values in schema)
        for (key, schema) in &self.schemas {
            if let Some(filter_value) = filters.get(key) {
                // Validate and convert the filter value according to its schema
                let converted_value =
                    self.convert_value_to_schema_type(filter_value, schema, key)?;
                result.insert(key.clone(), converted_value);
            } else if let Some(metadata) = &schema.metadata {
                if let Some(default_value) = &metadata.default {
                    result.insert(key.clone(), default_value.clone());
                } else {
                    // Log missing required filter for security audit
                    tracing::warn!(
                        filter = %key,
                        provided_filters = ?filters.keys().collect::<Vec<_>>(),
                        "Missing required filter in request"
                    );
                    return Err(OxyError::MissingRequiredFilter {
                        filter: key.clone(),
                    });
                }
            } else {
                // Log missing required filter for security audit
                tracing::warn!(
                    filter = %key,
                    provided_filters = ?filters.keys().collect::<Vec<_>>(),
                    "Missing required filter in request"
                );
                return Err(OxyError::MissingRequiredFilter {
                    filter: key.clone(),
                });
            }
        }

        // Check for unsupported filters (not in schema)
        for key in filters.keys() {
            if !self.schemas.contains_key(key) {
                // Log unsupported filter attempt for security audit
                tracing::warn!(
                    filter = %key,
                    provided_filters = ?filters.keys().collect::<Vec<_>>(),
                    supported_filters = ?self.schemas.keys().collect::<Vec<_>>(),
                    "Unsupported filter provided in request"
                );
                return Err(OxyError::UnsupportedFilter {
                    filter: key.clone(),
                });
            }
        }

        // Log successful filter validation
        tracing::debug!(
            filters = ?result.keys().collect::<Vec<_>>(),
            "Filter validation successful"
        );

        Ok(SessionFilters(result))
    }

    /// Convert a filter value to a database session variable string
    pub fn to_session_value(value: &Value) -> String {
        match value {
            Value::Number(n) => n.to_string(),
            Value::String(s) => s.clone(),
            Value::Bool(b) => b.to_string(),
            Value::Array(arr) => arr
                .iter()
                .map(Self::to_session_value)
                .collect::<Vec<_>>()
                .join(","),
            Value::Null => String::new(),
            Value::Object(_) => {
                // For complex objects, serialize as JSON
                serde_json::to_string(value).unwrap_or_default()
            }
        }
    }

    /// Convert and validate a filter value according to its schema type
    fn convert_value_to_schema_type(
        &self,
        value: &Value,
        schema: &SchemaObject,
        filter_name: &str,
    ) -> Result<Value, OxyError> {
        schema_type_converter::convert_value_to_schema_type(value, schema).map_err(
            |(expected, actual, details)| OxyError::InvalidFilterType {
                filter: filter_name.to_string(),
                expected,
                actual,
                details,
            },
        )
    }
}
