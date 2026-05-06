use agentic_core::back_target::BackTarget;
use agentic_core::tools::ToolError;
use agentic_llm::{InitialMessages, ThinkingConfig, ToolLoopConfig};
use serde_json::json;

use crate::schema_provider::EmptySchemaProvider;
use crate::tools::all_tools;
use crate::types::{BuilderAnswer, BuilderDomain, BuilderError, BuilderResult};

use super::solver::BuilderSolver;

impl BuilderSolver {
    pub(crate) async fn interpret_impl(
        &mut self,
        result: BuilderResult,
    ) -> Result<BuilderAnswer, (BuilderError, BackTarget<BuilderDomain>)> {
        let (initial_messages, tools) = if !result.prior_messages.is_empty() {
            // Pass the full native conversation (tool_use + tool_result blocks) so
            // the interpreter sees everything that happened without any summarisation.
            // We append a final user turn asking for the synthesis, and include the
            // solving tool definitions so the provider accepts the prior tool_use blocks.
            let mut msgs = result.prior_messages.clone();
            msgs.push(json!({
                "role": "user",
                "content": format!(
                    "The solving phase is complete. Write the final user-facing reply for this request: \"{}\"",
                    result.question
                )
            }));
            let schema = EmptySchemaProvider;
            let tools = all_tools(&schema);
            (InitialMessages::Messages(msgs), tools)
        } else {
            // Fallback for runs that predate prior_messages (e.g. resumed from old suspension).
            let tool_summary = if result.tool_exchanges.is_empty() {
                "No tools were used in the solving phase.".to_string()
            } else {
                result
                    .tool_exchanges
                    .iter()
                    .map(|exchange| {
                        format!(
                            "- {}({}) => {}",
                            exchange.name, exchange.input, exchange.output
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            let user_prompt = if result.tool_exchanges.is_empty() && !result.draft_text.is_empty() {
                format!(
                    "User request:\n{}\n\nSolving phase response:\n{}\n\nWrite the final user-facing reply based on the solving phase response above.",
                    result.question, result.draft_text
                )
            } else {
                format!(
                    "User request:\n{}\n\nTool exchanges:\n{}\n\nWrite the final user-facing reply based on the tool exchanges above.",
                    result.question, tool_summary
                )
            };
            (InitialMessages::User(user_prompt), Vec::new())
        };

        match self
            .client
            .run_with_tools(
                &self.build_interpreting_system_prompt(),
                initial_messages,
                &tools,
                |_name, _params| {
                    Box::pin(async move {
                        Err::<Box<dyn agentic_core::tools::ToolOutput>, ToolError>(
                            ToolError::UnknownTool("interpreting has no tools".to_string()),
                        )
                    })
                },
                &self.event_tx,
                ToolLoopConfig {
                    max_tool_rounds: 1,
                    state: "interpreting".to_string(),
                    thinking: ThinkingConfig::Disabled,
                    response_schema: None,
                    max_tokens_override: None,
                    sub_spec_index: None,
                    system_date_hint: Some(BuilderSolver::current_date_hint()),
                },
            )
            .await
        {
            Ok(output) => Ok(BuilderAnswer {
                text: output.text,
                tool_exchanges: result.tool_exchanges,
            }),
            Err(err) => Err((
                BuilderError::Llm(err.to_string()),
                BackTarget::Interpret(result, Default::default()),
            )),
        }
    }
}
