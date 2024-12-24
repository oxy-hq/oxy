use super::Tool;
use crate::{
    ai::retrieval::{embedding::VectorStore, get_vector_store},
    config::model::RetrievalTool as Retrieval,
};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, Debug, JsonSchema)]
pub struct RetrieveParams {
    pub query: String,
}

pub struct RetrieveTool {
    pub name: String,
    pub vector_db: Box<dyn VectorStore + Send + Sync>,
    pub tool_description: String,
}

impl RetrieveTool {
    pub fn new(agent_name: &str, config: &Retrieval) -> Self {
        let vector_db = get_vector_store(agent_name, &config).expect("Failed to init vector store");
        RetrieveTool {
            name: config.name.to_string(),
            vector_db,
            tool_description: config.description.to_string(),
        }
    }
}

#[async_trait]
impl Tool for RetrieveTool {
    type Input = RetrieveParams;

    fn name(&self) -> String {
        self.name.clone()
    }
    fn description(&self) -> String {
        self.tool_description.clone()
    }
    async fn call_internal(&self, parameters: &RetrieveParams) -> anyhow::Result<String> {
        let results = self.vector_db.search(&parameters.query).await;
        let mut output = String::new();
        output.push_str("Queries:\n");
        for result in results.ok().unwrap() {
            output.push_str(&format!("{}\n", result.content));
        }
        Ok(output)
    }
}
