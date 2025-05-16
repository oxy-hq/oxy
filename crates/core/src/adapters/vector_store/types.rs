use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Document {
    pub content: String,
    pub source_type: String,
    pub source_identifier: String,
    pub embeddings: Vec<f32>,
    pub embedding_content: String,
}

#[derive(Serialize, Deserialize)]
pub struct SearchRecord {
    #[serde(flatten)]
    pub document: Document,
    #[serde(alias = "_score")]
    pub score: f32,
    #[serde(alias = "_relevance_score")]
    pub relevance_score: f32,
}

impl std::fmt::Debug for SearchRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Id: {}\nType:{}\nContent: {}\nFTS Score: {}\nRRF Score: {}",
            self.document.source_identifier,
            self.document.source_type,
            self.document.content,
            self.score,
            self.relevance_score
        )
    }
}
