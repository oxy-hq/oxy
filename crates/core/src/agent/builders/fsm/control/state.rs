use async_openai::types::{
    ChatCompletionMessageToolCall, ChatCompletionRequestAssistantMessage,
    ChatCompletionRequestAssistantMessageContent, ChatCompletionRequestMessage,
    ChatCompletionRequestToolMessage, ChatCompletionRequestToolMessageContent,
    ChatCompletionRequestUserMessage, ChatCompletionRequestUserMessageContent,
};

use crate::agent::builders::fsm::control::TransitionContext;

#[derive(Debug)]
pub struct Memory {
    transition_name: String,
    user_query: String,
    instruction: String,
    synthesized_output: String,
    plan: Option<String>,
    messages: Vec<ChatCompletionRequestMessage>,
    current_iteration: usize,
}

impl Memory {
    pub fn new(
        transition_name: String,
        instruction: String,
        user_query: String,
        history: Vec<ChatCompletionRequestMessage>,
    ) -> Self {
        let user_message = ChatCompletionRequestUserMessage {
            content: ChatCompletionRequestUserMessageContent::Text(user_query.clone()),
            ..Default::default()
        };
        Self {
            transition_name,
            plan: None,
            instruction,
            user_query,
            messages: [history, vec![user_message.into()]].concat(),
            synthesized_output: String::new(),
            current_iteration: 0,
        }
    }
}

impl TransitionContext for Memory {
    fn increase_iteration(&mut self) {
        self.current_iteration += 1;
    }

    fn max_iterations_reached(&self, max_iteration: usize) -> bool {
        self.current_iteration >= max_iteration
    }

    fn user_query(&self) -> &str {
        &self.user_query
    }

    fn transition_name(&self) -> &str {
        &self.transition_name
    }

    fn set_transition_name(&mut self, name: &str) {
        self.transition_name = name.to_string();
    }

    fn add_message(&mut self, message: String) {
        self.messages.push(
            ChatCompletionRequestAssistantMessage {
                content: Some(ChatCompletionRequestAssistantMessageContent::Text(message)),
                ..Default::default()
            }
            .into(),
        );
    }

    fn add_tool_call(
        &mut self,
        objective: &str,
        tool_call: ChatCompletionMessageToolCall,
        tool_ret: String,
    ) {
        self.messages.extend([
            ChatCompletionRequestAssistantMessage {
                content: Some(ChatCompletionRequestAssistantMessageContent::Text(
                    objective.to_string(),
                )),
                tool_calls: Some(vec![tool_call.clone()]),
                ..Default::default()
            }
            .into(),
            ChatCompletionRequestToolMessage {
                tool_call_id: tool_call.id.clone(),
                content: ChatCompletionRequestToolMessageContent::Text(format!(
                    "## Outcome:\n{tool_ret}",
                )),
            }
            .into(),
        ]);
    }

    fn get_plan(&self) -> Option<&String> {
        self.plan.as_ref()
    }

    fn set_plan(&mut self, plan: String) {
        let is_first_plan = self.plan.is_none();
        let plan = if is_first_plan {
            format!("## Initial Plan\n{plan}")
        } else {
            format!("## Revised Plan\n{plan}")
        };
        self.messages.push(
            ChatCompletionRequestAssistantMessage {
                content: Some(ChatCompletionRequestAssistantMessageContent::Text(
                    plan.to_string(),
                )),
                ..Default::default()
            }
            .into(),
        );
        self.plan = Some(plan);
    }

    fn get_messages(&self) -> Vec<ChatCompletionRequestMessage> {
        self.messages.clone()
    }

    fn get_content(&self) -> &str {
        &self.synthesized_output
    }

    fn set_content(&mut self, content: String) {
        self.synthesized_output = content;
    }
}
