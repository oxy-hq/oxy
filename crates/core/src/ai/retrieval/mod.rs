use embedding::{Document, LanceDBStore, VectorStore};

use crate::StyledText;
use crate::config::ConfigManager;
use crate::config::model::{RetrievalTool, ToolConfig};
use crate::errors::OxyError;

pub mod embedding;
pub mod reranking;

async fn get_documents_from_files(
    src: &Vec<String>,
    config: &ConfigManager,
) -> anyhow::Result<Vec<Document>> {
    let files = config.resolve_glob(src).await?;
    println!("{}", format!("Found: {:?}", files).text());
    let documents = files
        .iter()
        .map(|file| (file, std::fs::read_to_string(file)))
        .filter(|(_file, content)| !content.as_ref().unwrap().is_empty())
        .map(|(file, content)| Document {
            content: content.unwrap(),
            source_type: "file".to_string(),
            source_identifier: file.to_string(),
            embeddings: vec![],
        })
        .collect();
    Ok(documents)
}

pub async fn build_embeddings(config: &ConfigManager) -> Result<(), OxyError> {
    for agent_dir in config.list_agents().await? {
        println!(
            "{}",
            format!("Building embeddings for agent: {:?}", agent_dir).text()
        );
        let agent = config.resolve_agent(&agent_dir).await?;

        for tool in agent.tools {
            if let ToolConfig::Retrieval(retrieval) = tool {
                let db_path = config
                    .resolve_file(format!(".db-{}-{}", &agent.name, retrieval.name))
                    .await?;
                let db = get_vector_store(&retrieval, &db_path)?;
                let documents = get_documents_from_files(&retrieval.src, config).await?;
                if documents.is_empty() {
                    println!(
                        "{}",
                        format!(
                            "No documents found for agent: {:?} tool: {}",
                            &agent.name, retrieval.name
                        )
                        .text()
                    );
                    continue;
                }
                db.embed(&documents).await?;
            }
        }
    }
    Ok(())
}

pub fn get_vector_store(
    tool_config: &RetrievalTool,
    db_path: &str,
) -> anyhow::Result<Box<dyn VectorStore + Send + Sync>> {
    let db = LanceDBStore::with_config(tool_config, db_path);
    Ok(Box::new(db))
}
