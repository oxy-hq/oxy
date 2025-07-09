use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RetrievalContent {
    pub embedding_content: String,
    pub embeddings: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub content: String,
    pub source_type: String,
    pub source_identifier: String,
    pub retrieval_inclusions: Vec<RetrievalContent>,
    pub retrieval_exclusions: Vec<RetrievalContent>,
    pub inclusion_midpoint: Vec<f32>,
    pub inclusion_radius: f32,
}

#[derive(Serialize, Deserialize)]
pub struct SearchRecord {
    #[serde(flatten)]
    pub document: Document,
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
            self.document.source_identifier,
            self.document.source_type,
            self.document.content,
            self.distance,
            self.score,
            self.relevance_score.unwrap_or(0.0)
        )
    }
}
