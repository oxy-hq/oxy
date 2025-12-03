use crate::config::model::IntegrationType;
use async_openai::{
    Client,
    config::{AzureConfig, Config, OpenAIConfig},
    types::{
        chat::{
            ChatCompletionMessageToolCall, ChatCompletionMessageToolCallChunk,
            ChatCompletionMessageToolCalls, ChatCompletionNamedToolChoice,
            ChatCompletionRequestMessage, ChatCompletionTool, ChatCompletionToolChoiceOption,
            ChatCompletionTools, CreateChatCompletionRequestArgs, CreateChatCompletionResponse,
            CreateChatCompletionStreamResponse, FunctionName, FunctionObject, FunctionObjectArgs,
            ReasoningEffort as OpenAIReasoningEffort,
        },
        responses::Reasoning,
    },
};
use axum::http::{HeaderMap, HeaderName, HeaderValue};
use omni::MetadataStorage;
use schemars::schema::RootSchema;
use secrecy::SecretString;
use std::{collections::HashMap, str::FromStr};
use tokio_stream::StreamExt;

use crate::{
    adapters::{
        create_app_schema, project::manager::ProjectManager, secrets::SecretsManager,
        semantic_tool_description::get_semantic_query_description, viz_schema,
    },
    config::{
        ConfigManager,
        constants::{ANTHROPIC_API_URL, GEMINI_API_URL},
        model::{Model, ReasoningConfig, ReasoningEffort, RetrievalConfig, ToolType},
    },
    errors::OxyError,
    execute::types::event::ArtifactKind,
    service::types::SemanticQueryParams,
    tools::types::{AgentParams, EmptySQLParams, OmniQueryParams, RetrievalParams, SQLParams},
};

#[derive(Debug, Clone)]
pub struct CustomOpenAIConfig {
    base_config: OpenAIConfig,
    custom_headers: HeaderMap,
}

impl CustomOpenAIConfig {
    pub fn new(base_config: OpenAIConfig, custom_headers: HashMap<String, String>) -> Self {
        let mut header_map = HeaderMap::new();

        for (key, value) in custom_headers {
            if let (Ok(header_name), Ok(header_value)) =
                (HeaderName::from_str(&key), HeaderValue::from_str(&value))
            {
                header_map.insert(header_name, header_value);
            } else {
                tracing::warn!("Invalid header: {} = {}", key, value);
            }
        }

        Self {
            base_config,
            custom_headers: header_map,
        }
    }
}

impl Config for CustomOpenAIConfig {
    fn headers(&self) -> HeaderMap {
        let mut headers = self.base_config.headers();

        // Add custom headers
        for (key, value) in &self.custom_headers {
            headers.insert(key.clone(), value.clone());
        }

        headers
    }

    fn url(&self, path: &str) -> String {
        self.base_config.url(path)
    }

    fn query(&self) -> Vec<(&str, &str)> {
        self.base_config.query()
    }

    fn api_base(&self) -> &str {
        self.base_config.api_base()
    }

    fn api_key(&self) -> &SecretString {
        self.base_config.api_key()
    }
}

#[derive(Debug, Clone)]
pub enum ConfigType {
    Default(OpenAIConfig),
    Azure(AzureConfig),
    WithHeaders(CustomOpenAIConfig),
}

/// This is a wrapper around OpenAIConfig and AzureConfig
/// to allow for dynamic configuration of the client
/// based on the model configuration
impl Config for ConfigType {
    fn headers(&self) -> HeaderMap {
        match &self {
            ConfigType::Default(config) => config.headers(),
            ConfigType::Azure(config) => config.headers(),
            ConfigType::WithHeaders(config) => config.headers(),
        }
    }
    fn url(&self, path: &str) -> String {
        match &self {
            ConfigType::Default(config) => config.url(path),
            ConfigType::Azure(config) => config.url(path),
            ConfigType::WithHeaders(config) => config.url(path),
        }
    }
    fn query(&self) -> Vec<(&str, &str)> {
        match &self {
            ConfigType::Default(config) => config.query(),
            ConfigType::Azure(config) => config.query(),
            ConfigType::WithHeaders(config) => config.query(),
        }
    }

    fn api_base(&self) -> &str {
        match &self {
            ConfigType::Default(config) => config.api_base(),
            ConfigType::Azure(config) => config.api_base(),
            ConfigType::WithHeaders(config) => config.api_base(),
        }
    }

    fn api_key(&self) -> &SecretString {
        match &self {
            ConfigType::Default(config) => config.api_key(),
            ConfigType::Azure(config) => config.api_key(),
            ConfigType::WithHeaders(config) => config.api_key(),
        }
    }
}

pub trait IntoOpenAIConfig {
    fn into_openai_config(
        &self,
        secrets_manager: &SecretsManager,
    ) -> impl std::future::Future<Output = Result<ConfigType, OxyError>> + std::marker::Send;
}

impl IntoOpenAIConfig for Model {
    async fn into_openai_config(
        &self,
        secrets_manager: &SecretsManager,
    ) -> Result<ConfigType, OxyError> {
        match self {
            Model::OpenAI {
                name: _,
                model_ref: _,
                api_url,
                azure,
                key_var,
                headers: custom_headers,
            } => {
                let api_key = secrets_manager.resolve_secret(key_var).await.map_err(|_| {
                    OxyError::ConfigurationError("OpenAI key not found".to_string())
                })?;
                let api_key = match api_key {
                    Some(secret) => secret,
                    None => {
                        return Err(OxyError::ConfigurationError(
                            "OpenAI key not found".to_string(),
                        ));
                    }
                };

                match azure {
                    Some(azure) => {
                        let mut config = AzureConfig::new()
                            .with_api_version(&azure.azure_api_version)
                            .with_deployment_id(&azure.azure_deployment_id)
                            .with_api_key(api_key);
                        if let Some(api_url) = api_url {
                            config = config.with_api_base(api_url);
                        }
                        Ok(ConfigType::Azure(config))
                    }
                    None => {
                        let mut config = OpenAIConfig::new().with_api_key(api_key);
                        if let Some(api_url) = api_url {
                            tracing::debug!("Setting API URL: {}", api_url);
                            config = config.with_api_base(api_url);
                        }

                        if let Some(custom_headers) = custom_headers
                            && !custom_headers.is_empty()
                        {
                            let resolved_headers = self.resolve_headers(secrets_manager).await?;
                            let config_with_headers =
                                CustomOpenAIConfig::new(config, resolved_headers);
                            return Ok(ConfigType::WithHeaders(config_with_headers));
                        }

                        tracing::debug!("Creating default OpenAI config without custom headers");
                        Ok(ConfigType::Default(config))
                    }
                }
            }
            Model::Ollama {
                name: _,
                model_ref: _,
                api_key,
                api_url,
            } => {
                let config = OpenAIConfig::new()
                    .with_api_base(api_url)
                    .with_api_key(api_key);
                Ok(ConfigType::Default(config))
            }
            Model::Google {
                name: _,
                model_ref: _,
                key_var,
            } => {
                let api_key = secrets_manager
                    .resolve_secret(key_var)
                    .await
                    .map_err(|_e| {
                        OxyError::ConfigurationError("Gemini API key not found".to_string())
                    })?;
                let api_key = match api_key {
                    Some(secret) => secret,
                    None => {
                        return Err(OxyError::ConfigurationError(
                            "Gemini API key not found".to_string(),
                        ));
                    }
                };
                let config = OpenAIConfig::new()
                    .with_api_base(GEMINI_API_URL)
                    .with_api_key(api_key);
                Ok(ConfigType::Default(config))
            }
            Model::Anthropic {
                name: _,
                model_ref: _,
                key_var,
                api_url,
            } => {
                let api_key = secrets_manager
                    .resolve_secret(key_var)
                    .await
                    .map_err(|_e| {
                        OxyError::ConfigurationError("Anthropic API key not found".to_string())
                    })?;
                let api_key = match api_key {
                    Some(secret) => secret,
                    None => {
                        return Err(OxyError::ConfigurationError(
                            "Anthropic API key not found".to_string(),
                        ));
                    }
                };
                let config = OpenAIConfig::new()
                    .with_api_base(api_url.clone().unwrap_or(ANTHROPIC_API_URL.to_string()))
                    .with_api_key(api_key);
                Ok(ConfigType::Default(config))
            }
        }
    }
}

impl IntoOpenAIConfig for RetrievalConfig {
    async fn into_openai_config(
        &self,
        secrets_manager: &SecretsManager,
    ) -> Result<ConfigType, OxyError> {
        let key_var = self.key_var.clone();
        let api_url = self.api_url.clone();
        let api_key = secrets_manager
            .resolve_secret(&key_var)
            .await
            .map_err(|e| {
                OxyError::ConfigurationError(format!("Retrieval API key not found: {e}"))
            })?;
        let api_key = match api_key {
            Some(secret) => secret,
            None => {
                return Err(OxyError::ConfigurationError(
                    "Retrieval API key not found".to_string(),
                ));
            }
        };
        Ok(ConfigType::Default(
            OpenAIConfig::new()
                .with_api_key(api_key)
                .with_api_base(api_url.to_string()),
        ))
    }
}

pub type OpenAIClient = Client<ConfigType>;

pub trait OpenAIToolConfig {
    async fn description(&self, config: &ConfigManager) -> String;
    fn tool_kind(&self) -> String;
    fn handle(&self) -> String;
    fn artifact(&self) -> Option<(String, ArtifactKind)>;
    async fn params_schema(&self, config: &ConfigManager) -> Result<serde_json::Value, OxyError>;
}

impl OpenAIToolConfig for &ToolType {
    async fn description(&self, config: &ConfigManager) -> String {
        match self {
            ToolType::ExecuteSQL(e) => e.description.clone(),
            ToolType::ValidateSQL(v) => v.description.clone(),
            ToolType::Retrieval(r) => r.description.clone(),
            ToolType::Workflow(w) => w.description.clone(),
            ToolType::Agent(agent_tool) => agent_tool.description.clone(),
            ToolType::Visualize(v) => v.description.clone(),
            ToolType::CreateDataApp(v) => v.description.clone(),
            ToolType::OmniQuery(o) => match get_omni_query_description(o, config).await {
                Ok(desc) => desc,
                Err(_) => o.description.clone(),
            },
            ToolType::SemanticQuery(s) => {
                // Try to get enhanced description with semantic layer metadata
                match get_semantic_query_description(s, config) {
                    Ok(desc) => desc,
                    Err(_) => {
                        format!(
                            "{}\n\nNo semantic layer metadata found. Please ensure you have semantic layer definitions in the 'semantics' directory.",
                            s.description
                        )
                    }
                }
            }
        }
    }

    fn handle(&self) -> String {
        match self {
            ToolType::ExecuteSQL(e) => e.name.clone(),
            ToolType::ValidateSQL(v) => v.name.clone(),
            ToolType::Retrieval(r) => r.name.clone(),
            ToolType::Workflow(w) => w.name.clone(),
            ToolType::Agent(agent_tool) => agent_tool.name.clone(),
            ToolType::Visualize(v) => v.name.clone(),
            ToolType::CreateDataApp(create_data_app_tool) => create_data_app_tool.name.clone(),
            ToolType::OmniQuery(o) => o.name.clone(),
            ToolType::SemanticQuery(s) => s.name.clone(),
        }
    }

    fn artifact(&self) -> Option<(String, ArtifactKind)> {
        match self {
            ToolType::ExecuteSQL(sql) => Some((
                self.handle(),
                ArtifactKind::ExecuteSQL {
                    sql: sql.sql.clone().unwrap_or_default(),
                    database: sql.database.to_string(),
                },
            )),
            ToolType::Workflow(wf) => Some((
                self.handle(),
                ArtifactKind::Workflow {
                    r#ref: wf.workflow_ref.clone(),
                },
            )),
            ToolType::Agent(ag) => Some((
                self.handle(),
                ArtifactKind::Agent {
                    r#ref: ag.agent_ref.clone(),
                },
            )),
            ToolType::SemanticQuery(_sm) => Some((self.handle(), ArtifactKind::SemanticQuery {})),
            ToolType::OmniQuery(om) => Some((
                self.handle(),
                ArtifactKind::OmniQuery {
                    topic: om.topic.clone(),
                    integration: om.integration.clone(),
                },
            )),
            _ => None,
        }
    }

    fn tool_kind(&self) -> String {
        match self {
            ToolType::ExecuteSQL(_) => "execute_sql".to_string(),
            ToolType::ValidateSQL(_) => "validate_sql".to_string(),
            ToolType::Retrieval(_) => "retrieval".to_string(),
            ToolType::Workflow(_) => "workflow".to_string(),
            ToolType::Agent(_) => "agent".to_string(),
            ToolType::Visualize(_) => "visualize".to_string(),
            ToolType::CreateDataApp(_) => "create_data_app".to_string(),
            ToolType::OmniQuery(_) => "omni_query".to_string(),
            ToolType::SemanticQuery(_) => "semantic_query".to_string(),
        }
    }

    async fn params_schema(&self, config: &ConfigManager) -> Result<serde_json::Value, OxyError> {
        match self {
            ToolType::ExecuteSQL(sql_tool) => match sql_tool.sql {
                None => Ok(serde_json::json!(&schemars::schema_for!(SQLParams))),
                Some(_) => Ok(serde_json::json!(&schemars::schema_for!(EmptySQLParams))),
            },
            ToolType::ValidateSQL(_) => Ok(serde_json::json!(&schemars::schema_for!(SQLParams))),
            ToolType::Retrieval(_) => {
                Ok(serde_json::json!(&schemars::schema_for!(RetrievalParams)))
            }
            ToolType::Workflow(w) => {
                generate_workflow_run_schema(&w.workflow_ref.clone(), config).await
            }
            ToolType::Agent(_) => Ok(serde_json::json!(&schemars::schema_for!(AgentParams))),
            ToolType::Visualize(_) => Ok(serde_json::from_str(viz_schema::VIZ_SCHEMA).unwrap()),
            ToolType::CreateDataApp(_) => {
                // we need to manually create the schema for CreateDataAppParams
                // because this schema is quite complex and the library we use
                // schemars does not generate a compatiible schema with OpenAI.
                Ok(serde_json::from_str(create_app_schema::CREATE_APP_SCHEMA).unwrap())
            }
            ToolType::OmniQuery(_) => {
                Ok(serde_json::json!(&schemars::schema_for!(OmniQueryParams)))
            }
            ToolType::SemanticQuery(_) => Ok(serde_json::json!(&schemars::schema_for!(
                SemanticQueryParams
            ))),
        }
    }
}

impl From<ToolType> for ChatCompletionNamedToolChoice {
    fn from(val: ToolType) -> Self {
        ChatCompletionNamedToolChoice {
            function: FunctionName {
                name: (&val).handle(),
            },
        }
    }
}

impl From<ReasoningEffort> for OpenAIReasoningEffort {
    fn from(effort: ReasoningEffort) -> Self {
        match effort {
            ReasoningEffort::Low => OpenAIReasoningEffort::Low,
            ReasoningEffort::Medium => OpenAIReasoningEffort::Medium,
            ReasoningEffort::High => OpenAIReasoningEffort::High,
        }
    }
}

impl From<ReasoningConfig> for Reasoning {
    fn from(reasoning_config: ReasoningConfig) -> Self {
        Reasoning {
            effort: Some(reasoning_config.effort.into()),
            ..Default::default()
        }
    }
}

async fn generate_workflow_run_schema(
    workflow_path: &str,
    config: &ConfigManager,
) -> Result<serde_json::Value, OxyError> {
    let workflow_config = config.resolve_workflow(workflow_path).await?;
    let schema = Into::<RootSchema>::into(&workflow_config.variables.unwrap_or_default());
    let json_schema = serde_json::json!(schema);
    Ok(json_schema)
}

async fn get_omni_query_description(
    omni_tool: &crate::config::model::OmniQueryTool,
    config: &ConfigManager,
) -> Result<String, OxyError> {
    let topic_name = omni_tool.topic.clone();

    // Find the model_id for the specific topic in the correct integration
    let model_id = config
        .get_config()
        .integrations
        .iter()
        .find_map(|integration| {
            // First check if this is the correct integration by name
            if integration.name == omni_tool.integration {
                match &integration.integration_type {
                    IntegrationType::Omni(int) => int
                        .topics
                        .iter()
                        .find(|t| t.name == topic_name)
                        .map(|t| t.model_id.as_str()),
                }
            } else {
                None
            }
        })
        .ok_or_else(|| {
            OxyError::ConfigurationError(format!(
                "Topic '{}' not found in integration '{}' or integration not found",
                topic_name, omni_tool.integration
            ))
        })?;

    get_omni_description(&topic_name, &omni_tool.integration, model_id, config)
}

fn get_omni_description(
    topic: &str,
    integration: &str,
    model_id: &str,
    config: &ConfigManager,
) -> Result<String, OxyError> {
    let storage = MetadataStorage::new(config.project_path(), integration.to_string());

    // Get all available topics for the model
    let topics = storage
        .list_base_topics(model_id)
        .map_err(|e| OxyError::ToolCallError {
            call_id: "unknown".to_string(),
            handle: "omni_query".to_string(),
            param: "".to_string(),
            msg: format!("Failed to list topics: {}", e),
        })?;

    if topics.is_empty() {
        return Ok(format!(
            "No metadata available for model '{}'. Please run 'oxy build' to synchronize metadata from the Omni API.",
            model_id
        ));
    }

    let mut description = String::new();
    description.push_str("Query data from Omni semantic layer\n\n");

    // Topic is always required, load its metadata
    let topic_metadata = storage
        .load_merged_metadata(model_id, topic)
        .map_err(|e| OxyError::ToolCallError {
            call_id: "unknown".to_string(),
            handle: "omni_query".to_string(),
            param: "topic".to_string(),
            msg: format!("Failed to load metadata for topic '{}': {}", topic, e),
        })?
        .ok_or_else(|| OxyError::ToolCallError {
            call_id: "unknown".to_string(),
            handle: "omni_query".to_string(),
            param: "topic".to_string(),
            msg: format!(
                "No metadata found for topic '{}'. Please run 'oxy build' to synchronize metadata.",
                topic
            ),
        })?;

    description.push_str(&format!("**Topic: {}**\n", topic));
    description.push_str(&format_topic_description(&topic_metadata));

    description.push_str("\n**Usage Notes:**\n");
    description.push_str("- Field names must use full format: {view}.{field_name}\n");
    // Filters are not supported by the Omni API; do not suggest them
    description.push_str("- Set appropriate limits for large datasets\n");
    description.push_str("- Sort by relevant fields for better results\n");

    Ok(description)
}

fn format_topic_description(topic: &omni::TopicMetadata) -> String {
    let mut desc = String::new();

    // Add custom description if available
    if let Some(custom_desc) = &topic.custom_description {
        desc.push_str(&format!("Additional Info: {}\n", custom_desc));
    }

    // Add agent hints if available
    if let Some(hints) = &topic.agent_hints {
        desc.push_str("Agent Hints:\n");
        for hint in hints {
            desc.push_str(&format!("- {}\n", hint));
        }
    }

    // Add views and their fields
    desc.push_str("\n**Views and Fields:**\n");
    for view in &topic.views {
        desc.push_str(&format!("\n*View: {}*\n", view.name));

        // Add dimensions
        if !view.dimensions.is_empty() {
            desc.push_str("Dimensions:\n");
            for dim in &view.dimensions {
                desc.push_str(&format!(
                    "- {}.{} ({})",
                    dim.view_name, dim.field_name, dim.data_type
                ));
                if let Some(label) = &dim.label {
                    desc.push_str(&format!(" - {}", label));
                }
                if let Some(dim_desc) = &dim.description {
                    desc.push_str(&format!(" - {}", dim_desc));
                }
                if let Some(ai_context) = &dim.ai_context {
                    desc.push_str(&format!(" [AI Context: {}]", ai_context));
                }
                desc.push('\n');
            }
        }

        // Add measures
        if !view.measures.is_empty() {
            desc.push_str("Measures:\n");
            for measure in &view.measures {
                desc.push_str(&format!(
                    "- {}.{} ({})",
                    measure.view_name, measure.field_name, measure.data_type
                ));
                if let Some(label) = &measure.label {
                    desc.push_str(&format!(" - {}", label));
                }
                if let Some(measure_desc) = &measure.description {
                    desc.push_str(&format!(" - {}", measure_desc));
                }
                if let Some(ai_context) = &measure.ai_context {
                    desc.push_str(&format!(" [AI Context: {}]", ai_context));
                }
                desc.push('\n');
            }
        }

        // Add filter-only fields
        if !view.filter_only_fields.is_empty() {
            desc.push_str(&format!(
                "Filter-only fields: {}\n",
                view.filter_only_fields.join(", ")
            ));
        }
    }

    // Add examples if available
    if let Some(examples) = &topic.examples {
        desc.push_str("\n**Query Examples:**\n");
        for example in examples {
            desc.push_str(&format!("- {}: {}\n", example.description, example.query));
            if let Some(expected) = &example.expected_result {
                desc.push_str(&format!("  Expected: {}\n", expected));
            }
        }
    }

    desc
}

pub trait AsyncFunctionObject {
    async fn from_tool_async(tool: &ToolType, config: &ConfigManager) -> Self;
}

impl AsyncFunctionObject for FunctionObject {
    async fn from_tool_async(tool: &ToolType, config: &ConfigManager) -> Self {
        let mut binding = FunctionObjectArgs::default();
        let mut function_args = binding
            .name(tool.handle())
            .description(tool.description(config).await);
        let params_schema = tool.params_schema(config).await.unwrap();
        if !params_schema.is_null()
            && params_schema.is_object()
            && params_schema
                .as_object()
                .unwrap()
                .get("properties")
                .is_some()
        {
            function_args = function_args.parameters(params_schema);
        }

        function_args.build().unwrap()
    }
}

impl AsyncFunctionObject for ChatCompletionTool {
    async fn from_tool_async(tool: &ToolType, config: &ConfigManager) -> Self {
        let function_obj = FunctionObject::from_tool_async(tool, config).await;
        ChatCompletionTool {
            function: function_obj,
        }
    }
}

pub enum StreamChunk {
    Text(String),
    ToolCall {
        id: String,
        name: String,
        args: String,
    },
}

#[derive(Clone)]
pub struct OpenAIAdapter {
    client: OpenAIClient,
    model_name: String,
}

impl OpenAIAdapter {
    pub async fn from_config(project: ProjectManager, model_ref: &str) -> Result<Self, OxyError> {
        let model = project.config_manager.resolve_model(model_ref)?;
        let config_type = model.into_openai_config(&project.secrets_manager).await?;
        let client = Client::with_config(config_type);
        Ok(Self {
            client,
            model_name: model.model_name().to_string(),
        })
    }

    pub fn new(client: OpenAIClient, model_name: String) -> Self {
        Self { client, model_name }
    }

    pub async fn generate_text<M: Into<Vec<ChatCompletionRequestMessage>>>(
        &self,
        messages: M,
    ) -> Result<String, OxyError> {
        let request = self
            .request_builder(messages)
            .build()
            .map_err(|e| OxyError::RuntimeError(format!("Failed to build request: {e}")))?;
        let response = self.client.chat().create(request).await?;
        let result = self
            .extract_response(&response)
            .ok_or_else(|| OxyError::RuntimeError("No response from OpenAI".to_string()))?;
        Ok(result)
    }

    pub async fn request_tool_call_with_usage<
        M: Into<Vec<ChatCompletionRequestMessage>>,
        C: Into<Vec<ChatCompletionTool>>,
    >(
        &self,
        execution_context: &crate::execute::ExecutionContext,
        messages: M,
        tools: C,
        tool_choice: Option<ChatCompletionToolChoiceOption>,
        parallel_tool_calls: Option<bool>,
    ) -> Result<Vec<ChatCompletionMessageToolCall>, OxyError> {
        let mut request_builder = self.request_builder(messages);
        let tools_vec: Vec<ChatCompletionTool> = tools.into();
        let tools_wrapped: Vec<ChatCompletionTools> = tools_vec
            .into_iter()
            .map(|t| ChatCompletionTools::Function(t))
            .collect();
        request_builder.tools(tools_wrapped);

        if let Some(tool_choice) = tool_choice {
            request_builder.tool_choice(tool_choice);
        }

        if let Some(parallel_tool_calls) = parallel_tool_calls {
            request_builder.parallel_tool_calls(parallel_tool_calls);
        }

        let request = request_builder
            .build()
            .map_err(|e| OxyError::RuntimeError(format!("Failed to build request: {e}")))?;
        let response = self
            .client
            .chat()
            .create(request)
            .await
            .map_err(|e| OxyError::RuntimeError(format!("OpenAI API error: {e}")))?;

        // Write usage data if available
        if let Some(usage_data) = &response.usage {
            execution_context
                .write_usage(crate::execute::types::Usage::new(
                    usage_data.prompt_tokens as i32,
                    usage_data.completion_tokens as i32,
                ))
                .await?;
        }

        let result = self.extract_tool_calls(&response);
        Ok(result)
    }

    pub async fn stream_text<M: Into<Vec<ChatCompletionRequestMessage>>>(
        &self,
        messages: M,
    ) -> Result<impl tokio_stream::Stream<Item = Result<Option<String>, OxyError>>, OxyError> {
        let request = self
            .request_builder(messages)
            .stream(true)
            .build()
            .map_err(|e| OxyError::RuntimeError(format!("Failed to build request: {e}")))?;

        let stream = self
            .client
            .chat()
            .create_stream(request)
            .await
            .map_err(|e| OxyError::RuntimeError(format!("OpenAI API error: {e}")))?
            .map(|result| match result {
                Ok(response) => {
                    let stream_response = self.extract_stream(&response);
                    Ok(stream_response)
                }
                Err(e) => Err(OxyError::RuntimeError(format!("OpenAI API error: {e}"))),
            });
        Ok(stream)
    }

    pub async fn stream_with_tool_calls<
        M: Into<Vec<ChatCompletionRequestMessage>>,
        C: Into<Vec<ChatCompletionTool>>,
    >(
        &self,
        messages: M,
        tools: C,
        tool_choice: Option<ChatCompletionToolChoiceOption>,
    ) -> Result<impl tokio_stream::Stream<Item = Result<StreamChunk, OxyError>>, OxyError> {
        let mut request_builder = self.request_builder(messages);
        let tools_vec: Vec<ChatCompletionTool> = tools.into();
        let tools_wrapped: Vec<ChatCompletionTools> = tools_vec
            .into_iter()
            .map(|t| ChatCompletionTools::Function(t))
            .collect();

        request_builder.tools(tools_wrapped);
        if let Some(tool_choice) = tool_choice {
            request_builder.tool_choice(tool_choice);
        }
        let request = request_builder
            .stream(true)
            .build()
            .map_err(|e| OxyError::RuntimeError(format!("Failed to build request: {e}")))?;
        let stream = self
            .client
            .chat()
            .create_stream(request)
            .await
            .map_err(|e| OxyError::RuntimeError(format!("OpenAI API error: {e}")))?;

        let mut tool_calls_buffer = HashMap::<u32, StreamChunk>::new();
        // Aggregate tool call chunks by their index to form complete tool calls
        let stream = stream.filter_map(move |result| match result {
            Ok(response) => {
                // Check if this chunk contains tool call data
                let tool_call_chunks = self.extract_stream_tool_calls(&response);
                if !tool_call_chunks.is_empty() {
                    for tool_call_chunk in tool_call_chunks {
                        if let Some(call_id) = &tool_call_chunk.id
                            && let Some(tool_name) = tool_call_chunk
                                .function
                                .as_ref()
                                .map(|f| f.name.as_ref())
                                .flatten()
                        {
                            tool_calls_buffer.entry(tool_call_chunk.index).or_insert(
                                StreamChunk::ToolCall {
                                    id: call_id.to_string(),
                                    name: tool_name.to_string(),
                                    args: String::new(),
                                },
                            );
                        } else if let Some(entry) =
                            tool_calls_buffer.get_mut(&tool_call_chunk.index)
                            && let StreamChunk::ToolCall { args, .. } = entry
                            && let Some(arg_chunk) = tool_call_chunk
                                .function
                                .as_ref()
                                .map(|f| f.arguments.as_ref())
                                .flatten()
                        {
                            args.push_str(arg_chunk);
                        }
                    }
                    // Return None since we are still accumulating tool call chunks
                    None
                } else {
                    // Emit any completed tool calls from the buffer first
                    for (_index, tool_call) in tool_calls_buffer.drain() {
                        if let StreamChunk::ToolCall { id, name, args } = tool_call {
                            return Some(Ok(StreamChunk::ToolCall {
                                id: id.clone(),
                                name: name.clone(),
                                args: args.clone(),
                            }));
                        }
                    }
                    // Regular text content
                    let stream_response = self.extract_stream(&response);
                    if let Some(text) = stream_response {
                        Some(Ok(StreamChunk::Text(text)))
                    } else {
                        None
                    }
                }
            }
            Err(_) => Some(Err(OxyError::RuntimeError("OpenAI API error".to_string()))),
        });
        Ok(stream)
    }

    fn request_builder<M: Into<Vec<ChatCompletionRequestMessage>>>(
        &self,
        messages: M,
    ) -> CreateChatCompletionRequestArgs {
        let mut builder = CreateChatCompletionRequestArgs::default();
        builder.model(&self.model_name).messages(messages);
        builder
    }

    fn extract_response(&self, response: &CreateChatCompletionResponse) -> Option<String> {
        response.choices.first().and_then(|choice| {
            if let Some(content) = &choice.message.content {
                return Some(content.clone());
            }
            None
        })
    }

    fn extract_tool_calls(
        &self,
        response: &CreateChatCompletionResponse,
    ) -> Vec<ChatCompletionMessageToolCall> {
        response
            .choices
            .first()
            .and_then(|choice| choice.message.tool_calls.clone())
            .map(|tool_calls| {
                tool_calls
                    .into_iter()
                    .filter_map(|tc| match tc {
                        ChatCompletionMessageToolCalls::Function(func_call) => Some(func_call),
                        ChatCompletionMessageToolCalls::Custom(_) => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn extract_stream(
        &self,
        stream_response: &CreateChatCompletionStreamResponse,
    ) -> Option<String> {
        stream_response.choices.first().and_then(|choice| {
            if let Some(content) = &choice.delta.content {
                return Some(content.clone());
            }
            None
        })
    }

    fn extract_stream_tool_calls(
        &self,
        stream_response: &CreateChatCompletionStreamResponse,
    ) -> Vec<ChatCompletionMessageToolCallChunk> {
        stream_response
            .choices
            .first()
            .and_then(|choice| choice.delta.tool_calls.clone())
            .unwrap_or_default()
    }
}
