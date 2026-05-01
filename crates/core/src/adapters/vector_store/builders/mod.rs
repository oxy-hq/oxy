use crate::{
    adapters::{
        secrets::SecretsManager,
        vector_store::{VectorStore, types::RetrievalObject},
    },
    config::{
        ConfigManager,
        agent_config::AgenticConfig,
        model::{AgentConfig, AgentType, RouteRetrievalConfig, ToolType, Workflow},
    },
};
use futures::StreamExt;
use oxy_semantic::Topic;
use oxy_shared::errors::OxyError;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use parse::parse_retrieval_object;
pub use parse::parse_sql_source_type;

pub mod parameterized;
mod parse;

// Minimal metadata used to build retrieval objects from various sources
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
struct RetrievalMetadata {
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    retrieval: Option<RouteRetrievalConfig>,
    #[serde(default)]
    source_type: String,
}

impl From<&Workflow> for RetrievalMetadata {
    fn from(w: &Workflow) -> Self {
        RetrievalMetadata {
            description: Some(w.description.clone()),
            retrieval: w.retrieval.clone(),
            source_type: "workflow".to_string(),
        }
    }
}

impl From<&AgentConfig> for RetrievalMetadata {
    fn from(a: &AgentConfig) -> Self {
        RetrievalMetadata {
            description: Some(a.description.clone()),
            retrieval: a.retrieval.clone(),
            source_type: "agent".to_string(),
        }
    }
}

impl From<&AgenticConfig> for RetrievalMetadata {
    fn from(a: &AgenticConfig) -> Self {
        let description = if a.instruction.trim().is_empty() {
            a.start.start.description.clone()
        } else {
            a.instruction.clone()
        };
        RetrievalMetadata {
            description: Some(description),
            retrieval: None,
            source_type: "agentic_workflow".to_string(),
        }
    }
}

impl From<&Topic> for RetrievalMetadata {
    fn from(t: &Topic) -> Self {
        RetrievalMetadata {
            description: t.description.clone(),
            retrieval: t.retrieval.as_ref().map(|r| RouteRetrievalConfig {
                include: r.include.clone(),
                exclude: r.exclude.clone(),
            }),
            source_type: "topic".to_string(),
        }
    }
}

impl RetrievalMetadata {
    fn from_yaml_str(content: &str) -> Result<Self, OxyError> {
        let mut r: RetrievalMetadata = serde_yaml::from_str(content)
            .map_err(|e| OxyError::ConfigurationError(format!("Failed to parse YAML: {}", e)))?;
        r.source_type = "yaml".to_string();
        Ok(r)
    }

    fn normalized_description(&self) -> Option<String> {
        self.description.as_ref().and_then(|s| {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        })
    }
}

pub async fn build_all_retrieval_objects(
    config: &ConfigManager,
) -> Result<Vec<RetrievalObject>, OxyError> {
    let mut all_objects = Vec::new();

    for agent_dir in config.list_agents().await? {
        tracing::info!(agent = %agent_dir.display(), "Building retrieval objects for agent");
        let agent = config.resolve_agent(&agent_dir).await?;
        match &agent.r#type {
            AgentType::Default(default_agent) => {
                for tool in &default_agent.tools_config.tools {
                    if let ToolType::Retrieval(retrieval) = tool {
                        let objects =
                            build_retrieval_objects_from_files(&retrieval.src, config).await?;
                        if !objects.iter().any(|o| !o.inclusions.is_empty()) {
                            tracing::warn!(
                                agent = %agent.name,
                                tool = %retrieval.name,
                                "No inclusion records found for agent tool"
                            );
                            continue;
                        }
                        all_objects.extend(objects);
                    }
                }
            }
            AgentType::Routing(routing_agent) => {
                let objects =
                    build_retrieval_objects_from_routes(&routing_agent.routes, config).await?;
                if !objects.iter().any(|o| !o.inclusions.is_empty()) {
                    tracing::warn!(agent = %agent.name, "No inclusion records found for routing agent");
                    continue;
                }
                all_objects.extend(objects);
            }
        }
    }

    // Process agentic workflows with routing config
    for aw_dir in config.list_agentic_workflows().await? {
        let aw = config.resolve_agentic_workflow(&aw_dir).await?;
        if let Some(routing_config) = &aw.start.routing {
            tracing::info!(workflow = %aw.name, "Building retrieval objects for agentic workflow routing");
            let objects =
                build_retrieval_objects_from_routes(&routing_config.routes, config).await?;
            if !objects.iter().any(|o| !o.inclusions.is_empty()) {
                tracing::warn!(workflow = %aw.name, "No inclusion records found for agentic workflow routing");
                continue;
            }
            all_objects.extend(objects);
        }
    }

    // Filter out empty retrieval objects (those with no inclusions)
    let non_empty_objects: Vec<RetrievalObject> = all_objects
        .into_iter()
        .filter(|o| !o.inclusions.is_empty())
        .collect();

    // Deduplicate by source_identifier, keeping first encountered
    let initial_count = non_empty_objects.len();
    let mut seen_source_identifiers = HashSet::new();
    let mut deduplicated_objects = Vec::new();

    for object in non_empty_objects {
        if seen_source_identifiers.insert(object.source_identifier.clone()) {
            deduplicated_objects.push(object);
        }
    }

    let duplicates_removed = initial_count - deduplicated_objects.len();
    if duplicates_removed > 0 {
        tracing::info!(
            duplicates_removed,
            total = initial_count,
            "Deduplicated retrieval objects with duplicate source_identifiers"
        );
    }

    Ok(deduplicated_objects)
}

// TODO: This function probably doesn't belong in builders:: and should be moved
pub async fn ingest_retrieval_objects(
    config: &ConfigManager,
    secrets_manager: &SecretsManager,
    retrieval_objects: &[RetrievalObject],
    drop_all_tables: bool,
) -> Result<(), OxyError> {
    for agent_dir in config.list_agents().await? {
        tracing::info!(agent = %agent_dir.display(), "Building embeddings for agent");
        let agent = config.resolve_agent(&agent_dir).await?;
        match &agent.r#type {
            AgentType::Default(default_agent) => {
                for tool in &default_agent.tools_config.tools {
                    if let ToolType::Retrieval(retrieval) = tool {
                        let db = VectorStore::from_retrieval(
                            config,
                            secrets_manager,
                            &agent.name,
                            retrieval,
                        )
                        .await?;

                        if drop_all_tables {
                            db.cleanup().await?;
                        }

                        if !retrieval_objects.iter().any(|o| !o.inclusions.is_empty()) {
                            tracing::warn!(
                                agent = %agent.name,
                                tool = %retrieval.name,
                                "No inclusion records found for agent tool"
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
                    secrets_manager,
                    &agent.name,
                    &agent.model,
                    routing_agent,
                )
                .await?;

                if drop_all_tables {
                    db.cleanup().await?;
                }

                let route_objects =
                    build_retrieval_objects_from_routes(&routing_agent.routes, config).await?;
                if !route_objects.iter().any(|o| !o.inclusions.is_empty()) {
                    tracing::warn!(agent = %agent.name, "No inclusion records found for routing agent");
                    continue;
                }
                db.ingest(&route_objects).await?;
            }
        }
    }

    // Process agentic workflows with routing config
    for aw_dir in config.list_agentic_workflows().await? {
        let aw = config.resolve_agentic_workflow(&aw_dir).await?;
        if let Some(routing_config) = &aw.start.routing {
            tracing::info!(workflow = %aw.name, "Building embeddings for agentic workflow routing");
            let model = config.resolve_model(&aw.model)?;
            let db = VectorStore::new(
                config,
                secrets_manager,
                &routing_config.db_config,
                &format!("{}-routing", aw.name),
                model.clone(),
                routing_config.embedding_config.clone(),
            )
            .await?;

            if drop_all_tables {
                db.cleanup().await?;
            }

            let route_objects =
                build_retrieval_objects_from_routes(&routing_config.routes, config).await?;
            if !route_objects.iter().any(|o| !o.inclusions.is_empty()) {
                tracing::warn!(workflow = %aw.name, "No inclusion records found for agentic workflow routing");
                continue;
            }
            db.ingest(&route_objects).await?;
        }
    }

    Ok(())
}

fn build_retrieval_object(
    metadata: RetrievalMetadata,
    file_path: &str,
) -> Result<RetrievalObject, OxyError> {
    let mut inclusions: Vec<String> = vec![];
    let mut exclusions: Vec<String> = vec![];

    if let Some(description) = metadata.normalized_description() {
        inclusions.push(description);
    }

    if let Some(retrieval) = metadata.retrieval {
        exclusions.extend(retrieval.exclude);
        inclusions.extend(retrieval.include);
    }

    // If nothing to include, return an empty retrieval object to be filtered out upstream
    if inclusions.is_empty() {
        tracing::warn!(
            source_type = %metadata.source_type,
            file = %file_path,
            "No description or retrieval.include entries found — this source will be skipped"
        );
        return Ok(RetrievalObject {
            ..Default::default()
        });
    }

    tracing::debug!(
        inclusions = inclusions.len(),
        exclusions = exclusions.len(),
        source_type = %metadata.source_type,
        file = %file_path,
        "Built retrieval object"
    );

    Ok(RetrievalObject {
        source_identifier: file_path.to_string(),
        source_type: metadata.source_type,
        inclusions,
        exclusions,
        ..Default::default()
    })
}

async fn build_retrieval_objects_from_routes(
    routes: &Vec<String>,
    config: &ConfigManager,
) -> Result<Vec<RetrievalObject>, OxyError> {
    let paths = config.resolve_glob(routes).await?;

    if paths.is_empty() {
        tracing::warn!(patterns = ?routes, "No paths resolved from routing glob patterns");
        return Ok(vec![]);
    }

    let paths_len = paths.len();
    let get_retrieval_object = async |path: String| -> Result<RetrievalObject, OxyError> {
        match &path {
            workflow_path
                if workflow_path.ends_with(".procedure.yml")
                    || workflow_path.ends_with(".workflow.yml")
                    || workflow_path.ends_with(".automation.yml") =>
            {
                workflow_to_retrieval_object(config, workflow_path).await
            }
            agent_path if agent_path.ends_with(".agent.yml") => {
                agent_to_retrieval_object(config, agent_path).await
            }
            sql_path if path.ends_with(".sql") => sql_to_retrieval_object(config, sql_path).await,
            topic_path if topic_path.ends_with(".topic.yml") => {
                topic_to_retrieval_object(topic_path).await
            }
            aw_path if aw_path.ends_with(".aw.yml") || aw_path.ends_with(".aw.yaml") => {
                aw_to_retrieval_object(config, aw_path).await
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

    tracing::info!(
        objects = all_objects.len(),
        paths = paths_len,
        "Routing object creation completed"
    );

    if !all_objects.iter().any(|o| !o.inclusions.is_empty()) {
        tracing::warn!(
            "No inclusion records were created for routing — check that agent/workflow/topic \
             files have non-empty descriptions and that SQL files reference valid databases"
        );
    }

    Ok(all_objects)
}

async fn build_retrieval_objects_from_files(
    src: &Vec<String>,
    config: &ConfigManager,
) -> Result<Vec<RetrievalObject>, OxyError> {
    let files = config.resolve_glob(src).await?;
    if files.is_empty() {
        tracing::warn!(patterns = ?src, "No files found from glob patterns");
        return Ok(vec![]);
    }

    let get_retrieval_object = async |path: String| -> Result<RetrievalObject, OxyError> {
        match &path {
            sql_path if sql_path.ends_with(".sql") => {
                sql_to_retrieval_object(config, sql_path).await
            }
            yaml_path if yaml_path.ends_with(".yml") || yaml_path.ends_with(".yaml") => {
                yaml_to_retrieval_object(yaml_path).await
            }
            _ => Err(OxyError::ConfigurationError(format!(
                "Unsupported file format for path: {path}"
            ))),
        }
    };

    let objects_list = async_stream::stream! {
        for path in files {
            yield get_retrieval_object(path.to_string());
        }
    }
    .buffered(10)
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?;

    Ok(objects_list)
}

async fn workflow_to_retrieval_object(
    config: &ConfigManager,
    workflow_path: &str,
) -> Result<RetrievalObject, OxyError> {
    let workflow = config.resolve_workflow(workflow_path).await?;
    let metadata = RetrievalMetadata::from(&workflow);
    let mut obj = build_retrieval_object(metadata, workflow_path)?;

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

async fn agent_to_retrieval_object(
    config: &ConfigManager,
    agent_path: &str,
) -> Result<RetrievalObject, OxyError> {
    let agent = config.resolve_agent(agent_path).await?;
    let metadata = RetrievalMetadata::from(&agent);

    build_retrieval_object(metadata, agent_path)
}

async fn aw_to_retrieval_object(
    config: &ConfigManager,
    aw_path: &str,
) -> Result<RetrievalObject, OxyError> {
    let aw = config.resolve_agentic_workflow(aw_path).await?;
    let metadata = RetrievalMetadata::from(&aw);

    build_retrieval_object(metadata, aw_path)
}

async fn sql_to_retrieval_object(
    config: &ConfigManager,
    sql_path: &str,
) -> Result<RetrievalObject, OxyError> {
    let content = tokio::fs::read_to_string(sql_path).await?;
    let mut obj = parse_retrieval_object(sql_path, &content);

    // Filter inclusions by valid database references; keep exclusions as-is
    if let Some(database_ref) = parse_sql_source_type(&obj.source_type) {
        if let Err(e) = config.resolve_database(&database_ref) {
            tracing::warn!(
                database = %database_ref,
                file = %sql_path,
                error = %e,
                "Invalid database reference in SQL file — dropping inclusions"
            );
            obj.inclusions.clear();
        }
    } else if !obj.inclusions.is_empty() {
        tracing::warn!(
            source_type = %obj.source_type,
            file = %sql_path,
            "Could not parse database reference from source_type — dropping inclusions"
        );
        obj.inclusions.clear();
    }

    tracing::debug!(
        inclusions = obj.inclusions.len(),
        exclusions = obj.exclusions.len(),
        file = %sql_path,
        "Built retrieval object from SQL file"
    );

    Ok(obj)
}

async fn topic_to_retrieval_object(topic_path: &str) -> Result<RetrievalObject, OxyError> {
    let content = tokio::fs::read_to_string(topic_path).await.map_err(|e| {
        OxyError::ConfigurationError(format!("Failed to read topic file {}: {}", topic_path, e))
    })?;

    let topic: Topic = serde_yaml::from_str(&content).map_err(|e| {
        OxyError::ConfigurationError(format!("Failed to parse topic file {}: {}", topic_path, e))
    })?;

    let metadata = RetrievalMetadata::from(&topic);
    let obj = build_retrieval_object(metadata, topic_path)?;

    tracing::debug!(topic = %topic.name, file = %topic_path, "Built retrieval object for topic");

    Ok(obj)
}

async fn yaml_to_retrieval_object(yaml_path: &str) -> Result<RetrievalObject, OxyError> {
    let raw_content = tokio::fs::read_to_string(yaml_path).await?;
    let metadata = RetrievalMetadata::from_yaml_str(&raw_content)?;
    let mut obj = build_retrieval_object(metadata, yaml_path)?;
    obj.context_content = raw_content;
    tracing::debug!(
        inclusions = obj.inclusions.len(),
        exclusions = obj.exclusions.len(),
        file = %yaml_path,
        "Built retrieval object from YAML file"
    );

    Ok(obj)
}
