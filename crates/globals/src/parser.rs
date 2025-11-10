use crate::errors::{GlobalError, GlobalResult};
use indexmap::IndexMap;
use serde_yaml::Value;
use std::fs;
use std::path::{Path, PathBuf};

/// Parser for global object files
pub struct GlobalParser {
    /// Path to the globals directory
    globals_dir: PathBuf,
}

impl GlobalParser {
    /// Create a new GlobalParser with the given globals directory path
    pub fn new<P: AsRef<Path>>(globals_dir: P) -> Self {
        Self {
            globals_dir: globals_dir.as_ref().to_path_buf(),
        }
    }

    /// Get the globals directory path
    pub fn globals_dir(&self) -> &Path {
        &self.globals_dir
    }

    /// Load a global file by name (without .yml extension)
    pub fn load_file(&self, file_name: &str) -> GlobalResult<IndexMap<String, Value>> {
        let file_path = self.globals_dir.join(format!("{}.yml", file_name));

        if !file_path.exists() {
            return Err(GlobalError::FileNotFound(format!(
                "Global file '{}' not found at path: {}",
                file_name,
                file_path.display()
            )));
        }

        let content = fs::read_to_string(&file_path).map_err(|e| {
            GlobalError::FileReadError(format!(
                "Failed to read file '{}': {}",
                file_path.display(),
                e
            ))
        })?;

        let yaml: IndexMap<String, Value> =
            serde_yaml::from_str(&content).map_err(|e| GlobalError::YamlParseError {
                file: file_name.to_string(),
                error: e.to_string(),
            })?;

        Ok(yaml)
    }

    /// Get an object from a global file using a path (e.g., "entities.customer")
    pub fn get_object_by_path(&self, file_name: &str, path: &str) -> GlobalResult<Value> {
        let yaml = self.load_file(file_name)?;

        // Split path into components
        let components: Vec<&str> = path.split('.').collect();

        if components.is_empty() {
            return Err(GlobalError::InvalidObjectPath(format!(
                "Path '{}' is empty",
                path
            )));
        }

        // Navigate through the path
        let mut current_value = Value::Mapping(
            yaml.into_iter()
                .map(|(k, v)| (Value::String(k), v))
                .collect(),
        );

        for (index, component) in components.iter().enumerate() {
            match &current_value {
                // Array format: [{ name: "Customer", ... }, ...]
                Value::Sequence(items) => {
                    let mut found = false;
                    for item in items {
                        if let Value::Mapping(map) = item {
                            if let Some(Value::String(name)) =
                                map.get(&Value::String("name".to_string()))
                            {
                                if name == component {
                                    current_value = item.clone();
                                    found = true;
                                    break;
                                }
                            }
                        }
                    }
                    if !found {
                        return Err(GlobalError::ObjectNotFound(format!(
                            "Object '{}' not found in path '{}' (component {} of {})",
                            component,
                            path,
                            index + 1,
                            components.len()
                        )));
                    }
                }
                // Map format: { customer_id: { ... }, ... }
                Value::Mapping(map) => {
                    current_value = map
                        .get(&Value::String(component.to_string()))
                        .cloned()
                        .ok_or_else(|| {
                            GlobalError::ObjectNotFound(format!(
                                "Key '{}' not found in path '{}' (component {} of {})",
                                component,
                                path,
                                index + 1,
                                components.len()
                            ))
                        })?;
                }
                _ => {
                    return Err(GlobalError::InvalidYamlStructure {
                        file: file_name.to_string(),
                        error: format!(
                            "Expected array or map at path component '{}', found {:?}",
                            component, current_value
                        ),
                    });
                }
            }
        }

        Ok(current_value)
    }

    /// Check if the globals directory exists
    pub fn globals_dir_exists(&self) -> bool {
        self.globals_dir.exists()
    }

    /// List all global files in the globals directory
    pub fn list_global_files(&self) -> GlobalResult<Vec<String>> {
        if !self.globals_dir.exists() {
            return Ok(Vec::new());
        }

        let entries = fs::read_dir(&self.globals_dir).map_err(|e| {
            GlobalError::FileReadError(format!(
                "Failed to read globals directory '{}': {}",
                self.globals_dir.display(),
                e
            ))
        })?;

        let mut files = Vec::new();
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if let Some(extension) = path.extension() {
                    if extension == "yml" || extension == "yaml" {
                        if let Some(file_stem) = path.file_stem() {
                            if let Some(name) = file_stem.to_str() {
                                files.push(name.to_string());
                            }
                        }
                    }
                }
            }
        }

        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_globals_dir() -> (TempDir, GlobalParser) {
        let temp_dir = TempDir::new().unwrap();
        let globals_dir = temp_dir.path().join("globals");
        fs::create_dir(&globals_dir).unwrap();

        // Create a test semantics.yml file
        let semantics_content = r#"
entities:
  - name: customer
    type: primary
    description: Primary customer entity
    key: customer_id
  - name: product
    type: foreign
    description: Product entity reference
    key: product_id

dimensions:
  - name: customer_id
    type: number
    description: Unique customer identifier
    expr: customer_id
  - name: product_name
    type: string
    description: Product name
    expr: name

measures:
  - name: total_sales
    type: sum
    description: Total sales amount
    expr: amount
  - name: order_count
    type: count
    description: Number of orders
"#;
        fs::write(globals_dir.join("semantics.yml"), semantics_content).unwrap();

        let parser = GlobalParser::new(&globals_dir);
        (temp_dir, parser)
    }

    #[test]
    fn test_load_file() {
        let (_temp_dir, parser) = create_test_globals_dir();
        let result = parser.load_file("semantics");
        assert!(result.is_ok());

        let yaml = result.unwrap();
        assert!(yaml.contains_key("entities"));
        assert!(yaml.contains_key("dimensions"));
        assert!(yaml.contains_key("measures"));
    }

    #[test]
    fn test_load_nonexistent_file() {
        let (_temp_dir, parser) = create_test_globals_dir();
        let result = parser.load_file("nonexistent");
        assert!(result.is_err());
        assert!(matches!(result, Err(GlobalError::FileNotFound(_))));
    }

    #[test]
    fn test_get_object_array_format() {
        let (_temp_dir, parser) = create_test_globals_dir();
        let result = parser.get_object_by_path("semantics", "entities.customer");
        assert!(result.is_ok());

        let object = result.unwrap();
        if let Value::Mapping(map) = object {
            assert_eq!(
                map.get(&Value::String("name".to_string())),
                Some(&Value::String("customer".to_string()))
            );
        } else {
            panic!("Expected mapping");
        }
    }

    #[test]
    fn test_get_object_map_format() {
        let (_temp_dir, parser) = create_test_globals_dir();
        let result = parser.get_object_by_path("semantics", "measures.total_sales");
        assert!(result.is_ok());

        let object = result.unwrap();
        if let Value::Mapping(map) = object {
            assert_eq!(
                map.get(&Value::String("type".to_string())),
                Some(&Value::String("sum".to_string()))
            );
        } else {
            panic!("Expected mapping");
        }
    }

    #[test]
    fn test_get_nonexistent_object() {
        let (_temp_dir, parser) = create_test_globals_dir();
        let result = parser.get_object_by_path("semantics", "entities.NonExistent");
        assert!(result.is_err());
        assert!(matches!(result, Err(GlobalError::ObjectNotFound(_))));
    }

    #[test]
    fn test_get_nonexistent_object_type() {
        let (_temp_dir, parser) = create_test_globals_dir();
        let result = parser.get_object_by_path("semantics", "nonexistent_type.customer");
        assert!(result.is_err());
        assert!(matches!(result, Err(GlobalError::ObjectNotFound(_))));
    }

    #[test]
    fn test_list_global_files() {
        let (_temp_dir, parser) = create_test_globals_dir();
        let files = parser.list_global_files().unwrap();
        assert_eq!(files.len(), 1);
        assert!(files.contains(&"semantics".to_string()));
    }

    #[test]
    fn test_globals_dir_exists() {
        let (_temp_dir, parser) = create_test_globals_dir();
        assert!(parser.globals_dir_exists());
    }

    #[test]
    fn test_globals_dir_not_exists() {
        let temp_dir = TempDir::new().unwrap();
        let non_existent = temp_dir.path().join("nonexistent");
        let parser = GlobalParser::new(&non_existent);
        assert!(!parser.globals_dir_exists());
    }
}
