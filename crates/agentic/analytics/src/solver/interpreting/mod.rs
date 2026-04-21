//! **Interpreting** pipeline stage.
//!
//! Owns:
//! - [`build_interpret_user_prompt`] — user-message builder
//! - [`AnalyticsSolver::interpret_impl`] — core LLM call
//! - [`build_interpreting_handler`] — `StateHandler` factory

use std::sync::{Arc, Mutex};

use agentic_core::{
    HumanInputQuestion, SuspendReason,
    back_target::BackTarget,
    human_input::SuspendedRunData,
    orchestrator::{CompletedTurn, RunContext, SessionMemory, StateHandler, TransitionResult},
    solver::DomainSolver,
    state::ProblemState,
};

use crate::llm::{InitialMessages, ThinkingConfig, ToolLoopConfig};
use crate::tools::{execute_interpreting_tool, interpreting_tools, suggest_chart_config};
use crate::types::{ConversationTurn, DisplayBlock, QuestionType};
use crate::{AnalyticsAnswer, AnalyticsDomain, AnalyticsError, AnalyticsResult};

use super::{
    AnalyticsSolver,
    prompts::{INTERPRET_SYSTEM_PROMPT, MULTI_RESULT_INTERPRET_ADDON},
};

mod prompts;
pub(super) use prompts::build_interpret_user_prompt;
use prompts::{cell_to_json, format_delegation_data, parse_delegation_result_sets};

impl AnalyticsSolver {
    /// Core interpret logic, shared by the trait impl and the interpreting handler.
    ///
    /// `raw_question` and `history` come from the spec's intent.  The trait's
    /// `interpret` method passes `""` / `&[]` / `None` (no run_ctx available);
    /// the custom interpreting handler supplies real values from `run_ctx.spec`.
    /// `session_turns` carries prior completed turns for comparative framing.
    /// `question_type` drives the deterministic chart suggestion.
    #[tracing::instrument(
        skip_all,
        fields(
            oxy.name = "analytics.interpret",
            oxy.span_type = "analytics",
            result_count = tracing::field::Empty,
        )
    )]
    pub(crate) async fn interpret_impl(
        &mut self,
        raw_question: &str,
        history: &[ConversationTurn],
        result: AnalyticsResult,
        session_turns: &[CompletedTurn<AnalyticsDomain>],
        question_type: Option<&QuestionType>,
    ) -> Result<AnalyticsAnswer, (AnalyticsError, BackTarget<AnalyticsDomain>)> {
        tracing::Span::current().record("result_count", result.results.len());
        // Pre-convert every result set's rows to JSON for the tool closure.
        let fresh_result_sets: Vec<(Vec<String>, Vec<Vec<serde_json::Value>>)> = result
            .results
            .iter()
            .map(|rs| {
                let columns = rs.data.columns.clone();
                let rows = rs
                    .data
                    .rows
                    .iter()
                    .map(|row| row.0.iter().map(cell_to_json).collect())
                    .collect();
                (columns, rows)
            })
            .collect();

        let system_base = self.build_system_prompt("interpreting", INTERPRET_SYSTEM_PROMPT, None);
        let system_prompt = if result.is_multi() {
            format!("{system_base}{MULTI_RESULT_INTERPRET_ADDON}")
        } else {
            system_base
        };
        let thinking = self.thinking_for_state("interpreting", ThinkingConfig::Disabled);
        let max_rounds_base = self.max_tool_rounds_for_state("interpreting", 2);

        // Check for a resume from a prior max_tool_rounds suspension.
        let mut resume_extra_rounds: u32 = 0;
        let has_resume = self.resume_data.is_some();
        tracing::info!(
            target: "coordinator",
            has_resume,
            from_state = self.resume_data.as_ref().map(|r| r.data.from_state.as_str()).unwrap_or("none"),
            answer_len = self.resume_data.as_ref().map(|r| r.answer.len()).unwrap_or(0),
            "interpret_impl: checking resume_data"
        );
        let (initial, result_sets) = if let Some(resume) = self.resume_data.take() {
            let prior: Vec<serde_json::Value> = resume.data.stage_data["conversation_history"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            match resume.data.stage_data["suspension_type"].as_str() {
                Some("max_tool_rounds") => {
                    resume_extra_rounds =
                        resume.data.stage_data["extra_rounds"].as_u64().unwrap_or(0) as u32;
                    // Restore result_sets from stage_data so the render_chart
                    // tool closure can still validate column names on resume.
                    let stored = resume.data.stage_data["result_sets"]
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| {
                                    let cols: Vec<String> = v["columns"]
                                        .as_array()?
                                        .iter()
                                        .filter_map(|c| c.as_str().map(str::to_string))
                                        .collect();
                                    let rows: Vec<Vec<serde_json::Value>> = v["rows"]
                                        .as_array()?
                                        .iter()
                                        .filter_map(|r| r.as_array().cloned())
                                        .collect();
                                    Some((cols, rows))
                                })
                                .collect()
                        })
                        .unwrap_or_else(|| fresh_result_sets.clone());
                    (
                        InitialMessages::Messages(crate::llm::LlmClient::build_continue_messages(
                            &prior,
                        )),
                        stored,
                    )
                }
                _ => {
                    // For delegation resumes (from_state == "executing"),
                    // the answer contains the workflow output as JSON.
                    // Parse it into result_sets and build a custom prompt
                    // so the LLM sees real data instead of the placeholder.
                    if resume.data.from_state == "executing" {
                        tracing::info!(
                            target: "coordinator",
                            answer_preview = &resume.answer[..resume.answer.len().min(200)],
                            "interpret_impl: executing delegation resume, parsing answer"
                        );
                        if let Some(result_sets) = parse_delegation_result_sets(&resume.answer) {
                            let data_section = format_delegation_data(&result_sets);
                            let user_prompt = format!(
                                "## User question\n{raw_question}\n\n\
                                 ## Procedure output\n{data_section}\n\n\
                                 Analyze these results and provide a clear, \
                                 data-driven answer to the user's question."
                            );
                            (InitialMessages::User(user_prompt), result_sets)
                        } else {
                            // Fallback if answer isn't valid JSON — use raw text.
                            let user_prompt = format!(
                                "## User question\n{raw_question}\n\n\
                                 ## Procedure output\n{}\n\n\
                                 Analyze these results and provide a clear, \
                                 data-driven answer.",
                                resume.answer
                            );
                            (InitialMessages::User(user_prompt), fresh_result_sets)
                        }
                    } else {
                        let suggested_config = question_type.and_then(|qt| {
                            fresh_result_sets
                                .first()
                                .and_then(|(cols, _)| suggest_chart_config(qt, cols))
                        });
                        let user_prompt = build_interpret_user_prompt(
                            raw_question,
                            history,
                            &result,
                            None,
                            session_turns,
                            suggested_config.as_ref(),
                        );
                        (InitialMessages::User(user_prompt), fresh_result_sets)
                    }
                }
            }
        } else {
            // Deterministic chart suggestion derived from the primary result set.
            let suggested_config = question_type.and_then(|qt| {
                fresh_result_sets
                    .first()
                    .and_then(|(cols, _)| suggest_chart_config(qt, cols))
            });
            let user_prompt = build_interpret_user_prompt(
                raw_question,
                history,
                &result,
                None,
                session_turns,
                suggested_config.as_ref(),
            );
            (InitialMessages::User(user_prompt), fresh_result_sets)
        };

        let max_rounds = max_rounds_base + resume_extra_rounds;
        let tools = AnalyticsSolver::tools_for_state_interpreting();

        // Shared collector: the tool closure appends a DisplayBlock for every
        // render_chart call that passes immediate validation.
        let valid_charts: Arc<Mutex<Vec<DisplayBlock>>> = Arc::new(Mutex::new(Vec::new()));

        let event_tx = self.event_tx.clone();
        let result_sets_for_tool = result_sets.clone();
        let valid_charts_for_tool = Arc::clone(&valid_charts);

        let output = match self
            .client_for_state("interpreting")
            .run_with_tools(
                &system_prompt,
                initial,
                &tools,
                move |name: String, params| {
                    let tx = event_tx.clone();
                    let sets = result_sets_for_tool.clone();
                    let charts = valid_charts_for_tool.clone();
                    Box::pin(async move {
                        execute_interpreting_tool(&name, params, &tx, &sets, &charts).await
                    })
                },
                &self.event_tx,
                ToolLoopConfig {
                    max_tool_rounds: max_rounds,
                    state: "interpreting".into(),
                    thinking,
                    response_schema: None,
                    max_tokens_override: self.max_tokens,
                    sub_spec_index: None,
                },
            )
            .await
        {
            Ok(v) => v,
            Err(crate::llm::LlmError::MaxToolRoundsReached {
                rounds,
                prior_messages,
            }) => {
                let prompt = format!(
                    "The agent used all {rounds} allotted tool rounds. \
                     Continue with more rounds?"
                );
                // Serialize result_sets so the tool closure can be reconstructed
                // on resume without re-running the SQL query.
                let result_sets_json: Vec<serde_json::Value> = result_sets
                    .iter()
                    .map(|(cols, rows)| serde_json::json!({ "columns": cols, "rows": rows }))
                    .collect();
                self.store_suspension_data(SuspendedRunData {
                    from_state: "interpreting".to_string(),
                    original_input: raw_question.to_string(),
                    trace_id: String::new(),
                    stage_data: serde_json::json!({
                        "conversation_history": prior_messages,
                        "suspension_type": "max_tool_rounds",
                        "extra_rounds": rounds,
                        "result_sets": result_sets_json,
                    }),
                    question: prompt.clone(),
                    suggestions: vec!["Continue".to_string()],
                });
                return Err((
                    AnalyticsError::NeedsUserInput {
                        prompt: prompt.clone(),
                    },
                    BackTarget::Suspend {
                        reason: SuspendReason::HumanInput {
                            questions: vec![HumanInputQuestion {
                                prompt,
                                suggestions: vec!["Continue".to_string()],
                            }],
                        },
                    },
                ));
            }
            Err(e) => {
                let msg = format!("LLM call failed during interpret: {e}");
                return Err((
                    AnalyticsError::NeedsUserInput { prompt: msg },
                    BackTarget::Interpret(result, Default::default()),
                ));
            }
        };

        // Charts were validated and collected by the tool closure.
        // No second-pass batch validation needed.
        let display_blocks = Arc::try_unwrap(valid_charts)
            .map(|m| m.into_inner().unwrap_or_default())
            .unwrap_or_default();

        Ok(AnalyticsAnswer {
            text: output.text,
            display_blocks,
            spec_hint: None, // Set by the handler from run_ctx.spec
        })
    }

    /// Returns the tool list for the interpreting state.
    ///
    /// Extracted here so `build_interpreting_handler` can call it without going
    /// through the trait's `tools_for_state` dispatch.
    pub(super) fn tools_for_state_interpreting() -> Vec<agentic_core::tools::ToolDef> {
        interpreting_tools()
    }
}

// ---------------------------------------------------------------------------
// State handler
// ---------------------------------------------------------------------------

/// Build the `StateHandler` for the **interpreting** state.
pub(super) fn build_interpreting_handler()
-> StateHandler<AnalyticsDomain, AnalyticsSolver, crate::AnalyticsEvent> {
    StateHandler {
        next: "done",
        execute: Arc::new(
            |solver: &mut AnalyticsSolver,
             state,
             _events,
             run_ctx: &RunContext<AnalyticsDomain>,
             memory: &SessionMemory<AnalyticsDomain>| {
                Box::pin(async move {
                    let result = match state {
                        ProblemState::Interpreting(r) => r,
                        _ => unreachable!("interpreting handler called with wrong state"),
                    };
                    let (raw_question, history, question_type) = run_ctx
                        .spec
                        .as_ref()
                        .map(|s| {
                            (
                                s.intent.raw_question.clone(),
                                s.intent.history.clone(),
                                Some(s.intent.question_type.clone()),
                            )
                        })
                        .or_else(|| {
                            // Fan-out path: run_ctx.spec is None (we jumped directly from
                            // Specifying to Interpreting).  Fall back to run_ctx.intent.
                            run_ctx.intent.as_ref().map(|i| {
                                (
                                    i.raw_question.clone(),
                                    i.history.clone(),
                                    Some(i.question_type.clone()),
                                )
                            })
                        })
                        .unwrap_or_default();
                    // Extract the query_request_item from the spec for
                    // cross-turn continuity.
                    let spec_hint = run_ctx
                        .spec
                        .as_ref()
                        .and_then(|s| s.query_request_item.clone());

                    match solver
                        .interpret_impl(
                            &raw_question,
                            &history,
                            result,
                            memory.turns(),
                            question_type.as_ref(),
                        )
                        .await
                    {
                        Ok(mut answer) => {
                            answer.spec_hint = spec_hint;
                            TransitionResult::ok(ProblemState::Done(answer))
                        }
                        Err((err, back)) => {
                            TransitionResult::diagnosing(ProblemState::Diagnosing {
                                error: err,
                                back,
                            })
                        }
                    }
                })
            },
        ),
        diagnose: None,
    }
}
