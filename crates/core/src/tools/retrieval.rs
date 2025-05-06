use crate::{
    adapters::vector_store::{SearchRecord, VectorStore},
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        types::{Chunk, Output, Prompt},
    },
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
    type Output = Vec<SearchRecord>;

    fn serialize_output(&self, output: &Self::Output) -> Result<String, OxyError> {
        Ok(output.iter().fold(String::new(), |acc, record| {
            acc + &format!("{}\n", record.document.content)
        }))
    }
}

#[async_trait::async_trait]
impl Executable<RetrievalInput> for RetrievalExecutable {
    type Response = Output;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: RetrievalInput,
    ) -> Result<Self::Response, OxyError> {
        execution_context
            .write_chunk(Chunk {
                key: None,
                delta: Output::Prompt(Prompt::new("Retrieving data...".to_string())),
                finished: true,
            })
            .await?;
        let RetrievalInput {
            agent_name,
            query,
            retrieval_config,
        } = input;
        let store =
            VectorStore::from_retrieval(&execution_context.config, &agent_name, &retrieval_config)
                .await?;
        let results = store.search(&query).await?;
        if !results.is_empty() {
            execution_context
                .write_chunk(Chunk {
                    key: None,
                    delta: Output::Documents(
                        results
                            .iter()
                            .map(|record| record.document.content.clone())
                            .collect(),
                    ),
                    finished: true,
                })
                .await?;
        }
        let output = self.serialize_output(&results)?;
        Ok(Output::Text(output))
    }
}
