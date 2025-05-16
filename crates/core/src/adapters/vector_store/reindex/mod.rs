use super::Document;
use crate::{
    adapters::vector_store::VectorStore,
    config::{
        ConfigManager,
        model::{AgentType, RoutingAgent, ToolType},
    },
    errors::OxyError,
};
use futures::StreamExt;
use itertools::Itertools;
use parse::parse_embed_document;
pub use parse::parse_sql_source_type;
use std::path::PathBuf;

mod parse;

pub async fn reindex_all(config: &ConfigManager, drop_all_tables: bool) -> Result<(), OxyError> {
    for agent_dir in config.list_agents().await? {
        println!(
            "{}",
            format!("Building embeddings for agent: {:?}", agent_dir)
        );
        let agent = config.resolve_agent(&agent_dir).await?;
        match &agent.r#type {
            AgentType::Default(default_agent) => {
                for tool in &default_agent.tools_config.tools {
                    if let ToolType::Retrieval(retrieval) = tool {
                        let db =
                            VectorStore::from_retrieval(config, &agent.name, retrieval).await?;
                        if drop_all_tables {
                            db.cleanup().await?;
                        }
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
            AgentType::Routing(routing_agent) => {
                let db = VectorStore::from_routing_agent(
                    config,
                    &agent.name,
                    &agent.model,
                    routing_agent,
                )
                .await?;
                if drop_all_tables {
                    db.cleanup().await?;
                }
                let documents = make_documents_from_routing_agent(config, routing_agent).await?;
                if documents.is_empty() {
                    println!(
                        "{}",
                        format!("No documents found for routing agent: {:?}", &agent.name)
                    );
                    continue;
                }
                db.embed(&documents).await?;
            }
        }
    }
    Ok(())
}

async fn make_documents_from_routing_agent(
    config: &ConfigManager,
    routing_agent: &RoutingAgent,
) -> Result<Vec<Document>, OxyError> {
    let paths = config.resolve_glob(&routing_agent.routes).await?;
    let get_documents = async |path: String| -> Result<Vec<Document>, OxyError> {
        match &path {
            workflow_path if workflow_path.ends_with(".workflow.yml") => {
                let workflow = config.resolve_workflow(workflow_path).await?;
                if workflow.description.is_empty() {
                    return Ok(vec![]);
                }
                Ok(vec![Document {
                    content: workflow.description.clone(),
                    source_type: "workflow".to_string(),
                    source_identifier: workflow_path.to_string(),
                    embedding_content: workflow.description,
                    embeddings: vec![],
                }])
            }
            agent_path if agent_path.ends_with(".agent.yml") => {
                let agent = config.resolve_agent(agent_path).await?;
                if agent.description.is_empty() {
                    return Ok(vec![]);
                }
                Ok(vec![Document {
                    content: agent.description.clone(),
                    source_type: "agent".to_string(),
                    source_identifier: agent_path.to_string(),
                    embedding_content: agent.description,
                    embeddings: vec![],
                }])
            }
            sql_path if path.ends_with(".sql") => {
                let content = tokio::fs::read_to_string(sql_path).await?;
                let mut documents = vec![];
                for document in parse_embed_document(sql_path, &content) {
                    match parse_sql_source_type(&document.source_type) {
                        Some(database_ref) => {
                            config.resolve_database(&database_ref)?;
                            documents.push(document);
                        }
                        None => {}
                    }
                }
                Ok(documents)
            }
            _ => {
                return Err(OxyError::ConfigurationError(format!(
                    "Unsupported file format for path: {}",
                    path
                )));
            }
        }
    };
    let documents = async_stream::stream! {
        for path in paths {
            yield get_documents(path.to_string());
        }
    }
    .buffered(10)
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .try_collect::<Vec<Document>, Vec<_>, OxyError>()?
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();

    Ok(documents)
}

async fn make_documents_from_files(
    src: &Vec<String>,
    config: &ConfigManager,
) -> anyhow::Result<Vec<Document>> {
    let files = config.resolve_glob(src).await?;
    println!("{}", format!("Found: {:?}", files));
    let documents = files
        .iter()
        .map(|file| (file, std::fs::read_to_string(file)))
        .filter(|(_file, content)| !content.as_ref().unwrap().is_empty())
        .flat_map(|(file, content)| {
            let content = content.unwrap_or("".to_owned());
            let documents = parse_embed_document(file, content.as_str());
            let file_name = PathBuf::from(file)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            tracing::info!("Found {} embeds for file: {:?}", documents.len(), file_name);
            documents
        })
        .collect::<Vec<_>>();
    Ok(documents)
}
