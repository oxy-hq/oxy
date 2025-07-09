use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Serialize, Deserialize, ToSchema, Debug)]
pub struct RetrievalContent {
    pub embedding_content: String,
    pub embeddings: Vec<f32>,
}

impl From<crate::adapters::vector_store::RetrievalContent> for RetrievalContent {
    fn from(vs_content: crate::adapters::vector_store::RetrievalContent) -> Self {
        Self {
            embedding_content: vs_content.embedding_content,
            embeddings: vs_content.embeddings,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, ToSchema, Hash)]
pub struct Document {
    pub id: String,
    pub kind: String,
    pub content: String,
}

impl std::fmt::Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self.content)
    }
}

impl std::fmt::Display for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.content)
    }
}
