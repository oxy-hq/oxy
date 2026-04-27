use agentic_core::back_target::BackTarget;
use agentic_core::tools::ToolError;
use agentic_llm::{InitialMessages, ThinkingConfig, ToolLoopConfig};

use crate::types::{BuilderAnswer, BuilderDomain, BuilderError, BuilderResult};

use super::solver::BuilderSolver;

impl BuilderSolver {
    pub(crate) async fn interpret_impl(
        &mut self,
        result: BuilderResult,
    ) -> Result<BuilderAnswer, (BuilderError, BackTarget<BuilderDomain>)> {
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

        let user_prompt = format!(
            "User request:\n{}\n\nTool exchanges:\n{}\n\nWrite the final user-facing reply based on the tool exchanges above.",
            result.question, tool_summary
        );

        let tools = Vec::new();
        match self
            .client
            .run_with_tools(
                &self.build_interpreting_system_prompt(),
                InitialMessages::User(user_prompt),
                &tools,
                |_name, _params| {
                    Box::pin(async move {
                        Err::<serde_json::Value, ToolError>(ToolError::UnknownTool(
                            "interpreting has no tools".to_string(),
                        ))
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
