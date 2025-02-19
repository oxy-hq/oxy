use super::Tool;
use crate::{
    ai::retrieval::{embedding::VectorStore, get_vector_store},
    config::model::{Config, RetrievalTool as Retrieval},
    execute::agent::ToolCall,
};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, Debug, JsonSchema)]
pub struct RetrieveParams {
    pub query: String,
}

#[derive(Debug)]
pub struct RetrieveTool {
    pub name: String,
    pub vector_db: Box<dyn VectorStore + Send + Sync>,
    pub tool_description: String,
}

impl RetrieveTool {
    pub fn new(agent_name: &str, retrieval: &Retrieval, config: &Config) -> Self {
        let vector_db =
            get_vector_store(agent_name, retrieval, config).expect("Failed to init vector store");
        RetrieveTool {
            name: retrieval.name.to_string(),
            vector_db,
            tool_description: retrieval.description.to_string(),
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
    fn validate(&self, parameters: &str) -> anyhow::Result<Self::Input> {
        match serde_json::from_str::<Self::Input>(parameters) {
            Ok(params) => Ok(params),
            Err(_) => Ok(RetrieveParams {
                query: parameters.to_string(),
            }),
        }
    }
    async fn call_internal(&self, parameters: &RetrieveParams) -> anyhow::Result<ToolCall> {
        let results = self.vector_db.search(&parameters.query).await;
        let mut output = String::new();
        match results {
            Ok(results) => {
                for result in results {
                    output.push_str(&format!("{}\n", result.content));
                }
            }
            Err(e) => {
                log::error!("Error: {e}");
            }
        }
        Ok(ToolCall {
            name: self.name(),
            output,
            metadata: None,
        })
    }
}
