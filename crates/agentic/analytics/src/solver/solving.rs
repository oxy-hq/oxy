//! **Solving** pipeline stage.
//!
//! Owns:
//! - [`build_solve_user_prompt`] — prompt builder
//! - [`AnalyticsSolver::solve_impl`] — core LLM call
//! - [`build_solving_handler`] — `StateHandler` factory (includes should_skip logic)

use std::sync::Arc;

use agentic_core::{
    HumanInputQuestion,
    back_target::BackTarget,
    back_target::RetryContext,
    human_input::SuspendedRunData,
    orchestrator::{RunContext, SessionMemory, StateHandler, TransitionResult},
    solver::DomainSolver,
    state::ProblemState,
};

use crate::events::AnalyticsEvent;
use crate::llm::{ThinkingConfig, ToolLoopConfig};
use crate::schemas::solve_response_schema;
use crate::tools::execute_solving_tool;

use crate::types::SolutionPayload;
use crate::{AnalyticsDomain, AnalyticsError, AnalyticsSolution, QuerySpec};

use super::{
    AnalyticsSolver, emit_domain, fmt_result_shape,
    prompts::{SOLVE_BASE_PROMPT, format_retry_section, solve_type_addendum},
    strip_json_fences,
};

// ---------------------------------------------------------------------------
// Prompt builder
// ---------------------------------------------------------------------------

pub(super) fn build_solve_user_prompt(
    spec: &QuerySpec,
    retry_ctx: Option<&RetryContext>,
) -> String {
    let join_path: Vec<String> = spec
        .join_path
        .iter()
        .map(|(l, r, k)| format!("{l} JOIN {r} ON {k}"))
        .collect();

    // When a QueryRequest was produced, show the original semantic query
    // so the Solve LLM understands what was intended.
    let query_request_section = if let Some(qr) = &spec.query_request {
        let mut parts =
            vec!["Original semantic query (could not be compiled automatically):".to_string()];
        if !qr.measures.is_empty() {
            parts.push(format!("  measures: [{}]", qr.measures.join(", ")));
        }
        if !qr.dimensions.is_empty() {
            parts.push(format!("  dimensions: [{}]", qr.dimensions.join(", ")));
        }
        if !qr.time_dimensions.is_empty() {
            let tds: Vec<String> = qr
                .time_dimensions
                .iter()
                .map(|td| {
                    let gran = td.granularity.as_deref().unwrap_or("none");
                    let range = td
                        .date_range
                        .as_ref()
                        .map_or("none".to_string(), |r| r.join(" to "));
                    format!("{} (granularity: {}, range: {})", td.dimension, gran, range)
                })
                .collect();
            parts.push(format!("  time_dimensions: [{}]", tds.join(", ")));
        }
        format!("\n\n{}", parts.join("\n"))
    } else {
        String::new()
    };

    let context_section = if let Some(ctx) = &spec.context {
        let mut parts = Vec::new();
        if !ctx.schema_description.is_empty() {
            parts.push(format!(
                "Schema context:\n{}",
                ctx.schema_description.trim()
            ));
        }
        if !ctx.metric_definitions.is_empty() {
            let defs: Vec<String> = ctx
                .metric_definitions
                .iter()
                .map(|m| format!("  {} = {}", m.name, m.expr))
                .collect();
            parts.push(format!("Metric definitions:\n{}", defs.join("\n")));
        }
        if !ctx.dimension_definitions.is_empty() {
            let dims: Vec<String> = ctx
                .dimension_definitions
                .iter()
                .map(|d| format!("  {} ({})", d.name, d.data_type))
                .collect();
            parts.push(format!("Dimension definitions:\n{}", dims.join("\n")));
        }
        if !ctx.join_paths.is_empty() {
            let paths: Vec<String> = ctx
                .join_paths
                .iter()
                .map(|(a, b, jp)| format!("  {a} -> {b}: {}", jp.path))
                .collect();
            parts.push(format!("Join paths:\n{}", paths.join("\n")));
        }
        if let Some(reason) = &ctx.compile_failure_reason {
            parts.push(format!("Note: direct compilation failed ({reason}); use the above context to write SQL manually."));
        }
        if parts.is_empty() {
            String::new()
        } else {
            format!("\n\n{}", parts.join("\n\n"))
        }
    } else {
        String::new()
    };

    let retry_section = format_retry_section(retry_ctx);
    format!(
        "Analytics Spec:\n\
         - Question type: {question_type:?}\n\
         - Metrics: {metrics}\n\
         - Dimensions: {dimensions}\n\
         - Resolved tables: {tables}\n\
         - Resolved metrics: {resolved_metrics}\n\
         - Resolved filters (WHERE clause): {resolved_filters}\n\
         - Join path: {join_path}\n\
         - Expected result shape: {shape}\n\
         - Assumptions: {assumptions}{query_request_section}{context_section}\n\n\
         Write the SQL query.{retry_section}",
        question_type = spec.intent.question_type,
        metrics = spec.intent.metrics.join(", "),
        dimensions = spec.intent.dimensions.join(", "),
        tables = spec.resolved_tables.join(", "),
        resolved_metrics = spec.resolved_metrics.join(", "),
        resolved_filters = if spec.resolved_filters.is_empty() {
            "(none)".to_string()
        } else {
            spec.resolved_filters.join("; ")
        },
        join_path = if join_path.is_empty() {
            "(none)".to_string()
        } else {
            join_path.join("; ")
        },
        shape = fmt_result_shape(&spec.expected_result_shape),
        assumptions = if spec.assumptions.is_empty() {
            "(none)".to_string()
        } else {
            spec.assumptions.join("; ")
        },
    )
}

// ---------------------------------------------------------------------------
// Solver impl method
// ---------------------------------------------------------------------------

impl AnalyticsSolver {
    /// Core solve logic; `retry_ctx` is appended to the prompt on retries.
    pub(crate) async fn solve_impl(
        &mut self,
        spec: QuerySpec,
        retry_ctx: Option<&RetryContext>,
    ) -> Result<AnalyticsSolution, (AnalyticsError, BackTarget<AnalyticsDomain>)> {
        let user_prompt = build_solve_user_prompt(&spec, retry_ctx);

        // On resume from a budget suspension, rebuild the message history and
        // apply the stored budget overrides.
        let mut resume_max_tokens_override: Option<u32> = None;
        let mut resume_extra_rounds: u32 = 0;
        let initial: crate::llm::InitialMessages = if let Some(resume) = self.resume_data.take() {
            let prior: Vec<serde_json::Value> = resume.data.stage_data["conversation_history"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            match resume.data.stage_data["suspension_type"].as_str() {
                Some("max_tokens") => {
                    resume_max_tokens_override = resume.data.stage_data["max_tokens_override"]
                        .as_u64()
                        .map(|v| v as u32);
                    crate::llm::InitialMessages::Messages(
                        crate::llm::LlmClient::build_continue_messages(&prior),
                    )
                }
                Some("max_tool_rounds") => {
                    resume_extra_rounds =
                        resume.data.stage_data["extra_rounds"].as_u64().unwrap_or(0) as u32;
                    crate::llm::InitialMessages::Messages(
                        crate::llm::LlmClient::build_continue_messages(&prior),
                    )
                }
                _ => crate::llm::InitialMessages::User(user_prompt),
            }
        } else {
            crate::llm::InitialMessages::User(user_prompt)
        };

        let tools = AnalyticsSolver::tools_for_state_solving();
        let type_addendum = solve_type_addendum(&spec.intent.question_type);
        let solve_prompt = format!("{SOLVE_BASE_PROMPT}{type_addendum}");
        let solve_dialect = self
            .connectors
            .get(&spec.connector_name)
            .map(|c| c.dialect().as_str());
        let system_prompt = self.build_system_prompt("solving", &solve_prompt, solve_dialect);
        let thinking = self.thinking_for_state("solving", ThinkingConfig::Adaptive);
        let max_rounds = self.max_tool_rounds_for_state("solving", 3) + resume_extra_rounds;
        let connector = self
            .connectors
            .get(&spec.connector_name)
            .cloned()
            .expect("connector for spec must be registered");
        let spec_for_stage = spec.clone();
        let output = match self
            .client
            .run_with_tools(
                &system_prompt,
                initial,
                &tools,
                |name: String, params| {
                    let connector = Arc::clone(&connector);
                    Box::pin(async move { execute_solving_tool(&name, params, &*connector).await })
                },
                &self.event_tx,
                ToolLoopConfig {
                    max_tool_rounds: max_rounds,
                    state: "solving".into(),
                    thinking,
                    response_schema: Some(solve_response_schema()),
                    max_tokens_override: resume_max_tokens_override.or(self.max_tokens),
                    sub_spec_index: None,
                },
            )
            .await
        {
            Ok(v) => v,
            Err(crate::llm::LlmError::MaxTokensReached {
                current_max_tokens,
                prior_messages,
                ..
            }) => {
                let doubled = current_max_tokens.saturating_mul(2);
                let prompt = format!(
                    "The model ran out of token budget ({current_max_tokens} tokens). \
                     Continue with double the budget ({doubled} tokens)?"
                );
                let spec_value = serde_json::to_value(&spec_for_stage).unwrap_or_default();
                self.store_suspension_data(SuspendedRunData {
                    from_state: "solving".to_string(),
                    original_input: spec.intent.raw_question.clone(),
                    trace_id: String::new(),
                    stage_data: serde_json::json!({
                        "spec": spec_value,
                        "conversation_history": prior_messages,
                        "suspension_type": "max_tokens",
                        "max_tokens_override": doubled,
                    }),
                    question: prompt.clone(),
                    suggestions: vec!["Continue with double budget".to_string()],
                });
                return Err((
                    AnalyticsError::NeedsUserInput {
                        prompt: prompt.clone(),
                    },
                    BackTarget::Suspend {
                        questions: vec![HumanInputQuestion {
                            prompt,
                            suggestions: vec!["Continue with double budget".to_string()],
                        }],
                    },
                ));
            }
            Err(crate::llm::LlmError::MaxToolRoundsReached {
                rounds,
                prior_messages,
            }) => {
                let prompt = format!(
                    "The agent used all {rounds} allotted tool rounds. \
                     Continue with more rounds?"
                );
                let spec_value = serde_json::to_value(&spec_for_stage).unwrap_or_default();
                self.store_suspension_data(SuspendedRunData {
                    from_state: "solving".to_string(),
                    original_input: spec.intent.raw_question.clone(),
                    trace_id: String::new(),
                    stage_data: serde_json::json!({
                        "spec": spec_value,
                        "conversation_history": prior_messages,
                        "suspension_type": "max_tool_rounds",
                        "extra_rounds": rounds,
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
                let msg = format!("LLM call failed during solve: {e}");
                return Err((
                    AnalyticsError::NeedsUserInput { prompt: msg },
                    BackTarget::Solve(spec.clone(), Default::default()),
                ));
            }
        };

        let sql = if let Some(structured) = output.structured_response {
            structured["sql"]
                .as_str()
                .unwrap_or_default()
                .trim()
                .to_string()
        } else {
            strip_json_fences(&output.text).trim().to_string()
        };
        emit_domain(
            &self.event_tx,
            AnalyticsEvent::QueryGenerated { sql: sql.clone() },
        )
        .await;

        let solution_source = spec.solution_source.clone();
        let connector_name = spec.connector_name.clone();
        Ok(AnalyticsSolution {
            payload: SolutionPayload::Sql(sql),
            solution_source,
            connector_name,
        })
    }

    /// Returns the tool list for the solving state.
    pub(super) fn tools_for_state_solving() -> Vec<agentic_core::tools::ToolDef> {
        crate::tools::solving_tools()
    }
}

// ---------------------------------------------------------------------------
// State handler
// ---------------------------------------------------------------------------

/// Build the `StateHandler` for the **solving** state.
///
/// Handles three cases:
/// 1. Precomputed SQL (SemanticLayer/VendorEngine) — handled by `should_skip`,
///    never reaches this handler.
/// 2. QueryRequest with no precomputed SQL (compile failed in Specifying) —
///    try compile once more, then translate to raw context for LLM fallback.
/// 3. No QueryRequest — standard LLM SQL generation via `solve_impl`.
pub(super) fn build_solving_handler()
-> StateHandler<AnalyticsDomain, AnalyticsSolver, AnalyticsEvent> {
    StateHandler {
        next: "executing",
        execute: Arc::new(
            |solver: &mut AnalyticsSolver,
             state,
             _events,
             run_ctx: &RunContext<AnalyticsDomain>,
             _memory: &SessionMemory<AnalyticsDomain>| {
                Box::pin(async move {
                    let mut spec = match state {
                        ProblemState::Solving(s) => s,
                        _ => unreachable!("solving handler called with wrong state"),
                    };
                    let retry_ctx = run_ctx.retry_ctx.clone();

                    // ── QueryRequest with failed compile from Specifying ────────
                    // Try compile once more; on failure, enrich spec with raw
                    // context so solve_impl can generate SQL via LLM.
                    if spec.query_request.is_some() && spec.precomputed.is_none() {
                        let qr = spec.query_request.as_ref().unwrap();
                        match solver.catalog.engine().compile_query(qr) {
                            Ok(result) => {
                                let sql = crate::airlayer_compat::substitute_params(
                                    &result.sql,
                                    &result.params,
                                );
                                eprintln!(
                                    "[solving] re-compile SUCCESS: {}",
                                    &sql[..sql.len().min(200)]
                                );
                                // Validate before forwarding to Executing.
                                if let Some(run_spec) = &run_ctx.spec
                                    && let Err(err) = solver.validator.validate_solvable(
                                        &sql,
                                        run_spec,
                                        &solver.catalog,
                                    )
                                {
                                    emit_domain(
                                        &solver.event_tx,
                                        AnalyticsEvent::ValidationFailed {
                                            state: "solving".to_string(),
                                            reason: err.to_string(),
                                            model_response: sql.clone(),
                                        },
                                    )
                                    .await;
                                    let base = retry_ctx.clone().unwrap_or_default();
                                    let hint = base.advance(err.to_string());
                                    let back = BackTarget::Solve(run_spec.clone(), hint);
                                    return TransitionResult::diagnosing(
                                        ProblemState::Diagnosing { error: err, back },
                                    );
                                }
                                let solution = AnalyticsSolution {
                                    payload: SolutionPayload::Sql(sql),
                                    solution_source: crate::SolutionSource::SemanticLayer,
                                    connector_name: spec.connector_name.clone(),
                                };
                                return TransitionResult::ok(ProblemState::Executing(solution));
                            }
                            Err(e) => {
                                eprintln!("[solving] re-compile FAILED: {e}, falling back to LLM");
                                // Enrich spec with raw context for LLM SQL generation.
                                let translation =
                                    solver.catalog.translate_to_raw_context(qr, &e.to_string());
                                spec.context = Some(translation.context);
                                spec.resolved_metrics = translation.resolved_metrics;
                                spec.resolved_tables = translation.resolved_tables;
                                spec.resolved_filters = translation.resolved_filters;
                                spec.join_path = translation.join_path;
                                // Fall through to solve_impl below.
                            }
                        }
                    }

                    // ── Standard LLM SQL generation ─────────────────────────────
                    let solution_source = spec.solution_source.clone();
                    match solver.solve_impl(spec, retry_ctx.as_ref()).await {
                        Ok(mut solution) => {
                            solution.solution_source = solution_source;
                            if let Some(spec) = &run_ctx.spec
                                && let Err(err) = solver.validator.validate_solvable(
                                    solution.payload.expect_sql(),
                                    spec,
                                    &solver.catalog,
                                )
                            {
                                emit_domain(
                                    &solver.event_tx,
                                    AnalyticsEvent::ValidationFailed {
                                        state: "solving".to_string(),
                                        reason: err.to_string(),
                                        model_response: solution.payload.expect_sql().to_string(),
                                    },
                                )
                                .await;
                                let base = retry_ctx.clone().unwrap_or_default();
                                let hint = base.advance(err.to_string());
                                let back = BackTarget::Solve(spec.clone(), hint);
                                return TransitionResult::diagnosing(ProblemState::Diagnosing {
                                    error: err,
                                    back,
                                });
                            }
                            TransitionResult::ok(ProblemState::Executing(solution))
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
