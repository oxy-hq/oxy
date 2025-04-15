use crate::{
    config::{ConfigManager, model::ToolType},
    errors::OxyError,
};

use super::{VectorStore, types::SearchRecord};

pub async fn search_agent(
    config: &ConfigManager,
    agent_ref: &str,
    query: &str,
) -> Result<Vec<SearchRecord>, OxyError> {
    let agent = config.resolve_agent(agent_ref).await?;
    let mut results = vec![];
    for retrieval in &agent.tools_config.tools {
        if let ToolType::Retrieval(retrieval) = retrieval {
            println!(
                "{}",
                format!(
                    "Searching using agent {} tool {} ...",
                    &agent.name, retrieval.name
                )
            );
            let vector_store = VectorStore::from_retrieval(config, &agent.name, retrieval).await?;
            let documents = vector_store.search(query).await?;
            for document in documents.iter() {
                println!("\n{}\n", format!("{:?}", document));
                println!("---");
            }
            results.extend(documents);
        }
    }
    Ok(results)
}
