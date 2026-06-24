use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Indexable retrieval source — used by the enum-index builder to discover
/// parameterized SQL templates with enum variables.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RetrievalObject {
    /// Identifier of the source (e.g. file path)
    pub source_identifier: String,
    /// Source type, e.g. "file", or "sql::<database_name>", or domain-specific
    pub source_type: String,
    /// Content to aid testing and understanding, esp for LLM tool calls (e.g. raw SQL query)
    pub context_content: String,
    /// Inclusion contents tied to this source that will be embedded for retrieval
    pub inclusions: Vec<String>,
    /// Exclusion contents tied to this source that will be embedded for retrieval
    pub exclusions: Vec<String>,
    /// Indicates whether it's a derived object (e.g. built from parameterized templates with enum variables)
    #[serde(default)]
    pub is_child: bool,
    /// Optional enum variables from automation (for enum index building)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enum_variables: Option<HashMap<String, Vec<serde_json::Value>>>,
}
