use embedding::{Document, LanceDBStore, VectorStore};

use crate::config::model::{Config, ProjectPath, RetrievalTool, ToolConfig};
use crate::utils::expand_globs;
use crate::StyledText;

pub mod embedding;
pub mod reranking;

fn get_documents_from_files(src: &Vec<String>) -> anyhow::Result<Vec<Document>> {
    let files = expand_globs(src, ProjectPath::get())?;
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

pub async fn build_embeddings(config: &Config) -> anyhow::Result<()> {
    for agent_dir in config.list_agents(&ProjectPath::get()) {
        println!(
            "{}",
            format!("Building embeddings for agent: {:?}", agent_dir).text()
        );
        let (agent, agent_name) = config.load_agent_config(Some(&agent_dir))?;

        for tool in agent.tools {
            if let ToolConfig::Retrieval(retrieval) = tool {
                let db = get_vector_store(&agent_name, &retrieval)?;
                let documents = get_documents_from_files(&retrieval.src)?;
                db.embed(&documents).await?;
            }
        }
    }
    Ok(())
}

pub fn get_vector_store(
    agent: &str,
    tool_config: &RetrievalTool,
) -> anyhow::Result<Box<dyn VectorStore + Send + Sync>> {
    let db = LanceDBStore::with_config(agent, tool_config);
    Ok(Box::new(db))
}
