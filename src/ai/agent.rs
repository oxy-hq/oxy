use crate::{
    ai::utils::{record_batches_to_json, record_batches_to_markdown},
    config::model::{FileFormat, OutputFormat},
    connector::load_result,
    errors::OnyxError,
    execute::{
        agent::AgentEvent,
        core::{value::ContextValue, write::Write, Executable, ExecutionContext},
    },
    utils::{format_table_output, truncate_datasets},
};
use std::{collections::HashMap, sync::Arc};

use super::{anonymizer::base::Anonymizer, toolbox::ToolBox, MultiTool};
use async_openai::{
    config::{AzureConfig, OpenAIConfig, OPENAI_API_BASE},
    error::OpenAIError,
    types::{
        ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage,
        ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestToolMessageArgs,
        ChatCompletionRequestUserMessageArgs, ChatCompletionResponseMessage, ChatCompletionTool,
        ChatCompletionToolArgs, ChatCompletionToolType, CreateChatCompletionRequestArgs,
        FunctionObjectArgs, ResponseFormat, ResponseFormatJsonSchema,
    },
    Client,
};
use async_trait::async_trait;
use pyo3::pyclass;
use schemars::{schema_for, JsonSchema};
use serde::Deserialize;
use serde_json::json;

const CONTEXT_WINDOW_EXCEEDED_CODE: &str = "string_above_max_length";

#[pyclass(module = "onyx_py")]
pub struct AgentResult {
    #[pyo3(get)]
    pub output: ContextValue,
}

#[derive(Deserialize, Debug, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FilePathOutput {
    pub file_path: String,
}

#[async_trait]
pub trait LLMAgent {
    async fn request(
        &self,
        system_instruction: &str,
        input: &str,
        execution_context: &mut ExecutionContext<'_, AgentEvent>,
    ) -> Result<String, OnyxError>;
}

enum OpenAIClientConfig {
    Azure(AzureConfig),
    OpenAI(OpenAIConfig),
}

enum OpenAIClient {
    Azure(Client<AzureConfig>),
    OpenAI(Client<OpenAIConfig>),
}

pub struct OpenAIAgent {
    client: OpenAIClient,
    model: String,
    system_instruction: String,
    max_tries: u8,
    output_format: OutputFormat,
    anonymizer: Option<Box<dyn Anonymizer + Send + Sync>>,
    file_format: FileFormat,
    pub tools: Arc<ToolBox<MultiTool>>,
}

impl OpenAIAgent {
    pub fn new(
        model: String,
        api_url: Option<String>,
        api_key: String,
        azure_deployment_id: Option<String>,
        azure_api_version: Option<String>,
        system_instruction: String,
        output_format: OutputFormat,
        anonymizer: Option<Box<dyn Anonymizer + Send + Sync>>,
        file_format: FileFormat,
        tools: Arc<ToolBox<MultiTool>>,
    ) -> Self {
        let url = api_url.unwrap_or(OPENAI_API_BASE.to_string());
        let client_config = if url.contains("azure.com") {
            OpenAIClientConfig::Azure(
                AzureConfig::new()
                    .with_api_key(api_key)
                    .with_api_base(url)
                    .with_deployment_id(azure_deployment_id.unwrap())
                    .with_api_version(azure_api_version.unwrap()),
            )
        } else {
            OpenAIClientConfig::OpenAI(OpenAIConfig::new().with_api_key(api_key).with_api_base(url))
        };

        let client = match client_config {
            OpenAIClientConfig::Azure(client_config) => {
                OpenAIClient::Azure(Client::with_config(client_config))
            }
            OpenAIClientConfig::OpenAI(client_config) => {
                OpenAIClient::OpenAI(Client::with_config(client_config))
            }
        };

        let max_tries = 5;

        OpenAIAgent {
            client,
            model,
            max_tries,
            system_instruction,
            output_format,
            anonymizer,
            file_format,
            tools,
        }
    }

    async fn completion_request(
        &self,
        messages: Vec<ChatCompletionRequestMessage>,
        tools: Vec<ChatCompletionTool>,
        response_format: Option<ResponseFormat>,
    ) -> Result<ChatCompletionResponseMessage, OpenAIError> {
        let mut request_builder = CreateChatCompletionRequestArgs::default();
        if tools.is_empty() {
            request_builder.model(self.model.clone()).messages(messages);
        } else {
            request_builder
                .model(self.model.clone())
                .tools(tools)
                .parallel_tool_calls(false)
                .messages(messages);
        }
        if let Some(format) = response_format {
            request_builder.response_format(format);
        }

        let request = request_builder.build().unwrap();

        let response = match &self.client {
            OpenAIClient::Azure(client) => client.chat().create(request).await?,
            OpenAIClient::OpenAI(client) => client.chat().create(request).await?,
        };

        Ok(response.choices[0].message.clone())
    }

    fn spec_serializer(
        name: String,
        description: String,
        parameters: serde_json::Value,
    ) -> ChatCompletionTool {
        ChatCompletionToolArgs::default()
            .r#type(ChatCompletionToolType::Function)
            .function(
                FunctionObjectArgs::default()
                    .name(name)
                    .description(description)
                    .parameters(parameters)
                    .build()
                    .unwrap(),
            )
            .build()
            .unwrap()
    }
}

#[async_trait]
impl LLMAgent for OpenAIAgent {
    async fn request(
        &self,
        system_message: &str,
        input: &str,
        execution_context: &mut ExecutionContext<'_, AgentEvent>,
    ) -> Result<String, OnyxError> {
        let anonymized_items = HashMap::new();
        let (anonymized_system_message, anonymized_items) = match self.anonymizer {
            Some(ref anonymizer) => anonymizer.anonymize(&system_message, Some(anonymized_items)),
            None => Ok((system_message.to_string(), anonymized_items)),
        }?;
        let (anonymized_user_message, anonymized_items) = match self.anonymizer {
            Some(ref anonymizer) => anonymizer.anonymize(input, Some(anonymized_items)),
            None => Ok((input.to_string(), anonymized_items)),
        }?;

        let messages: Vec<ChatCompletionRequestMessage> = vec![
            ChatCompletionRequestSystemMessageArgs::default()
                .name("onyx")
                .content(anonymized_system_message)
                .build()
                .map_err(|e| {
                    OnyxError::RuntimeError(format!("Unable to build LLM request: {e}").into())
                })?
                .into(),
            ChatCompletionRequestUserMessageArgs::default()
                .name("Human")
                .content(anonymized_user_message)
                .build()
                .map_err(|e| {
                    OnyxError::RuntimeError(format!("Unable to build LLM request: {e}").into())
                })?
                .into(),
        ];
        let tools = self.tools.to_spec(OpenAIAgent::spec_serializer);

        let mut tries: u8 = 0;
        let mut output = "Something went wrong".to_string();
        let mut tool_returns = Vec::<ChatCompletionRequestMessage>::new();
        let mut tool_calls = Vec::<ChatCompletionRequestMessage>::new();

        let mut contextualize_anonymized_items = anonymized_items.clone();
        while tries < self.max_tries {
            let message_with_replies =
                [messages.clone(), tool_calls.clone(), tool_returns.clone()].concat();
            tool_returns.clear();
            tool_calls.clear();
            log::debug!("Start completion request {:?}", message_with_replies);
            let response_format: Option<ResponseFormat> = match self.output_format {
                OutputFormat::Default => None,
                OutputFormat::File => {
                    let schema = json!(schema_for!(FilePathOutput));
                    log::info!("Schema: {}", schema);
                    Some(ResponseFormat::JsonSchema {
                        json_schema: ResponseFormatJsonSchema {
                            name: "file_path".to_string(),
                            description: Some(
                                "Path to the arrow file containing the query results".to_string(),
                            ),
                            schema: Some(schema),
                            strict: Some(true),
                        },
                    })
                }
            };
            let ret_message = self
                .completion_request(message_with_replies, tools.clone(), response_format)
                .await
                .map_err(|e|  {
                    if let OpenAIError::ApiError(ref api_error) = e {
                        if api_error.code == Some(CONTEXT_WINDOW_EXCEEDED_CODE.to_string()) {
                            return OnyxError::LLMError(
                                "Context window length exceeded. Shorten the prompt being sent to the LLM.".into()
                            );
                        }
                    }
                    OnyxError::RuntimeError(format!("Error in completion request: {}", e))
                })?;

            output = ret_message
                .content
                .unwrap_or("Empty response from OpenAI".to_string())
                .clone();
            let tool_call_requests = ret_message.tool_calls.unwrap_or_default();
            log::info!(
                "Number of tool calls: {} on {}",
                &tool_call_requests.len(),
                tries,
            );
            for tool in tool_call_requests.clone() {
                let tool_call_ret = self
                    .tools
                    .run_tool(&tool.function.name, tool.function.arguments.clone())
                    .await;

                let mut tool_ret = tool_call_ret.get_truncated_output();
                if self.anonymizer.is_some() {
                    let result = self
                        .anonymizer
                        .as_ref()
                        .unwrap()
                        .anonymize(&tool_ret, Some(contextualize_anonymized_items.clone()))
                        .map_err(|e| {
                            OnyxError::RuntimeError(format!(
                                "Error in anonymizing tool output: {}",
                                e
                            ))
                        })?;
                    contextualize_anonymized_items.extend(result.1);
                    tool_ret = result.0;
                }
                log::info!("Tool output: {}", tool_ret);
                tool_returns.push(
                    ChatCompletionRequestToolMessageArgs::default()
                        .tool_call_id(tool.id.clone())
                        .content(tool_ret)
                        .build()
                        .map_err(|e| {
                            OnyxError::RuntimeError(
                                format!("Unable to build LLM request: {e}").into(),
                            )
                        })?
                        .into(),
                );
                execution_context.notify(AgentEvent::ToolCall(tool_call_ret));
            }

            if tool_returns.is_empty() {
                break;
            }
            tool_calls.push(
                ChatCompletionRequestAssistantMessageArgs::default()
                    .tool_calls(tool_call_requests.clone())
                    .build()
                    .map_err(|e| {
                        OnyxError::RuntimeError(format!("Unable to build LLM request: {e}").into())
                    })?
                    .into(),
            );

            tries += 1;
        }
        let mut parsed_output = map_output(&output, &self.output_format, &self.file_format).await?;
        parsed_output = match self.anonymizer {
            Some(ref anonymizer) => {
                anonymizer.deanonymize(&parsed_output, &contextualize_anonymized_items)
            }
            None => parsed_output,
        };
        Ok(parsed_output)
    }
}

#[async_trait]
impl Executable<AgentEvent> for OpenAIAgent {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, AgentEvent>,
    ) -> Result<(), OnyxError> {
        execution_context.notify(AgentEvent::Started);
        let input = execution_context.get_context_str();
        let context = execution_context.get_context();
        let system_instruction = execution_context
            .renderer
            .render_async(&self.system_instruction, context)
            .await?;
        log::info!("System instruction: {}", system_instruction);
        log::info!("Prompt: {}", input);
        let result = self
            .request(&system_instruction, &input, execution_context)
            .await?;
        let event = AgentEvent::Finished {
            output: result.clone(),
        };
        execution_context.notify(event);
        execution_context.write(ContextValue::Text(result));
        Ok(())
    }
}

async fn map_output(
    output: &str,
    output_format: &OutputFormat,
    file_format: &FileFormat,
) -> Result<String, OnyxError> {
    match output_format {
        OutputFormat::Default => Ok(output.to_string()),
        OutputFormat::File => {
            log::info!("File path: {}", output);
            let file_output = serde_json::from_str::<FilePathOutput>(output).map_err(|e| {
                OnyxError::RuntimeError(format!("Error in parsing output file: {}", e))
            })?;
            let (batches, schema) = load_result(&file_output.file_path).map_err(|e| {
                OnyxError::RuntimeError(format!("Error in loading result file: {}", e))
            })?;
            let (dataset, truncated) = truncate_datasets(batches);
            match file_format {
                FileFormat::Json => {
                    let json_blob = record_batches_to_json(&dataset).map_err(|e| {
                        OnyxError::RuntimeError(format!(
                            "Error in converting record batch to json: {}",
                            e
                        ))
                    })?;
                    Ok(format_table_output(&json_blob, truncated))
                }
                FileFormat::Markdown => {
                    let markdown_table =
                        record_batches_to_markdown(&dataset, &schema).map_err(|e| {
                            OnyxError::RuntimeError(format!(
                                "Error in converting record batch to markdown: {}",
                                e
                            ))
                        })?;
                    Ok(format_table_output(&markdown_table.to_string(), truncated))
                }
            }
        }
    }
}
