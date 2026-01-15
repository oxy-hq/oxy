// Standard library imports
use std::collections::HashMap;

// =============================================================================
// Helper Functions
// =============================================================================

/// Converts a JSON object to a HashMap
pub fn json_to_hashmap(
    json: serde_json::Map<String, serde_json::Value>,
) -> HashMap<String, serde_json::Value> {
    json.into_iter().collect()
}

/// Extracts description from SQL file comments
/// Looks for comments like: -- Description: This query does X
pub fn extract_sql_description(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("-- Description:") || trimmed.starts_with("--Description:") {
            let desc = trimmed
                .trim_start_matches("--")
                .trim_start_matches("Description:")
                .trim();
            if !desc.is_empty() {
                return Some(desc.to_string());
            }
        }
    }
    None
}
