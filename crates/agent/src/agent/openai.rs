use std::{collections::HashMap, sync::Arc};

use async_openai::{
    error::OpenAIError,
    types::chat::{
        ChatCompletionMessageToolCall, ChatCompletionMessageToolCallChunk,
        ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage,
        ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
        ChatCompletionStreamOptions, ChatCompletionTool, ChatCompletionToolChoiceOption,
        ChatCompletionTools, CreateChatCompletionRequestArgs, FunctionCall,
    },
};
use deser_incomplete::from_json_str;
use futures::StreamExt;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::openai_response::OpenAIResponseExecutable;
use crate::types::Message;

use oxy::{
    adapters::{lenient_types::LenientChatCompletionResponse, openai::OpenAIClient},
    config::{
        constants::{AGENT_RETRY_MAX_ELAPSED_TIME, AGENT_SOURCE_CONTENT},
        model::ReasoningConfig,
    },
    execute::{
        Executable, ExecutionContext,
        types::{Chunk, EventKind, Output, Usage},
    },
    observability::events,
    theme::StyledText,
    utils::variant_eq,
};
use oxy_shared::errors::OxyError;

#[derive(JsonSchema, Deserialize, Debug, Clone)]
#[serde(untagged, rename_all = "camelCase", deny_unknown_fields)]
pub enum AgentResponseData {
    #[schemars(
        description = "Use when returning the result of an SQL query for the specified file_path. Do not use if file_path is not provided. Don't use for data app."
    )]
    Table { file_path: String },
    #[schemars(description = "Default response type")]
    Text { text: String },
    #[schemars(description = "Use when the user explicitly asks for generating SQL")]
    SQL { sql: String },
}

#[derive(JsonSchema, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct AgentResponse {
    pub data: AgentResponseData,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenAIExecutableResponse {
    pub content: Output,
    pub tool_calls: Vec<ChatCompletionMessageToolCall>,
}

impl From<AgentResponseData> for Output {
    fn from(val: AgentResponseData) -> Self {
        match val {
            AgentResponseData::Table { file_path } => Output::table(file_path),
            AgentResponseData::Text { text } => Output::Text(text),
            AgentResponseData::SQL { sql } => Output::sql(sql),
        }
    }
}

impl From<AgentResponse> for Output {
    fn from(val: AgentResponse) -> Self {
        val.data.into()
    }
}

impl Default for AgentResponse {
    fn default() -> Self {
        AgentResponse {
            data: AgentResponseData::Text {
                text: "".to_string(),
            },
        }
    }
}

impl Default for OpenAIExecutableResponse {
    fn default() -> Self {
        OpenAIExecutableResponse {
            content: Output::Text("".to_string()),
            tool_calls: vec![],
        }
    }
}

fn is_oss_model(model: &str) -> bool {
    model.contains("gpt-oss")
        || model.contains("llama")
        || model.contains("ollama")
        || (!model.starts_with("gpt-")
            && !model.starts_with("claude")
            && !model.starts_with("gemini"))
}

#[derive(Clone, Debug, Serialize)]
pub enum OpenAIOrOSSExecutable {
    OpenAI(OpenAIExecutable),
    OSS(OSSExecutable),
    OpenAIResponse(OpenAIResponseExecutable),
}

#[async_trait::async_trait]
impl Executable<Vec<ChatCompletionRequestMessage>> for OpenAIOrOSSExecutable {
    type Response = OpenAIExecutableResponse;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: Vec<ChatCompletionRequestMessage>,
    ) -> Result<Self::Response, OxyError> {
        match self {
            OpenAIOrOSSExecutable::OpenAI(exec) => exec.execute(execution_context, input).await,
            OpenAIOrOSSExecutable::OSS(exec) => exec.execute(execution_context, input).await,
            OpenAIOrOSSExecutable::OpenAIResponse(exec) => {
                exec.execute(execution_context, input).await
            }
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct OpenAIExecutable {
    #[serde(skip)]
    client: Arc<OpenAIClient>,
    model: String,
    tool_configs: Vec<ChatCompletionTool>,
    tool_choice: Option<ChatCompletionToolChoiceOption>,
    reasoning_config: Option<ReasoningConfig>,
    synthesize_mode: bool,
}

impl OpenAIExecutable {
    pub fn new(
        client: OpenAIClient,
        model: String,
        tool_configs: Vec<ChatCompletionTool>,
        tool_choice: Option<ChatCompletionToolChoiceOption>,
        reasoning_config: Option<ReasoningConfig>,
        synthesize_mode: bool,
    ) -> Self {
        print!("Building OpenAI executable for model: {}", model);
        Self {
            client: Arc::new(client),
            model,
            tool_configs,
            tool_choice,
            reasoning_config,
            synthesize_mode,
        }
    }

    fn clear_tools(&mut self) {
        self.tool_choice = None;
        self.tool_configs.clear();
    }

    fn parse_tool_call_chunks(
        &self,
        tool_calls: &mut HashMap<(u32, u32), ChatCompletionMessageToolCall>,
        chunk_index: u32,
        tool_call_chunks: &Vec<ChatCompletionMessageToolCallChunk>,
    ) {
        for tool_call_chunk in tool_call_chunks.iter() {
            let key = (chunk_index, tool_call_chunk.index);
            let state = tool_calls
                .entry(key)
                .or_insert_with(|| ChatCompletionMessageToolCall {
                    id: tool_call_chunk.id.clone().unwrap_or_default(),
                    function: FunctionCall {
                        name: tool_call_chunk
                            .function
                            .as_ref()
                            .and_then(|f| f.name.clone())
                            .unwrap_or_default(),
                        arguments: tool_call_chunk
                            .function
                            .as_ref()
                            .and_then(|f| f.arguments.clone())
                            .unwrap_or_default(),
                    },
                });
            if let Some(arguments) = tool_call_chunk
                .function
                .as_ref()
                .and_then(|f| f.arguments.as_ref())
            {
                state.function.arguments.push_str(arguments);
            }
        }
    }

    async fn process_content_chunk(
        &self,
        execution_context: &ExecutionContext,
        content: &mut String,
        tool_calls: &HashMap<(u32, u32), ChatCompletionMessageToolCall>,
        last_parsed_length: &mut usize,
        has_written: &mut bool,
        message: &str,
    ) -> Result<(), OxyError> {
        content.push_str(message);

        // Try structured parsing first, fallback to plain text
        if tool_calls.is_empty()
            && let Ok(data) = from_json_str::<AgentResponse>(content)
        {
            self.handle_structured_response(
                execution_context,
                &data,
                content,
                last_parsed_length,
                has_written,
                message,
            )
            .await
        } else if !content.is_empty() {
            self.handle_plain_text_response(
                execution_context,
                content,
                last_parsed_length,
                has_written,
            )
            .await
        } else {
            Ok(())
        }
    }

    async fn handle_structured_response(
        &self,
        execution_context: &ExecutionContext,
        data: &AgentResponse,
        _content: &str,
        last_parsed_length: &mut usize,
        has_written: &mut bool,
        message: &str,
    ) -> Result<(), OxyError> {
        let (parsed_content, mut output) = match &data.data {
            AgentResponseData::Table { file_path } => {
                (file_path.clone(), Output::table(message.to_string()))
            }
            AgentResponseData::Text { text } => (text.clone(), Output::Text(message.to_string())),
            AgentResponseData::SQL { sql } => (sql.clone(), Output::sql(message.to_string())),
        };

        if *last_parsed_length != parsed_content.len()
            && variant_eq(&Output::Text("".to_string()), &output)
        {
            if !*has_written {
                execution_context
                    .write_kind(EventKind::Message {
                        message: "\nOutput:".primary().to_string(),
                    })
                    .await?;
            }

            *has_written = true;
            let chunk = if parsed_content.len() > *last_parsed_length {
                &parsed_content[*last_parsed_length..]
            } else {
                ""
            };
            output.replace(chunk.to_string());
            *last_parsed_length = parsed_content.len();
            execution_context
                .write_chunk(Chunk {
                    key: Some(AGENT_SOURCE_CONTENT.to_string()),
                    delta: output,
                    finished: false,
                })
                .await?;
        }
        Ok(())
    }

    async fn handle_plain_text_response(
        &self,
        execution_context: &ExecutionContext,
        content: &str,
        last_parsed_length: &mut usize,
        has_written: &mut bool,
    ) -> Result<(), OxyError> {
        if !*has_written {
            execution_context
                .write_kind(EventKind::Message {
                    message: "\nOutput:".primary().to_string(),
                })
                .await?;
            *has_written = true;
        }

        if content.len() > *last_parsed_length {
            let new_chunk = &content[*last_parsed_length..];
            execution_context
                .write_chunk(Chunk {
                    key: Some(AGENT_SOURCE_CONTENT.to_string()),
                    delta: Output::Text(new_chunk.to_string()),
                    finished: false,
                })
                .await?;
            *last_parsed_length = content.len();
        }
        Ok(())
    }

    fn finalize_response(&self, content: &str) -> AgentResponse {
        AgentResponse {
            data: AgentResponseData::Text {
                text: content.to_string(),
            },
        }
    }
}

#[async_trait::async_trait]
impl Executable<Vec<ChatCompletionRequestMessage>> for OpenAIExecutable {
    type Response = OpenAIExecutableResponse;

    #[tracing::instrument(skip_all,err, fields(
        otel.name = events::llm::LLM_OPENAI_CALL,
        oxy.span_type = events::llm::LLM_CALL_TYPE,
        gen_ai.request.model = %self.model,
    ))]
    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: Vec<ChatCompletionRequestMessage>,
    ) -> Result<Self::Response, OxyError> {
        events::llm::input(&input);

        let chat = self.client.chat();

        let func = || async {
            let mut builder = CreateChatCompletionRequestArgs::default();
            builder
                .model(self.model.clone())
                .messages(input.clone())
                .stream_options(ChatCompletionStreamOptions {
                    include_usage: Some(true),
                    include_obfuscation: Some(false),
                })
                .stream(true);

            if let Some(ReasoningConfig { effort }) = &self.reasoning_config {
                builder.reasoning_effort(effort.clone());
            }

            if let Some(tool_choice) = &self.tool_choice {
                builder.tool_choice(tool_choice.clone());
            }

            if !self.tool_configs.is_empty() {
                let tools_wrapped: Vec<ChatCompletionTools> = self
                    .tool_configs
                    .clone()
                    .into_iter()
                    .map(ChatCompletionTools::Function)
                    .collect();
                builder.tools(tools_wrapped);
            }

            let request = builder.build().map_err(|err| {
                tracing::error!("Failed to build completion request: {err:?}");
                OxyError::RuntimeError(format!("Error building completion request: {err:?}"))
            })?;

            let mut response = chat.create_stream(request).await.map_err(|err| {
                tracing::error!("Streaming request failed: {err}");
                if matches!(err, OpenAIError::StreamError(_)) {
                    backoff::Error::<OxyError>::transient(err.into())
                } else {
                    backoff::Error::<OxyError>::Permanent(err.into())
                }
            })?;

            let mut content = String::new();
            let mut tool_calls = HashMap::<(u32, u32), ChatCompletionMessageToolCall>::new();
            let mut last_parsed_length = 0;
            let mut has_written = false;

            while let Some(response) = response.next().await.transpose().map_err(|err| {
                tracing::error!("Stream processing error: {err}");
                if matches!(err, OpenAIError::StreamError(_)) {
                    backoff::Error::<OxyError>::transient(err.into())
                } else {
                    backoff::Error::<OxyError>::Permanent(err.into())
                }
            })? {
                if let Some(usage_data) = response.usage {
                    events::llm::usage(
                        usage_data.prompt_tokens as i64,
                        usage_data.completion_tokens as i64,
                    );
                    execution_context
                        .write_usage(Usage::new(
                            usage_data.prompt_tokens as i32,
                            usage_data.completion_tokens as i32,
                        ))
                        .await?;
                }

                if let Some(chunk) = response.choices.first() {
                    if let Some(tool_call_chunks) = &chunk.delta.tool_calls {
                        self.parse_tool_call_chunks(&mut tool_calls, chunk.index, tool_call_chunks);
                    }
                    if let Some(message) = &chunk.delta.content {
                        self.process_content_chunk(
                            execution_context,
                            &mut content,
                            &tool_calls,
                            &mut last_parsed_length,
                            &mut has_written,
                            message,
                        )
                        .await?;
                    }
                }
            }

            let parsed_content = self.finalize_response(&content);
            let tool_call_count = tool_calls.len();
            if tool_call_count > 0 {
                tracing::info!(
                    llm.tool_calls_count = tool_call_count,
                    "LLM returned tool calls"
                );
            }

            let delta: Output = if has_written {
                let mut output = Into::<Output>::into(parsed_content.data.clone());
                output.replace("".to_string());
                output
            } else {
                parsed_content.data.clone().into()
            };

            execution_context
                .write_chunk(Chunk {
                    key: Some(AGENT_SOURCE_CONTENT.to_string()),
                    delta,
                    finished: true,
                })
                .await?;

            tracing::info!(
                llm.response_content_length = content.len(),
                llm.tool_calls_returned = tool_calls.len(),
                "OpenAI LLM call completed"
            );

            let tool_calls_vec: Vec<_> = tool_calls.into_values().collect();

            let response = OpenAIExecutableResponse {
                content: parsed_content.into(),
                tool_calls: tool_calls_vec,
            };

            events::llm::output(&response);

            Ok(response)
        };

        let result = self.execute_with_retry(func, execution_context).await;

        if self.synthesize_mode {
            self.clear_tools();
        }

        result
    }
}

impl OpenAIExecutable {
    async fn execute_with_retry<F, Fut>(
        &self,
        func: F,
        execution_context: &ExecutionContext,
    ) -> Result<OpenAIExecutableResponse, OxyError>
    where
        F: Fn() -> Fut + Send + Sync,
        Fut: std::future::Future<Output = Result<OpenAIExecutableResponse, backoff::Error<OxyError>>>
            + Send,
    {
        let func_with_log = || async {
            match func().await {
                Ok(response) => {
                    tracing::debug!("OpenAI execution completed successfully");
                    Ok(response)
                }
                Err(err) => {
                    tracing::error!("OpenAI execution failed: {err}");
                    execution_context
                        .write_kind(EventKind::Error {
                            message: "🔴 Error while calling LLM model. Retrying..."
                                .primary()
                                .to_string(),
                        })
                        .await?;
                    Err(err)
                }
            }
        };

        backoff::future::retry_notify(
            backoff::ExponentialBackoffBuilder::default()
                .with_max_elapsed_time(Some(AGENT_RETRY_MAX_ELAPSED_TIME))
                .build(),
            func_with_log,
            |err, duration| {
                tracing::debug!("Retry after {:?}: {:?}", duration, err);
            },
        )
        .await
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct OSSExecutable {
    #[serde(skip)]
    client: Arc<OpenAIClient>,
    model: String,
    tool_configs: Vec<ChatCompletionTool>,
    tool_choice: Option<ChatCompletionToolChoiceOption>,
    reasoning_config: Option<ReasoningConfig>,
}

impl OSSExecutable {
    pub fn new(
        client: OpenAIClient,
        model: String,
        tool_configs: Vec<ChatCompletionTool>,
        tool_choice: Option<ChatCompletionToolChoiceOption>,
        reasoning_config: Option<ReasoningConfig>,
    ) -> Self {
        Self {
            client: Arc::new(client),
            model,
            tool_configs,
            tool_choice,
            reasoning_config,
        }
    }

    fn finalize_response(&self, content: &str) -> AgentResponse {
        AgentResponse {
            data: AgentResponseData::Text {
                text: content.to_string(),
            },
        }
    }
}

#[async_trait::async_trait]
impl Executable<Vec<ChatCompletionRequestMessage>> for OSSExecutable {
    type Response = OpenAIExecutableResponse;

    #[tracing::instrument(skip_all, err, fields(
        otel.name = events::llm::LLM_OSS_CALL,
        oxy.span_type = events::llm::LLM_CALL_TYPE,
        gen_ai.request.model = %self.model,
    ))]
    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: Vec<ChatCompletionRequestMessage>,
    ) -> Result<Self::Response, OxyError> {
        events::llm::input(&input);

        tracing::debug!("Starting OSS model execution with model: {}", self.model);

        let chat = self.client.chat();

        // OSS models: always use non-streaming, plain text output, no structured output
        let mut builder = CreateChatCompletionRequestArgs::default();
        builder.model(self.model.clone()).messages(input.clone());
        builder.stream(false);

        if let Some(tool_choice) = &self.tool_choice {
            builder.tool_choice(tool_choice.clone());
        }

        if !self.tool_configs.is_empty() {
            let tools_wrapped: Vec<ChatCompletionTools> = self
                .tool_configs
                .clone()
                .into_iter()
                .map(ChatCompletionTools::Function)
                .collect();
            builder.tools(tools_wrapped);
        }

        if let Some(ReasoningConfig { effort }) = &self.reasoning_config {
            builder.reasoning_effort(effort.clone());
        }

        let request = builder.build().map_err(|err| {
            OxyError::RuntimeError(format!("Error in building completion request: {err:?}"))
        })?;

        // Use lenient types for better compatibility with OpenAI-compatible APIs (Groq, Mistral, etc.)
        let response: LenientChatCompletionResponse =
            chat.create_byot(request).await.map_err(|err| {
                OxyError::RuntimeError(format!("Error in completion request: {err:?}"))
            })?;

        if let Some(usage_data) = &response.usage {
            events::llm::usage(
                usage_data.prompt_tokens as i64,
                usage_data.completion_tokens as i64,
            );
            execution_context
                .write_usage(Usage::new(
                    usage_data.prompt_tokens as i32,
                    usage_data.completion_tokens as i32,
                ))
                .await?;
        }

        let choice = response
            .choices
            .first()
            .ok_or_else(|| OxyError::RuntimeError("No choices returned from API".to_string()))?;

        let content = choice.message.content.as_deref().unwrap_or("");
        let tool_calls: Vec<ChatCompletionMessageToolCall> = choice
            .message
            .tool_calls
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(Into::into)
            .collect();

        execution_context
            .write_kind(EventKind::Message {
                message: "\nOutput:".primary().to_string(),
            })
            .await?;

        let parsed_content = self.finalize_response(content);
        let delta: Output = parsed_content.data.clone().into();
        execution_context
            .write_chunk(Chunk {
                key: Some(AGENT_SOURCE_CONTENT.to_string()),
                delta,
                finished: true,
            })
            .await?;

        let response = OpenAIExecutableResponse {
            content: parsed_content.into(),
            tool_calls: tool_calls.clone(),
        };

        events::llm::output(&response);

        Ok(response)
    }
}

#[derive(Debug, Clone)]
pub struct OneShotInput {
    pub system_instructions: String,
    pub user_input: Option<String>,
    pub memory: Vec<Message>,
}

/// Maps OneShotInput to ChatCompletion messages
pub async fn map_oneshot_to_messages(
    input: OneShotInput,
) -> Result<Vec<ChatCompletionRequestMessage>, OxyError> {
    let mut messages = vec![
        ChatCompletionRequestSystemMessageArgs::default()
            .content(input.system_instructions)
            .build()?
            .into(),
    ];

    for message in input.memory {
        let chat_message = if message.is_human {
            ChatCompletionRequestUserMessageArgs::default()
                .content(message.content)
                .build()
                .map_err(|e| {
                    OxyError::RuntimeError(format!("Failed to build user message from memory: {e}"))
                })?
                .into()
        } else {
            ChatCompletionRequestAssistantMessageArgs::default()
                .content(message.content)
                .build()
                .map_err(|e| {
                    OxyError::RuntimeError(format!(
                        "Failed to build assistant message from memory: {e}"
                    ))
                })?
                .into()
        };
        messages.push(chat_message);
    }

    if let Some(user_input) = input.user_input {
        messages.push(
            ChatCompletionRequestUserMessageArgs::default()
                .content(user_input)
                .build()?
                .into(),
        );
    }

    Ok(messages)
}

#[derive(Clone, Debug)]
pub struct SimpleMapper;

#[async_trait::async_trait]
impl oxy::execute::builders::map::ParamMapper<OneShotInput, Vec<ChatCompletionRequestMessage>>
    for SimpleMapper
{
    async fn map(
        &self,
        _execution_context: &oxy::execute::ExecutionContext,
        input: OneShotInput,
    ) -> Result<
        (
            Vec<ChatCompletionRequestMessage>,
            Option<oxy::execute::ExecutionContext>,
        ),
        OxyError,
    > {
        let messages = map_oneshot_to_messages(input).await?;
        Ok((messages, None))
    }
}

pub fn build_openai_executable(
    client: OpenAIClient,
    model: String,
    tool_configs: Vec<ChatCompletionTool>,
    tool_choice: Option<ChatCompletionToolChoiceOption>,
    reasoning_config: Option<ReasoningConfig>,
    synthesize_mode: bool,
) -> OpenAIOrOSSExecutable {
    if reasoning_config.is_some() {
        return OpenAIOrOSSExecutable::OpenAIResponse(OpenAIResponseExecutable::new(
            client,
            model,
            tool_configs,
            tool_choice,
            reasoning_config,
            synthesize_mode,
        ));
    }

    if is_oss_model(&model) {
        return OpenAIOrOSSExecutable::OSS(OSSExecutable::new(
            client,
            model,
            tool_configs,
            tool_choice,
            reasoning_config,
        ));
    }

    OpenAIOrOSSExecutable::OpenAI(OpenAIExecutable::new(
        client,
        model,
        tool_configs,
        tool_choice,
        reasoning_config,
        synthesize_mode,
    ))
}
