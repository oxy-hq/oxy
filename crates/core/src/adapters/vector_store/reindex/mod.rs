use super::Document;
use crate::{
    adapters::vector_store::{
        VectorStore,
        types::RetrievalContent,
        utils::build_content_for_llm_retrieval,
    },
    config::{
        ConfigManager,
        model::{AgentConfig, AgentType, RouteRetrievalConfig, RoutingAgent, ToolType, Workflow},
    },
    errors::OxyError,
};
use futures::StreamExt;
use itertools::Itertools;
use parse::parse_embed_document;
pub use parse::parse_sql_source_type;
use std::path::PathBuf;

mod parse;

trait DocumentSource {
    fn description(&self) -> &str;
    fn retrieval(&self) -> &Option<RouteRetrievalConfig>;
}

impl DocumentSource for Workflow {
    fn description(&self) -> &str {
        &self.description
    }

    fn retrieval(&self) -> &Option<RouteRetrievalConfig> {
        &self.retrieval
    }
}

impl DocumentSource for AgentConfig {
    fn description(&self) -> &str {
        &self.description
    }

    fn retrieval(&self) -> &Option<RouteRetrievalConfig> {
        &self.retrieval
    }
}

fn create_document_from_source<T: DocumentSource>(
    source: &T,
    source_type: &str,
    file_path: &str,
) -> Result<Option<Document>, OxyError> {
    let nothing_to_embed = source.description().is_empty() && 
        source.retrieval().as_ref()
            .map_or(true, |retrieval| retrieval.include.is_empty());
    
    if nothing_to_embed {
        println!("WARNING: {} {} has empty description and no retrieval include patterns, skipping", source_type, file_path);
        return Ok(None);
    }

    let (retrieval_inclusions, retrieval_exclusions) = if let Some(retrieval) = source.retrieval() {
        let mut inclusions = vec![];
        
        if !source.description().is_empty() {
            inclusions.push(RetrievalContent {
                embedding_content: source.description().to_string(),
                embeddings: vec![],
            });
        }
        
        for pattern in &retrieval.include {
            inclusions.push(RetrievalContent {
                embedding_content: pattern.clone(),
                embeddings: vec![],
            });
        }
        
        let exclusions = retrieval.exclude
            .iter()
            .map(|pattern| RetrievalContent {
                embedding_content: pattern.clone(),
                embeddings: vec![],
            })
            .collect();
        (inclusions, exclusions)
    } else {
        println!("{} {} has no retrieval config, using description as inclusion and empty exclusions", source_type, file_path);
        if source.description().is_empty() {
            return Err(OxyError::ConfigurationError(format!(
                "Unexpected state: {} {} has empty description and no retrieval config", 
                source_type, file_path
            )));
        }
        (vec![RetrievalContent {
            embedding_content: source.description().to_string(),
            embeddings: vec![],
        }], vec![])
    };
    
    if retrieval_inclusions.is_empty() {
        return Err(OxyError::ConfigurationError(format!(
            "No embeddable content found for {} {}", 
            source_type, file_path
        )));
    }

    let mut document = Document {
        content: String::new(),
        source_type: source_type.to_string(),
        source_identifier: file_path.to_string(),
        retrieval_inclusions,
        retrieval_exclusions,
        inclusion_midpoint: vec![],
        inclusion_radius: 0.0,
    };
    
    document.content = build_content_for_llm_retrieval(&document);
    
    println!("Created document for {}: {} with {} exclusions", source_type, file_path, document.retrieval_exclusions.len());
    Ok(Some(document))
}

async fn process_workflow_file(
    config: &ConfigManager,
    workflow_path: &str,
) -> Result<Vec<Document>, OxyError> {
    let workflow = config.resolve_workflow(workflow_path).await?;
    
    match create_document_from_source(&workflow, "workflow", workflow_path)? {
        Some(document) => Ok(vec![document]),
        None => Ok(vec![]),
    }
}

async fn process_agent_file(
    config: &ConfigManager,
    agent_path: &str,
) -> Result<Vec<Document>, OxyError> {
    let agent = config.resolve_agent(agent_path).await?;
    
    match create_document_from_source(&agent, "agent", agent_path)? {
        Some(document) => Ok(vec![document]),
        None => Ok(vec![]),
    }
}

pub async fn reindex_all(config: &ConfigManager, drop_all_tables: bool) -> Result<(), OxyError> {
    for agent_dir in config.list_agents().await? {
        println!(
            "{}",
            format!("Building embeddings for agent: {:?}", agent_dir)
        );
        let agent = config.resolve_agent(&agent_dir).await?;
        match &agent.r#type {
            AgentType::Default(default_agent) => {
                println!("Processing DEFAULT agent: {}", agent.name);
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
    
    if paths.is_empty() {
        println!("WARNING: No paths resolved from routing agent glob patterns: {:?}", routing_agent.routes);
        return Ok(vec![]);
    }
    
    let paths_len = paths.len();
    let get_documents = async |path: String| -> Result<Vec<Document>, OxyError> {
        match &path {
            workflow_path if workflow_path.ends_with(".workflow.yml") => {
                process_workflow_file(config, workflow_path).await
            }
            agent_path if agent_path.ends_with(".agent.yml") => {
                process_agent_file(config, agent_path).await
            }
            sql_path if path.ends_with(".sql") => {
                let content = tokio::fs::read_to_string(sql_path).await?;
                
                let mut documents = vec![];
                let parsed_document = parse_embed_document(sql_path, &content);
                
                if let Some(database_ref) = parse_sql_source_type(&parsed_document.source_type) {
                    config.resolve_database(&database_ref)?;
                    documents.push(parsed_document);
                } else {
                    println!("WARNING: Could not parse database reference from source_type '{}' for document in {}", parsed_document.source_type, sql_path);
                }
                
                println!("Created {} documents from SQL file: {}", documents.len(), sql_path);
                Ok(documents)
            }
            _ => Err(OxyError::ConfigurationError(format!(
                "Unsupported file format for path: {path}"
            ))),
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

    println!("Routing agent document creation completed: {} total documents created from {} paths", documents.len(), paths_len);
    
    if documents.is_empty() {
        println!("WARNING: No documents were created for routing agent. This may indicate:");
        println!("  - All workflow/agent files have empty descriptions");
        println!("  - SQL files contain no valid embeddable content");
        println!("  - Database references in SQL files are invalid");
        println!("  - File parsing failed for all files");
    }

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
        .map(|(file, content)| {
            let content = content.unwrap_or("".to_owned());
            let document = parse_embed_document(file, content.as_str());
            let file_name = PathBuf::from(file)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            tracing::info!("Found embeddable content for file: {:?}", file_name);
            document
        })
        .collect::<Vec<_>>();
    Ok(documents)
}
