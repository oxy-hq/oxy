use std::{collections::HashMap, sync::Arc};

use async_openai::{
    error::OpenAIError,
    types::{
        ChatCompletionMessageToolCall, ChatCompletionMessageToolCallChunk,
        ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage,
        ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
        ChatCompletionStreamOptions, ChatCompletionTool, ChatCompletionToolChoiceOption,
        ChatCompletionToolType, CreateChatCompletionRequestArgs, FunctionCall, ResponseFormat,
        ResponseFormatJsonSchema, responses::ReasoningConfig,
    },
};
use deser_incomplete::from_json_str;
use futures::StreamExt;
use schemars::{JsonSchema, schema_for};
use serde::Deserialize;
use serde_json::json;

use crate::{
    adapters::openai::{IntoOpenAIConfig, OpenAIClient},
    config::{
        constants::{AGENT_RETRY_MAX_ELAPSED_TIME, AGENT_SOURCE_CONTENT},
        model::Model,
    },
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        builders::{
            ExecutableBuilder,
            map::{MapInput, ParamMapper},
        },
        types::{Chunk, EventKind, Output},
    },
    service::agent::Message,
    theme::StyledText,
    utils::variant_eq,
};

use crate::execute::types::Usage;

#[derive(Clone, Debug)]
pub struct OpenAIExecutable {
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
                    r#type: ChatCompletionToolType::Function,
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
        // Only try to parse as structured JSON if we don't have tool calls
        // When tools are present, content is usually plain text
        if tool_calls.is_empty() && let Ok(data) = from_json_str::<AgentResponse>(&content) {
            self.handle_structured_response(execution_context, &data, content, last_parsed_length, has_written, message).await
        } else if !content.is_empty() {
            self.handle_plain_text_response(execution_context, content, last_parsed_length, has_written).await
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
            AgentResponseData::Text { text } => {
                (text.clone(), Output::Text(message.to_string()))
            }
            AgentResponseData::SQL { sql } => {
                (sql.clone(), Output::sql(message.to_string()))
            }
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
        
        // Stream the content as plain text
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
        if content.is_empty() {
            AgentResponse::default()
        } else {
            // Try to parse as structured JSON first, fallback to plain text
            match serde_json::from_str::<AgentResponse>(&content) {
                Ok(structured_response) => structured_response,
                Err(_) => {
                    // If JSON parsing fails, treat as plain text response
                    tracing::debug!("Response is not structured JSON, treating as plain text");
                    AgentResponse {
                        data: AgentResponseData::Text { text: content.to_string() }
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct OpenAIExecutableResponse {
    pub content: Output,
    pub tool_calls: Vec<ChatCompletionMessageToolCall>,
}

impl Default for OpenAIExecutableResponse {
    fn default() -> Self {
        OpenAIExecutableResponse {
            content: Output::Text("".to_string()),
            tool_calls: vec![],
        }
    }
}

#[async_trait::async_trait]
impl Executable<Vec<ChatCompletionRequestMessage>> for OpenAIExecutable {
    type Response = OpenAIExecutableResponse;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: Vec<ChatCompletionRequestMessage>,
    ) -> Result<Self::Response, OxyError> {
        tracing::debug!("Starting OpenAI agent execution");
        tracing::debug!("Model: {}", self.model);
        tracing::debug!("Tools configured: {}", self.tool_configs.len());
        tracing::debug!("Input messages: {}", input.len());
        tracing::trace!("Input message details: {:?}", input);
        
        let chat = self.client.chat();
        let schema = json!(schema_for!(AgentResponse));

        let func = || async {
            tracing::debug!("Building OpenAI completion request");
            
            // Helper function to build a request with optional structured output
            let build_request = |use_structured: bool| -> Result<_, backoff::Error<OxyError>> {
                let mut builder = CreateChatCompletionRequestArgs::default();
                builder
                    .model(self.model.clone())
                    .stream_options(ChatCompletionStreamOptions {
                        include_usage: true,
                    })
                    .stream(true)
                    .messages(input.clone());

                if use_structured {
                    builder.response_format(ResponseFormat::JsonSchema {
                        json_schema: ResponseFormatJsonSchema {
                            name: "AgentResponse".to_string(),
                            description: Some("Agent response".to_string()),
                            schema: Some(schema.clone()),
                            strict: Some(false),
                        },
                    });
                }

                if let Some(ReasoningConfig {
                    effort: Some(reasoning_effort),
                    ..
                }) = &self.reasoning_config
                {
                    builder.reasoning_effort(reasoning_effort.clone());
                }

                if let Some(tool_choice) = &self.tool_choice {
                    builder.tool_choice(tool_choice.clone());
                }

                if !self.tool_configs.is_empty() {
                    builder.tools(self.tool_configs.clone());
                }

                builder
                    .build()
                    .map_err(|err| {
                        tracing::error!("Failed to build completion request: {err:?}");
                        OxyError::RuntimeError(format!("Error in building completion request: {err:?}"))
                    })
                    .map_err(backoff::Error::Permanent)
            };
            
            // For most models, use plain text directly as structured output often produces poor results
            // Only use structured output for known compatible models like OpenAI GPT
            tracing::debug!("Using plain text request for better compatibility");
            let plain_request = build_request(false)?;
            let mut response = chat.create_stream(plain_request)
                .await
                .map_err(|err| {
                    tracing::error!("Plain text request failed: {err}");
                    OxyError::RuntimeError(format!("Error in completion request: {err:?}"))
                })
                .map_err(backoff::Error::Permanent)?;
            
            tracing::debug!("API call successful, processing stream");
            let mut content = String::new();
            let mut tool_calls = HashMap::<(u32, u32), ChatCompletionMessageToolCall>::new();
            let mut last_parsed_length = 0;
            let mut has_written = false;

            tracing::trace!("Starting to process response stream chunks");
            let mut chunk_count = 0;
            
            while let Some(response) =
                response.next().await.transpose().map_err(|err| {
                    tracing::error!("Stream processing error: {err}");
                    if matches!(err, OpenAIError::StreamError(_)) {
                        tracing::debug!("Transient stream error, will retry");
                        backoff::Error::<OxyError>::transient(err.into())
                    } else {
                        tracing::error!("Permanent stream error, not retrying");
                        backoff::Error::<OxyError>::Permanent(err.into())
                    }
                })?
            {
                chunk_count += 1;
                tracing::trace!("Processing chunk #{}: {:?}", chunk_count, response);
                
                if let Some(usage_data) = response.usage {
                    tracing::debug!("Usage data - Prompt tokens: {}, Completion tokens: {}", 
                        usage_data.prompt_tokens, usage_data.completion_tokens);
                    execution_context
                        .write_usage(Usage::new(
                            usage_data.prompt_tokens,
                            usage_data.completion_tokens,
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
                        ).await?;
                    }
                }
            }
            let parsed_content = self.finalize_response(&content);
            tracing::info!(
                "Agent response: {:?},\nTool Calls: {:?}",
                content,
                tool_calls
            );

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

            tracing::debug!("Stream processing completed successfully");
            tracing::debug!("Total raw content length: {}", content.len());
            tracing::debug!("Tool calls generated: {}", tool_calls.len());
            tracing::trace!("Total chunks processed: {}", chunk_count);

            Ok(OpenAIExecutableResponse {
                content: parsed_content.into(),
                tool_calls: tool_calls.into_values().collect(),
            })
        };
        let func_with_log = async || {
            let result = func().await;
            match result {
                Ok(rs) => {
                    tracing::debug!("OpenAI execution completed successfully");
                    Ok(rs)
                },
                Err(err) => {
                    tracing::error!("OpenAI execution failed: {err}");
                    
                    // Log specific error context
                    match &err {
                        backoff::Error::Permanent(perm_err) => {
                            tracing::error!("Permanent error (will not retry): {perm_err}");
                        },
                        backoff::Error::Transient { err: trans_err, .. } => {
                            tracing::debug!("Transient error (retrying): {trans_err}");
                        }
                    };
                    
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

        let mut attempt = 0;
        let response = backoff::future::retry_notify(
            backoff::ExponentialBackoffBuilder::default()
                .with_max_elapsed_time(Some(AGENT_RETRY_MAX_ELAPSED_TIME))
                .build(),
            func_with_log,
            |err, backoff_duration| {
                attempt += 1;
                tracing::debug!("Retry #{} - Error occurred after {:?} elapsed", attempt, backoff_duration);
                tracing::debug!("Error details: {:?}", err);
                tracing::debug!("Waiting {:?} before retry #{}", backoff_duration, attempt);
                
                // Log specific error types for better debugging
                if let OxyError::RuntimeError(runtime_msg) = &err {
                    tracing::debug!("Runtime error context: {}", runtime_msg);
                }
            },
        )
        .await;

        // Clear tools if we are in synthesize mode
        if self.synthesize_mode {
            self.clear_tools();
        }

        match &response {
            Ok(_) => {
                tracing::debug!("OpenAI agent execution completed successfully after {} attempts", attempt.max(1));
            },
            Err(err) => {
                tracing::error!("OpenAI agent execution failed permanently after {} attempts: {:?}", attempt, err);
            }
        }

        response
    }
}

#[derive(Debug, Clone)]
pub struct OneShotInput {
    pub system_instructions: String,
    pub user_input: Option<String>,
    pub memory: Vec<Message>,
}

#[derive(Clone, Debug)]
pub struct SimpleMapper;

#[async_trait::async_trait]
impl ParamMapper<OneShotInput, Vec<ChatCompletionRequestMessage>> for SimpleMapper {
    async fn map(
        &self,
        _execution_context: &ExecutionContext,
        input: OneShotInput,
    ) -> Result<(Vec<ChatCompletionRequestMessage>, Option<ExecutionContext>), OxyError> {
        tracing::info!("Mapping OneShotInput: {:?}", input);
        let mut messages = vec![
            ChatCompletionRequestSystemMessageArgs::default()
                .content(input.system_instructions)
                .build()?
                .into(),
        ];
        messages.extend(
            input
                .memory
                .into_iter()
                .map(
                    |message| -> Result<ChatCompletionRequestMessage, OxyError> {
                        let result = if message.is_human {
                            ChatCompletionRequestUserMessageArgs::default()
                                .content(message.content)
                                .build()
                                .map_err(|e| {
                                    OxyError::RuntimeError(format!(
                                        "Failed to build user message from memory: {e}"
                                    ))
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
                        Ok(result)
                    },
                )
                .collect::<Result<Vec<_>, _>>()?,
        );
        if let Some(user_input) = input.user_input {
            messages.push(
                ChatCompletionRequestUserMessageArgs::default()
                    .content(user_input)
                    .build()?
                    .into(),
            );
        }
        Ok((messages, None))
    }
}

pub async fn build_openai_executable(
    model: &Model,
) -> Result<MapInput<OpenAIExecutable, SimpleMapper, Vec<ChatCompletionRequestMessage>>, OxyError> {
    Ok(ExecutableBuilder::new()
        .map(SimpleMapper)
        .executable(build_openai_executable_with_tools(model, vec![]).await?))
}

pub async fn build_openai_executable_with_tools(
    model: &Model,
    tools: Vec<ChatCompletionTool>,
) -> Result<OpenAIExecutable, OxyError> {
    Ok(OpenAIExecutable::new(
        OpenAIClient::with_config(model.into_openai_config().await?),
        model.model_name().to_string(),
        tools,
        None,
        None,
        false,
    ))
}

#[derive(JsonSchema, Deserialize, Debug, Clone)]
#[serde(untagged, rename_all = "camelCase", deny_unknown_fields)]
enum AgentResponseData {
    #[schemars(
        description = "Use when returning the result of an SQL query for the specified file_path. Do not use if file_path is not provided. Don't use for data app."
    )]
    Table { file_path: String },
    #[schemars(description = "Default response type")]
    Text { text: String },
    #[schemars(description = "Use when the user explicitly asks for generating SQL")]
    SQL { sql: String },
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

#[derive(JsonSchema, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct AgentResponse {
    pub data: AgentResponseData,
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

impl From<AgentResponse> for Output {
    fn from(val: AgentResponse) -> Self {
        val.data.into()
    }
}
