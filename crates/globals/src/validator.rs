use crate::errors::{GlobalError, GlobalResult};
use crate::parser::GlobalParser;
use crate::reference::GlobalReference;
use serde_yaml::Value;

/// Validator for global object references
pub struct GlobalValidator {
    parser: GlobalParser,
}

impl GlobalValidator {
    /// Create a new GlobalValidator with the given parser
    pub fn new(parser: GlobalParser) -> Self {
        Self { parser }
    }

    /// Validate that a global reference exists and is accessible
    pub fn validate_reference(&self, reference: &GlobalReference) -> GlobalResult<()> {
        // Check if globals directory exists
        if !self.parser.globals_dir_exists() {
            return Err(GlobalError::FileNotFound(format!(
                "Globals directory not found at: {}",
                self.parser.globals_dir().display()
            )));
        }

        // Try to get the object - this will validate file exists and path is valid
        self.parser
            .get_object_by_path(&reference.file_name, &reference.path)?;

        Ok(())
    }

    /// Validate a global reference string
    pub fn validate_reference_string(&self, reference_str: &str) -> GlobalResult<()> {
        let reference = GlobalReference::parse(reference_str)?;
        self.validate_reference(&reference)
    }

    /// Validate and retrieve a global object
    pub fn validate_and_get(&self, reference: &GlobalReference) -> GlobalResult<Value> {
        self.parser
            .get_object_by_path(&reference.file_name, &reference.path)
    }

    /// Validate multiple references at once
    pub fn validate_references(&self, references: &[GlobalReference]) -> GlobalResult<()> {
        for reference in references {
            self.validate_reference(reference)?;
        }
        Ok(())
    }

    /// Check if a reference exists without returning an error
    pub fn reference_exists(&self, reference: &GlobalReference) -> bool {
        self.validate_reference(reference).is_ok()
    }

    /// Get helpful suggestions when a reference doesn't exist
    pub fn get_suggestions(&self, reference: &GlobalReference) -> Vec<String> {
        let mut suggestions = Vec::new();

        // Check if file exists
        if let Err(_) = self.parser.load_file(&reference.file_name) {
            // Suggest available files
            if let Ok(files) = self.parser.list_global_files()
                && !files.is_empty()
            {
                suggestions.push(format!("Available global files: {}", files.join(", ")));
            }
        } else {
            // File exists, check path components
            if let Ok(yaml) = self.parser.load_file(&reference.file_name) {
                let path_components: Vec<&str> = reference.path.split('.').collect();

                if let Some(first_component) = path_components.first() {
                    let available_types: Vec<String> = yaml.keys().map(|k| k.to_string()).collect();

                    if !available_types.contains(&first_component.to_string()) {
                        suggestions.push(format!(
                            "Available object types in '{}': {}",
                            reference.file_name,
                            available_types.join(", ")
                        ));
                    } else if path_components.len() >= 2 {
                        // First component exists, suggest available object names
                        if let Some(Value::Sequence(items)) = yaml.get(*first_component) {
                            let names: Vec<String> = items
                                .iter()
                                .filter_map(|item| {
                                    if let Value::Mapping(map) = item {
                                        map.get(Value::String("name".to_string()))
                                            .and_then(|v| v.as_str())
                                            .map(|s| s.to_string())
                                    } else {
                                        None
                                    }
                                })
                                .collect();

                            if !names.is_empty() {
                                suggestions.push(format!(
                                    "Available objects in '{}.{}': {}",
                                    reference.file_name,
                                    first_component,
                                    names.join(", ")
                                ));
                            }
                        } else if let Some(Value::Mapping(map)) = yaml.get(*first_component) {
                            let names: Vec<String> = map
                                .keys()
                                .filter_map(|k| k.as_str().map(|s| s.to_string()))
                                .collect();

                            if !names.is_empty() {
                                suggestions.push(format!(
                                    "Available objects in '{}.{}': {}",
                                    reference.file_name,
                                    first_component,
                                    names.join(", ")
                                ));
                            }
                        }
                    }
                }
            }
        }

        suggestions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_validator() -> (TempDir, GlobalValidator) {
        let temp_dir = TempDir::new().unwrap();
        let globals_dir = temp_dir.path().join("globals");
        fs::create_dir(&globals_dir).unwrap();

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
  - name: product_id
    type: number
    description: Unique product identifier
    expr: product_id

measures:
  - name: total_sales
    type: sum
    description: Total sales amount
    expr: amount
"#;
        fs::write(globals_dir.join("semantics.yml"), semantics_content).unwrap();

        let parser = GlobalParser::new(&globals_dir);
        let validator = GlobalValidator::new(parser);

        (temp_dir, validator)
    }

    #[test]
    fn test_validate_valid_reference() {
        let (_temp_dir, validator) = create_test_validator();
        let reference = GlobalReference::parse("globals.semantics.entities.customer").unwrap();
        assert!(validator.validate_reference(&reference).is_ok());
    }

    #[test]
    fn test_validate_invalid_file() {
        let (_temp_dir, validator) = create_test_validator();
        let reference = GlobalReference::parse("globals.nonexistent.entities.customer").unwrap();
        assert!(validator.validate_reference(&reference).is_err());
    }

    #[test]
    fn test_validate_invalid_object_type() {
        let (_temp_dir, validator) = create_test_validator();
        let reference = GlobalReference::parse("globals.semantics.invalid.customer").unwrap();
        assert!(validator.validate_reference(&reference).is_err());
    }

    #[test]
    fn test_validate_invalid_object_name() {
        let (_temp_dir, validator) = create_test_validator();
        let reference = GlobalReference::parse("globals.semantics.entities.InvalidName").unwrap();
        assert!(validator.validate_reference(&reference).is_err());
    }

    #[test]
    fn test_validate_reference_string() {
        let (_temp_dir, validator) = create_test_validator();
        assert!(
            validator
                .validate_reference_string("globals.semantics.entities.customer")
                .is_ok()
        );
    }

    #[test]
    fn test_validate_and_get() {
        let (_temp_dir, validator) = create_test_validator();
        let reference = GlobalReference::parse("globals.semantics.entities.customer").unwrap();
        let result = validator.validate_and_get(&reference);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_multiple_references() {
        let (_temp_dir, validator) = create_test_validator();
        let references = vec![
            GlobalReference::parse("globals.semantics.entities.customer").unwrap(),
            GlobalReference::parse("globals.semantics.entities.product").unwrap(),
        ];
        assert!(validator.validate_references(&references).is_ok());
    }

    #[test]
    fn test_reference_exists() {
        let (_temp_dir, validator) = create_test_validator();
        let reference = GlobalReference::parse("globals.semantics.entities.customer").unwrap();
        assert!(validator.reference_exists(&reference));

        let invalid_reference =
            GlobalReference::parse("globals.semantics.entities.Invalid").unwrap();
        assert!(!validator.reference_exists(&invalid_reference));
    }

    #[test]
    fn test_get_suggestions_invalid_file() {
        let (_temp_dir, validator) = create_test_validator();
        let reference = GlobalReference::parse("globals.nonexistent.entities.customer").unwrap();
        let suggestions = validator.get_suggestions(&reference);
        assert!(!suggestions.is_empty());
        assert!(suggestions[0].contains("Available global files"));
    }

    #[test]
    fn test_get_suggestions_invalid_object_type() {
        let (_temp_dir, validator) = create_test_validator();
        let reference = GlobalReference::parse("globals.semantics.invalid.customer").unwrap();
        let suggestions = validator.get_suggestions(&reference);
        assert!(!suggestions.is_empty());
        assert!(suggestions[0].contains("Available object types"));
    }

    #[test]
    fn test_get_suggestions_invalid_object_name() {
        let (_temp_dir, validator) = create_test_validator();
        let reference = GlobalReference::parse("globals.semantics.entities.Invalid").unwrap();
        let suggestions = validator.get_suggestions(&reference);
        assert!(!suggestions.is_empty());
        assert!(suggestions[0].contains("Available objects"));
    }
}
