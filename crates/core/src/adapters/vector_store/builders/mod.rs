use std::{collections::HashSet, path::PathBuf, fs::read_to_string};
use crate::{
    adapters::vector_store::{
        types::RetrievalObject,
        VectorStore,
    },
    config::{
        ConfigManager,
        model::{AgentConfig, AgentType, RouteRetrievalConfig, RoutingAgent, ToolType, Workflow},
    },
    errors::OxyError,
};
use futures::StreamExt;
use oxy_semantic::Topic;

use parse::parse_retrieval_object;
pub use parse::parse_sql_source_type;

mod parse;
pub mod parameterized;

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

pub async fn build_all_retrieval_objects(config: &ConfigManager) -> Result<Vec<RetrievalObject>, OxyError> {
    let mut all_objects = Vec::new();

    for agent_dir in config.list_agents().await? {
        println!(
            "{}",
            format!("Building retrieval objects for agent: {:?}", agent_dir)
        );
        let agent = config.resolve_agent(&agent_dir).await?;
        match &agent.r#type {
            AgentType::Default(default_agent) => {
                println!("Processing DEFAULT agent: {}", agent.name);
                for tool in &default_agent.tools_config.tools {
                    if let ToolType::Retrieval(retrieval) = tool {
                        let objects = build_retrieval_objects_from_files(&retrieval.src, config).await?;
                        if !objects.iter().any(|o| !o.inclusions.is_empty()) {
                            println!(
                                "{}",
                                format!(
                                    "No inclusion records found for agent: {:?} tool: {}",
                                    &agent.name, retrieval.name
                                )
                            );
                            continue;
                        }
                        all_objects.extend(objects);
                    }
                }
            }
            AgentType::Routing(routing_agent) => {
                let objects = build_retrieval_objects_from_routing_agent(config, routing_agent).await?;
                if !objects.iter().any(|o| !o.inclusions.is_empty()) {
                    println!(
                        "{}",
                        format!("No inclusion records found for routing agent: {:?}", &agent.name)
                    );
                    continue;
                }
                all_objects.extend(objects);
            }
        }
    }

    // Deduplicate by source_identifier, keeping first encountered
    let initial_count = all_objects.len();
    let mut seen_source_identifiers = HashSet::new();
    let mut deduplicated_objects = Vec::new();
    
    for object in all_objects {
        if seen_source_identifiers.insert(object.source_identifier.clone()) {
            deduplicated_objects.push(object);
        }
    }
    
    let deduplicated_count = initial_count - deduplicated_objects.len();
    if deduplicated_count > 0 {
        println!(
            "Deduplicated {} retrieval objects with duplicate source_identifiers from {} total objects",
            deduplicated_count, initial_count
        );
    }
    
    Ok(deduplicated_objects)
}

pub async fn ingest_retrieval_objects(
    config: &ConfigManager,
    retrieval_objects: &[RetrievalObject],
    drop_all_tables: bool,
) -> Result<(), OxyError> {
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
                        
                        if !retrieval_objects.iter().any(|o| !o.inclusions.is_empty()) {
                            println!(
                                "{}",
                                format!(
                                    "No inclusion records found for agent: {:?} tool: {}",
                                    &agent.name, retrieval.name
                                )
                            );
                            continue;
                        }
                        db.ingest(&retrieval_objects.to_vec()).await?;
                    }
                }
            }
            AgentType::Routing(routing_agent) => {
                let db = VectorStore::from_routing_agent(
                    config,
                    &agent.name,
                    &agent.model,
                    routing_agent,
                ).await?;
                
                if drop_all_tables {
                    db.cleanup().await?;
                }
                    
                if !retrieval_objects.iter().any(|o| !o.inclusions.is_empty()) {
                    println!(
                        "{}",
                        format!("No inclusion records found for routing agent: {:?}", &agent.name)
                    );
                    continue;
                }
                db.ingest(&retrieval_objects.to_vec()).await?;
            }
        }
    }
    Ok(())
}

fn build_retrieval_object_from_source<T: DocumentSource>(
    source: &T,
    source_type: &str,
    file_path: &str,
) -> Result<RetrievalObject, OxyError> {
    let nothing_to_embed = source.description().is_empty()
        && source
            .retrieval()
            .as_ref()
            .is_none_or(|retrieval| retrieval.include.is_empty());

    if nothing_to_embed {
        println!(
            "WARNING: {source_type} {file_path} has empty description and no retrieval include patterns, skipping"
        );
        return Ok(RetrievalObject {
            source_identifier: file_path.to_string(),
            source_type: source_type.to_string(),
            ..Default::default()
        });
    }

    let mut inclusions: Vec<String> = vec![];
    let mut exclusions: Vec<String> = vec![];

    if let Some(retrieval) = source.retrieval() {
        exclusions.extend(retrieval.exclude.clone());
        if !source.description().is_empty() {
            inclusions.push(source.description().to_string());
        }
        inclusions.extend(retrieval.include.clone());
    } else {
        println!(
            "{source_type} {file_path} has no retrieval config, using description as inclusion and empty exclusions"
        );
        if source.description().is_empty() {
            return Err(OxyError::ConfigurationError(format!(
                "Unexpected state: {source_type} {file_path} has empty description and no retrieval config"
            )));
        }
        inclusions.push(source.description().to_string());
    }

    if inclusions.is_empty() {
        return Err(OxyError::ConfigurationError(format!(
            "No embeddable content found for {source_type} {file_path}"
        )));
    }

    println!(
        "Created {} inclusions and {} exclusions for {}: {}",
        inclusions.len(),
        exclusions.len(),
        source_type,
        file_path
    );
    
    Ok(RetrievalObject {
        source_identifier: file_path.to_string(),
        source_type: source_type.to_string(),
        inclusions,
        exclusions,
        ..Default::default()
    })
}

async fn build_retrieval_objects_from_routing_agent(
    config: &ConfigManager,
    routing_agent: &RoutingAgent,
) -> Result<Vec<RetrievalObject>, OxyError> {
    let paths = config.resolve_glob(&routing_agent.routes).await?;

    if paths.is_empty() {
        println!(
            "WARNING: No paths resolved from routing agent glob patterns: {:?}",
            routing_agent.routes
        );
        return Ok(vec![]);
    }

    let paths_len = paths.len();
    let get_retrieval_object = async |path: String| -> Result<RetrievalObject, OxyError> {
        match &path {
            workflow_path if workflow_path.ends_with(".workflow.yml") => {
                process_workflow_file(config, workflow_path).await
            }
            agent_path if agent_path.ends_with(".agent.yml") => {
                process_agent_file(config, agent_path).await
            }
            topic_path if topic_path.ends_with(".topic.yml") => {
                process_topic_file(config, topic_path).await
            }
            sql_path if path.ends_with(".sql") => {
                let content = tokio::fs::read_to_string(sql_path).await?;
                let mut obj = parse_retrieval_object(sql_path, &content);

                // Filter inclusions by valid database references; keep exclusions as-is
                if let Some(database_ref) = parse_sql_source_type(&obj.source_type) {
                    if let Err(e) = config.resolve_database(&database_ref) {
                        println!(
                            "WARNING: Invalid database reference '{}' in {}: {:?}. Dropping inclusions for this file",
                            database_ref, sql_path, e
                        );
                        obj.inclusions.clear();
                    }
                } else {
                    if !obj.inclusions.is_empty() {
                        println!(
                            "WARNING: Could not parse database reference from source_type '{}' for inclusion(s) in {}. Dropping inclusions.",
                            obj.source_type, sql_path
                        );
                        obj.inclusions.clear();
                    }
                }

                println!(
                    "Created {} inclusions and {} exclusions from SQL file: {}",
                    obj.inclusions.len(),
                    obj.exclusions.len(),
                    sql_path
                );
                Ok(obj)
            }
            _ => Err(OxyError::ConfigurationError(format!(
                "Unsupported file format for path: {path}"
            ))),
        }
    };
    let objects_list = async_stream::stream! {
        for path in paths {
            yield get_retrieval_object(path.to_string());
        }
    }
    .buffered(10)
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?;

    let all_objects: Vec<RetrievalObject> = objects_list;

    println!(
        "Routing agent object creation completed: {} objects created from {} paths",
        all_objects.len(),
        paths_len
    );

    if !all_objects.iter().any(|o| !o.inclusions.is_empty()) {
        println!("WARNING: No inclusion records were created for routing agent. This may indicate:");
        println!("  - All workflow/agent files have empty descriptions");
        println!("  - Topic files have empty descriptions");
        println!("  - SQL files contain no valid embeddable content");
        println!("  - Database references in SQL files are invalid");
        println!("  - File parsing failed for all files");
    }

    Ok(all_objects)
}

async fn process_workflow_file(
    config: &ConfigManager,
    workflow_path: &str,
) -> Result<RetrievalObject, OxyError> {
    let workflow = config.resolve_workflow(workflow_path).await?;
    let mut obj = build_retrieval_object_from_source(&workflow, "workflow", workflow_path)?;
    
    // Add enum variable information for workflows
    if let Some(variables) = &workflow.variables {
        let (enum_vars, _) = variables.extract_enum_variables();
        let mut enum_variables = std::collections::HashMap::new();
        for (name, values) in enum_vars {
            enum_variables.insert(name, values);
        }
        if !enum_variables.is_empty() {
            obj.enum_variables = Some(enum_variables);
        }
    }
    
    Ok(obj)
}

async fn process_agent_file(
    config: &ConfigManager,
    agent_path: &str,
) -> Result<RetrievalObject, OxyError> {
    let agent = config.resolve_agent(agent_path).await?;
    build_retrieval_object_from_source(&agent, "agent", agent_path)
}

async fn process_topic_file(
    _config: &ConfigManager,
    topic_path: &str,
) -> Result<RetrievalObject, OxyError> {
    tracing::info!("Processing topic file: {}", topic_path);

    // Read and parse the topic file
    let content = tokio::fs::read_to_string(topic_path).await.map_err(|e| {
        OxyError::ConfigurationError(format!("Failed to read topic file {}: {}", topic_path, e))
    })?;

    let topic: Topic = serde_yaml::from_str(&content).map_err(|e| {
        OxyError::ConfigurationError(format!("Failed to parse topic file {}: {}", topic_path, e))
    })?;

    let retrieval_object = RetrievalObject {
        context_content: topic.description.clone(),
        source_type: "topic".to_string(),
        source_identifier: topic_path.to_string(),
        inclusions: topic.inclusions,
        exclusions: topic.exclusions,
        ..Default::default()
    };

    println!(
        "Created retrieval object for topic: {} from file: {}",
        topic.name, topic_path
    );

    Ok(retrieval_object)
}

async fn build_retrieval_objects_from_files(
    src: &Vec<String>,
    config: &ConfigManager,
) -> anyhow::Result<Vec<RetrievalObject>> {
    let files = config.resolve_glob(src).await?;
    println!("{}", format!("Found: {:?}", files));

    let mut all_objects: Vec<RetrievalObject> = vec![];

    for file in files.iter() {
        if let Ok(content) = read_to_string(file) {
            if !content.is_empty() {
                let obj = parse_retrieval_object(file, content.as_str());
                let file_name = PathBuf::from(file)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                tracing::info!(
                    "Found {} inclusions and {} exclusions for file: {:?}", 
                    obj.inclusions.len(),
                    obj.exclusions.len(),
                    file_name
                );
                all_objects.push(obj);
            }
        }
    }

    Ok(all_objects)
}
