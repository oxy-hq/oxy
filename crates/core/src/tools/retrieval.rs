use crate::{
    adapters::vector_store::VectorStore,
    execute::{
        Executable, ExecutionContext,
        types::{Chunk, Document, Output, Prompt},
    },
    observability::events,
};
use oxy_shared::errors::OxyError;

use super::types::RetrievalInput;

#[derive(Debug, Clone)]
pub struct RetrievalExecutable;

impl Default for RetrievalExecutable {
    fn default() -> Self {
        Self::new()
    }
}

impl RetrievalExecutable {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl Executable<RetrievalInput> for RetrievalExecutable {
    type Response = Output;

    #[tracing::instrument(skip_all, err, fields(
        otel.name = events::tool::RETRIEVAL_EXECUTE,
        oxy.span_type = events::tool::TOOL_CALL_TYPE,
    ))]
    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: RetrievalInput,
    ) -> Result<Self::Response, OxyError> {
        events::tool::tool_call_input(&input);
        execution_context
            .write_chunk(Chunk {
                key: None,
                delta: Output::Prompt(Prompt::new("Retrieving data...".to_string())),
                finished: true,
            })
            .await?;
        let config_manager = &execution_context.project.config_manager;
        let secrets_manager = &execution_context.project.secrets_manager;
        let store = VectorStore::from_retrieval(
            config_manager,
            secrets_manager,
            &input.agent_name,
            &input.retrieval_config,
        )
        .await?;
        let results = store.search(&input.query).await?;
        let output = Output::Documents(
            results
                .iter()
                .map(|record| Document {
                    content: record.retrieval_item.content.clone(),
                    id: record.retrieval_item.source_identifier.clone(),
                    kind: record.retrieval_item.source_type.clone(),
                })
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

        events::tool::tool_call_output(&output);
        Ok(output)
    }
}
