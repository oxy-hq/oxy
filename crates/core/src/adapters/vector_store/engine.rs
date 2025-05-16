use super::types::{Document, SearchRecord};
use crate::errors::OxyError;

#[enum_dispatch::enum_dispatch]
pub(super) trait VectorEngine {
    async fn embed(&self, documents: &Vec<Document>) -> Result<(), OxyError>;
    async fn search(&self, query: &str) -> Result<Vec<SearchRecord>, OxyError>;
    async fn cleanup(&self) -> Result<(), OxyError>;
}
