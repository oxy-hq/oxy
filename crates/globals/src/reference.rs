use crate::errors::{GlobalError, GlobalResult};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Represents a reference to a global object
/// Format: globals.<file_name>.<path>
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GlobalReference {
    /// Name of the file in the globals/ directory (without .yml extension)
    pub file_name: String,
    /// The path to the referenced object (e.g., "entities.customer" or "measures.total_sales")
    pub path: String,
}

impl GlobalReference {
    /// Create a new GlobalReference from components
    pub fn new(file_name: String, path: String) -> Self {
        Self { file_name, path }
    }

    /// Parse a global reference string into its components
    /// Expected format: globals.<file_name>.<path>
    /// where path can be: object_type.object_name (e.g., "entities.customer")
    pub fn parse(reference: &str) -> GlobalResult<Self> {
        if !reference.starts_with("globals.") {
            return Err(GlobalError::InvalidReferenceSyntax(format!(
                "Reference '{}' must start with 'globals.'",
                reference
            )));
        }

        // Remove "globals." prefix
        let without_prefix = &reference[8..];

        // Split by first dot to get file_name and path
        let parts: Vec<&str> = without_prefix.splitn(2, '.').collect();

        if parts.len() < 2 {
            return Err(GlobalError::InvalidReferenceSyntax(format!(
                "Reference '{}' does not match expected format 'globals.<file_name>.<path>'",
                reference
            )));
        }

        Ok(Self {
            file_name: parts[0].to_string(),
            path: parts[1].to_string(),
        })
    }

    /// Convert the reference back to a string representation
    pub fn to_string_ref(&self) -> String {
        format!("globals.{}.{}", self.file_name, self.path)
    }

    /// Get the expected file path for this reference
    pub fn file_path(&self) -> String {
        format!("{}.yml", self.file_name)
    }

    /// Check if a string is a global reference
    pub fn is_global_reference(s: &str) -> bool {
        s.starts_with("globals.")
    }

    /// Split the path into components (e.g., "entities.customer" -> ["entities", "customer"])
    pub fn path_components(&self) -> Vec<String> {
        self.path.split('.').map(|s| s.to_string()).collect()
    }

    /// Get the object type (first component of path)
    pub fn object_type(&self) -> Option<String> {
        self.path_components().first().cloned()
    }

    /// Get the object name (second component of path)
    pub fn object_name(&self) -> Option<String> {
        let components = self.path_components();
        if components.len() >= 2 {
            components.get(1).cloned()
        } else {
            None
        }
    }
}

impl fmt::Display for GlobalReference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "globals.{}.{}", self.file_name, self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_reference() {
        let reference = "globals.semantics.entities.customer";
        let parsed = GlobalReference::parse(reference).unwrap();

        assert_eq!(parsed.file_name, "semantics");
        assert_eq!(parsed.path, "entities.customer");
        assert_eq!(parsed.object_type(), Some("entities".to_string()));
        assert_eq!(parsed.object_name(), Some("customer".to_string()));
    }

    #[test]
    fn test_parse_with_underscores() {
        let reference = "globals.my_file.my_type.my_object";
        let parsed = GlobalReference::parse(reference).unwrap();

        assert_eq!(parsed.file_name, "my_file");
        assert_eq!(parsed.path, "my_type.my_object");
        assert_eq!(parsed.object_type(), Some("my_type".to_string()));
        assert_eq!(parsed.object_name(), Some("my_object".to_string()));
    }

    #[test]
    fn test_parse_with_hyphens() {
        let reference = "globals.my-file.my-type.my-object";
        let parsed = GlobalReference::parse(reference).unwrap();

        assert_eq!(parsed.file_name, "my-file");
        assert_eq!(parsed.path, "my-type.my-object");
        assert_eq!(parsed.object_type(), Some("my-type".to_string()));
        assert_eq!(parsed.object_name(), Some("my-object".to_string()));
    }

    #[test]
    fn test_parse_invalid_prefix() {
        let reference = "global.semantics.entities.customer";
        let result = GlobalReference::parse(reference);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_too_few_parts() {
        let reference = "globals.semantics";
        let result = GlobalReference::parse(reference);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_nested_path() {
        let reference = "globals.semantics.entities.customer.extra.nested";
        let parsed = GlobalReference::parse(reference).unwrap();
        assert_eq!(parsed.file_name, "semantics");
        assert_eq!(parsed.path, "entities.customer.extra.nested");
        assert_eq!(parsed.object_type(), Some("entities".to_string()));
        assert_eq!(parsed.object_name(), Some("customer".to_string()));
    }

    #[test]
    fn test_to_string_ref() {
        let reference =
            GlobalReference::new("semantics".to_string(), "entities.customer".to_string());
        assert_eq!(
            reference.to_string_ref(),
            "globals.semantics.entities.customer"
        );
    }

    #[test]
    fn test_file_path() {
        let reference =
            GlobalReference::new("semantics".to_string(), "entities.customer".to_string());
        assert_eq!(reference.file_path(), "semantics.yml");
    }

    #[test]
    fn test_is_global_reference() {
        assert!(GlobalReference::is_global_reference(
            "globals.semantics.entities.customer"
        ));
        assert!(!GlobalReference::is_global_reference(
            "notglobals.semantics.entities.customer"
        ));
        assert!(!GlobalReference::is_global_reference(
            "semantics.entities.customer"
        ));
    }

    #[test]
    fn test_display() {
        let reference =
            GlobalReference::new("semantics".to_string(), "entities.customer".to_string());
        assert_eq!(
            format!("{}", reference),
            "globals.semantics.entities.customer"
        );
    }

    #[test]
    fn test_path_components() {
        let reference = GlobalReference::new(
            "semantics".to_string(),
            "entities.customer.nested".to_string(),
        );
        assert_eq!(
            reference.path_components(),
            vec!["entities", "customer", "nested"]
        );
    }
}
