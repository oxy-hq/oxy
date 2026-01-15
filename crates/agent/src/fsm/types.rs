use async_openai::types::chat::{
    ChatCompletionMessageToolCall, ChatCompletionMessageToolCalls,
    ChatCompletionRequestAssistantMessage, ChatCompletionRequestAssistantMessageContent,
    ChatCompletionRequestMessage, ChatCompletionRequestToolMessage,
    ChatCompletionRequestToolMessageContent, ChatCompletionRequestUserMessage,
    ChatCompletionRequestUserMessageContent, FunctionCall,
};

use oxy::config::model::AppConfig;
use oxy::execute::types::{Table, VizParams};

#[derive(Clone, Debug)]
pub struct ToolReq {
    call_id: String,
    tool_name: String,
    args: String,
}

impl ToolReq {
    pub fn new(call_id: String, tool_name: String, args: String) -> Self {
        Self {
            call_id,
            tool_name,
            args,
        }
    }

    pub fn call_id(&self) -> &str {
        &self.call_id
    }
}

impl From<ToolReq> for ChatCompletionMessageToolCall {
    fn from(val: ToolReq) -> Self {
        ChatCompletionMessageToolCall {
            id: val.call_id,
            function: FunctionCall {
                name: val.tool_name,
                arguments: val.args,
            },
        }
    }
}

impl From<ChatCompletionMessageToolCall> for ToolReq {
    fn from(call: ChatCompletionMessageToolCall) -> Self {
        Self {
            call_id: call.id,
            tool_name: call.function.name,
            args: call.function.arguments,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ToolRes {
    call_id: String,
    result: String,
}

impl ToolRes {
    pub fn new(call_id: String, result: String) -> Self {
        Self { call_id, result }
    }
}

#[derive(Clone, Debug)]
pub enum Message {
    User { content: String },
    Assistant { content: String },
    Thinking { content: String },
    Planning { content: String },
    ToolReq(ToolReq),
    ToolRes(ToolRes),
}

impl From<ChatCompletionRequestMessage> for Message {
    fn from(msg: ChatCompletionRequestMessage) -> Self {
        match msg {
            ChatCompletionRequestMessage::User(user_msg) => Message::User {
                content: match user_msg.content {
                    ChatCompletionRequestUserMessageContent::Text(text) => text,
                    _ => String::new(),
                },
            },
            ChatCompletionRequestMessage::Assistant(assistant_msg) => Message::Assistant {
                content: match assistant_msg.content {
                    Some(ChatCompletionRequestAssistantMessageContent::Text(text)) => text,
                    _ => String::new(),
                },
            },
            _ => unimplemented!("Conversion for this message type is not implemented"),
        }
    }
}

impl From<Message> for ChatCompletionRequestMessage {
    fn from(val: Message) -> Self {
        match val {
            Message::User { content } => {
                ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
                    content: ChatCompletionRequestUserMessageContent::Text(content),
                    ..Default::default()
                })
            }
            Message::Assistant { content }
            | Message::Thinking { content }
            | Message::Planning { content } => {
                ChatCompletionRequestMessage::Assistant(ChatCompletionRequestAssistantMessage {
                    content: Some(ChatCompletionRequestAssistantMessageContent::Text(content)),
                    ..Default::default()
                })
            }
            Message::ToolReq(ToolReq {
                call_id,
                tool_name,
                args,
            }) => ChatCompletionRequestMessage::Assistant(ChatCompletionRequestAssistantMessage {
                content: None,
                tool_calls: Some(vec![ChatCompletionMessageToolCalls::Function(
                    ChatCompletionMessageToolCall {
                        id: call_id,
                        function: FunctionCall {
                            name: tool_name,
                            arguments: args,
                        },
                    },
                )]),
                ..Default::default()
            }),
            Message::ToolRes(ToolRes { call_id, result }) => {
                ChatCompletionRequestMessage::Tool(ChatCompletionRequestToolMessage {
                    tool_call_id: call_id,
                    content: ChatCompletionRequestToolMessageContent::Text(result),
                    ..Default::default()
                })
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum Artifact {
    Viz {
        viz_name: String,
        description: String,
        params: VizParams,
    },
    Table {
        table_name: String,
        description: String,
        table: Table,
    },
    Insight {
        content: String,
    },
    DataApp {
        app_name: String,
        description: String,
        app_config: AppConfig,
    },
}

impl Artifact {
    pub fn describe(&self) -> String {
        match self {
            Artifact::Viz {
                viz_name,
                params,
                description,
            } => serde_json::json!({
                "type": "viz",
                "params": params,
                "description": description,
                "name": viz_name,
            })
            .to_string(),
            Artifact::Table { table, .. } => table.summary(),
            Artifact::DataApp {
                app_name,
                description: _,
                app_config,
            } => serde_json::json!({
                "type": "data_app",
                "name": app_name,
                "components": app_config.display.iter()
                    .filter_map(|p| serde_json::to_value(p).ok())
                    .collect::<Vec<_>>(),
            })
            .to_string(),
            Artifact::Insight { content } => serde_json::json!({
                "type": "insight",
                "content": content,
            })
            .to_string(),
        }
    }
}
