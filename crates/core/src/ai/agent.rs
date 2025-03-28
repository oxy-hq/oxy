use crate::{
    ai::utils::{record_batches_to_json, record_batches_to_markdown},
    config::model::{FileFormat, OutputFormat},
    connector::load_result,
    errors::OxyError,
    execute::{
        agent::{AgentEvent, AgentInput, AgentReference},
        core::{
            Executable, ExecutionContext,
            value::{AgentOutput, ContextValue},
            write::Write,
        },
    },
    utils::{format_table_output, truncate_datasets},
};
use std::{collections::HashMap, sync::Arc};

use super::{MultiTool, anonymizer::base::Anonymizer, toolbox::ToolBox};
use async_openai::{
    Client,
    config::{AzureConfig, OPENAI_API_BASE, OpenAIConfig},
    types::{
        ChatChoice, ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage,
        ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestToolMessageArgs,
        ChatCompletionRequestUserMessageArgs, ChatCompletionResponseMessage, ChatCompletionTool,
        ChatCompletionToolArgs, ChatCompletionToolChoiceOption, ChatCompletionToolType,
        CompletionUsage, CreateChatCompletionRequestArgs, FunctionObjectArgs, ResponseFormat,
        ResponseFormatJsonSchema, ServiceTierResponse,
    },
};
use async_trait::async_trait;
use pyo3::pyclass;
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[pyclass(module = "oxy_py")]
pub struct AgentResult {
    #[pyo3(get)]
    pub output: ContextValue,
    pub references: Vec<AgentReference>,
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
        input: &str,
        system_message: &str,
        execution_context: &mut ExecutionContext<'_, AgentEvent>,
    ) -> Result<String, OxyError>;
}

enum OpenAIClientConfig {
    Azure(AzureConfig),
    OpenAI(OpenAIConfig),
}

enum OpenAIClient {
    Azure(Client<AzureConfig>),
    OpenAI(Client<OpenAIConfig>),
}

pub enum OpenAIClientProvider {
    OpenAI,
    Google,
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
    provider: OpenAIClientProvider,
}

// response from gemini does not have `id` so we need to use custom type
#[derive(Debug, Deserialize, Serialize)]
pub struct GeminiCreateChatCompletionResponse {
    pub choices: Vec<ChatChoice>,
    pub created: u32,
    pub model: String,
    pub service_tier: Option<ServiceTierResponse>,
    pub system_fingerprint: Option<String>,
    pub object: String,
    pub usage: Option<CompletionUsage>,
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
        provider: OpenAIClientProvider,
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
            provider,
        }
    }

    pub async fn simple_request(&self, system_instruction: String) -> Result<String, OxyError> {
        let messages = vec![
            ChatCompletionRequestSystemMessageArgs::default()
                .name("oxy")
                .content(system_instruction)
                .build()
                .map_err(|e| OxyError::RuntimeError(format!("Unable to build LLM request: {e}")))?
                .into(),
        ];
        let response = self.completion_request(messages, vec![], None).await?;
        log::info!("Response: {:?}", response);
        match response.content {
            Some(content) => Ok(content),
            None => Err(OxyError::RuntimeError(
                "Empty response from OpenAI".to_string(),
            )),
        }
    }

    async fn completion_request(
        &self,
        messages: Vec<ChatCompletionRequestMessage>,
        tools: Vec<ChatCompletionTool>,
        response_format: Option<ResponseFormat>,
    ) -> Result<ChatCompletionResponseMessage, OxyError> {
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

        let mut request = request_builder.build().unwrap();

        match &self.client {
            OpenAIClient::Azure(client) => {
                let rs = client.chat().create(request).await?;
                return Ok(rs.choices[0].message.clone());
            }
            OpenAIClient::OpenAI(client) => match self.provider {
                OpenAIClientProvider::OpenAI => {
                    let rs = client.chat().create(request).await?;

                    return Ok(rs.choices[0].message.clone());
                }
                OpenAIClientProvider::Google => {
                    request.tool_choice = Some(ChatCompletionToolChoiceOption::Auto);
                    let rs: GeminiCreateChatCompletionResponse =
                        client.chat().create_byot(request).await?;

                    return Ok(rs.choices[0].message.clone());
                }
            },
        };
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
        input: &str,
        system_message: &str,
        execution_context: &mut ExecutionContext<'_, AgentEvent>,
    ) -> Result<String, OxyError> {
        let anonymized_items = HashMap::new();
        let (anonymized_system_message, anonymized_items) = match self.anonymizer {
            Some(ref anonymizer) => anonymizer.anonymize(system_message, Some(anonymized_items)),
            None => Ok((system_message.to_string(), anonymized_items)),
        }?;
        let (anonymized_user_message, anonymized_items) = match self.anonymizer {
            Some(ref anonymizer) => anonymizer.anonymize(input, Some(anonymized_items)),
            None => Ok((input.to_string(), anonymized_items)),
        }?;

        let mut messages: Vec<ChatCompletionRequestMessage> = vec![
            ChatCompletionRequestSystemMessageArgs::default()
                .name("oxy")
                .content(anonymized_system_message)
                .build()
                .map_err(|e| OxyError::RuntimeError(format!("Unable to build LLM request: {e}")))?
                .into(),
        ];

        if !input.is_empty() {
            messages.push(
                ChatCompletionRequestUserMessageArgs::default()
                    .content(anonymized_user_message)
                    .build()
                    .map_err(|e| {
                        OxyError::RuntimeError(format!("Unable to build LLM request: {e}"))
                    })?
                    .into(),
            );
        }
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
                .await?;

            output = ret_message
                .content
                .unwrap_or("Empty response from OpenAI".to_string());
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
                            OxyError::RuntimeError(format!(
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
                            OxyError::RuntimeError(format!("Unable to build LLM request: {e}"))
                        })?
                        .into(),
                );
                execution_context
                    .notify(AgentEvent::ToolCall(tool_call_ret))
                    .await?;
            }

            if tool_returns.is_empty() {
                break;
            }
            tool_calls.push(
                ChatCompletionRequestAssistantMessageArgs::default()
                    .tool_calls(tool_call_requests.clone())
                    .build()
                    .map_err(|e| {
                        OxyError::RuntimeError(format!("Unable to build LLM request: {e}"))
                    })?
                    .into(),
            );

            tries += 1;
        }

        if !tool_calls.is_empty() {
            return Err(OxyError::AgentError(
                "Failed to resolve tool calls. Max tries exceeded".to_string(),
            ));
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
impl Executable<AgentInput, AgentEvent> for OpenAIAgent {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, AgentEvent>,
        input: AgentInput,
    ) -> Result<(), OxyError> {
        execution_context.notify(AgentEvent::Started).await?;
        log::info!("AgentInput: {:?}", input);
        let system_instruction = execution_context
            .renderer
            .render_async(&self.system_instruction)
            .await?;
        let input = input.prompt.unwrap_or_default();
        let result = self
            .request(&input, &system_instruction, execution_context)
            .await?;
        let event = AgentEvent::Finished {
            output: result.clone(),
        };
        execution_context.notify(event).await?;
        execution_context.write(ContextValue::Agent(AgentOutput {
            output: Box::new(ContextValue::Text(result)),
            prompt: input,
        }));
        Ok(())
    }
}

async fn map_output(
    output: &str,
    output_format: &OutputFormat,
    file_format: &FileFormat,
) -> Result<String, OxyError> {
    match output_format {
        OutputFormat::Default => Ok(output.to_string()),
        OutputFormat::File => {
            log::info!("File path: {}", output);
            let file_output = serde_json::from_str::<FilePathOutput>(output).map_err(|e| {
                OxyError::RuntimeError(format!("Error in parsing output file: {}", e))
            })?;
            let (batches, schema) = load_result(&file_output.file_path).map_err(|e| {
                OxyError::RuntimeError(format!("Error in loading result file: {}", e))
            })?;
            let (dataset, truncated) = truncate_datasets(batches);
            match file_format {
                FileFormat::Json => {
                    let json_blob = record_batches_to_json(&dataset).map_err(|e| {
                        OxyError::RuntimeError(format!(
                            "Error in converting record batch to json: {}",
                            e
                        ))
                    })?;
                    Ok(format_table_output(&json_blob, truncated))
                }
                FileFormat::Markdown => {
                    let markdown_table =
                        record_batches_to_markdown(&dataset, &schema).map_err(|e| {
                            OxyError::RuntimeError(format!(
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
