use std::sync::{Arc, Mutex};

use agentic_core::{HumanInputQuestion, back_target::BackTarget, human_input::SuspendedRunData};
use agentic_llm::{InitialMessages, LlmError, LlmOutput};

use agentic_core::orchestrator::RunContext;

use crate::{
    tools::{all_tools, apply_change, delete_file},
    types::{BuilderDomain, BuilderError, BuilderSolution, BuilderSpec, ToolExchange},
};

use super::solver::{BuilderSolver, dispatch_tool, make_resume_stage_data, record_tool_exchange};
use agentic_core::solver::DomainSolver;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct ProposeChangePayload {
    file_path: String,
    #[serde(default)]
    new_content: String,
    #[serde(default)]
    delete: bool,
}

/// Extract all `propose_change` payloads from the most recent assistant turn.
/// Handles all three provider message formats:
/// - Anthropic: `{"role":"assistant","content":[{"type":"tool_use",...}]}`
/// - OpenAI Responses API: `[{"type":"function_call","name":...,"arguments":...}]`
/// - OpenAI Chat Completions: `{"role":"assistant","tool_calls":[...]}`
///
/// Unlike `find_all_unmatched_tool_ids`, this does not filter by matched status.
/// `propose_change` always suspends immediately, so no call in the last turn
/// will ever have a result yet — all of them need to be applied.
fn extract_all_propose_changes(prior_messages: &[serde_json::Value]) -> Vec<ProposeChangePayload> {
    let mut result = Vec::new();

    let parse_args = |args: &str| serde_json::from_str::<ProposeChangePayload>(args).ok();

    for m in prior_messages.iter().rev() {
        // Anthropic: role:"assistant", content array with tool_use blocks.
        if m["role"].as_str() == Some("assistant") {
            if let Some(blocks) = m["content"].as_array() {
                for block in blocks {
                    if block["type"].as_str() == Some("tool_use")
                        && block["name"].as_str() == Some("propose_change")
                    {
                        if let Ok(payload) =
                            serde_json::from_value::<ProposeChangePayload>(block["input"].clone())
                        {
                            result.push(payload);
                        }
                    }
                }
            }
            // OpenAI Chat Completions: tool_calls array.
            if let Some(tool_calls) = m["tool_calls"].as_array() {
                for tc in tool_calls {
                    if tc["function"]["name"].as_str() == Some("propose_change") {
                        if let Some(p) = tc["function"]["arguments"].as_str().and_then(parse_args) {
                            result.push(p);
                        }
                    }
                }
            }
            break; // only inspect the most recent assistant turn
        }
        // OpenAI Responses API: Value::Array of flat function_call items.
        if let Some(items) = m.as_array() {
            for item in items {
                if item["type"].as_str() == Some("function_call")
                    && item["name"].as_str() == Some("propose_change")
                {
                    if let Some(p) = item["arguments"].as_str().and_then(parse_args) {
                        result.push(p);
                    }
                }
            }
            if !result.is_empty() {
                break;
            }
        }
    }
    result
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
        let (current_initial, prior_tool_exchanges) = match self.resume_data.take() {
            Some(resume) => {
                let mut prior_tool_exchanges = resume
                    .data
                    .stage_data
                    .get("tool_exchanges")
                    .cloned()
                    .and_then(|value| serde_json::from_value::<Vec<ToolExchange>>(value).ok())
                    .unwrap_or_default();
                let (initial_messages, resumed_exchanges) =
                    self.resume_initial_messages(&spec, resume).await?;
                prior_tool_exchanges.extend(resumed_exchanges);
                (initial_messages, prior_tool_exchanges)
            }
            None => (
                self.build_initial_messages(&spec.question, &spec.history),
                Vec::new(),
            ),
        };
        let exchanges = Arc::new(Mutex::new(prior_tool_exchanges));

        let test_runner = self.test_runner.clone();
        let workspace_root = self.workspace_root.clone();
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
                    let workspace_root = workspace_root.clone();
                    let event_tx = event_tx.clone();
                    let test_runner = test_runner.clone();
                    let human_input = human_input.clone();
                    let secrets_manager = secrets_manager.clone();
                    let exchanges = Arc::clone(&exchanges_for_tools);
                    Box::pin(async move {
                        let result = dispatch_tool(
                            &name,
                            &params,
                            &workspace_root,
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
    ) -> Result<(InitialMessages, Vec<ToolExchange>), (BuilderError, BackTarget<BuilderDomain>)>
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

                // IMPORTANT: if a new suspending tool is added, add a branch here to
                // apply its side effects across ALL batched calls in the last assistant
                // turn (not just the first).  The LLM can batch multiple calls of the
                // same suspending tool in one response; `build_resume_messages` already
                // generates tool_results for all unmatched IDs, but the side effects
                // (e.g. writing files for `propose_change`) must be handled here.
                // See `extract_all_propose_changes` for the pattern to follow.
                let (exchanges, resume_answer) = match prompt_type.as_deref() {
                    Some("propose_change") => {
                        let answer_lower = resume.answer.to_lowercase();
                        let accepting = answer_lower.contains("accept");

                        let all_changes = extract_all_propose_changes(&prior_messages);
                        // Fall back to `question` if prior_messages didn't yield any payloads.
                        let changes_to_apply: Vec<ProposeChangePayload> = if all_changes.is_empty()
                        {
                            serde_json::from_str::<ProposeChangePayload>(&question)
                                .ok()
                                .into_iter()
                                .collect()
                        } else {
                            all_changes
                        };

                        if accepting {
                            let spec_fallback =
                                serde_json::from_value(resume.data.stage_data["spec"].clone())
                                    .unwrap_or(BuilderSpec {
                                        question: resume.data.original_input.clone(),
                                        history: vec![],
                                    });
                            for change in &changes_to_apply {
                                let apply_result = if change.delete {
                                    delete_file(&self.workspace_root, &change.file_path).await
                                } else {
                                    apply_change(
                                        &self.workspace_root,
                                        &change.file_path,
                                        &change.new_content,
                                    )
                                    .await
                                };
                                if let Err(err) = apply_result {
                                    // TODO: files applied before this one are not rolled back.
                                    // apply_change/delete_file should be made transactional, or
                                    // the loop should collect all errors before failing.
                                    return Err((
                                        BuilderError::Resume(err),
                                        BackTarget::Solve(spec_fallback, Default::default()),
                                    ));
                                }
                            }
                        }

                        let resume_answer = if accepting {
                            let paths: Vec<&str> = changes_to_apply
                                .iter()
                                .map(|c| c.file_path.as_str())
                                .collect();
                            if paths.is_empty() {
                                "The user accepted the proposed change.".to_string()
                            } else {
                                format!(
                                    "The user accepted the proposed change{}. The {} been updated.",
                                    if paths.len() == 1 {
                                        format!(" to '{}'", paths[0])
                                    } else {
                                        format!(
                                            "s to {}",
                                            paths
                                                .iter()
                                                .map(|p| format!("'{p}'"))
                                                .collect::<Vec<_>>()
                                                .join(", ")
                                        )
                                    },
                                    if paths.len() == 1 {
                                        "file has"
                                    } else {
                                        "files have"
                                    }
                                )
                            }
                        } else {
                            "The user rejected the proposed change. Please reconsider or propose an alternative approach."
                                .to_string()
                        };

                        let tool_exchanges: Vec<ToolExchange> = changes_to_apply
                            .iter()
                            .map(|c| ToolExchange {
                                name: "propose_change".to_string(),
                                input: serde_json::to_string(c).unwrap_or_default(),
                                output: resume_answer.clone(),
                            })
                            .collect();

                        (tool_exchanges, resume_answer)
                    }
                    _ => {
                        let exchange = ToolExchange {
                            name: "ask_user".to_string(),
                            input: serde_json::json!({
                                "prompt": question,
                                "suggestions": suggestions,
                            })
                            .to_string(),
                            output: resume.answer.clone(),
                        };
                        (vec![exchange], resume.answer.clone())
                    }
                };

                Ok((
                    InitialMessages::Messages(self.client.build_resume_messages(
                        &prior_messages,
                        &question,
                        &suggestions,
                        &resume_answer,
                    )),
                    exchanges,
                ))
            }
            Some("max_tool_rounds") | Some("max_tokens") => Ok((
                InitialMessages::Messages(agentic_llm::LlmClient::build_continue_messages(
                    &prior_messages,
                )),
                vec![],
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
