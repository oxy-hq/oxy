use super::{prompt::PromptBuilder, toolbox::ToolBox};
use crate::yaml_parsers::config_parser::ParsedConfig;
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage,
        ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestToolMessageArgs,
        ChatCompletionRequestUserMessageArgs, ChatCompletionResponseMessage, ChatCompletionTool,
        ChatCompletionToolArgs, ChatCompletionToolType, CreateChatCompletionRequestArgs,
        FunctionObjectArgs,
    },
    Client,
};
use async_trait::async_trait;
use log::debug;
use std::{env, error::Error};

#[async_trait]
pub trait LLMAgent {
    async fn request(&self, input: &str) -> Result<String, Box<dyn Error>>;
}

pub struct OpenAIAgent {
    tools: ToolBox,
    prompt_builder: PromptBuilder,
    client: Client<OpenAIConfig>,
    model: String,
    max_tries: u8,
}

impl OpenAIAgent {
    pub fn new(config: ParsedConfig, tools: ToolBox, prompt_builder: PromptBuilder) -> Self {
        let model = config.model.model_ref.clone();
        let api_key = env::var(&config.model.key_var).expect("Environment variable not found");
        let client_config = OpenAIConfig::new().with_api_key(api_key);
        let client = Client::with_config(client_config);
        let max_tries = 5;

        OpenAIAgent {
            client,
            tools,
            prompt_builder,
            model,
            max_tries,
        }
    }

    async fn completion_request(
        &self,
        messages: Vec<ChatCompletionRequestMessage>,
        tools: Vec<ChatCompletionTool>,
    ) -> Result<ChatCompletionResponseMessage, Box<dyn Error>> {
        let request = CreateChatCompletionRequestArgs::default()
            .model(self.model.clone())
            .tools(tools)
            .messages(messages)
            .build()
            .unwrap();

        let response = self
            .client
            .chat() // Get the API "group" (completions, images, etc.) from the client
            .create(request) // Make the API call in that "group"
            .await?;

        Ok(response.choices[0].message.clone())
    }
}

#[async_trait]
impl LLMAgent for OpenAIAgent {
    async fn request(&self, input: &str) -> Result<String, Box<dyn Error>> {
        let system_message = self.prompt_builder.system();
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
        let tools = self.tools.to_spec(spec_serializer);

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
            let ret_message = self
                .completion_request(message_with_replies, tools.clone())
                .await?;
            output = ret_message
                .content
                .unwrap_or("Empty response from OpenAI".to_string())
                .clone();

            let tool_call_requests = ret_message.tool_calls.unwrap_or_default();
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
        return Ok(output);
    }
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
