use crate::{
    adapters::{
        openai::ConfigType,
        vector_store::{VectorStore, parse_sql_source_type},
    },
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        types::{Chunk, Document, Output, Prompt},
    },
};

use super::types::RetrievalInput;

#[derive(Debug, Clone)]
pub struct RetrievalExecutable;

impl RetrievalExecutable {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl<C> Executable<RetrievalInput<C>> for RetrievalExecutable
where
    C: TryInto<ConfigType, Error = OxyError> + Send + 'static,
{
    type Response = Output;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: RetrievalInput<C>,
    ) -> Result<Self::Response, OxyError> {
        execution_context
            .write_chunk(Chunk {
                key: None,
                delta: Output::Prompt(Prompt::new("Retrieving data...".to_string())),
                finished: true,
            })
            .await?;
        let RetrievalInput {
            query,
            db_config,
            db_name,
            openai_config,
            embedding_config,
        } = input;
        let store = VectorStore::new(
            &execution_context.config,
            &db_config,
            &db_name,
            openai_config,
            embedding_config,
        )
        .await?;
        let results = store.search(&query).await?;
        let output = Output::Documents(
            results
                .iter()
                .map(
                    |record| match parse_sql_source_type(&record.document.source_type) {
                        Some(_) => Document {
                            content: record.document.embedding_content.clone(),
                            id: record.document.source_identifier.clone(),
                            kind: record.document.source_type.clone(),
                        },
                        None => Document {
                            content: record.document.content.clone(),
                            id: record.document.source_identifier.clone(),
                            kind: record.document.source_type.clone(),
                        },
                    },
                )
                .collect(),
        );
        if !results.is_empty() {
            execution_context
                .write_chunk(Chunk {
                    key: None,
                    delta: output.clone(),
                    finished: true,
                })
                .await?;
        }
        Ok(output)
    }
}
