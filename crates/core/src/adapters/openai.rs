use crate::observability::events;
use async_openai::{
    Client,
    types::{
        chat::{
            ChatCompletionMessageToolCall, ChatCompletionMessageToolCallChunk,
            ChatCompletionRequestMessage, ChatCompletionTool, ChatCompletionToolChoiceOption,
            ChatCompletionTools, CreateChatCompletionRequestArgs,
            CreateChatCompletionStreamResponse, ReasoningEffort as OpenAIReasoningEffort,
        },
        responses::Reasoning,
    },
};
// Re-export types from oxy-openai for use elsewhere in core
pub use oxy_openai::{ConfigType, CustomOpenAIConfig, OpenAIClient, StreamChunk};
use std::collections::HashMap;
use tokio_stream::StreamExt;

use crate::config::WorkingCopy;
use crate::{
    adapters::{secrets::SecretsManager, workspace::manager::WorkspaceManager},
    config::model::{HeaderValue, Model, ReasoningConfig, ReasoningEffort},
};
use oxy_shared::errors::OxyError;

pub trait IntoOpenAIConfig {
    fn into_openai_config(
        &self,
        secrets_manager: &SecretsManager,
    ) -> impl std::future::Future<Output = Result<ConfigType, OxyError>> + std::marker::Send;
}

/// Extension trait for resolving secrets in HeaderValue
pub trait HeaderValueExt {
    fn resolve(
        &self,
        secrets_manager: &SecretsManager,
    ) -> impl std::future::Future<Output = Result<String, OxyError>> + std::marker::Send;
}

impl HeaderValueExt for HeaderValue {
    async fn resolve(&self, secrets_manager: &SecretsManager) -> Result<String, OxyError> {
        match self {
            HeaderValue::Direct(value) => Ok(value.clone()),
            HeaderValue::EnvVar { env_var } => {
                let result = secrets_manager.resolve_secret(env_var).await?;
                match result {
                    Some(res) => Ok(res),
                    None => Err(OxyError::SecretNotFound(Some(env_var.clone()))),
                }
            }
        }
    }
}

pub trait ModelHeadersExt {
    fn resolve_headers(
        &self,
        secrets_manager: &SecretsManager,
    ) -> impl std::future::Future<Output = Result<HashMap<String, String>, OxyError>> + std::marker::Send;
}

impl ModelHeadersExt for Model {
    async fn resolve_headers(
        &self,
        secrets_manager: &SecretsManager,
    ) -> Result<HashMap<String, String>, OxyError> {
        let headers_map = match self {
            Model::OpenAI { config } => config.headers.as_ref(),
            Model::Anthropic { config } => config.headers.as_ref(),
            Model::OpenAICompat { config } => config.headers.as_ref(),
            _ => None,
        };
        let mut resolved_headers = HashMap::new();
        if let Some(headers_map) = headers_map {
            for (key, header_value) in headers_map {
                let resolved_value = header_value.resolve(secrets_manager).await?;
                resolved_headers.insert(key.clone(), resolved_value);
            }
        }
        Ok(resolved_headers)
    }
}

impl IntoOpenAIConfig for Model {
    async fn into_openai_config(
        &self,
        secrets_manager: &SecretsManager,
    ) -> Result<ConfigType, OxyError> {
        match self {
            // Both variants build the same client here: this adapter drives
            // `async_openai`'s Chat Completions surface, which an OpenAI-compat
            // gateway serves natively. They diverge only in the agentic
            // pipeline, which picks Responses vs Chat Completions by vendor.
            Model::OpenAI { config } | Model::OpenAICompat { config } => {
                let api_key = secrets_manager
                    .resolve_secret(&config.key_var)
                    .await
                    .map_err(|_| OxyError::ConfigurationError("OpenAI key not found".to_string()))?
                    .ok_or_else(|| {
                        OxyError::ConfigurationError("OpenAI key not found".to_string())
                    })?;

                // Resolve custom headers if present
                let resolved_headers = if config.headers.is_some() {
                    Some(self.resolve_headers(secrets_manager).await?)
                } else {
                    None
                };

                // Delegate to oxy-openai for config creation
                let openai_config = oxy_openai::create_config_from_model(
                    api_key,
                    config.api_url.clone(),
                    config.azure.clone(),
                    resolved_headers,
                );
                Ok(openai_config)
            }
            Model::Ollama { config } => {
                // Delegate to oxy-ollama for config creation
                Ok(oxy_ollama::create_openai_config(
                    &config.api_key,
                    &config.api_url,
                ))
            }
            Model::Google { config } => {
                let api_key = secrets_manager
                    .resolve_secret(&config.key_var)
                    .await
                    .map_err(|_e| {
                        OxyError::ConfigurationError("Gemini API key not found".to_string())
                    })?
                    .ok_or_else(|| {
                        OxyError::ConfigurationError("Gemini API key not found".to_string())
                    })?;

                // Delegate to oxy-gemini for config creation
                Ok(oxy_gemini::create_openai_config(api_key))
            }
            Model::Anthropic { config } => {
                let api_key = secrets_manager
                    .resolve_secret(&config.key_var)
                    .await
                    .map_err(|_e| {
                        OxyError::ConfigurationError("Anthropic API key not found".to_string())
                    })?
                    .ok_or_else(|| {
                        OxyError::ConfigurationError("Anthropic API key not found".to_string())
                    })?;

                // Resolve custom headers if present
                let resolved_headers = if config.headers.is_some() {
                    Some(self.resolve_headers(secrets_manager).await?)
                } else {
                    None
                };

                // Delegate to oxy-anthropic for config creation
                Ok(oxy_anthropic::create_openai_config(
                    api_key,
                    config.api_url.clone(),
                    resolved_headers,
                ))
            }
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

#[derive(Clone)]
pub struct OpenAIAdapter {
    client: OpenAIClient,
    model_name: String,
}

impl OpenAIAdapter {
    pub async fn from_config(
        workspace: WorkspaceManager<WorkingCopy>,
        model_ref: &str,
    ) -> Result<Self, OxyError> {
        let model = workspace.config_manager.resolve_model(model_ref)?;
        let config_type = model.into_openai_config(&workspace.secrets_manager).await?;
        let client = Client::with_config(config_type);
        Ok(Self {
            client,
            model_name: model.model_name().to_string(),
        })
    }

    pub fn new(client: OpenAIClient, model_name: String) -> Self {
        Self { client, model_name }
    }

    #[tracing::instrument(
        skip_all,
        fields(
            oxy.name = events::llm::LLM_OPENAI_CALL,
            oxy.span_type = events::llm::LLM_CALL_TYPE,
            gen_ai.request.model = %self.model_name,
            gen_ai.system = events::llm::OPENAI
        )
    )]
    pub async fn generate_text<M: Into<Vec<ChatCompletionRequestMessage>>>(
        &self,
        messages: M,
    ) -> Result<String, OxyError> {
        let request = self
            .request_builder(messages)
            .build()
            .map_err(|e| OxyError::RuntimeError(format!("Failed to build request: {e}")))?;

        // Use lenient types for better compatibility with OpenAI-compatible APIs (Groq, Mistral, etc.)
        let response: super::lenient_types::LenientChatCompletionResponse =
            self.client.chat().create_byot(request).await?;

        if let Some(usage) = &response.usage {
            events::llm::usage(usage.prompt_tokens as i64, usage.completion_tokens as i64);
        }

        let result = response
            .extract_content()
            .ok_or_else(|| OxyError::RuntimeError("No response from OpenAI".to_string()))?;
        Ok(result)
    }

    #[tracing::instrument(
        skip_all,
        fields(
            oxy.name = events::llm::LLM_TOOL_CALL,
            oxy.span_type = events::llm::LLM_CALL_TYPE,
            gen_ai.request.model = %self.model_name,
            gen_ai.system = events::llm::OPENAI
        )
    )]
    pub async fn request_tool_call_with_usage<
        M: Into<Vec<ChatCompletionRequestMessage>>,
        C: Into<Vec<ChatCompletionTool>>,
    >(
        &self,
        execution_context: &crate::exec_runtime::ExecutionContext,
        messages: M,
        tools: C,
        tool_choice: Option<ChatCompletionToolChoiceOption>,
        parallel_tool_calls: Option<bool>,
    ) -> Result<(Option<String>, Vec<ChatCompletionMessageToolCall>), OxyError> {
        let mut request_builder = self.request_builder(messages);
        let tools_vec: Vec<ChatCompletionTool> = tools.into();
        let tools_wrapped: Vec<ChatCompletionTools> = tools_vec
            .into_iter()
            .map(ChatCompletionTools::Function)
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

        // Use lenient types for better compatibility with OpenAI-compatible APIs (Groq, Mistral, etc.)
        let response: super::lenient_types::LenientChatCompletionResponse = self
            .client
            .chat()
            .create_byot(request)
            .await
            .map_err(|e| OxyError::RuntimeError(format!("OpenAI API error: {e}")))?;

        if let Some(usage_data) = &response.usage {
            events::llm::usage(
                usage_data.prompt_tokens as i64,
                usage_data.completion_tokens as i64,
            );
            execution_context
                .write_usage(crate::exec_types::Usage::new(
                    usage_data.prompt_tokens as i32,
                    usage_data.completion_tokens as i32,
                ))
                .await?;
        }

        let result = response.extract_tool_calls();
        Ok(result.unwrap_or((None, vec![])))
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
        tracing::debug!("Starting LLM stream with tool calls");
        let mut request_builder = self.request_builder(messages);
        let tools_vec: Vec<ChatCompletionTool> = tools.into();
        let tools_wrapped: Vec<ChatCompletionTools> = tools_vec
            .into_iter()
            .map(ChatCompletionTools::Function)
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
                                .and_then(|f| f.name.as_ref())
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
                                .and_then(|f| f.arguments.as_ref())
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
                    stream_response.map(|text| Ok(StreamChunk::Text(text)))
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
