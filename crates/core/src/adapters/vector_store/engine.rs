use super::types::{RetrievalObject, SearchRecord};
use crate::errors::OxyError;
use enum_dispatch::enum_dispatch;

#[enum_dispatch]
pub(super) trait VectorEngine {
    async fn ingest(&self, retrieval_objects: &Vec<RetrievalObject>) -> Result<(), OxyError>;
    async fn search(&self, query: &str) -> Result<Vec<SearchRecord>, OxyError>;
    async fn cleanup(&self) -> Result<(), OxyError>;
}
