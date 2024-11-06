use crate::{connector::load_result, yaml_parsers::agent_parser::OutputType};

use super::{toolbox::ToolBox, tools::Tool};
use arrow::util::pretty::pretty_format_batches;
use async_openai::{
    config::{OpenAIConfig, OPENAI_API_BASE},
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
use log::debug;
use schemars::{schema_for, JsonSchema};
use serde::Deserialize;
use serde_json::json;

#[async_trait]
pub trait LLMAgent {
    async fn request(&self, input: &str) -> anyhow::Result<String>;
}

pub struct OpenAIAgent<T> {
    tools: ToolBox<T>,
    client: Client<OpenAIConfig>,
    model: String,
    system_instruction: String,
    max_tries: u8,
    output_type: OutputType,
}

impl<T> OpenAIAgent<T> {
    pub fn new(
        model: String,
        api_url: Option<String>,
        api_key: String,
        tools: ToolBox<T>,
        system_instruction: String,
        output_type: OutputType,
    ) -> Self {
        let client_config = OpenAIConfig::new()
            .with_api_key(api_key)
            .with_api_base(api_url.unwrap_or(OPENAI_API_BASE.to_string()));
        let client = Client::with_config(client_config);
        let max_tries = 5;

        OpenAIAgent {
            client,
            tools,
            model,
            max_tries,
            system_instruction,
            output_type,
        }
    }

    async fn completion_request(
        &self,
        messages: Vec<ChatCompletionRequestMessage>,
        tools: Vec<ChatCompletionTool>,
        response_format: Option<ResponseFormat>,
    ) -> anyhow::Result<ChatCompletionResponseMessage> {
        let mut request_builder = CreateChatCompletionRequestArgs::default();
        request_builder
            .model(self.model.clone())
            .tools(tools)
            .parallel_tool_calls(false)
            .messages(messages);
        if response_format.is_some() {
            request_builder.response_format(response_format.unwrap());
        }

        let request = request_builder.build().unwrap();

        let response = self
            .client
            .chat() // Get the API "group" (completions, images, etc.) from the client
            .create(request) // Make the API call in that "group"
            .await?;

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
impl<T> LLMAgent for OpenAIAgent<T>
where
    T: Tool + Send + Sync,
{
    async fn request(&self, input: &str) -> anyhow::Result<String> {
        let system_message = self.system_instruction.to_string();
        debug!("System message: {}", system_message);

        let messages: Vec<ChatCompletionRequestMessage> = vec![
            ChatCompletionRequestSystemMessageArgs::default()
                .name("onyx")
                .content(system_message)
                .build()?
                .into(),
            ChatCompletionRequestUserMessageArgs::default()
                .name("Human")
                .content(input)
                .build()?
                .into(),
        ];
        let tools = self.tools.to_spec(OpenAIAgent::<T>::spec_serializer);

        let mut tries: u8 = 0;
        let mut output = "Something went wrong".to_string();
        let mut tool_returns = Vec::<ChatCompletionRequestMessage>::new();
        let mut tool_calls = Vec::<ChatCompletionRequestMessage>::new();

        while tries < self.max_tries {
            let message_with_replies =
                [messages.clone(), tool_calls.clone(), tool_returns.clone()].concat();
            tool_returns.clear();
            tool_calls.clear();
            log::debug!("Start completion request {:?}", message_with_replies);
            let response_format = match self.output_type {
                OutputType::Default => None,
                OutputType::File => {
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
                .unwrap_or("Empty response from OpenAI".to_string())
                .clone();

            let tool_call_requests = ret_message.tool_calls.unwrap_or_default();
            log::info!(
                "Number of tool calls: {} on {}",
                &tool_call_requests.len(),
                tries
            );
            for tool in tool_call_requests.clone() {
                let tool_ret: String = self
                    .tools
                    .run_tool(tool.function.name.clone(), tool.function.arguments.clone())
                    .await;
                tool_returns.push(
                    ChatCompletionRequestToolMessageArgs::default()
                        .tool_call_id(tool.id.clone())
                        .content(tool_ret)
                        .build()?
                        .into(),
                );
            }

            if tool_returns.len() == 0 {
                break;
            }
            tool_calls.push(
                ChatCompletionRequestAssistantMessageArgs::default()
                    .tool_calls(tool_call_requests.clone())
                    .build()?
                    .into(),
            );

            tries += 1;
        }
        println!("\n\x1b[1;32mInterpretation:\x1b[0m");
        println!("{}", output);
        let parsed_output = map_output(&output, &self.output_type).await?;
        return Ok(parsed_output);
    }
}

#[derive(Deserialize, Debug, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FilePathOutput {
    pub file_path: String,
}

async fn map_output(output: &str, output_type: &OutputType) -> anyhow::Result<String> {
    match output_type {
        OutputType::Default => Ok(output.to_string()),
        OutputType::File => {
            log::info!("File path: {}", output);
            let file_output = serde_json::from_str::<FilePathOutput>(output)?;
            let dataset = load_result(&file_output.file_path)?;
            let batches_display = pretty_format_batches(&dataset)?;
            println!("\n\x1b[1;32mResults:\x1b[0m");
            println!("{}", batches_display);
            Ok(batches_display.to_string())
        }
    }
}
