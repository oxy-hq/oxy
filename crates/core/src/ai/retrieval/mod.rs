use embedding::{Document, LanceDBStore, VectorStore};
use serde::Deserialize;

use crate::StyledText;
use crate::config::ConfigManager;
use crate::config::model::{RetrievalConfig, ToolType};
use crate::errors::OxyError;

pub mod embedding;
pub mod reranking;

#[derive(Debug, Clone, Deserialize)]
struct ContextHeader {
    oxy: OxyHeaderData,
}

#[derive(Debug, Clone, Deserialize)]
struct OxyHeaderData {
    embed: String,
}

// example format
// /*
// oxy:
//     embed: |
//         this return fruit with sales
//         fruit including apple, banana, kiwi, cherry and orange
// */
// select 'apple' as name, 325 as sales
// union all
// select 'banana' as name, 2000 as sales
// union all
// select 'cherry' as name, 18 as sales
// union all
// select 'kiwi' as name, 120 as sales
// union all
// select 'orange' as name, 1500 as sales
fn parse_embed_document(content: &str) -> Option<(String, ContextHeader)> {
    let context_regex = regex::Regex::new(r"(?m)^\/\*((?:.|\n)+)\*\/((.|\n)+)$").unwrap();
    let context_match = context_regex.captures(content);
    if context_match.is_none() {
        return None;
    }
    let context_match = context_match.unwrap();
    let comment_content = context_match[1].replace("\n*", "\n");
    let context_content = context_match[2].to_string();
    let header_data: Result<ContextHeader, serde_yaml::Error> =
        serde_yaml::from_str(&comment_content.as_str());
    if header_data.is_err() {
        return None;
    }

    let header_data = header_data.unwrap();
    return Some((context_content.trim().to_owned(), header_data));
}

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
        .map(|(file, content)| {
            let content = content.unwrap_or("".to_owned());
            let parsed_content = parse_embed_document(content.as_str());
            match parsed_content {
                Some((context_content, header_data)) => {
                    return Document {
                        content: format!("{}\n\n{}", header_data.oxy.embed, context_content),
                        source_type: "file".to_string(),
                        source_identifier: file.to_string(),
                        embedding_content: header_data.oxy.embed,
                        embeddings: vec![],
                    };
                }
                None => {
                    return Document {
                        content: content.to_owned(),
                        source_type: "file".to_string(),
                        source_identifier: file.to_string(),
                        embedding_content: content.to_owned(),
                        embeddings: vec![],
                    };
                }
            }
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

        for tool in agent.tools_config.tools {
            if let ToolType::Retrieval(retrieval) = tool {
                let db_path = config
                    .resolve_file(format!(".db-{}-{}", &agent.name, retrieval.name))
                    .await?;
                let db: Box<dyn VectorStore + Send + Sync> =
                    get_vector_store(&retrieval, &db_path)?;
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
    tool_config: &RetrievalConfig,
    db_path: &str,
) -> anyhow::Result<Box<dyn VectorStore + Send + Sync>> {
    let db = LanceDBStore::with_config(tool_config, db_path)?;
    Ok(Box::new(db))
}
