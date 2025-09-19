use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    /// Optional enum variables from workflow (for enum index building)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enum_variables: Option<HashMap<String, Vec<serde_json::Value>>>,
}

impl RetrievalObject {
    pub fn determine_content(&self) -> String {
        // If this is a sql or general yaml source, return full context_content.
        // This is so that when using retrieval tool, the full sql or yaml file contents
        // can be injected into system instructions.
        // TODO: we may want to truncate the context_content to a certain
        //       number of lines/chars/tokens
        if self.source_type.starts_with("sql::") || self.source_type == "yaml" {
            return self.context_content.clone();
        }

        let mut content_parts: Vec<String> = Vec::new();

        for inclusion in &self.inclusions {
            content_parts.push(inclusion.clone());
        }

        // NOTE: exclusions should already be excluded via epsilon ball filtering.
        //       This is a final guard rail to prevent LLM from choosing excluded prompts.
        for exclusion in &self.exclusions {
            content_parts.push(format!("DO NOT USE FOR PROMPT: '{exclusion}'"));
        }

        content_parts.join("\n")
    }
}

pub type Embedding = Vec<f32>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalItem {
    pub source_identifier: String,
    pub embedding_content: String,
    pub embedding: Embedding,
    pub content: String,
    pub source_type: String,
    pub radius: f32,
}

#[derive(Serialize, Deserialize)]
pub struct SearchRecord {
    #[serde(flatten)]
    pub retrieval_item: RetrievalItem,
    /// Distance from LanceDB vector search (cosine distance)
    pub distance: f32,
    /// Optional score (may not be meaningful for all search types)
    #[serde(alias = "_score")]
    pub score: Option<f32>,
    #[serde(alias = "_relevance_score")]
    pub relevance_score: Option<f32>,
}

impl std::fmt::Debug for SearchRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Id: {}\nType:{}\nContent: {}\nDistance: {:.3}\nScore: {:?}\nRelevance Score: {}",
            self.retrieval_item.source_identifier,
            self.retrieval_item.source_type,
            self.retrieval_item.content,
            self.distance,
            self.score,
            self.relevance_score.unwrap_or(0.0)
        )
    }
}
