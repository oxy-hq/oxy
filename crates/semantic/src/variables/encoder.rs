use regex::Regex;
use std::collections::HashMap;

use super::errors::VariableError;

/// Maps original variable expressions to their encoded equivalents
pub type VariableMapping = HashMap<String, String>;

/// Handles encoding and decoding of variable expressions for CubeJS compatibility
///
/// Transforms expressions like `{{variables.user_table}}` into CubeJS-safe identifiers
/// like `__VAR_user_table__` and maintains bidirectional mapping for decoding.
#[derive(Debug, Clone)]
pub struct VariableEncoder {
    /// Maps encoded placeholders back to original variable expressions
    variable_mapping: HashMap<String, String>,
    /// Regex pattern to match variable expressions: {{variables.name}}
    variable_regex: Regex,
}

impl Default for VariableEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl VariableEncoder {
    /// Create a new variable encoder with default patterns
    pub fn new() -> Self {
        let variable_regex = Regex::new(r"\{\{variables\.([^}]+)\}\}")
            .expect("Variable regex pattern should be valid");

        Self {
            variable_mapping: HashMap::new(),
            variable_regex,
        }
    }

    /// Encode variable expressions in a string to CubeJS-safe format
    ///
    /// Transforms `{{variables.user_table}}` → `__VAR_user_table__`
    /// Handles nested paths: `{{variables.schema.table}}` → `__VAR_schema_table__`
    ///
    /// # Arguments
    /// * `expr` - Expression containing variable references
    ///
    /// # Returns
    /// * Encoded expression with variables replaced by placeholders
    ///
    /// # Examples
    /// ```
    /// use oxy_semantic::variables::VariableEncoder;
    ///
    /// let mut encoder = VariableEncoder::new();
    /// let encoded = encoder.encode_expression("SELECT * FROM {{variables.user_table}}");
    /// assert_eq!(encoded, "SELECT * FROM __VAR_user_table__");
    /// ```
    pub fn encode_expression(&mut self, expr: &str) -> String {
        let mut result = expr.to_string();
        let mut mappings_to_add = Vec::new();

        // First pass: collect all matches and replacements
        for capture in self.variable_regex.captures_iter(expr) {
            let variable_path = &capture[1];
            let full_match = &capture[0];
            let encoded = self.encode_variable_path(variable_path);

            // Store for later insertion into mapping
            mappings_to_add.push((encoded.clone(), full_match.to_string()));

            // Replace in the result string
            result = result.replace(full_match, &encoded);
        }

        // Second pass: update the mapping
        for (encoded, original) in mappings_to_add {
            self.variable_mapping.insert(encoded, original);
        }

        result
    }

    /// Decode encoded variable expressions back to original format
    ///
    /// Transforms `__VAR_user_table__` → `{{variables.user_table}}`
    ///
    /// # Arguments
    /// * `encoded` - Expression containing encoded variable placeholders
    ///
    /// # Returns
    /// * Result containing decoded expression or VariableError if placeholders not found
    ///
    /// # Examples
    /// ```
    /// use oxy_semantic::variables::VariableEncoder;
    ///
    /// let mut encoder = VariableEncoder::new();
    /// // First encode to populate mapping
    /// encoder.encode_expression("{{variables.user_table}}");
    ///
    /// let decoded = encoder.decode_expression("SELECT * FROM __VAR_user_table__").unwrap();
    /// assert_eq!(decoded, "SELECT * FROM {{variables.user_table}}");
    /// ```
    pub fn decode_expression(&self, encoded: &str) -> Result<String, VariableError> {
        let placeholder_regex =
            Regex::new(r"__VAR_[0-9a-f]+__").expect("Placeholder regex should be valid");

        let mut result = encoded.to_string();

        for placeholder in placeholder_regex.find_iter(encoded) {
            let placeholder_str = placeholder.as_str();

            match self.variable_mapping.get(placeholder_str) {
                Some(original) => {
                    result = result.replace(placeholder_str, original);
                }
                None => {
                    return Err(VariableError::VariableNotFound(format!(
                        "No mapping found for placeholder: {}",
                        placeholder_str
                    )));
                }
            }
        }

        Ok(result)
    }

    /// Decode all encoded variables in an expression, even if not in mapping
    ///
    /// This method decodes variables based on the encoding pattern, not just stored mappings.
    /// It's useful for decoding variables that were encoded by a different encoder instance.
    ///
    /// # Arguments
    /// * `encoded` - Expression containing encoded variable placeholders
    ///
    /// # Returns
    /// * Expression with all placeholders decoded to variable syntax
    ///
    /// # Examples
    /// ```
    /// use oxy_semantic::variables::VariableEncoder;
    ///
    /// let encoder = VariableEncoder::new();
    /// let decoded = encoder.decode_all_variables("SELECT __VAR_6964_636f6c756d6e__ FROM __VAR_7461626c655f6e616d65__");
    /// assert_eq!(decoded, "SELECT {{variables.id_column}} FROM {{variables.table_name}}");
    /// ```
    pub fn decode_all_variables(&self, encoded: &str) -> String {
        let placeholder_regex =
            Regex::new(r"__VAR_([0-9a-f]+)__").expect("Placeholder regex should be valid");

        placeholder_regex
            .replace_all(encoded, |caps: &regex::Captures| {
                let hex_path = &caps[1];
                // Decode hex back to original variable path
                match self.decode_hex_variable_path(hex_path) {
                    Ok(original_path) => format!("{{{{variables.{}}}}}", original_path),
                    Err(_) => {
                        // If decoding fails, return the placeholder as-is
                        format!("__VAR_{}__", hex_path)
                    }
                }
            })
            .to_string()
    }

    /// Extract all variable names from an expression
    ///
    /// # Arguments  
    /// * `expr` - Expression to extract variables from
    ///
    /// # Returns
    /// * Vector of variable paths found in the expression
    ///
    /// # Examples
    /// ```
    /// use oxy_semantic::variables::VariableEncoder;
    ///
    /// let encoder = VariableEncoder::new();
    /// let vars = encoder.extract_variables("{{variables.schema}}.{{variables.table}}");
    /// assert_eq!(vars, vec!["schema", "table"]);
    /// ```
    pub fn extract_variables(&self, expr: &str) -> Vec<String> {
        self.variable_regex
            .captures_iter(expr)
            .map(|caps| caps[1].to_string())
            .collect()
    }

    /// Check if an expression contains any variable references
    ///
    /// # Arguments
    /// * `expr` - Expression to check
    ///
    /// # Returns
    /// * True if expression contains {{variables.*}} references
    pub fn has_variables(&self, expr: &str) -> bool {
        self.variable_regex.is_match(expr)
    }

    /// Get the current variable mapping (for debugging/inspection)
    pub fn get_mapping(&self) -> &HashMap<String, String> {
        &self.variable_mapping
    }

    /// Clear all stored variable mappings
    pub fn clear_mapping(&mut self) {
        self.variable_mapping.clear();
    }

    /// Encode a variable path to CubeJS-safe identifier format
    ///
    /// Converts variable path to hex encoding to avoid ambiguity
    /// `schema.table` → `__VAR_736368656d612e7461626c65__`
    /// `orders_table` → `__VAR_6f72646572735f7461626c65__`
    ///
    /// # Arguments
    /// * `variable_path` - Variable path like "schema.table" or "orders_table"
    ///
    /// # Returns
    /// * Encoded identifier safe for CubeJS parsing with unambiguous decoding
    fn encode_variable_path(&self, variable_path: &str) -> String {
        // Encode variable path as hex to avoid ambiguity with dots/underscores
        let hex_encoded = variable_path
            .bytes()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();
        format!("__VAR_{}__", hex_encoded)
    }

    /// Decode a hex-encoded variable path back to original format
    ///
    /// # Arguments
    /// * `hex_path` - Hex-encoded variable path
    ///
    /// # Returns
    /// * Result containing decoded variable path or error if invalid hex
    fn decode_hex_variable_path(&self, hex_path: &str) -> Result<String, VariableError> {
        // Convert hex string back to bytes
        if !hex_path.len().is_multiple_of(2) {
            return Err(VariableError::InvalidEncoding(format!(
                "Hex path must have even length: {}",
                hex_path
            )));
        }

        let mut bytes = Vec::new();
        for i in (0..hex_path.len()).step_by(2) {
            let hex_pair = &hex_path[i..i + 2];
            match u8::from_str_radix(hex_pair, 16) {
                Ok(byte) => bytes.push(byte),
                Err(_) => {
                    return Err(VariableError::InvalidEncoding(format!(
                        "Invalid hex sequence: {}",
                        hex_pair
                    )));
                }
            }
        }

        String::from_utf8(bytes).map_err(|_| {
            VariableError::InvalidEncoding("Invalid UTF-8 in decoded variable path".to_string())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_simple_variable() {
        let mut encoder = VariableEncoder::new();
        let encoded = encoder.encode_expression("{{variables.user_table}}");
        // "user_table" hex encoded is "757365725f7461626c65"
        assert_eq!(encoded, "__VAR_757365725f7461626c65__");
    }

    #[test]
    fn test_encode_nested_variable() {
        let mut encoder = VariableEncoder::new();
        let encoded = encoder.encode_expression("{{variables.schema.table}}");
        // "schema.table" hex encoded is "736368656d612e7461626c65"
        assert_eq!(encoded, "__VAR_736368656d612e7461626c65__");
    }

    #[test]
    fn test_encode_multiple_variables() {
        let mut encoder = VariableEncoder::new();
        let encoded =
            encoder.encode_expression("SELECT * FROM {{variables.schema}}.{{variables.table}}");
        // "schema" hex = "736368656d61", "table" hex = "7461626c65"
        assert_eq!(
            encoded,
            "SELECT * FROM __VAR_736368656d61__.__VAR_7461626c65__"
        );
    }

    #[test]
    fn test_encode_expression_with_sql() {
        let mut encoder = VariableEncoder::new();
        let encoded = encoder.encode_expression("SUM({{variables.amount_field}})");
        // "amount_field" hex encoded is "616d6f756e745f6669656c64"
        assert_eq!(encoded, "SUM(__VAR_616d6f756e745f6669656c64__)");
    }

    #[test]
    fn test_decode_simple_variable() {
        let mut encoder = VariableEncoder::new();
        // First encode to populate mapping
        let encoded = encoder.encode_expression("{{variables.user_table}}");

        let decoded = encoder.decode_expression(&encoded).unwrap();
        assert_eq!(decoded, "{{variables.user_table}}");
    }

    #[test]
    fn test_decode_multiple_variables() {
        let mut encoder = VariableEncoder::new();
        // First encode to populate mapping
        let encoded =
            encoder.encode_expression("SELECT * FROM {{variables.schema}}.{{variables.table}}");

        let decoded = encoder.decode_expression(&encoded).unwrap();
        assert_eq!(
            decoded,
            "SELECT * FROM {{variables.schema}}.{{variables.table}}"
        );
    }

    #[test]
    fn test_decode_unknown_placeholder() {
        let encoder = VariableEncoder::new();
        let result = encoder.decode_expression("__VAR_deadbeef__");

        assert!(matches!(result, Err(VariableError::VariableNotFound(_))));
    }

    #[test]
    fn test_extract_variables() {
        let encoder = VariableEncoder::new();
        let vars = encoder.extract_variables(
            "SELECT {{variables.field}} FROM {{variables.schema}}.{{variables.table}}",
        );

        assert_eq!(vars.len(), 3);
        assert!(vars.contains(&"field".to_string()));
        assert!(vars.contains(&"schema".to_string()));
        assert!(vars.contains(&"table".to_string()));
    }

    #[test]
    fn test_has_variables() {
        let encoder = VariableEncoder::new();

        assert!(encoder.has_variables("{{variables.test}}"));
        assert!(encoder.has_variables("SUM({{variables.field}})"));
        assert!(!encoder.has_variables("SELECT * FROM users"));
        assert!(!encoder.has_variables("{{other.template}}"));
    }

    #[test]
    fn test_roundtrip_encoding() {
        let mut encoder = VariableEncoder::new();
        let original = "SELECT {{variables.field}} FROM {{variables.schema}}.{{variables.table}}";

        let encoded = encoder.encode_expression(original);
        let decoded = encoder.decode_expression(&encoded).unwrap();

        assert_eq!(original, decoded);
    }

    #[test]
    fn test_no_variables() {
        let mut encoder = VariableEncoder::new();
        let original = "SELECT * FROM users WHERE status = 'active'";

        let encoded = encoder.encode_expression(original);
        assert_eq!(encoded, original);

        let decoded = encoder.decode_expression(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_ambiguous_variable_names() {
        let mut encoder = VariableEncoder::new();

        // Test case that was problematic with underscore encoding
        let expr1 = "{{variables.orders_table}}"; // underscore in name
        let expr2 = "{{variables.orders.table}}"; // dot in path

        let encoded1 = encoder.encode_expression(expr1);
        let encoded2 = encoder.encode_expression(expr2);

        // Should produce different encodings
        assert_ne!(encoded1, encoded2);

        // Should decode back correctly
        let decoded1 = encoder.decode_expression(&encoded1).unwrap();
        let decoded2 = encoder.decode_expression(&encoded2).unwrap();

        assert_eq!(decoded1, expr1);
        assert_eq!(decoded2, expr2);
    }

    #[test]
    fn test_decode_all_variables_with_hex() {
        let encoder = VariableEncoder::new();

        // Test the decode_all_variables method with hex encoded placeholders
        // "id_column" hex = "69645f636f6c756d6e", "table_name" hex = "7461626c655f6e616d65"
        let encoded = "SELECT __VAR_69645f636f6c756d6e__ FROM __VAR_7461626c655f6e616d65__";
        let decoded = encoder.decode_all_variables(encoded);

        assert_eq!(
            decoded,
            "SELECT {{variables.id_column}} FROM {{variables.table_name}}"
        );
    }

    #[test]
    fn test_hex_encoding_roundtrip() {
        let encoder = VariableEncoder::new();

        let test_cases = vec![
            "user_table",
            "schema.table",
            "orders_table",
            "orders.table",
            "complex_var.with.dots_and_underscores",
        ];

        for case in test_cases {
            let hex_encoded = case
                .bytes()
                .map(|b| format!("{:02x}", b))
                .collect::<String>();
            let placeholder = format!("__VAR_{}__", hex_encoded);

            let decoded = encoder.decode_all_variables(&placeholder);
            let expected = format!("{{{{variables.{}}}}}", case);

            assert_eq!(decoded, expected, "Failed roundtrip for: {}", case);
        }
    }

    #[test]
    fn test_encoded_format_is_valid_sql_identifier() {
        let mut encoder = VariableEncoder::new();

        let test_cases = vec![
            "user_table",
            "schema.table",
            "orders_table",
            "orders.table",
            "UPPER_CASE",
            "mixed_Case_123",
            "complex.var_with.dots_and_underscores",
        ];

        for case in test_cases {
            let encoded = encoder.encode_expression(&format!("{{{{variables.{}}}}}", case));

            // Extract the encoded placeholder
            let placeholder_start = encoded.find("__VAR_").unwrap();
            let placeholder_end = encoded.rfind("__").unwrap() + 2;
            let placeholder = &encoded[placeholder_start..placeholder_end];

            // Verify it starts with underscore (valid SQL identifier start)
            assert!(
                placeholder.starts_with('_'),
                "Placeholder should start with underscore: {}",
                placeholder
            );

            // Verify it contains only valid SQL identifier characters (letters, digits, underscores)
            assert!(
                placeholder
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_'),
                "Placeholder contains invalid SQL identifier characters: {}",
                placeholder
            );

            // Verify the hex part is valid hex
            let hex_part = &placeholder[6..placeholder.len() - 2]; // Remove "__VAR_" and "__"
            assert!(
                hex_part.chars().all(|c| c.is_ascii_hexdigit()),
                "Hex part contains non-hex characters: {}",
                hex_part
            );

            println!("✓ {} -> {} (valid SQL identifier)", case, placeholder);
        }
    }
}
