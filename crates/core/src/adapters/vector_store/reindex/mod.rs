use super::Document;
use crate::{
    adapters::vector_store::VectorStore,
    config::{ConfigManager, model::ToolType},
    errors::OxyError,
};
use parse::{Embed, parse_embed_document};
use std::path::PathBuf;

mod parse;

pub async fn reindex_all(config: &ConfigManager) -> Result<(), OxyError> {
    for agent_dir in config.list_agents().await? {
        println!(
            "{}",
            format!("Building embeddings for agent: {:?}", agent_dir)
        );
        let agent = config.resolve_agent(&agent_dir).await?;

        for tool in agent.tools_config.tools {
            if let ToolType::Retrieval(retrieval) = tool {
                let db = VectorStore::from_retrieval(config, &agent.name, &retrieval).await?;
                let documents = make_documents_from_files(&retrieval.src, config).await?;
                if documents.is_empty() {
                    println!(
                        "{}",
                        format!(
                            "No documents found for agent: {:?} tool: {}",
                            &agent.name, retrieval.name
                        )
                    );
                    continue;
                }
                db.embed(&documents).await?;
            }
        }
    }
    Ok(())
}

async fn make_documents_from_files(
    src: &Vec<String>,
    config: &ConfigManager,
) -> anyhow::Result<Vec<Document>> {
    let files = config.resolve_glob(src).await?;
    println!("{}", format!("Found: {:?}", files));
    let mut documents = vec![];
    files
        .iter()
        .map(|file| (file, std::fs::read_to_string(file)))
        .filter(|(_file, content)| !content.as_ref().unwrap().is_empty())
        .for_each(|(file, content)| {
            let content = content.unwrap_or("".to_owned());
            let parsed_content = parse_embed_document(content.as_str());
            let file_name = PathBuf::from(file)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            match parsed_content {
                Some((context_content, header_data)) => match header_data.oxy.embed {
                    Embed::String(embed) => {
                        let doc = Document {
                            content: format!("{}\n\n{}", embed, context_content),
                            source_type: "file".to_string(),
                            source_identifier: file.to_string(),
                            embedding_content: embed,
                            embeddings: vec![],
                        };
                        tracing::info!("Found 1 embed for file: {:?}", file_name);
                        documents.push(doc);
                    }
                    Embed::Multiple(embeds) => {
                        let length = embeds.clone().len();
                        for embed in embeds {
                            let doc = Document {
                                content: format!("{}\n\n{}", embed, context_content),
                                source_type: "file".to_string(),
                                source_identifier: file.to_string(),
                                embedding_content: embed,
                                embeddings: vec![],
                            };
                            documents.push(doc);
                        }
                        tracing::info!("Found {} embeds for file: {:?}", length, file_name);
                    }
                },
                None => {
                    documents.push(Document {
                        content: content.to_owned(),
                        source_type: "file".to_string(),
                        source_identifier: file.to_string(),
                        embedding_content: content.to_owned(),
                        embeddings: vec![],
                    });
                    tracing::info!(
                        "No embed found for file: {:?}, embedding the whole file content",
                        file_name
                    );
                }
            }
        });
    Ok(documents)
}
