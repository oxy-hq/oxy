use std::{collections::HashMap, sync::Arc};

use async_openai::{
    error::OpenAIError,
    types::{
        ChatCompletionMessageToolCall, ChatCompletionMessageToolCallChunk,
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestUserMessageArgs, ChatCompletionTool, ChatCompletionToolType,
        CreateChatCompletionRequestArgs, FunctionCall, ResponseFormat, ResponseFormatJsonSchema,
    },
};
use deser_incomplete::from_json_str;
use futures::StreamExt;
use schemars::{JsonSchema, schema_for};
use serde::Deserialize;
use serde_json::json;

use crate::{
    adapters::openai::OpenAIClient,
    config::{constants::AGENT_SOURCE_CONTENT, model::Model},
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        builders::{
            ExecutableBuilder,
            map::{MapInput, ParamMapper},
        },
        types::{Chunk, EventKind, Output},
    },
    theme::StyledText,
    utils::variant_eq,
};

#[derive(Clone, Debug)]
pub struct OpenAIExecutable {
    client: Arc<OpenAIClient>,
    model: String,
    tool_configs: Vec<ChatCompletionTool>,
}

impl OpenAIExecutable {
    pub fn new(client: OpenAIClient, model: String, tool_configs: Vec<ChatCompletionTool>) -> Self {
        Self {
            client: Arc::new(client),
            model,
            tool_configs,
        }
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
}

pub struct OpenAIExecutableResponse {
    pub content: Output,
    pub tool_calls: Vec<ChatCompletionMessageToolCall>,
}

#[async_trait::async_trait]
impl Executable<Vec<ChatCompletionRequestMessage>> for OpenAIExecutable {
    type Response = OpenAIExecutableResponse;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: Vec<ChatCompletionRequestMessage>,
    ) -> Result<Self::Response, OxyError> {
        tracing::debug!("Executing OpenAI executable with input: {:?}", input);
        let chat = self.client.chat();
        let mut request_builder = CreateChatCompletionRequestArgs::default();
        let schema = json!(schema_for!(AgentResponse));
        request_builder
            .model(self.model.clone())
            .response_format(ResponseFormat::JsonSchema {
                json_schema: ResponseFormatJsonSchema {
                    name: "AgentResponse".to_string(),
                    description: Some("Agent response".to_string()),
                    schema: Some(schema),
                    strict: Some(true),
                },
            })
            .stream(true)
            .messages(input);

        if !self.tool_configs.is_empty() {
            request_builder.tools(self.tool_configs.clone());
        }

        let func = || async {
            let request = request_builder
                .build()
                .map_err(|err| {
                    OxyError::RuntimeError(format!(
                        "Error in building completion request: {:?}",
                        err
                    ))
                })
                .map_err(backoff::Error::Permanent)?;
            tracing::debug!("OpenAI request: {:?}", request);
            let mut response = chat
                .create_stream(request)
                .await
                .map_err(|err| {
                    OxyError::RuntimeError(format!("Error in completion request: {:?}", err))
                })
                .map_err(backoff::Error::Permanent)?;
            let mut content = String::new();
            let mut tool_calls = HashMap::<(u32, u32), ChatCompletionMessageToolCall>::new();
            let mut last_parsed_content = String::new();
            let mut has_written = false;

            while let Some(response) =
                response.next().await.transpose().map_err(|err| match err {
                    OpenAIError::StreamError(_) => {
                        backoff::Error::<OxyError>::transient(err.into())
                    }
                    _ => backoff::Error::<OxyError>::Permanent(err.into()),
                })?
            {
                if let Some(chunk) = response.choices.first() {
                    if let Some(tool_call_chunks) = &chunk.delta.tool_calls {
                        self.parse_tool_call_chunks(&mut tool_calls, chunk.index, tool_call_chunks);
                    }
                    if let Some(message) = &chunk.delta.content {
                        content.push_str(message);
                        // Check if the content is a valid JSON string and parse it
                        // then write the chunk to the execution context
                        if let Ok(data) = from_json_str::<AgentResponse>(&content) {
                            let (parsed_content, mut output) = match data.data {
                                AgentResponseData::Table { file_path } => {
                                    (file_path, Output::table(message.to_string()))
                                }
                                AgentResponseData::Text { text } => {
                                    (text, Output::Text(message.to_string()))
                                }
                                AgentResponseData::SQL { sql } => {
                                    (sql, Output::sql(message.to_string()))
                                }
                            };
                            if last_parsed_content != parsed_content
                                && variant_eq(&Output::Text("".to_string()), &output)
                            {
                                if !has_written {
                                    execution_context
                                        .write_kind(EventKind::Message {
                                            message: "\nOutput:".primary().to_string(),
                                        })
                                        .await?;
                                }

                                has_written = true;
                                let chunk = parsed_content.replace(&last_parsed_content, "");
                                output.replace(chunk);
                                last_parsed_content = parsed_content;
                                execution_context
                                    .write_chunk(Chunk {
                                        key: Some(AGENT_SOURCE_CONTENT.to_string()),
                                        delta: output,
                                        finished: false,
                                    })
                                    .await?;
                            }
                        }
                    }

                    if chunk.finish_reason.is_some() {
                        break;
                    }
                }
            }
            let content = {
                if content.is_empty() {
                    AgentResponse::default()
                } else {
                    serde_json::from_str::<AgentResponse>(&content).map_err(|err| {
                        OxyError::SerializerError(format!(
                            "Failed to deserialize OpenAI response: \"{}\"\n{}",
                            content, err
                        ))
                    })?
                }
            };
            tracing::info!("Agent response: {:?}", content);

            let delta: Output = if has_written {
                let mut output = Into::<Output>::into(content.data.clone());
                output.replace("".to_string());
                output
            } else {
                content.data.clone().into()
            };
            execution_context
                .write_chunk(Chunk {
                    key: Some(AGENT_SOURCE_CONTENT.to_string()),
                    delta,
                    finished: true,
                })
                .await?;

            Ok(OpenAIExecutableResponse {
                content: content.into(),
                tool_calls: tool_calls.into_values().collect(),
            })
        };
        let mut attempt = 0;
        backoff::future::retry_notify(backoff::ExponentialBackoff::default(), func, |err, b| {
            attempt += 1;
            tracing::error!("Error happened at {:?} in OpenAI executable: {:?}", b, err);
            tracing::warn!("Retrying({})...", attempt);
        })
        .await
    }
}

#[derive(Debug, Clone)]
pub struct OneShotInput {
    pub system_instructions: String,
    pub user_input: Option<String>,
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
        let mut messages = vec![
            ChatCompletionRequestSystemMessageArgs::default()
                .content(input.system_instructions)
                .build()?
                .into(),
        ];
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

pub fn build_openai_executable(
    model: &Model,
) -> MapInput<OpenAIExecutable, SimpleMapper, Vec<ChatCompletionRequestMessage>> {
    let executable = ExecutableBuilder::new()
        .map(SimpleMapper)
        .executable(OpenAIExecutable::new(
            OpenAIClient::with_config(model.try_into().unwrap()),
            model.model_name().to_string(),
            vec![],
        ));
    executable
}

pub fn build_openai_executable_with_tools(
    model: &Model,
    tools: Vec<ChatCompletionTool>,
) -> MapInput<OpenAIExecutable, SimpleMapper, Vec<ChatCompletionRequestMessage>> {
    let executable = ExecutableBuilder::new()
        .map(SimpleMapper)
        .executable(OpenAIExecutable::new(
            OpenAIClient::with_config(model.try_into().unwrap()),
            model.model_name().to_string(),
            tools,
        ));
    executable
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
