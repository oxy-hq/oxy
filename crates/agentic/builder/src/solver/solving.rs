use std::sync::{Arc, Mutex};

use agentic_core::{back_target::BackTarget, human_input::SuspendedRunData, HumanInputQuestion};
use agentic_llm::{InitialMessages, LlmError, LlmOutput};

use agentic_core::orchestrator::RunContext;

use crate::{
    tools::{all_tools, apply_change, delete_file},
    types::{BuilderDomain, BuilderError, BuilderSolution, BuilderSpec, ToolExchange},
};

use super::solver::{dispatch_tool, make_resume_stage_data, record_tool_exchange, BuilderSolver};
use agentic_core::solver::DomainSolver;

#[derive(Clone, Debug, serde::Deserialize)]
struct ProposeChangePayload {
    file_path: String,
    #[serde(default)]
    new_content: String,
    #[serde(default)]
    delete: bool,
}

struct ResolvedSuspendedTool {
    exchange: ToolExchange,
    resume_answer: String,
}

/// Maximum number of transient-error retries before surfacing the failure to
/// the user.  Prevents infinite retry loops when the LLM keeps returning
/// recoverable errors.
const MAX_SOLVE_RETRIES: u32 = 3;

impl BuilderSolver {
    pub(crate) async fn solve_impl(
        &mut self,
        spec: BuilderSpec,
        run_ctx: &RunContext<BuilderDomain>,
    ) -> Result<BuilderSolution, (BuilderError, BackTarget<BuilderDomain>)> {
        let system = self.build_solving_system_prompt();
        let tools = all_tools();
        let (current_initial, _resumed_tool_exchange, prior_tool_exchanges) =
            match self.resume_data.take() {
                Some(resume) => {
                    let mut prior_tool_exchanges = resume
                        .data
                        .stage_data
                        .get("tool_exchanges")
                        .cloned()
                        .and_then(|value| serde_json::from_value::<Vec<ToolExchange>>(value).ok())
                        .unwrap_or_default();
                    let (initial_messages, resumed_tool_exchange) =
                        self.resume_initial_messages(&spec, resume).await?;
                    if let Some(exchange) = resumed_tool_exchange.as_ref() {
                        prior_tool_exchanges.push(exchange.clone());
                    }
                    (
                        initial_messages,
                        resumed_tool_exchange,
                        prior_tool_exchanges,
                    )
                }
                None => (
                    self.build_initial_messages(&spec.question, &spec.history),
                    None,
                    Vec::new(),
                ),
            };
        let exchanges = Arc::new(Mutex::new(prior_tool_exchanges));

        let test_runner = self.test_runner.clone();
        let project_root = self.project_root.clone();
        let event_tx = self.event_tx.clone();
        let human_input = self.human_input.clone();
        let secrets_manager = self.secrets_manager.clone();
        let exchanges_for_tools = Arc::clone(&exchanges);

        let result = self
            .client
            .run_with_tools(
                &system,
                current_initial,
                &tools,
                move |name, params| {
                    let project_root = project_root.clone();
                    let event_tx = event_tx.clone();
                    let test_runner = test_runner.clone();
                    let human_input = human_input.clone();
                    let secrets_manager = secrets_manager.clone();
                    let exchanges = Arc::clone(&exchanges_for_tools);
                    Box::pin(async move {
                        let result = dispatch_tool(
                            &name,
                            &params,
                            &project_root,
                            &event_tx,
                            test_runner,
                            human_input,
                            secrets_manager.as_ref(),
                        )
                        .await;
                        let mut guard = exchanges.lock().unwrap_or_else(|e| e.into_inner());
                        record_tool_exchange(&mut guard, &name, &params, &result);
                        result
                    })
                },
                &self.event_tx,
                BuilderSolver::solving_loop_config(),
            )
            .await;

        match result {
            Ok(output) => Ok(self.to_solution(spec, output, exchanges)),
            Err(LlmError::Suspended {
                prompt,
                suggestions,
                prior_messages,
            }) => {
                let tool_exchanges = exchanges.lock().unwrap_or_else(|e| e.into_inner()).clone();
                <BuilderSolver as DomainSolver<BuilderDomain>>::store_suspension_data(
                    self,
                    SuspendedRunData {
                        from_state: "solving".to_string(),
                        original_input: spec.question.clone(),
                        trace_id: String::new(),
                        stage_data: make_resume_stage_data(
                            &spec,
                            &prior_messages,
                            "tool_suspended",
                            &prompt,
                            &suggestions,
                            &tool_exchanges,
                        ),
                        question: prompt.clone(),
                        suggestions: suggestions.clone(),
                    },
                );
                Err((
                    BuilderError::NeedsUserInput {
                        prompt: prompt.clone(),
                    },
                    BackTarget::Suspend {
                        questions: vec![HumanInputQuestion {
                            prompt,
                            suggestions,
                        }],
                    },
                ))
            }
            Err(LlmError::MaxToolRoundsReached {
                rounds,
                prior_messages,
            }) => {
                let tool_exchanges = exchanges.lock().unwrap_or_else(|e| e.into_inner()).clone();
                let prompt = format!(
                    "The builder used all {rounds} allotted tool rounds. Continue with more rounds?"
                );
                let suggestions = vec!["Continue".to_string()];
                <BuilderSolver as DomainSolver<BuilderDomain>>::store_suspension_data(
                    self,
                    SuspendedRunData {
                        from_state: "solving".to_string(),
                        original_input: spec.question.clone(),
                        trace_id: String::new(),
                        stage_data: make_resume_stage_data(
                            &spec,
                            &prior_messages,
                            "max_tool_rounds",
                            &prompt,
                            &suggestions,
                            &tool_exchanges,
                        ),
                        question: prompt.clone(),
                        suggestions: suggestions.clone(),
                    },
                );
                Err((
                    BuilderError::NeedsUserInput {
                        prompt: prompt.clone(),
                    },
                    BackTarget::Suspend {
                        questions: vec![HumanInputQuestion {
                            prompt,
                            suggestions,
                        }],
                    },
                ))
            }
            Err(LlmError::MaxTokensReached { prior_messages, .. }) => {
                let tool_exchanges = exchanges.lock().unwrap_or_else(|e| e.into_inner()).clone();
                let prompt =
                    "The builder reached its token limit. Continue from the current progress?"
                        .to_string();
                let suggestions = vec!["Continue".to_string()];
                <BuilderSolver as DomainSolver<BuilderDomain>>::store_suspension_data(
                    self,
                    SuspendedRunData {
                        from_state: "solving".to_string(),
                        original_input: spec.question.clone(),
                        trace_id: String::new(),
                        stage_data: make_resume_stage_data(
                            &spec,
                            &prior_messages,
                            "max_tokens",
                            &prompt,
                            &suggestions,
                            &tool_exchanges,
                        ),
                        question: prompt.clone(),
                        suggestions: suggestions.clone(),
                    },
                );
                Err((
                    BuilderError::NeedsUserInput {
                        prompt: prompt.clone(),
                    },
                    BackTarget::Suspend {
                        questions: vec![HumanInputQuestion {
                            prompt,
                            suggestions,
                        }],
                    },
                ))
            }
            Err(ref err @ (LlmError::Http(_) | LlmError::Auth(_))) => {
                // Non-transient errors — retrying won't help.  Surface the
                // error to the user via Suspend so the pipeline stops.
                let msg = err.to_string();
                tracing::error!("builder solving fatal LLM error: {msg}");
                <BuilderSolver as DomainSolver<BuilderDomain>>::store_suspension_data(
                    self,
                    SuspendedRunData {
                        from_state: "solving".to_string(),
                        original_input: spec.question.clone(),
                        trace_id: String::new(),
                        stage_data: Default::default(),
                        question: msg.clone(),
                        suggestions: vec![],
                    },
                );
                Err((
                    BuilderError::Llm(msg.clone()),
                    BackTarget::Suspend {
                        questions: vec![HumanInputQuestion {
                            prompt: format!("Builder encountered a fatal error: {msg}"),
                            suggestions: vec![],
                        }],
                    },
                ))
            }
            Err(err) => {
                let current_attempt = run_ctx.retry_ctx.as_ref().map_or(0, |ctx| ctx.attempt);
                if current_attempt >= MAX_SOLVE_RETRIES {
                    let msg = format!("Builder failed after {} retries: {err}", current_attempt);
                    tracing::error!("{msg}");
                    <BuilderSolver as DomainSolver<BuilderDomain>>::store_suspension_data(
                        self,
                        SuspendedRunData {
                            from_state: "solving".to_string(),
                            original_input: spec.question.clone(),
                            trace_id: String::new(),
                            stage_data: Default::default(),
                            question: msg.clone(),
                            suggestions: vec![],
                        },
                    );
                    Err((
                        BuilderError::Llm(msg.clone()),
                        BackTarget::Suspend {
                            questions: vec![HumanInputQuestion {
                                prompt: msg,
                                suggestions: vec![],
                            }],
                        },
                    ))
                } else {
                    tracing::warn!(
                        "builder solving LLM error (attempt {}, will retry): {err}",
                        current_attempt + 1
                    );
                    let retry_ctx = run_ctx
                        .retry_ctx
                        .clone()
                        .unwrap_or_default()
                        .advance(err.to_string());
                    Err((
                        BuilderError::Llm(err.to_string()),
                        BackTarget::Solve(spec, retry_ctx),
                    ))
                }
            }
        }
    }

    fn to_solution(
        &self,
        spec: BuilderSpec,
        output: LlmOutput,
        exchanges: Arc<Mutex<Vec<ToolExchange>>>,
    ) -> BuilderSolution {
        let tool_exchanges = exchanges.lock().unwrap_or_else(|e| e.into_inner()).clone();
        BuilderSolution {
            question: spec.question,
            history: spec.history,
            draft_text: output.text,
            tool_exchanges,
        }
    }

    async fn resume_initial_messages(
        &self,
        _spec: &BuilderSpec,
        resume: agentic_core::human_input::ResumeInput,
    ) -> Result<(InitialMessages, Option<ToolExchange>), (BuilderError, BackTarget<BuilderDomain>)>
    {
        let prior_messages = resume
            .data
            .stage_data
            .get("prior_messages")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let question = resume.data.question.clone();
        let suggestions = resume.data.suggestions.clone();

        match resume
            .data
            .stage_data
            .get("suspension_type")
            .and_then(|v| v.as_str())
        {
            Some("tool_suspended") => {
                // Determine the suspension sub-type from the prompt JSON.
                let prompt_type = serde_json::from_str::<serde_json::Value>(&question)
                    .ok()
                    .and_then(|v| v["type"].as_str().map(String::from));

                let resolved_tool = match prompt_type.as_deref() {
                    Some("propose_change") => {
                        let change: Option<ProposeChangePayload> =
                            serde_json::from_str(&question).ok();
                        let answer_lower = resume.answer.to_lowercase();
                        if answer_lower.contains("accept") {
                            if let Some(change) = change.as_ref() {
                                let apply_result = if change.delete {
                                    delete_file(&self.project_root, &change.file_path).await
                                } else {
                                    apply_change(
                                        &self.project_root,
                                        &change.file_path,
                                        &change.new_content,
                                    )
                                    .await
                                };
                                if let Err(err) = apply_result {
                                    return Err((
                                        BuilderError::Resume(err),
                                        BackTarget::Solve(
                                            serde_json::from_value(
                                                resume.data.stage_data["spec"].clone(),
                                            )
                                            .unwrap_or(BuilderSpec {
                                                question: resume.data.original_input.clone(),
                                                history: vec![],
                                            }),
                                            Default::default(),
                                        ),
                                    ));
                                }
                            }
                        }

                        let resume_answer = if answer_lower.contains("accept") {
                            format!(
                                "The user accepted the proposed change{}. The file has been updated.",
                                change
                                    .as_ref()
                                    .map(|c| format!(" to '{}'", c.file_path))
                                    .unwrap_or_default()
                            )
                        } else {
                            "The user rejected the proposed change. Please reconsider or propose an alternative approach."
                                .to_string()
                        };

                        Some(ResolvedSuspendedTool {
                            exchange: ToolExchange {
                                name: "propose_change".to_string(),
                                input: question.clone(),
                                output: resume_answer.clone(),
                            },
                            resume_answer,
                        })
                    }
                    _ => Some(ResolvedSuspendedTool {
                        exchange: ToolExchange {
                            name: "ask_user".to_string(),
                            input: serde_json::json!({
                                "prompt": question,
                                "suggestions": suggestions,
                            })
                            .to_string(),
                            output: resume.answer.clone(),
                        },
                        resume_answer: resume.answer.clone(),
                    }),
                };

                let resume_answer = resolved_tool
                    .as_ref()
                    .map(|tool| tool.resume_answer.as_str())
                    .unwrap_or(resume.answer.as_str());

                Ok((
                    InitialMessages::Messages(self.client.build_resume_messages(
                        &prior_messages,
                        &question,
                        &suggestions,
                        resume_answer,
                    )),
                    resolved_tool.map(|tool| tool.exchange),
                ))
            }
            Some("max_tool_rounds") | Some("max_tokens") => Ok((
                InitialMessages::Messages(agentic_llm::LlmClient::build_continue_messages(
                    &prior_messages,
                )),
                None,
            )),
            other => Err((
                BuilderError::Resume(format!(
                    "unsupported solving resume type: {}",
                    other.unwrap_or("unknown")
                )),
                BackTarget::Solve(
                    serde_json::from_value(resume.data.stage_data["spec"].clone()).unwrap_or(
                        BuilderSpec {
                            question: resume.data.original_input.clone(),
                            history: vec![],
                        },
                    ),
                    Default::default(),
                ),
            )),
        }
    }
}
