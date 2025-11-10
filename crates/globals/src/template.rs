use crate::errors::{GlobalError, GlobalResult};
use crate::registry::GlobalRegistry;
use regex::Regex;
use serde_yaml::Value;
use std::sync::OnceLock;

/// Template engine for resolving {{globals.path}} expressions in YAML values
pub struct TemplateEngine<'a> {
    registry: &'a GlobalRegistry,
}

impl<'a> TemplateEngine<'a> {
    /// Create a new TemplateEngine with the given GlobalRegistry
    pub fn new(registry: &'a GlobalRegistry) -> Self {
        Self { registry }
    }

    /// Resolve all template expressions in a YAML Value
    ///
    /// This method recursively walks through the YAML structure and replaces
    /// any {{globals.path}} expressions with their corresponding values from
    /// the GlobalRegistry.
    ///
    /// # Arguments
    ///
    /// * `value` - The YAML value that may contain template expressions
    ///
    /// # Returns
    ///
    /// A new YAML value with all template expressions resolved
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Template syntax is invalid
    /// - Referenced global path does not exist
    /// - Registry cannot be accessed
    pub fn resolve_templates(&self, value: &Value) -> GlobalResult<Value> {
        match value {
            Value::String(s) => self.resolve_string_templates(s),
            Value::Mapping(m) => {
                let mut resolved_map = serde_yaml::Mapping::new();
                for (k, v) in m {
                    let resolved_key = self.resolve_templates(k)?;
                    let resolved_value = self.resolve_templates(v)?;
                    resolved_map.insert(resolved_key, resolved_value);
                }
                Ok(Value::Mapping(resolved_map))
            }
            Value::Sequence(seq) => {
                let mut resolved_seq = Vec::new();
                for item in seq {
                    resolved_seq.push(self.resolve_templates(item)?);
                }
                Ok(Value::Sequence(resolved_seq))
            }
            // For other value types (Number, Bool, Null), return as-is
            _ => Ok(value.clone()),
        }
    }

    /// Resolve template expressions in a string value
    fn resolve_string_templates(&self, s: &str) -> GlobalResult<Value> {
        static TEMPLATE_REGEX: OnceLock<Regex> = OnceLock::new();

        let regex = TEMPLATE_REGEX.get_or_init(|| {
            Regex::new(r"\{\{\s*globals\.([^}]+?)\s*\}\}")
                .expect("Failed to compile template regex")
        });

        // Check if the string contains any template expressions
        if !regex.is_match(s) {
            return Ok(Value::String(s.to_string()));
        }

        // If the entire string is a single template expression, return the raw value
        if let Some(captures) = regex.captures(s) {
            let full_match = captures.get(0).unwrap().as_str();
            if full_match == s {
                // This is a pure template expression like "{{globals.tables.users}}"
                let path = captures.get(1).unwrap().as_str().trim();
                return self.resolve_global_path(path);
            }
        }

        // Handle mixed content like "{{globals.files.users}}.csv"
        let mut result = s.to_string();
        for captures in regex.captures_iter(s) {
            let full_match = captures.get(0).unwrap().as_str();
            let path = captures.get(1).unwrap().as_str();

            let resolved_value = self.resolve_global_path(path)?;

            // Convert resolved value to string for substitution
            let replacement = match resolved_value {
                Value::String(s) => s,
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                Value::Null => "null".to_string(),
                _ => {
                    return Err(GlobalError::InvalidYamlStructure {
                        file: "template".to_string(),
                        error: format!(
                            "Template expression {{{{globals.{}}}}} resolved to complex object, cannot substitute in string",
                            path
                        ),
                    });
                }
            };

            result = result.replace(full_match, &replacement);
        }

        Ok(Value::String(result))
    }

    /// Resolve a specific global path to its value
    fn resolve_global_path(&self, path: &str) -> GlobalResult<Value> {
        // Parse the path into file and object path components
        let parts: Vec<&str> = path.split('.').collect();
        if parts.is_empty() {
            return Err(GlobalError::InvalidObjectPath(format!(
                "Empty global path in template: {}",
                path
            )));
        }

        let file_name = parts[0];
        let object_path = if parts.len() > 1 {
            parts[1..].join(".")
        } else {
            return Err(GlobalError::InvalidObjectPath(format!(
                "Global path must have at least file.path format: {}",
                path
            )));
        };

        // Check for runtime overrides first
        let override_key = format!("{}.{}", file_name, object_path);
        if let Ok(overrides) = self.registry.get_overrides() {
            if let Some(override_value) = overrides.get(&override_key) {
                return Ok(override_value.clone());
            }
        }

        // Fallback to file-based values
        self.registry.get_object_by_path(file_name, &object_path)
    }
}

/// Helper trait to add template resolution to GlobalRegistry
pub trait TemplateResolver {
    /// Resolve templates in a YAML value using this registry
    fn resolve_templates(&self, value: &Value) -> GlobalResult<Value>;

    /// Check if a string contains template expressions
    fn has_templates(&self, s: &str) -> bool;
}

impl TemplateResolver for GlobalRegistry {
    fn resolve_templates(&self, value: &Value) -> GlobalResult<Value> {
        let engine = TemplateEngine::new(self);
        engine.resolve_templates(value)
    }

    fn has_templates(&self, s: &str) -> bool {
        static TEMPLATE_REGEX: OnceLock<Regex> = OnceLock::new();

        let regex = TEMPLATE_REGEX.get_or_init(|| {
            Regex::new(r"\{\{globals\.([^}]+)\}\}").expect("Failed to compile template regex")
        });

        regex.is_match(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml::Value;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_globals() -> (TempDir, GlobalRegistry) {
        let temp_dir = TempDir::new().unwrap();
        let globals_dir = temp_dir.path().join("globals");
        fs::create_dir(&globals_dir).unwrap();

        // Create test files
        let tables_content = r#"
production:
  users: "prod_users_table"
  orders: "prod_orders_table"

development:
  users: "dev_users_table"
  orders: "dev_orders_table"
"#;
        fs::write(globals_dir.join("tables.yml"), tables_content).unwrap();

        let files_content = r#"
datasets:
  users: "user_data_2024"
  products: "product_catalog_v2"

config:
  port: 8080
  enabled: true
"#;
        fs::write(globals_dir.join("files.yml"), files_content).unwrap();

        let registry = GlobalRegistry::new(&globals_dir);
        (temp_dir, registry)
    }

    #[test]
    fn test_simple_template_resolution() {
        let (_temp_dir, registry) = create_test_globals();

        let template = Value::String("{{globals.tables.production.users}}".to_string());
        let result = registry.resolve_templates(&template).unwrap();

        assert_eq!(result, Value::String("prod_users_table".to_string()));
    }

    #[test]
    fn test_mixed_template_resolution() {
        let (_temp_dir, registry) = create_test_globals();

        let template = Value::String("{{globals.files.datasets.users}}.csv".to_string());
        let result = registry.resolve_templates(&template).unwrap();

        assert_eq!(result, Value::String("user_data_2024.csv".to_string()));
    }

    #[test]
    fn test_nested_yaml_template_resolution() {
        let (_temp_dir, registry) = create_test_globals();

        let yaml = serde_yaml::from_str::<Value>(
            r#"
name: test_view
table: "{{globals.tables.production.users}}"
file_path: "{{globals.files.datasets.users}}.csv"
config:
  port: "{{globals.files.config.port}}"
"#,
        )
        .unwrap();

        let result = registry.resolve_templates(&yaml).unwrap();

        let expected = serde_yaml::from_str::<Value>(
            r#"
name: test_view
table: "prod_users_table"
file_path: "user_data_2024.csv"
config:
  port: 8080
"#,
        )
        .unwrap();

        assert_eq!(result, expected);
    }

    #[test]
    fn test_has_templates() {
        let (_temp_dir, registry) = create_test_globals();

        assert!(registry.has_templates("{{globals.tables.users}}"));
        assert!(registry.has_templates("prefix_{{globals.tables.users}}_suffix"));
        assert!(!registry.has_templates("no templates here"));
        assert!(!registry.has_templates("{{ not_globals.path }}"));
    }

    #[test]
    fn test_template_with_overrides() {
        let (_temp_dir, registry) = create_test_globals();

        // Set an override
        registry
            .set_override(
                "tables",
                "production.users",
                Value::String("override_table".to_string()),
            )
            .unwrap();

        let template = Value::String("{{globals.tables.production.users}}".to_string());
        let result = registry.resolve_templates(&template).unwrap();

        assert_eq!(result, Value::String("override_table".to_string()));
    }

    #[test]
    fn test_invalid_template_path() {
        let (_temp_dir, registry) = create_test_globals();

        let template = Value::String("{{globals.nonexistent.path}}".to_string());
        let result = registry.resolve_templates(&template);

        assert!(result.is_err());
    }

    #[test]
    fn test_complex_object_in_string_template() {
        let (_temp_dir, registry) = create_test_globals();

        // This should fail because we're trying to substitute a complex object into a string
        let template = Value::String("prefix_{{globals.tables.production}}_suffix".to_string());
        let result = registry.resolve_templates(&template);

        assert!(result.is_err());
    }

    #[test]
    fn test_apply_global_overrides() {
        let (_temp_dir, registry) = create_test_globals();

        // Apply some global overrides
        let mut overrides = indexmap::IndexMap::new();
        overrides.insert(
            "tables.production.users".to_string(),
            Value::String("overridden_users_table".to_string()),
        );
        overrides.insert(
            "files.datasets.products".to_string(),
            Value::String("overridden_products".to_string()),
        );

        registry.apply_global_overrides(overrides).unwrap();

        // Test that templates resolve to the overridden values
        let template1 = Value::String("{{globals.tables.production.users}}".to_string());
        let result1 = registry.resolve_templates(&template1).unwrap();
        assert_eq!(result1, Value::String("overridden_users_table".to_string()));

        let template2 = Value::String("{{globals.files.datasets.products}}".to_string());
        let result2 = registry.resolve_templates(&template2).unwrap();
        assert_eq!(result2, Value::String("overridden_products".to_string()));

        // Test that non-overridden values still work
        let template3 = Value::String("{{globals.tables.production.orders}}".to_string());
        let result3 = registry.resolve_templates(&template3).unwrap();
        assert_eq!(result3, Value::String("prod_orders_table".to_string()));
    }

    #[test]
    fn test_validate_template_paths() {
        let (_temp_dir, registry) = create_test_globals();

        // Valid templates
        let valid_yaml = serde_yaml::from_str::<Value>(
            r#"
table: "{{globals.tables.production.users}}"
file: "{{globals.files.datasets.users}}.csv"
"#,
        )
        .unwrap();

        let errors = registry.validate_template_paths(&valid_yaml).unwrap();
        assert!(errors.is_empty());

        // Invalid templates
        let invalid_yaml = serde_yaml::from_str::<Value>(
            r#"
table: "{{globals.nonexistent.table}}"
file: "{{globals.tables.missing.file}}"
"#,
        )
        .unwrap();

        let errors = registry.validate_template_paths(&invalid_yaml).unwrap();
        assert_eq!(errors.len(), 2);
        assert!(errors[0].contains("nonexistent.table"));
        assert!(errors[1].contains("missing.file"));
    }

    #[test]
    fn test_invalid_global_override_path() {
        let (_temp_dir, registry) = create_test_globals();

        let mut overrides = indexmap::IndexMap::new();
        overrides.insert(
            "invalid_path_without_dot".to_string(),
            Value::String("test".to_string()),
        );

        let result = registry.apply_global_overrides(overrides);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("must contain at least one dot")
        );
    }
}
