use minijinja::{Environment, Value};
use regex::Regex;
use serde_json::Value as JsonValue;
use std::collections::HashMap;

use crate::variables::{VariableEncoder, VariableError};

/// Trait for resolving variables in semantic layer expressions
pub trait VariableResolver {
    /// Resolve a single variable path to its value
    fn resolve_variable(&self, variable_path: &str) -> Result<JsonValue, VariableError>;

    /// Resolve all variables in an expression template
    fn resolve_expression(&self, expr: &str) -> Result<String, VariableError>;

    /// Check if an expression contains variables
    fn has_variables(&self, expr: &str) -> bool;

    /// Get all variable paths referenced in an expression
    fn extract_variables(&self, expr: &str) -> Vec<String>;
}

/// Runtime variable resolver that integrates with minijinja template engine
pub struct RuntimeVariableResolver {
    /// Minijinja environment for template rendering
    env: Environment<'static>,
    /// Variable context containing all available variables
    context: Value,
    /// Variable encoder for handling encoded placeholders
    encoder: VariableEncoder,
    /// Regex for detecting variable patterns
    variable_regex: Regex,
}

impl RuntimeVariableResolver {
    /// Create a new resolver with the given variable context
    pub fn new(context: JsonValue) -> Result<Self, VariableError> {
        let mut env = Environment::new();

        // Add support for template evaluation
        env.set_auto_escape_callback(|_| minijinja::AutoEscape::None);

        Ok(RuntimeVariableResolver {
            env,
            context: Value::from_serialize(context),
            encoder: VariableEncoder::new(),
            variable_regex: Regex::new(r"\{\{variables\.([^}]+)\}\}")
                .map_err(|e| VariableError::InvalidSyntax(format!("Invalid regex: {}", e)))?,
        })
    }

    /// Create a resolver from multiple variable sources with priority order
    pub fn from_sources(
        workflow_vars: Option<HashMap<String, JsonValue>>,
        agent_vars: Option<HashMap<String, JsonValue>>,
        globals: Option<HashMap<String, JsonValue>>,
        env_vars: Option<HashMap<String, JsonValue>>,
    ) -> Result<Self, VariableError> {
        let mut context = JsonValue::Object(serde_json::Map::new());

        // Build context with priority order (later sources override earlier ones)
        if let Some(env) = env_vars
            && let JsonValue::Object(ref mut ctx) = context
        {
            for (key, value) in env {
                ctx.insert(key, value);
            }
        }

        if let Some(globals) = globals
            && let JsonValue::Object(ref mut ctx) = context
        {
            for (key, value) in globals {
                ctx.insert(key, value);
            }
        }

        if let Some(agent) = agent_vars
            && let JsonValue::Object(ref mut ctx) = context
        {
            for (key, value) in agent {
                ctx.insert(key, value);
            }
        }

        if let Some(workflow) = workflow_vars
            && let JsonValue::Object(ref mut ctx) = context
        {
            for (key, value) in workflow {
                ctx.insert(key, value);
            }
        }

        // Wrap in variables namespace
        let variables_context = serde_json::json!({
            "variables": context
        });

        Self::new(variables_context)
    }

    /// Resolve variables in encoded SQL from CubeJS
    pub fn resolve_sql_variables(&self, encoded_sql: String) -> Result<String, VariableError> {
        // First decode any encoded variables back to template syntax
        let decoded_sql = self.encoder.decode_all_variables(&encoded_sql);

        // Then resolve the template variables
        self.resolve_expression(&decoded_sql)
    }

    /// Extract nested value from context using dot notation
    fn get_nested_value(&self, path: &str) -> Option<JsonValue> {
        // Use minijinja's built-in path resolution
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = self.context.clone();

        for part in parts {
            if let Ok(value) = current.get_attr(part) {
                // Check if the value is undefined (which means the key doesn't exist)
                if value.is_undefined() {
                    return None;
                }
                current = value;
            } else {
                return None;
            }
        }

        // Convert minijinja Value to JsonValue
        let json_value = serde_json::to_value(current).ok()?;

        // Return None if the final value is null (meaning key doesn't exist)
        if json_value.is_null() {
            None
        } else {
            Some(json_value)
        }
    }
}

impl VariableResolver for RuntimeVariableResolver {
    fn resolve_variable(&self, variable_path: &str) -> Result<JsonValue, VariableError> {
        // For paths starting with 'variables.', use the full path
        let lookup_path = if variable_path.starts_with("variables.") {
            variable_path
        } else {
            &format!("variables.{}", variable_path)
        };

        self.get_nested_value(lookup_path)
            .ok_or_else(|| VariableError::VariableNotFound(variable_path.to_string()))
    }

    fn resolve_expression(&self, expr: &str) -> Result<String, VariableError> {
        if !self.has_variables(expr) {
            return Ok(expr.to_string());
        }

        // Use minijinja to render the template
        let template = self.env.template_from_str(expr).map_err(|e| {
            VariableError::InvalidSyntax(format!("Template compilation failed: {}", e))
        })?;

        let result = template.render(&self.context).map_err(|e| {
            if e.to_string().contains("undefined") {
                // Extract variable name from error message if possible
                let var_name = self
                    .variable_regex
                    .captures(expr)
                    .and_then(|caps| caps.get(1))
                    .map(|m| m.as_str())
                    .unwrap_or("unknown");
                VariableError::VariableNotFound(var_name.to_string())
            } else {
                VariableError::InvalidSyntax(format!("Template rendering failed: {}", e))
            }
        })?;

        Ok(result)
    }

    fn has_variables(&self, expr: &str) -> bool {
        self.variable_regex.is_match(expr)
    }

    fn extract_variables(&self, expr: &str) -> Vec<String> {
        self.variable_regex
            .captures_iter(expr)
            .map(|caps| caps.get(1).unwrap().as_str().to_string())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_resolve_simple_variable() {
        let context = json!({
            "variables": {
                "table_name": "users",
                "schema": "public"
            }
        });

        let resolver = RuntimeVariableResolver::new(context).unwrap();

        let result = resolver.resolve_variable("table_name").unwrap();
        assert_eq!(result, JsonValue::String("users".to_string()));
    }

    #[test]
    fn test_resolve_nested_variable() {
        let context = json!({
            "variables": {
                "database": {
                    "schema": "analytics",
                    "table": "events"
                }
            }
        });

        let resolver = RuntimeVariableResolver::new(context).unwrap();

        let result = resolver.resolve_variable("database.schema").unwrap();
        assert_eq!(result, JsonValue::String("analytics".to_string()));
    }

    #[test]
    fn test_resolve_expression() {
        let context = json!({
            "variables": {
                "schema": "prod",
                "table": "users"
            }
        });

        let resolver = RuntimeVariableResolver::new(context).unwrap();

        let result = resolver
            .resolve_expression("{{variables.schema}}.{{variables.table}}")
            .unwrap();
        assert_eq!(result, "prod.users");
    }

    #[test]
    fn test_resolve_expression_with_conditional() {
        let context = json!({
            "variables": {
                "env": "prod",
                "table": "users"
            }
        });

        let resolver = RuntimeVariableResolver::new(context).unwrap();

        let result = resolver
            .resolve_expression(
                "{{ 'production_' if variables.env == 'prod' else 'dev_' }}{{variables.table}}",
            )
            .unwrap();
        assert_eq!(result, "production_users");
    }

    #[test]
    fn test_has_variables() {
        let resolver = RuntimeVariableResolver::new(json!({})).unwrap();

        assert!(resolver.has_variables("{{variables.test}}"));
        assert!(resolver.has_variables("SELECT * FROM {{variables.table}}"));
        assert!(!resolver.has_variables("SELECT * FROM users"));
    }

    #[test]
    fn test_extract_variables() {
        let resolver = RuntimeVariableResolver::new(json!({})).unwrap();

        let vars = resolver.extract_variables("{{variables.schema}}.{{variables.table}}");
        assert_eq!(vars, vec!["schema", "table"]);
    }

    #[test]
    fn test_variable_not_found() {
        let context = json!({
            "variables": {
                "existing": "value"
            }
        });

        let resolver = RuntimeVariableResolver::new(context).unwrap();

        let result = resolver.resolve_variable("missing");
        assert!(matches!(result, Err(VariableError::VariableNotFound(_))));
    }

    #[test]
    fn test_from_sources_priority() {
        let workflow_vars = HashMap::from([
            ("table".to_string(), json!("workflow_table")),
            ("schema".to_string(), json!("workflow_schema")),
        ]);

        let agent_vars = HashMap::from([("table".to_string(), json!("agent_table"))]);

        let resolver = RuntimeVariableResolver::from_sources(
            Some(workflow_vars),
            Some(agent_vars),
            None,
            None,
        )
        .unwrap();

        // Workflow variables should override agent variables
        let table = resolver.resolve_variable("table").unwrap();
        assert_eq!(table, JsonValue::String("workflow_table".to_string()));

        // Agent variables should be used when workflow doesn't override
        let schema = resolver.resolve_variable("schema").unwrap();
        assert_eq!(schema, JsonValue::String("workflow_schema".to_string()));
    }

    #[test]
    fn test_resolve_sql_variables() {
        let context = json!({
            "variables": {
                "table_name": "users",
                "id_column": "user_id"
            }
        });

        let resolver = RuntimeVariableResolver::new(context).unwrap();

        // Simulate encoded SQL from CubeJS using hex encoding
        // "id_column" hex = "69645f636f6c756d6e", "table_name" hex = "7461626c655f6e616d65"
        let encoded_sql = "SELECT __VAR_69645f636f6c756d6e__ FROM __VAR_7461626c655f6e616d65__";

        let result = resolver
            .resolve_sql_variables(encoded_sql.to_string())
            .unwrap();
        assert_eq!(result, "SELECT user_id FROM users");
    }
}
