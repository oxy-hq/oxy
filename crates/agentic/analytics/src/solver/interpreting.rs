//! **Interpreting** pipeline stage.
//!
//! Owns:
//! - [`build_interpret_user_prompt`] — user-message builder
//! - [`AnalyticsSolver::interpret_impl`] — core LLM call
//! - [`build_interpreting_handler`] — `StateHandler` factory

use std::sync::{Arc, Mutex};

use agentic_core::{
    back_target::BackTarget,
    back_target::RetryContext,
    human_input::SuspendedRunData,
    orchestrator::{CompletedTurn, RunContext, SessionMemory, StateHandler, TransitionResult},
    result::CellValue,
    solver::DomainSolver,
    state::ProblemState,
    HumanInputQuestion,
};

use crate::llm::{InitialMessages, ThinkingConfig, ToolLoopConfig};
use crate::tools::{execute_interpreting_tool, interpreting_tools, suggest_chart_config};
use crate::types::{ConversationTurn, DisplayBlock, QuestionType};
use crate::{AnalyticsAnswer, AnalyticsDomain, AnalyticsError, AnalyticsResult};

use super::{
    emit_domain,
    prompts::{
        format_history_section, format_retry_section, INTERPRET_SYSTEM_PROMPT,
        MULTI_RESULT_INTERPRET_ADDON,
    },
    AnalyticsSolver,
};

fn cell_to_json(cell: &CellValue) -> serde_json::Value {
    match cell {
        CellValue::Text(s) => serde_json::Value::String(s.clone()),
        CellValue::Number(n) => serde_json::json!(n),
        CellValue::Null => serde_json::Value::Null,
    }
}

// ---------------------------------------------------------------------------
// User-prompt builder
// ---------------------------------------------------------------------------

/// Format a single `QueryResultSet` as a markdown block for the LLM prompt.
fn format_result_set(rs: &crate::types::QueryResultSet, label: Option<&str>) -> String {
    let columns = &rs.data.columns;
    let sample_size = rs.data.rows.len();
    let total_row_count = rs.data.total_row_count;
    let is_tabular = columns.len() >= 2 && sample_size >= 2;

    let rows: Vec<Vec<String>> = rs
        .data
        .rows
        .iter()
        .map(|row| {
            row.0
                .iter()
                .map(|cell| match cell {
                    CellValue::Text(s) => s.clone(),
                    CellValue::Number(n) => n.to_string(),
                    CellValue::Null => "NULL".to_string(),
                })
                .collect()
        })
        .collect();

    let row_context = if (sample_size as u64) < total_row_count {
        format!("{total_row_count} rows total, showing {sample_size}.")
    } else {
        format!("{total_row_count} rows total.")
    };

    let summary_context = if let Some(summary) = &rs.summary {
        let stats: Vec<String> = summary
            .columns
            .iter()
            .filter_map(|c| {
                let mean = c.mean.map(|m| format!("mean={m:.2}"))?;
                let std_dev = c
                    .std_dev
                    .map(|s| format!("std_dev={s:.2}"))
                    .unwrap_or_default();
                Some(format!("  {}: {mean} {std_dev}", c.name))
            })
            .collect();
        if !stats.is_empty() {
            format!("\nColumn stats:\n{}", stats.join("\n"))
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let data_section = if is_tabular {
        let header = format!("| {} |", columns.join(" | "));
        let separator = format!(
            "| {} |",
            columns
                .iter()
                .map(|_| "---")
                .collect::<Vec<_>>()
                .join(" | ")
        );
        let body: Vec<String> = rows
            .iter()
            .map(|r| format!("| {} |", r.join(" | ")))
            .collect();
        format!("{}\n{}\n{}", header, separator, body.join("\n"))
    } else {
        let flat: Vec<String> = rows.iter().map(|r| r.join(" | ")).collect();
        format!(
            "Columns: {}\nRows:\n{}",
            columns.join(", "),
            flat.join("\n")
        )
    };

    match label {
        Some(lbl) => format!("**{lbl}** ({row_context}){summary_context}\n{data_section}"),
        None => format!("{row_context}{summary_context}\n{data_section}"),
    }
}

/// Build the user-turn message for the Interpret LLM call.
///
/// `pub(super)` so the unit tests in `mod.rs` can access it directly.
pub(super) fn build_interpret_user_prompt(
    raw_question: &str,
    history: &[ConversationTurn],
    result: &crate::types::AnalyticsResult,
    retry_ctx: Option<&RetryContext>,
    session_turns: &[CompletedTurn<AnalyticsDomain>],
    suggested_config: Option<&crate::types::ChartConfig>,
) -> String {
    // Format result data — one block per result set for fan-out queries.
    let data_section = if result.is_multi() {
        result
            .results
            .iter()
            .enumerate()
            .map(|(i, rs)| {
                format_result_set(
                    rs,
                    Some(&format!("Result set {} (result_index: {})", i + 1, i)),
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    } else {
        format_result_set(result.primary(), None)
    };

    // Prior turn context for comparative framing (most recent turn only).
    let prior_turn_section = if let Some(last) = session_turns.last() {
        format!(
            "\nPrevious question: {}\nPrevious answer: {}\n\n\
             If the current question is a follow-up, frame the answer \
             comparatively (e.g. \"Unlike the previous result…\" or \
             \"Breaking down the same data differently…\").",
            last.intent.raw_question, last.answer.text,
        )
    } else {
        String::new()
    };

    // All chart configs from every prior session turn — the LLM needs the full
    // history to handle chart-edit requests like "change that to a bar chart".
    let prior_charts_section = {
        let charts: Vec<String> = session_turns
            .iter()
            .enumerate()
            .flat_map(|(turn_idx, t)| {
                t.answer
                    .display_blocks
                    .iter()
                    .enumerate()
                    .map(move |(chart_idx, db)| {
                        let json = serde_json::to_string(&db.config).unwrap_or_default();
                        format!("  Turn {}, chart {}: {json}", turn_idx + 1, chart_idx + 1)
                    })
            })
            .collect();
        if charts.is_empty() {
            String::new()
        } else {
            format!(
                "\n\nPrevious chart configs (reference when the user asks to edit a chart):\n{}",
                charts.join("\n")
            )
        }
    };

    let chart_suggestion_section = if let Some(cfg) = suggested_config {
        let json = serde_json::to_string(cfg).unwrap_or_default();
        format!(
            "\n\nSuggested chart config (auto-computed from result shape — use as a starting point):\n{json}"
        )
    } else {
        String::new()
    };

    let history_section = format_history_section(history);
    let retry_section = format_retry_section(retry_ctx);

    format!(
        "{history_section}Original question: {raw_question}\n\n\
         Query results:\n{data_section}\
         {prior_charts_section}\
         {chart_suggestion_section}\n\n\
         Answer the original question based on these results.\
         {prior_turn_section}{retry_section}",
    )
}

// ---------------------------------------------------------------------------
// interpret_impl
// ---------------------------------------------------------------------------

impl AnalyticsSolver {
    /// Core interpret logic, shared by the trait impl and the interpreting handler.
    ///
    /// `raw_question` and `history` come from the spec's intent.  The trait's
    /// `interpret` method passes `""` / `&[]` / `None` (no run_ctx available);
    /// the custom interpreting handler supplies real values from `run_ctx.spec`.
    /// `session_turns` carries prior completed turns for comparative framing.
    /// `question_type` drives the deterministic chart suggestion.
    pub(crate) async fn interpret_impl(
        &mut self,
        raw_question: &str,
        history: &[ConversationTurn],
        result: AnalyticsResult,
        session_turns: &[CompletedTurn<AnalyticsDomain>],
        question_type: Option<&QuestionType>,
    ) -> Result<AnalyticsAnswer, (AnalyticsError, BackTarget<AnalyticsDomain>)> {
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
            .client
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
                        questions: vec![HumanInputQuestion {
                            prompt,
                            suggestions: vec!["Continue".to_string()],
                        }],
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
pub(super) fn build_interpreting_handler(
) -> StateHandler<AnalyticsDomain, AnalyticsSolver, crate::AnalyticsEvent> {
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
                        Ok(answer) => TransitionResult::ok(ProblemState::Done(answer)),
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
