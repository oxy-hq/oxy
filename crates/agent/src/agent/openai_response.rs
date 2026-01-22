use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use deser_incomplete::from_json_str;
use futures::{Stream, StreamExt};
use serde::Serialize;

use async_openai::types::{
    chat::{
        ChatCompletionMessageToolCall, ChatCompletionRequestMessage, ChatCompletionTool,
        ChatCompletionToolChoiceOption, FunctionCall,
    },
    responses::{OutputItem, ResponseStreamEvent},
};
use async_openai::{
    error::OpenAIError,
    types::responses::{CreateResponse, CreateResponseArgs, Reasoning, ReasoningSummary},
};

use crate::agent::openai::{AgentResponse, AgentResponseData, OpenAIExecutableResponse};

use oxy::{
    adapters::openai::OpenAIClient,
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
use oxy_openai::{convert_messages_to_response_input, convert_tool_choice, convert_tools};
use oxy_shared::errors::OxyError;

#[derive(Clone, Debug, Serialize)]
pub struct OpenAIResponseExecutable {
    #[serde(skip)]
    client: Arc<OpenAIClient>,
    model: String,
    tool_configs: Vec<ChatCompletionTool>,
    tool_choice: Option<ChatCompletionToolChoiceOption>,
    reasoning_config: Option<ReasoningConfig>,
    synthesize_mode: bool,
}

impl OpenAIResponseExecutable {
    pub fn new(
        client: OpenAIClient,
        model: String,
        tool_configs: Vec<ChatCompletionTool>,
        tool_choice: Option<ChatCompletionToolChoiceOption>,
        reasoning_config: Option<ReasoningConfig>,
        synthesize_mode: bool,
    ) -> Self {
        tracing::debug!("Building OpenAI executable for model: {}", model);
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

    async fn process_content_chunk(
        &self,
        execution_context: &ExecutionContext,
        content: &mut String,
        tool_calls: &HashMap<String, ChatCompletionMessageToolCall>,
        last_parsed_length: &mut usize,
        has_written: &mut bool,
        message: &str,
    ) -> Result<(), OxyError> {
        content.push_str(message);

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

        tracing::debug!("Parsed structured content: {}", parsed_content);

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

    fn build_request(
        &self,
        input: Vec<ChatCompletionRequestMessage>,
    ) -> Result<CreateResponse, OxyError> {
        let mut builder = CreateResponseArgs::default();
        let response_input = convert_messages_to_response_input(input)?;

        builder
            .model(self.model.clone())
            .input(response_input.clone())
            .stream(true);

        tracing::debug!("Building response for response_input: {:?}", response_input);

        if let Some(reasoning_config) = &self.reasoning_config {
            builder.reasoning(Reasoning {
                effort: Some(reasoning_config.effort.clone().into()),
                summary: Some(ReasoningSummary::Auto),
            });
        }

        if let Some(tool_choice) = &self.tool_choice {
            let response_tool_choice = convert_tool_choice(tool_choice)?;
            builder.tool_choice(response_tool_choice);
        }

        if !self.tool_configs.is_empty() {
            let response_tools = convert_tools(&self.tool_configs)?;
            builder.tools(response_tools);
        }

        builder.build().map_err(|err| {
            tracing::error!("Failed to build response request: {err:?}");
            OxyError::RuntimeError(format!("Error building response request: {err:?}"))
        })
    }

    async fn process_stream(
        &self,
        mut event_stream: impl Stream<Item = Result<ResponseStreamEvent, OpenAIError>> + Unpin + Send,
        execution_context: &ExecutionContext,
    ) -> Result<OpenAIExecutableResponse, backoff::Error<OxyError>> {
        let mut content = String::new();
        let mut tool_calls = HashMap::<String, ChatCompletionMessageToolCall>::new();
        let mut item_to_output_index = HashMap::<String, u32>::new();
        let mut reasoning_items_written = HashSet::<String>::new();
        let mut last_parsed_length = 0;
        let mut has_written = false;

        while let Some(event) = event_stream.next().await.transpose().map_err(|err| {
            tracing::error!("Stream processing error: {err}");
            if matches!(err, OpenAIError::StreamError(_)) {
                backoff::Error::<OxyError>::transient(err.into())
            } else {
                backoff::Error::<OxyError>::Permanent(err.into())
            }
        })? {
            tracing::trace!("Received response event: {:?}", event);
            match event {
                ResponseStreamEvent::ResponseOutputTextDelta(delta_event) => {
                    let message = &delta_event.delta;
                    self.process_content_chunk(
                        execution_context,
                        &mut content,
                        &tool_calls,
                        &mut last_parsed_length,
                        &mut has_written,
                        message,
                    )
                    .await
                    .map_err(backoff::Error::Permanent)?;
                }
                ResponseStreamEvent::ResponseOutputItemAdded(added_event) => {
                    if let OutputItem::FunctionCall(ref func_call) = added_event.item
                        && let Some(item_id) = func_call.id.clone()
                    {
                        item_to_output_index.insert(item_id.clone(), added_event.output_index);

                        tracing::debug!(
                            "Function call item added - call_id: {}, name: {}, item_id: {}, output_index: {}",
                            func_call.call_id,
                            func_call.name,
                            item_id,
                            added_event.output_index
                        );

                        tool_calls.insert(
                            item_id,
                            ChatCompletionMessageToolCall {
                                id: func_call.call_id.clone(),
                                function: FunctionCall {
                                    name: func_call.name.clone(),
                                    arguments: String::new(),
                                },
                            },
                        );
                    }
                }
                ResponseStreamEvent::ResponseCompleted(completed) => {
                    if let Some(usage_data) = completed.response.usage {
                        events::llm::usage(
                            usage_data.input_tokens as i64,
                            usage_data.output_tokens as i64,
                        );
                        execution_context
                            .write_usage(Usage::new(
                                usage_data.input_tokens as i32,
                                usage_data.output_tokens as i32,
                            ))
                            .await
                            .map_err(backoff::Error::Permanent)?;
                    }
                    break;
                }
                ResponseStreamEvent::ResponseFunctionCallArgumentsDelta(func_delta) => {
                    let item_id = &func_delta.item_id;

                    tracing::debug!(
                        "Function call arguments delta for item_id: {}, delta: {:?}",
                        item_id,
                        func_delta.delta
                    );

                    if let Some(tool_call) = tool_calls.get_mut(item_id) {
                        tool_call.function.arguments.push_str(&func_delta.delta);
                    } else {
                        tracing::warn!("Received arguments delta for unknown item_id: {}", item_id);
                    }
                }
                ResponseStreamEvent::ResponseOutputItemDone(done_event) => {
                    if let OutputItem::FunctionCall(func_call) = done_event.item {
                        let item_id = func_call.id.clone();

                        tracing::debug!(
                            "Function call completed - item_id: {:?}, call_id: {}, name: {}, arguments length: {}",
                            item_id,
                            func_call.call_id,
                            func_call.name,
                            func_call.arguments.len()
                        );

                        if let Some(ref id) = item_id
                            && let Some(tool_call) = tool_calls.get_mut(id)
                            && (tool_call.function.arguments.is_empty()
                                || tool_call.function.arguments != func_call.arguments)
                        {
                            tool_call.function.arguments = func_call.arguments;
                        }
                    }
                }
                ResponseStreamEvent::ResponseError(error) => {
                    return Err(backoff::Error::Permanent(OxyError::RuntimeError(format!(
                        "Response API error: {}",
                        error.message
                    ))));
                }
                ResponseStreamEvent::ResponseReasoningSummaryTextDelta(delta_event) => {
                    let reasoning_delta = &delta_event.delta;
                    execution_context
                        .write_chunk(Chunk {
                            key: Some(AGENT_SOURCE_CONTENT.to_string()),
                            delta: Output::Text(reasoning_delta.to_string()),
                            finished: false,
                        })
                        .await
                        .map_err(backoff::Error::Permanent)?;
                }
                ResponseStreamEvent::ResponseReasoningSummaryTextDone(done_event) => {
                    tracing::debug!(
                        "Reasoning summary complete - item_id: {}, summary_index: {}, total text length: {}",
                        done_event.item_id,
                        done_event.summary_index,
                        done_event.text.len()
                    );

                    if reasoning_items_written.contains(&format!(
                        "{}-{}",
                        done_event.item_id, done_event.summary_index
                    )) {
                        tracing::debug!(
                            "Finalizing reasoning summary output for item_id: {}",
                            done_event.item_id
                        );
                        execution_context
                            .write_chunk(Chunk {
                                key: Some(AGENT_SOURCE_CONTENT.to_string()),
                                delta: Output::Text("\n\n:::\n\n".to_string()),
                                finished: false,
                            })
                            .await
                            .map_err(backoff::Error::Permanent)?;
                    }
                }
                ResponseStreamEvent::ResponseReasoningSummaryPartAdded(part_event) => {
                    tracing::debug!(
                        "Reasoning summary part added - item_id: {}, summary_index: {}",
                        part_event.item_id,
                        part_event.summary_index
                    );

                    if reasoning_items_written.insert(format!(
                        "{}-{}",
                        part_event.item_id, part_event.summary_index
                    )) {
                        execution_context
                            .write_chunk(Chunk {
                                key: Some(AGENT_SOURCE_CONTENT.to_string()),
                                delta: Output::Text("\n\n:::reasoning\n".to_string()),
                                finished: false,
                            })
                            .await
                            .map_err(backoff::Error::Permanent)?;
                    }
                }
                ResponseStreamEvent::ResponseReasoningSummaryPartDone(part_event) => {
                    tracing::debug!(
                        "Reasoning summary part done - item_id: {}, summary_index: {}",
                        part_event.item_id,
                        part_event.summary_index
                    );
                }
                _ => {
                    tracing::trace!("Received response event: {:?}", event);
                }
            }
        }

        let parsed_content = self.finalize_response(&content);
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
            .await
            .map_err(backoff::Error::Permanent)?;

        tracing::debug!(
            "Finished writing chunk for key: {:?}",
            tool_calls.clone().into_values()
        );
        Ok(OpenAIExecutableResponse {
            content: parsed_content.into(),
            tool_calls: tool_calls.into_values().collect(),
        })
    }
}

#[async_trait::async_trait]
impl Executable<Vec<ChatCompletionRequestMessage>> for OpenAIResponseExecutable {
    type Response = OpenAIExecutableResponse;

    #[tracing::instrument(skip_all,err, fields(
        otel.name = events::llm::LLM_OPENAI_RESPONSE_CALL,
        oxy.span_type = events::llm::LLM_CALL_TYPE,
        gen_ai.request.model = %self.model,
    ))]
    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: Vec<ChatCompletionRequestMessage>,
    ) -> Result<Self::Response, OxyError> {
        events::llm::input(&input);

        tracing::debug!("Starting OpenAI execution with model: {}", self.model);

        let responses = self.client.responses();
        let input_clone = input.clone();

        let func = || async {
            let request = self
                .build_request(input_clone.clone())
                .map_err(backoff::Error::Permanent)?;

            let event_stream = responses.create_stream(request).await.map_err(|err| {
                tracing::error!("Streaming request failed: {err}");
                if matches!(err, OpenAIError::StreamError(_)) {
                    backoff::Error::<OxyError>::transient(err.into())
                } else {
                    backoff::Error::<OxyError>::Permanent(err.into())
                }
            })?;

            self.process_stream(event_stream, execution_context).await
        };

        let result = self.execute_with_retry(func, execution_context).await;

        if let Ok(ref response) = result {
            events::llm::output(response);
        }

        if self.synthesize_mode {
            self.clear_tools();
        }

        result
    }
}

impl OpenAIResponseExecutable {
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
