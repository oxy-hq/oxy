use crate::{
    adapters::vector_store::{Document, VectorStore},
    errors::OxyError,
    execute::{Executable, ExecutionContext, types::Output},
};

use super::{
    tool::Tool,
    types::{RetrievalInput, RetrievalParams},
};

#[derive(Debug, Clone)]
pub struct RetrievalExecutable;

impl RetrievalExecutable {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for RetrievalExecutable {
    type Param = RetrievalParams;
    type Output = Vec<Document>;

    fn serialize_output(&self, output: &Self::Output) -> Result<String, OxyError> {
        Ok(output.iter().fold(String::new(), |acc, doc| {
            acc + &format!("{}\n", doc.content)
        }))
    }
}

#[async_trait::async_trait]
impl Executable<RetrievalInput> for RetrievalExecutable {
    type Response = Output;

    async fn execute(
        &mut self,
        _execution_context: &ExecutionContext,
        input: RetrievalInput,
    ) -> Result<Self::Response, OxyError> {
        let RetrievalInput {
            query,
            retrieval_config,
        } = input;
        let store = VectorStore::from_retrieval(&retrieval_config).await?;
        let results = store.search(&query).await?;
        let output = self.serialize_output(&results)?;
        Ok(Output::Text(output))
    }
}
